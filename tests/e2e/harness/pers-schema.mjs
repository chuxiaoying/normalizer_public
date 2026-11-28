import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import Ajv2020 from '../../../implementation/tools/frontend/node_modules/ajv/dist/2020.js';
import YAML from '../../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const schemaFile = new URL('../../../implementation/contracts/pers/schema.yaml', import.meta.url);
const ajv = new Ajv2020({ allErrors: true, keywords: ['x-enums'] });
const validate = ajv.compile(YAML.parse(readFileSync(schemaFile, 'utf8')));

const scalar = new Set(['number', 'money', 'date', 'bool', 'text', 'void', 'map<text,text>']);

// Runtime publication proves exhaustiveness using its typed terminal
// alternatives, before serialization. The YAML-level checker can also
// independently certify terminal coverage for simple published guards; it
// intentionally does not guess the type or value of an arbitrary expression.
function hasGuaranteedThrows(document) {
  const conditions = (document.throws ?? []).map(item => item.when?.trim() ?? 'true');
  if (conditions.includes('true')) return true;

  const parameterTypes = document.P?.parameters ?? {};
  function totalType(name) {
    if (name.startsWith('P.')) return parameterTypes[name.slice(2)]?.type;
    if (name.startsWith('E.')) return document.E?.[name.slice(2)]?.type;
    const p = parameterTypes[name];
    const e = document.E?.[name];
    return p && e ? undefined : p?.type ?? e?.type;
  }
  const truth = new Set();
  const comparisons = new Map();
  for (const condition of conditions) {
    const boolean = /^(?:(NOT)\s*\(\s*)?((?:[PE]\.)?[\w$]+(?:\.[\w$]+)*)(?:\s*\))?$/.exec(condition);
    if (boolean && totalType(boolean[2]) === 'bool'
        && Boolean(boolean[1]) === condition.endsWith(')')) {
      const key = boolean[2];
      const polarity = Boolean(boolean[1]);
      if (truth.has(key + ':' + !polarity)) return true;
      truth.add(key + ':' + polarity);
      continue;
    }

    // Relative to a single constant boundary, a non-null business number
    // lies in exactly one of less/equal/greater. Do not use this for native
    // machine floats, nullables, or receipt fields with a protected domain.
    const subject = String.raw`(?:(LENGTH)\(((?:[PE]\.)?[\w$]+)\)|((?:[PE]\.)?[\w$]+(?:\.[\w$]+)*))`;
    const literalToken = String.raw`(-?(?:0|[1-9]\d*)(?:\.\d+)?)`;
    const opToken = '(<=|>=|==|!=|<|>)';
    const toRight = new RegExp(`^${subject}\\s*${opToken}\\s*${literalToken}$`).exec(condition);
    const toLeft = toRight ? null
      : new RegExp(`^${literalToken}\\s*${opToken}\\s*${subject}$`).exec(condition);
    if (!toRight && !toLeft) continue;
    const reverse = {'<': '>', '>': '<', '<=': '>=', '>=': '<=', '==': '==', '!=': '!='};
    const [length, lengthRoot, numberRoot, operator, boundary] = toRight
      ? toRight.slice(1) : [toLeft[3], toLeft[4], toLeft[5], reverse[toLeft[2]], toLeft[1]];
    const type = totalType(lengthRoot ?? numberRoot);
    // LENGTH is total only for a known non-null array root. A nullable
    // element type (number?[]) does not make the array itself nullable.
    if (length ? !type?.endsWith('[]') : type !== 'number') continue;
    const literal = boundary.replace(/(\.\d*?)0+$/, '$1').replace(/\.$/, '').replace(/^-0$/, '0');
    const key = (length ? 'LENGTH(' + lengthRoot + ')' : numberRoot) + ':' + literal;
    const domains = { '<': 1, '==': 2, '>': 4, '<=': 3, '>=': 6, '!=': 5 };
    // Relative to this boundary, a natural length can only be greater
    // than a negative, never less than zero, nor equal to a fraction.
    const feasible = !length ? 7 : literal.startsWith('-') ? 4
      : literal === '0' ? 6 : literal.includes('.') ? 5 : 7;
    const covered = (comparisons.get(key) ?? 0) | domains[operator];
    if ((covered & feasible) === feasible) return true;
    comparisons.set(key, covered);
  }
  return false;
}

export function assertPers(document, label) {
  if (!validate(document)) assert.fail(label + ': ' + ajv.errorsText(validate.errors));
  const hasResult = Object.hasOwn(document, 'R');
  const definitelyThrows = hasGuaranteedThrows(document);
  assert.ok(hasResult || definitelyThrows, label + ': absent R needs a proven full-domain throw');
  assert.ok(!hasResult || !definitelyThrows, label + ': a guaranteed throw cannot also publish R');
  const types = document.types ?? {};
  const typeRef = (type, location) => {
    const base = type.replace(/\[\]|\?/g, '');
    assert.ok(scalar.has(base) || Object.hasOwn(types, base), label + ': undeclared type ' + type + ' at ' + location);
  };
  const fields = (object, location) => {
    for (const [name, type] of Object.entries(object)) {
      const field = location + '.' + name;
      if (typeof type === 'string') typeRef(type, field);
      else fields(type, field);
    }
  };
  // SE-03: a wide value can leave the entry unchanged. Allow the exact
  // declared wide return type, without admitting arbitrary undeclared types.
  if (document.entry.returns !== 'unknown') typeRef(document.entry.returns, 'entry.returns');
  for (const [name, declaration] of Object.entries(types)) fields(declaration.fields, 'types.' + name);
  for (const [name, parameter] of Object.entries(document.P.parameters)) {
    if (parameter.type !== 'unknown') typeRef(parameter.type, 'P.' + name);
  }
  for (const [name, source] of Object.entries(document.E ?? {})) {
    if (source.type) typeRef(source.type, 'E.' + name);
    else if (source.kind !== 'acquisition') fields(source.fields ?? source.columns, 'E.' + name);
  }
  const receipts = new Map();
  const acquisitions = Object.entries(document.E ?? {}).filter(([, source]) => source.kind === 'acquisition')
    .sort(([left], [right]) => Number(left.slice(3)) - Number(right.slice(3)));
  acquisitions.forEach(([name], ordinal) => assert.equal(name, '$a_' + ordinal, label + ': noncontiguous acquisition identities'));
  const acquisitionByName = new Map(acquisitions);
  const checkReferences = (expression, location, availableAcquisitions = acquisitionByName,
      earlierThanS = null, latestClockPrefix = null) => {
    if (typeof expression !== 'string') return;
    // PERS double-quoted text uses escapes; SQL text uses doubled single quotes.
    const tokens = /"(?:[^"\\]|\\[\s\S])*"|'(?:[^']|'')*'|\$[ea]_\d+\b/g;
    for (const [token] of expression.matchAll(tokens)) {
      if (token.startsWith('$e_')) {
        assert.ok(receipts.has(token), label + ': unbound receipt ' + token + ' at ' + location);
        if (latestClockPrefix !== null) {
          assert.ok(receipts.get(token) < latestClockPrefix, label + ': clock guard refers to a later receipt ' + token);
        }
      }
      if (token.startsWith('$a_')) {
        assert.ok(availableAcquisitions.has(token), label + ': unbound acquisition ' + token + ' at ' + location);
        if (earlierThanS !== null && acquisitionByName.get(token).afterS !== undefined) {
          assert.ok(acquisitionByName.get(token).afterS <= earlierThanS, label + ': S references a later clock ' + token);
        }
      }
    }
  };
  for (const [i, operation] of (document.S ?? []).entries()) {
    const location = 'S[' + i + ']';
    const { call, write, sql } = operation;
    const binding = call ?? (sql ? operation : null);
    if (call) typeRef(call.returns, location + '.call.returns');
    const checkSReferences = (expression, suffix) =>
      checkReferences(expression, location + suffix, acquisitionByName, i);
    checkSReferences((call ?? write ?? operation).when, '.when');
    call?.args.forEach((arg, j) => checkSReferences(arg, '.args[' + j + ']'));
    if (write) checkSReferences(write.value, '.write.value');
    checkSReferences(sql, '.sql');
    if (binding) {
      assert.ok(!receipts.has(binding.result), label + ': duplicate receipt ' + binding.result + ' at ' + location);
      receipts.set(binding.result, i);
    }
  }
  const previousAcquisitions = new Set();
  let previousClockPrefix = 0;
  for (const [name, source] of acquisitions) {
    if (source.operation) {
      assert.equal(Object.hasOwn(source, 'afterS'), Object.hasOwn(document, 'S'),
        label + ': clock position must identify an existing S prefix');
      if (source.afterS !== undefined) {
        assert.ok(source.afterS >= previousClockPrefix && source.afterS <= document.S.length,
          label + ': invalid clock position in S');
        previousClockPrefix = source.afterS;
      }
    }
    checkReferences(source.when, 'E.' + name + '.when', previousAcquisitions, null, source.afterS ?? null);
    if (source.source) assert.equal(document.E[source.source]?.kind, 'table', label + ': wrong acquired table ' + source.source);
    for (const dependency of source.after ?? []) {
      checkReferences(dependency.result, 'E.' + name + '.after.result', previousAcquisitions);
      checkReferences(dependency.when, 'E.' + name + '.after.when', previousAcquisitions);
    }
    previousAcquisitions.add(name);
  }
  for (const [i, choice] of (document.T ?? []).entries()) {
    for (const field of ['when', 'members', 'empty']) checkReferences(choice[field], 'T[' + i + '].' + field);
  }
  checkReferences(document.R, 'R');
  for (const [i, declaration] of (document.throws ?? []).entries()) {
    checkReferences(declaration.when, 'throws[' + i + '].when');
    checkReferences(declaration.payload, 'throws[' + i + '].payload');
  }
}
