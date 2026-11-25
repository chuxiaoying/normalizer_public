// Lower an observed async module entry in source order. An await binds the
// completion of the request it waits for. Host slots are read when encountered.
import ts from 'typescript';
import { basename, extname } from 'node:path';
import { literalShape } from './module-projection.mjs';
import { localDirectives } from './local-directives.mjs';

function fail(message) {
  throw new Error(`SCENARIO_MODULE_INPUT: ${message}`);
}

function hostType(type) {
  if (type === 'boolean') return 'bool';
  if (type === 'string') return 'text';
  if (type === 'number' || type === 'void' || /^[A-Za-z_$][\w$]*(\[\])?$/.test(type)) return type;
  fail(`unsupported module type ${type}`);
}

export function readModuleEvent(modulePath, text, entryName, originPath) {
  const sf = ts.createSourceFile(modulePath, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  if (sf.parseDiagnostics.length)
    fail(ts.flattenDiagnosticMessageText(sf.parseDiagnostics[0].messageText, '\n'));
  const types = Object.create(null),
    states = Object.create(null),
    parameters = Object.create(null);
  const inputs = new Map(),
    callbacks = new Map(),
    ports = new Map();
  const page = basename(originPath, extname(originPath));
  const tags = (node) => (node.jsDoc || []).flatMap((doc) => [...(doc.tags || [])]);

  for (const statement of sf.statements) {
    for (const doc of statement.jsDoc || []) {
      for (const tag of doc.tags || []) {
        if (ts.isJSDocTypedefTag(tag)) {
          const fields = tag.typeExpression?.jsDocPropertyTags;
          if (!fields?.length || !tag.name) fail('record typedef requires named fields');
          types[tag.name.text] = {
            fields: Object.fromEntries(
              fields.map((field) => [
                field.name.getText(),
                hostType(field.typeExpression.type.getText()),
              ]),
            ),
          };
        }
        if (ts.isJSDocCallbackTag(tag)) {
          const result = tag.typeExpression?.type?.typeExpression?.type;
          if (
            !result ||
            !ts.isTypeReferenceNode(result) ||
            result.typeName.getText() !== 'Promise' ||
            result.typeArguments?.length !== 1
          ) {
            fail('awaited external callback must declare Promise<Element>');
          }
          const rejects = [...doc.tags]
            .filter((t) => t.tagName.text === 'rejects')
            .map((t) => {
              const name = typeof t.comment === 'string' && /^\{([\w.]+)\}$/.exec(t.comment)?.[1];
              if (!name) fail('@rejects needs an exception type');
              return name;
            });
          callbacks.set(tag.name.text, {
            params: tag.typeExpression.parameters.map((p) => ({
              name: p.name.text,
              type: hostType(p.typeExpression.type.getText()),
            })),
            returns: hostType(result.typeArguments[0].getText()),
            throws: rejects,
          });
        }
      }
    }
  }
  for (const st of sf.statements) {
    if (!ts.isVariableStatement(st)) continue;
    const flags = new Set(tags(st).map((t) => t.tagName.text));
    if (![...flags].some((flag) => ['entryState', 'entryInputs', 'external'].includes(flag))) continue;
    if (
      st.declarationList.declarations.length !== 1 ||
      !ts.isIdentifier(st.declarationList.declarations[0].name)
    ) {
      fail('a host declaration must mark one named binding');
    }
    const dec = st.declarationList.declarations[0];
    const root = dec.name.getText();
    if (flags.has('entryState')) {
      states[root] = {
        kind: 'state',
        source: `${originPath}#${page}.${root}`,
        fields: literalShape(dec.initializer),
      };
    }
    if (flags.has('entryInputs') || flags.has('external')) {
      const record = ts.getJSDocType(st) || ts.getJSDocType(dec);
      if (!record || !ts.isTypeLiteralNode(record)) fail(`${root} requires a closed record type`);
      for (const item of record.members) {
        if (
          !ts.isPropertySignature(item) ||
          !ts.isIdentifier(item.name) ||
          item.questionToken ||
          !item.type
        )
          fail('required named source slot needed');
        const name = item.name.text,
          path = root + '.' + name;
        if (flags.has('entryInputs')) {
          if (Object.hasOwn(parameters, name)) fail(`duplicate host parameter ${name}`);
          parameters[name] = {
            position: Object.keys(parameters).length,
            type: hostType(item.type.getText()),
          };
          inputs.set(path, name);
        } else {
          const signature = callbacks.get(item.type.getText());
          if (!signature) fail(`${path} requires a named callback`);
          ports.set(path, { ...signature, id: 'port_' + ports.size, endpoint: path });
        }
      }
    }
  }
  const fn = sf.statements.find((s) => ts.isFunctionDeclaration(s) && s.name?.text === entryName);
  if (
    !fn?.body ||
    !fn.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword) ||
    !fn.modifiers.some((m) => m.kind === ts.SyntaxKind.AsyncKeyword) ||
    fn.parameters.length
  )
    fail('entry needs an exported async function with no explicit arguments');
  let serial = 0;
  const local = () => 'observed_' + serial++;
  const variable = (name) => ({ op: 'var', name });
  const outstanding = new Set();
  function readInput(path, steps) {
    const input = inputs.get(path);
    if (!input) fail(`undeclared host input ${path}`);
    const name = local();
    steps.push({ op: 'read_input', name, input });
    return name;
  }
  function lower(n, steps) {
    if (ts.isParenthesizedExpression(n)) return lower(n.expression, steps);
    if (inputs.has(n.getText())) return variable(readInput(n.getText(), steps));
    if (ts.isIdentifier(n)) return variable(n.text);
    if (ts.isNumericLiteral(n) && Number.isSafeInteger(Number(n.text)))
      return { op: 'const', type: 'number', value: Number(n.text) };
    if (n.kind === ts.SyntaxKind.FalseKeyword || n.kind === ts.SyntaxKind.TrueKeyword)
      return { op: 'const', type: 'bool', value: n.kind === ts.SyntaxKind.TrueKeyword };
    if (ts.isStringLiteral(n)) return { op: 'const', type: 'text?', value: n.text };
    if (ts.isPropertyAccessExpression(n))
      return { op: 'value.field', value: lower(n.expression, steps), name: n.name.text };
    if (ts.isConditionalExpression(n)) {
      // Stateful reads in lazy branches require per-branch bindings.
      const condition = lower(n.condition, steps);
      const yes = [],
        no = [];
      const then = lower(n.whenTrue, yes),
        otherwise = lower(n.whenFalse, no);
      if (yes.length || no.length) fail('host reads in conditional arms require guarded observation');
      return { op: 'if', cond: condition, then, else: otherwise };
    }
    if (ts.isBinaryExpression(n)) {
      const op = new Map([
        [ts.SyntaxKind.GreaterThanToken, 'gt'],
        [ts.SyntaxKind.EqualsEqualsEqualsToken, 'eq'],
        [ts.SyntaxKind.PlusToken, 'add'],
        [ts.SyntaxKind.MinusToken, 'subtract'],
      ]).get(n.operatorToken.kind);
      if (op) return { op, left: lower(n.left, steps), right: lower(n.right, steps) };
    }
    fail(`unsupported expression ${n.getText()}`);
  }
  const directives = localDirectives(sf, fn.body);
  function sequence(block, caught, caughtTypes = []) {
    const steps = [];
    const locals = new Map();
    let before = block.getFirstToken(sf).end;
    let uninitialized;
    for (let index = 0; index <= block.statements.length; index++) {
      const st = block.statements[index];
      const end = st ? st.getStart(sf) : block.end - 1;
      for (let source of directives.after(before, end)) {
        const binding = /^([A-Za-z_$][\w$]*)\s*(\+=|=(?!=))/.exec(source);
        if (!binding) fail('local IR requires a named assignment');
        const [, name, operator] = binding;
        if (!locals.get(name))
          fail(`local IR requires a writable local ${name} in this block`);
        if (uninitialized && (name !== uninitialized || operator !== '='))
          fail(`local IR must initialize ${uninitialized}`);
        // Host observations execute at the directive's original location.
        source = source.replace(
          /"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|\b[A-Za-z_$][\w$]*\.[A-Za-z_$][\w$]*\b/g,
          (token) => (inputs.has(token) ? readInput(token, steps) : token),
        );
        steps.push({ op: 'local_ir', source });
        uninitialized = undefined;
      }
      if (uninitialized) fail(`uninitialized local ${uninitialized} needs a local IR assignment`);
      if (!st) break;
      before = st.end;
      if (ts.isVariableStatement(st) && st.declarationList.declarations.length === 1) {
        const dec = st.declarationList.declarations[0];
        if (!ts.isIdentifier(dec.name)) fail('async local must be a named binding');
        const name = dec.name.text;
        const init = dec.initializer;
        const writable = Boolean(st.declarationList.flags & ts.NodeFlags.Let);
        if (locals.has(name)) fail(`duplicate local ${name}`);
        locals.set(name, writable);
        if (!init) {
          if (!writable) fail(`local IR initialization requires a let declaration for ${name}`);
          uninitialized = name;
          continue;
        }
        if (ts.isAwaitExpression(init) && ts.isCallExpression(init.expression)) {
          const call = init.expression,
            external = call.expression.getText(),
            port = ports.get(external);
          if (!port) fail(`await needs a declared external callback: ${external}`);
          const args = call.arguments.map((arg) => lower(arg, steps));
          if (args.length !== port.params.length) fail(`incorrect arity for ${external}`);
          const task = local();
          steps.push(
            { op: 'task_invoke', port: port.id, site: 'call_' + st.getStart(sf), args, name: task },
            { op: 'task_wait', name, task: variable(task) },
          );
          for (const error of port.throws) outstanding.add(error);
          continue;
        }
        steps.push({ op: 'let', name, value: lower(init, steps) });
        continue;
      }
      if (
        ts.isExpressionStatement(st) &&
        ts.isBinaryExpression(st.expression) &&
        st.expression.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
        ts.isPropertyAccessExpression(st.expression.left)
      ) {
        const target = st.expression.left.getText().split('.');
        if (!Object.hasOwn(states, target[0]))
          fail(`assignment to undeclared state ${target.join('.')}`);
        const value = lower(st.expression.right, steps);
        steps.push({ op: 'write_state', target, value });
        continue;
      }
      if (ts.isTryStatement(st) && st.catchClause && !st.finallyBlock) {
        const enclosing = [...outstanding];
        outstanding.clear();
        const body = sequence(st.tryBlock, undefined);
        const classes = [...outstanding].sort();
        const name = st.catchClause.variableDeclaration?.name.getText();
        outstanding.clear();
        const handler = sequence(st.catchClause.block, name, classes);
        if (!classes.length) fail('catch body has no closed thrown-type declaration');
        for (const error of enclosing) outstanding.add(error);
        steps.push({ op: 'catch', classes, body, handler });
        continue;
      }
      if (
        ts.isThrowStatement(st) &&
        caught &&
        ts.isIdentifier(st.expression) &&
        st.expression.text === caught
      ) {
        for (const type of caughtTypes) outstanding.add(type);
        steps.push({ op: 'rethrow' });
        continue;
      }
      if (ts.isReturnStatement(st) && !st.expression) {
        steps.push({ op: 'return', value: { op: 'const', type: 'unit', value: null } });
        continue;
      }
      fail(`unsupported async statement: ${st.getText()}`);
    }
    return steps;
  }
  const steps = sequence(fn.body, undefined);
  directives.requireConsumed();
  if (steps.at(-1)?.op !== 'return')
    steps.push({ op: 'return', value: { op: 'const', type: 'unit', value: null } });
  return {
    function: originPath + '#' + entryName,
    types,
    environment: states,
    parameters,
    ports: [...ports.values()],
    steps,
  };
}
