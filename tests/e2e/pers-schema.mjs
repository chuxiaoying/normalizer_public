import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import Ajv2020 from '../../implementation/tools/frontend/node_modules/ajv/dist/2020.js';
import YAML from '../../implementation/tools/frontend/node_modules/yaml/dist/index.js';

const schemaFile = new URL('../../implementation/contracts/pers/schema.yaml', import.meta.url);
const ajv = new Ajv2020({ allErrors: true, keywords: ['x-enums'] });
const validate = ajv.compile(YAML.parse(readFileSync(schemaFile, 'utf8')));

const scalar = new Set(['number', 'money', 'date', 'bool', 'text', 'void']);

export function assertPers(document, label) {
  assert.ok(validate(document), () => label + ': ' + ajv.errorsText(validate.errors));
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
  for (const [i, operation] of (document.S ?? []).entries()) {
    if (operation.call) typeRef(operation.call.returns, 'S[' + i + '].call.returns');
  }
}
