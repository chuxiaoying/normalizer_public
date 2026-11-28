// Lower an observed module entry in source order. An await binds the
// completion of the request it waits for. Inputs bind to actual formals.
import ts from 'typescript';
import { normalizerMarkers, singleAnnotatedBinding, stateBinding } from './module-projection.mjs';
import { definedModuleExport, exactIntegerLiteral, isLocalExportList, parseSourceModule, staticMemberName, unwrapParens } from './ir.mjs';
import { localDirectives } from './local-directives.mjs';
import { assertTypeScript, typeScriptProgram } from './typed-module.mjs';
import { isOfficialNamespaceImport } from './standard-library.mjs';

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

function closedFormals(formals, role) {
  return formals.map(p => {
    if (!ts.isIdentifier(p.name) || p.dotDotDotToken || p.questionToken || p.initializer) {
      fail(`${role} requires named required parameters`);
    }
    return {name: p.name.text, type: hostType(p.type), declaration: p};
  });
}

function closedErrorDeclaration(declared, checker, sf) {
  if (!declared || declared.getSourceFile() !== sf || !ts.isClassDeclaration(declared)
      || declared.members.length || declared.modifiers?.some(ts.isDecorator)) return false;
  const heritage = declared.heritageClauses?.flatMap(clause => clause.types) ?? [];
  return heritage.length === 1 && heritage[0].expression.getText() === 'Error'
    && checker.getSymbolAtLocation(heritage[0].expression)?.declarations?.every(
      declaration => declaration.getSourceFile().isDeclarationFile);
}

function rejectionNames(method, checker, sf) {
  const names = [];
  for (const tag of ts.getJSDocTags(method)) {
    if (tag.tagName.text !== 'N') continue;
    const name = typeof tag.comment === 'string' && /^rejects\s+([A-Za-z_$][\w$]*)$/.exec(tag.comment.trim())?.[1];
    if (!name || names.includes(name)) fail('external method requires explicit, nonduplicate @N rejects ErrorType tags');
    const declared = checker.resolveName(name, method, ts.SymbolFlags.Value, false)?.valueDeclaration;
    if (!declared && name === 'ExternalFailure') {
      names.push(name);
      continue;
    }
    if (!closedErrorDeclaration(declared, checker, sf)) {
      fail('unresolved project exception declaration ' + name);
    }
    names.push(name);
  }
  return names;
}

export function readModuleEvent(modulePath, text, entryName, physicalPath = modulePath, options = {}) {
  const sf = parseSourceModule(physicalPath, text, fail, ts.ScriptKind.TS);
  const {checker, errors} = typeScriptProgram(physicalPath, sf, fail, {
    allowOfficial: sf.statements.some(isOfficialNamespaceImport),
  });
  const symbol = node => checker.getSymbolAtLocation(node)?.valueDeclaration;
  const types = Object.create(null), states = Object.create(null), parameters = Object.create(null);
  const typedIRReads = new Set();
  const inputs = new Map(), ports = new Map(), stateDeclarations = new Map();
  const fn = definedModuleExport(sf, entryName, fail);
  const async = fn.modifiers?.some(m => m.kind === ts.SyntaxKind.AsyncKeyword);
  const synchronous = options.synchronous ?? !async;
  if (synchronous) {
    // A local directive can prove definite assignment later in this pass.
    // It cannot make another source type or syntax error valid.
    const earlyErrors = errors.filter(d => d.code !== 2454);
    if (earlyErrors.length) assertTypeScript(sf, checker, earlyErrors, fail);
  }
  if (synchronous) {
    if (fn.asteriskToken || async || fn.type?.kind !== ts.SyntaxKind.VoidKeyword) {
      fail('selected synchronous event requires an explicitly void exported function');
    }
  } else if (fn.asteriskToken || !async) {
    fail('entry needs an exported async function');
  }
  function promiseValue(type, allowed = undefined) {
    if (!type || !ts.isTypeReferenceNode(type) || type.typeName.getText() !== 'Promise' ||
        type.typeArguments?.length !== 1 ||
        !checker.getSymbolAtLocation(type.typeName)?.declarations?.some(d => d.getSourceFile().isDeclarationFile) ||
        (allowed && type.typeArguments[0].kind !== allowed)) {
      fail('awaited external port requires a closed Promise<Element>');
    }
    return type.typeArguments[0];
  }
  if (!synchronous) promiseValue(fn.type, ts.SyntaxKind.VoidKeyword);
  for (const [position, p] of closedFormals(fn.parameters, 'entry').entries()) {
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
    if (!marks.size) {
      if (ts.isFunctionDeclaration(st) || ts.isTypeAliasDeclaration(st) || ts.isInterfaceDeclaration(st)
          || isLocalExportList(st) || isOfficialNamespaceImport(st)
          || closedErrorDeclaration(st, checker, sf)) continue;
      if (synchronous && ts.isVariableStatement(st)) {
        fail('synchronous event module data requires a declared source role, such as @N entryState');
      }
      fail('event module cannot skip host installation or executable module initialization');
    }
    if (st !== fn && marks.size === 1 && marks.has('componentProp') && ts.isFunctionDeclaration(st)
        && st.body && !st.parameters.length && !st.asteriskToken
        && !st.modifiers?.some(modifier => modifier.kind === ts.SyntaxKind.AsyncKeyword)) continue;
    if (!ts.isVariableStatement(st) || marks.size !== 1 || marks.has('componentProp')) {
      fail('event host marks must attach to one declaration');
    }
    const declaration = singleAnnotatedBinding(st, 'a host declaration must mark one named binding', fail);
    const root = declaration.name.text;
    if (marks.has('entryState')) {
      stateDeclarations.set(root, declaration);
      states[root] = {kind: 'state', ...stateBinding(declaration, root, modulePath, checker)};
      continue;
    }
    if (synchronous) fail('synchronous state event does not admit a port without a closed synchronous contract');
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
        params: closedFormals(method.parameters, 'external port').map(({name, type}) => ({name, type})),
        returns: hostType(promiseValue(method.type)), throws: rejectionNames(method, checker, sf),
      });
    }
  }
  let serial = 0;
  const local = () => 'observed_' + serial++;
  const variable = (name) => ({ op: 'var', name });
  // The only callback available in a value expression reads invariant P.
  // State, port and task effects are emitted by sequence at their own site.
  function lowerValue(n, scope) {
    n = unwrapParens(n);
    if (ts.isIdentifier(n)) {
      const declaration = symbol(n);
      if (inputs.get(n.text) === declaration && declaration) return variable(n.text);
      const slot = scope.get(n.text);
      if (!slot || declaration !== slot.declaration) fail('unbound local ' + n.text);
      if (slot.recordFields) fail('local record reference cannot escape its synchronous scope: ' + n.text);
      if (slot.assignedByDirective) typedIRReads.add(n.getStart(sf));
      return variable(n.text);
    }
    const number = exactIntegerLiteral(n, sf);
    if (number !== undefined) return { op: 'const', type: 'number', value: number };
    if (n.kind === ts.SyntaxKind.FalseKeyword || n.kind === ts.SyntaxKind.TrueKeyword)
      return { op: 'const', type: 'bool', value: n.kind === ts.SyntaxKind.TrueKeyword };
    if ((ts.isPropertyAccessExpression(n) || ts.isElementAccessExpression(n)) && n.questionDotToken) {
      fail('optional access needs a checked nullish branch before field observation');
    }
    const member = staticMemberName(n);
    if (member !== undefined) {
      const fields = [];
      let root = n, name;
      while ((name = staticMemberName(root)) !== undefined) {
        fields.unshift(name);
        root = unwrapParens(root.expression);
      }
      if (ts.isIdentifier(root)) {
        const slot = scope.get(root.text);
        if (slot?.recordFields && symbol(root) === slot.declaration) {
          const field = fields.length === 1 && slot.recordFields.get(fields[0]);
          if (!field) fail('unbound local record field ' + n.getText(sf));
          return variable(field);
        }
      }
      if (ts.isIdentifier(root) && stateDeclarations.has(root.text)
          && symbol(root) === stateDeclarations.get(root.text)) {
        return {op: 'source.state_value', path: [root.text, ...fields]};
      }
      return { op: 'value.field', value: lowerValue(n.expression, scope), name: member };
    }
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
  function sequence(block, caught, caughtTypes, enclosing = new Map(), outstanding = new Set()) {
    const scope = new Map(enclosing);
    const steps = [];
    const locals = new Map();
    let before = block.getFirstToken(sf).end;
    let uninitialized;
    for (let index = 0; index <= block.statements.length; index++) {
      const st = block.statements[index];
      const end = st ? st.getStart(sf) : block.end - 1;
      for (const {text, line} of directives.after(before, end)) {
        const binding = /^([A-Za-z_$][\w$]*)\s*(\+=|=(?!=))/.exec(text);
        if (!binding || !locals.get(binding[1])
            || uninitialized && (binding[1] !== uninitialized || binding[2] !== '='))
          fail('local IR requires a writable local assignment in this block and must initialize any pending binding');
        steps.push({ op: 'local_ir', source: {text, location: modulePath + ':' + line} });
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
        if (synchronous && ts.isObjectLiteralExpression(init) && !writable) {
          // These nonescaping records are local to this synchronous entry.
          // Use bound field versions so updating a shadowing local does
          // not become a write to the module's entry-state object.
          const recordFields = new Map();
          for (const property of init.properties) {
            if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)
                || property.name.text === '__proto__' || recordFields.has(property.name.text)) {
              fail('local record requires plain unique initialized fields');
            }
            const key = local();
            steps.push({op: 'let', name: key, value: lowerValue(property.initializer, scope)});
            recordFields.set(property.name.text, key);
          }
          scope.set(name, {declaration: dec, recordFields});
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
        [ts.SyntaxKind.EqualsToken, ts.SyntaxKind.PlusEqualsToken].includes(st.expression.operatorToken.kind) &&
        staticMemberName(st.expression.left) !== undefined
      ) {
        const target = [];
        let root = st.expression.left, name;
        while ((name = staticMemberName(root)) !== undefined) {
          target.unshift(name);
          root = unwrapParens(root.expression);
        }
        const assignedValue = () => {
          const {left, right, operatorToken} = st.expression;
          if (operatorToken.kind === ts.SyntaxKind.EqualsToken) return lowerValue(right, scope);
          if (![left, right].every(value => checker.getTypeAtLocation(value).flags & ts.TypeFlags.NumberLike)) {
            fail('numeric compound assignment requires two checked numeric operands');
          }
          // The target path has a fixed, checked root. JS reads its old
          // value before the RHS; the one following write publishes the sum.
          return {op: 'add', left: lowerValue(left, scope), right: lowerValue(right, scope)};
        };
        if (ts.isIdentifier(root)) {
          const slot = scope.get(root.text);
          if (slot?.recordFields && symbol(root) === slot.declaration) {
            if (target.length !== 1 || !slot.recordFields.has(target[0])) fail('unbound local record write');
            const value = assignedValue();
            const key = local();
            steps.push({op: 'let', name: key, value});
            slot.recordFields.set(target[0], key);
            continue;
          }
        }
        if (!ts.isIdentifier(root) || !stateDeclarations.has(root.text) ||
            symbol(root) !== stateDeclarations.get(root.text)) {
          fail('write has no bound @N entryState root: ' + root.getText());
        }
        target.unshift(root.text);
        const value = assignedValue();
        steps.push({ op: 'write_state', target, value });
        continue;
      }
      if (!synchronous && ts.isTryStatement(st) && st.catchClause && !st.finallyBlock) {
        const raised = new Set();
        const body = sequence(st.tryBlock, caught, caughtTypes, scope, raised);
        const classes = [...raised].sort();
        const name = st.catchClause.variableDeclaration?.name.getText();
        const escapes = new Set();
        const handler = sequence(st.catchClause.block, name, classes, scope, escapes);
        if (classes.length) for (const error of escapes) outstanding.add(error);
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
  assertTypeScript(sf, checker, errors.filter(d => d.code !== 2454 || d.file !== sf || !typedIRReads.has(d.start)), fail);
  steps.push({ op: 'return', value: { op: 'const', type: 'unit', value: null } });
  let environment = states;
  if (synchronous) {
    environment = Object.create(null);
    const retain = target => {
      const [root, ...path] = target;
      const declared = states[root];
      environment[root] ??= {kind: 'state', source: declared.source, fields: Object.create(null)};
      let full = declared.fields;
      let observed = environment[root].fields;
      for (const name of path.slice(0, -1)) {
        full = full[name];
        observed = observed[name] ??= Object.create(null);
      }
      const last = path.at(-1);
      observed[last] = full[last];
    };
    const retainAccesses = node => {
      if (!node || typeof node !== 'object') return;
      if (node.op === 'write_state') retain(node.target);
      if (node.op === 'source.state_value') retain(node.path);
      for (const item of Object.values(node)) retainAccesses(item);
    };
    retainAccesses(steps);
  }
  return {
    function: modulePath + '#' + entryName,
    types,
    environment,
    parameters,
    ports: [...ports.values()].map(({declaration: _d, method: _m, ...port}) => port),
    steps,
  };
}
