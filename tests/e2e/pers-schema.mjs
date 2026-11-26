import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import Ajv2020 from '../../implementation/tools/frontend/node_modules/ajv/dist/2020.js';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';
import { hasGuaranteedThrows } from './pers-completion.mjs';

const schemaFile = new URL('../../implementation/contracts/pers/schema.yaml', import.meta.url);
const ajv = new Ajv2020({ allErrors: true, keywords: ['x-enums'] });
const validate = ajv.compile(YAML.parse(readFileSync(schemaFile, 'utf8')));

const scalar = new Set(['number', 'money', 'date', 'bool', 'text', 'void', 'map<text,text>']);

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
  typeRef(document.entry.returns, 'entry.returns');
  for (const [name, declaration] of Object.entries(types)) fields(declaration.fields, 'types.' + name);
  for (const [name, parameter] of Object.entries(document.P.parameters)) typeRef(parameter.type, 'P.' + name);
  for (const [name, source] of Object.entries(document.E ?? {})) {
    if (source.type) typeRef(source.type, 'E.' + name);
    else fields(source.fields ?? source.columns, 'E.' + name);
  }
  const receipts = new Set();
  const checkReferences = (expression, location) => {
    if (typeof expression !== 'string') return;
    // PERS double-quoted text uses escapes; SQL text uses doubled single quotes.
    const tokens = /"(?:[^"\\]|\\[\s\S])*"|'(?:[^']|'')*'|\$e_\d+\b/g;
    for (const [token] of expression.matchAll(tokens)) {
      if (token.startsWith('$')) {
        assert.ok(receipts.has(token), label + ': unbound receipt ' + token + ' at ' + location);
      }
    }
  };
  for (const [i, operation] of (document.S ?? []).entries()) {
    const location = 'S[' + i + ']';
    const { call, write, sql } = operation;
    const binding = call ?? (sql ? operation : null);
    if (call) typeRef(call.returns, location + '.call.returns');
    checkReferences((call ?? write ?? operation).when, location + '.when');
    call?.args.forEach((arg, j) => checkReferences(arg, location + '.args[' + j + ']'));
    if (write) checkReferences(write.value, location + '.write.value');
    checkReferences(sql, location + '.sql');
    if (binding) {
      assert.ok(!receipts.has(binding.result), label + ': duplicate receipt ' + binding.result + ' at ' + location);
      receipts.add(binding.result);
    }
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
