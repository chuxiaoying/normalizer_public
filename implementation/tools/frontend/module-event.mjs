// Lower an observed async module entry in source order. An await binds the
// completion of the request it waits for. Inputs bind to actual formals.
import ts from 'typescript';
import { normalizerMarkers, singleAnnotatedBinding, stateBinding } from './module-projection.mjs';
import { definedModuleExport, parseSourceModule, unwrapParens } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';
import { assertTypeScript, typeScriptProgram } from './typed-module.mjs';

function fail(message) {
  throw new Error(`SCENARIO_MODULE_INPUT: ${message}`);
}

const parameterAliases = new Map([['boolean', 'bool'], ['string', 'text']]);
function hostType(node) {
  if (!node) fail('entry and external bindings require explicit types');
  if (ts.isArrayTypeNode(node)) return hostType(node.elementType) + '[]';
  const type = node.getText();
  if (!/^[A-Za-z_$][\w$]*$/.test(type)) fail(`unsupported module type ${type}`);
  return parameterAliases.get(type) ?? type;
}

function closedFormals(formals) {
  return formals.map(p => {
    if (!ts.isIdentifier(p.name) || p.dotDotDotToken || p.questionToken || p.initializer) {
      fail('entry and external port require named required parameters');
    }
    return {name: p.name.text, type: hostType(p.type), declaration: p};
  });
}

function rejectionNames(method, checker, sf) {
  const names = [];
  for (const tag of ts.getJSDocTags(method)) {
    if (tag.tagName.text !== 'N') continue;
    const name = typeof tag.comment === 'string' && /^rejects\s+([A-Za-z_$][\w$]*)$/.exec(tag.comment.trim())?.[1];
    if (!name || names.includes(name)) fail('external method requires explicit, nonduplicate @N rejects ErrorType tags');
    const declared = checker.resolveName(name, method, ts.SymbolFlags.Value, false)?.valueDeclaration;
    const heritage = declared?.heritageClauses?.flatMap(h => h.types) ?? [];
    if (!declared || declared.getSourceFile() !== sf || !ts.isClassDeclaration(declared) ||
        heritage.length !== 1 || heritage[0].expression.getText() !== 'Error' ||
        !checker.getSymbolAtLocation(heritage[0].expression)?.declarations?.some(d => d.getSourceFile().isDeclarationFile)) {
      fail('unresolved project exception declaration ' + name);
    }
    names.push(name);
  }
  if (names.length === 0) fail('awaited external port requires a closed Promise<Element> and named rejection types');
  return names;
}

export function readModuleEvent(modulePath, text, entryName, physicalPath = modulePath) {
  const sf = parseSourceModule(physicalPath, text, fail, ts.ScriptKind.TS);
  const {checker, errors} = typeScriptProgram(physicalPath, sf, fail);
  const symbol = node => checker.getSymbolAtLocation(node)?.valueDeclaration;
  const types = Object.create(null), states = Object.create(null), parameters = Object.create(null);
  const typedIRReads = new Set();
  const inputs = new Map(), ports = new Map(), stateDeclarations = new Map();
  const fn = definedModuleExport(sf, entryName, fail);
  if (fn.asteriskToken || !fn.modifiers.some(m => m.kind === ts.SyntaxKind.AsyncKeyword)) {
    fail('entry needs an exported async function');
  }
  function promiseValue(type, allowed = undefined) {
    if (!type || !ts.isTypeReferenceNode(type) || type.typeName.getText() !== 'Promise' ||
        type.typeArguments?.length !== 1 ||
        !checker.getSymbolAtLocation(type.typeName)?.declarations?.some(d => d.getSourceFile().isDeclarationFile) ||
        (allowed && type.typeArguments[0].kind !== allowed)) {
      fail('awaited external port requires a closed Promise<Element> and named rejection types');
    }
    return type.typeArguments[0];
  }
  promiseValue(fn.type, ts.SyntaxKind.VoidKeyword);
  for (const [position, p] of closedFormals(fn.parameters).entries()) {
    parameters[p.name] = {position, type: p.type};
    inputs.set(p.name, p.declaration);
  }
  for (const st of sf.statements) {
    const marks = normalizerMarkers(st, fail);
    if (ts.isTypeAliasDeclaration(st) && ts.isTypeLiteralNode(st.type)) {
      const fields = Object.create(null);
      for (const field of st.type.members) {
        if (!ts.isPropertySignature(field) || !ts.isIdentifier(field.name) || field.questionToken ||
            Object.hasOwn(fields, field.name.text)) fail('record types require distinct named typed fields');
        fields[field.name.text] = hostType(field.type);
      }
      types[st.name.text] = {fields};
    }
    if (!marks.size) continue;
    if (!ts.isVariableStatement(st) || marks.size !== 1 || marks.has('componentProp')) {
      fail('event host marks must attach to one declaration');
    }
    const declaration = singleAnnotatedBinding(st, 'a host declaration must mark one named binding', fail);
    const root = declaration.name.text;
    if (marks.has('entryState')) {
      stateDeclarations.set(root, declaration);
      states[root] = {kind: 'state', ...stateBinding(declaration, root, modulePath)};
      continue;
    }
    if (!st.modifiers?.some(m => m.kind === ts.SyntaxKind.DeclareKeyword) || !declaration.type ||
        !ts.isTypeReferenceNode(declaration.type) || declaration.type.typeArguments?.length || declaration.initializer) {
      fail('@N external requires a declared receiver with a closed interface');
    }
    const declarations = checker.getSymbolAtLocation(declaration.type.typeName)?.declarations;
    const iface = declarations?.length === 1 && declarations[0];
    if (!iface || iface.getSourceFile() !== sf || !ts.isInterfaceDeclaration(iface) ||
        iface.typeParameters?.length || iface.heritageClauses?.length) {
      fail('@N external receiver requires one source interface');
    }
    for (const method of iface.members) {
      if (!ts.isMethodSignature(method) || !ts.isIdentifier(method.name) || method.questionToken || method.typeParameters?.length) {
        fail('@N external interface requires named, required methods');
      }
      const path = root + '.' + method.name.text;
      if (ports.has(path)) fail('ambiguous external method ' + path);
      ports.set(path, {
        id: 'port_' + ports.size, endpoint: modulePath + '#' + path, declaration, method, excerpt: method.getText(sf),
        params: closedFormals(method.parameters).map(({name, type}) => ({name, type})),
        returns: hostType(promiseValue(method.type)), throws: rejectionNames(method, checker, sf),
      });
    }
  }
  let serial = 0;
  const local = () => 'observed_' + serial++;
  const variable = (name) => ({ op: 'var', name });
  const outstanding = new Set();
  // The only callback available in a value expression reads invariant P.
  // State, port and task effects are emitted by sequence at their own site.
  function lowerValue(n, scope) {
    n = unwrapParens(n);
    if (ts.isIdentifier(n)) {
      const declaration = symbol(n);
      if (inputs.get(n.text) === declaration && declaration) return variable(n.text);
      const slot = scope.get(n.text);
      if (!slot || declaration !== slot.declaration) fail('unbound local ' + n.text);
      if (slot.assignedByDirective) typedIRReads.add(n.getStart(sf));
      return variable(n.text);
    }
    if (ts.isNumericLiteral(n) && Number.isSafeInteger(Number(n.text)))
      return { op: 'const', type: 'number', value: Number(n.text) };
    if (n.kind === ts.SyntaxKind.FalseKeyword || n.kind === ts.SyntaxKind.TrueKeyword)
      return { op: 'const', type: 'bool', value: n.kind === ts.SyntaxKind.TrueKeyword };
    if (ts.isPropertyAccessExpression(n))
      return { op: 'value.field', value: lowerValue(n.expression, scope), name: n.name.text };
    if (ts.isConditionalExpression(n)) {
      const cond = lowerValue(n.condition, scope);
      const then = lowerValue(n.whenTrue, scope);
      const otherwise = lowerValue(n.whenFalse, scope);
      return { op: 'if', cond, then, else: otherwise };
    }
    if (ts.isBinaryExpression(n)) {
      const op = new Map([
        [ts.SyntaxKind.GreaterThanToken, 'gt'],
        [ts.SyntaxKind.EqualsEqualsEqualsToken, 'eq'],
        [ts.SyntaxKind.PlusToken, 'add'],
      ]).get(n.operatorToken.kind);
      if (op) return { op, left: lowerValue(n.left, scope), right: lowerValue(n.right, scope) };
    }
  }
  const directives = localDirectives(sf, fn.body);
  function sequence(block, caught, caughtTypes, enclosing = new Map()) {
    const scope = new Map(enclosing);
    const steps = [];
    const locals = new Map();
    let before = block.getFirstToken(sf).end;
    let uninitialized;
    for (let index = 0; index <= block.statements.length; index++) {
      const st = block.statements[index];
      const end = st ? st.getStart(sf) : block.end - 1;
      for (const source of directives.after(before, end)) {
        const binding = /^([A-Za-z_$][\w$]*)\s*(\+=|=(?!=))/.exec(source);
        if (!binding || !locals.get(binding[1])
            || uninitialized && (binding[1] !== uninitialized || binding[2] !== '='))
          fail('local IR requires a writable local assignment in this block and must initialize any pending binding');
        steps.push({ op: 'local_ir', source });
        if (uninitialized === binding[1]) scope.get(binding[1]).assignedByDirective = true;
        uninitialized = undefined;
      }
      if (!st) break;
      before = st.end;
      if (ts.isVariableStatement(st) && st.declarationList.declarations.length === 1) {
        const dec = st.declarationList.declarations[0];
        if (!ts.isIdentifier(dec.name)) fail('event local requires a named binding');
        const name = dec.name.text;
        if (scope.has(name) || inputs.has(name) || name.startsWith('observed_')) {
          fail('event local must have a fresh transport binding: ' + name);
        }
        const init = dec.initializer;
        const writable = Boolean(st.declarationList.flags & ts.NodeFlags.Let);
        locals.set(name, writable);
        if (!init) {
          if (!writable || !dec.type || uninitialized) fail('uninitialized event local needs one writable, explicitly typed binding');
          scope.set(name, {declaration: dec, assignedByDirective: false});
          uninitialized = name;
          continue;
        }
        if (ts.isAwaitExpression(init) && ts.isCallExpression(init.expression)) {
          const call = init.expression, callee = call.expression;
          if (!ts.isPropertyAccessExpression(callee) || !ts.isIdentifier(callee.expression) ||
              callee.questionDotToken || call.questionDotToken) fail('await requires a direct closed external call');
          const external = callee.expression.text + '.' + callee.name.text;
          const port = ports.get(external);
          if (!port || symbol(callee.expression) !== port.declaration ||
              !checker.getSymbolAtLocation(callee.name)?.declarations?.includes(port.method)) {
            fail('awaited call has no bound @N external receiver: ' + external);
          }
          const args = call.arguments.map((arg) => lowerValue(arg, scope));
          const task = local();
          steps.push(
            { op: 'task_invoke', port: port.id, site: 'call_' + st.getStart(sf), args, name: task },
            { op: 'task_wait', name, task: variable(task) },
          );
          scope.set(name, {declaration: dec});
          for (const error of port.throws) outstanding.add(error);
          continue;
        }
        steps.push({ op: 'let', name, value: lowerValue(init, scope) });
        scope.set(name, {declaration: dec});
        continue;
      }
      if (
        ts.isExpressionStatement(st) &&
        ts.isBinaryExpression(st.expression) &&
        st.expression.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
        ts.isPropertyAccessExpression(st.expression.left)
      ) {
        const target = [];
        let root = st.expression.left;
        while (ts.isPropertyAccessExpression(root) && !root.questionDotToken) {
          target.unshift(root.name.text);
          root = unwrapParens(root.expression);
        }
        if (!ts.isIdentifier(root) || !stateDeclarations.has(root.text) ||
            symbol(root) !== stateDeclarations.get(root.text)) {
          fail('write has no bound @N entryState root: ' + root.getText());
        }
        target.unshift(root.text);
        const value = lowerValue(st.expression.right, scope);
        steps.push({ op: 'write_state', target, value });
        continue;
      }
      if (ts.isTryStatement(st) && st.catchClause && !st.finallyBlock) {
        outstanding.clear();
        const body = sequence(st.tryBlock, undefined, undefined, scope);
        const classes = [...outstanding].sort();
        const name = st.catchClause.variableDeclaration?.name.getText();
        outstanding.clear();
        const handler = sequence(st.catchClause.block, name, classes, scope);
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
      fail('unlowered event statement: ' + ts.SyntaxKind[st.kind]);
    }
    return steps;
  }
  // Formals denote the fixed invocation snapshot. Publishing their
  // internal locals does not perform an environment observation. Local
  // relation IR can now resolve P by ordinary lexical names, without
  // rewriting an identically named row alias or field in that language.
  const formals = [...inputs.keys()].map(input => ({op: 'read_input', name: input, input}));
  const steps = [...formals, ...sequence(fn.body, undefined)];
  directives.requireConsumed();
  // TypeScript does not execute relation comments. Only definite reads of
  // their same bound declaration after a recognized assignment in its
  // actual lexical scope may bypass this one TS diagnostic. All other
  // failures, including those on unreachable source branches, still reject.
  assertTypeScript(errors.filter(d => d.code !== 2454 || d.file !== sf || !typedIRReads.has(d.start)), fail);
  steps.push({ op: 'return', value: { op: 'const', type: 'unit', value: null } });
  return {
    function: modulePath + '#' + entryName,
    types,
    environment: states,
    parameters,
    ports: [...ports.values()].map(({declaration: _d, method: _m, ...port}) => port),
    steps,
  };
}
