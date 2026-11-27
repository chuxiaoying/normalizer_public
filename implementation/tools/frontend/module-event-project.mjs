// Keep a single source-linked event carrier, shared by scenario and module CLI paths.
import { readModuleEvent } from './module-event.mjs';

const carriers = {
  bool: ['boolean', 'bool'], text: ['String', 'text?'],
  number: ['int', 'number'], void: ['void', 'unit'],
};

export function eventContract(modulePath, moduleText, name, physicalPath, options) {
  const event = readModuleEvent(modulePath, moduleText, name, physicalPath, options);
  const entry = 'T.run';
  const source = 'class T { static native void run(); }\n';
  function kernelType(domain, seen = new Set()) {
    if (domain.endsWith('[]')) return `vector<${kernelType(domain.slice(0, -2), seen)}>`;
    if (Object.hasOwn(carriers, domain)) return carriers[domain][1];
    const record = event.types[domain];
    if (!record || seen.has(domain)) throw new Error(`SCENARIO_MODULE_INPUT: unknown or recursive entry type ${domain}`);
    const next = new Set(seen).add(domain);
    const fields = Object.entries(record.fields).map(([key, type]) => key + ':' + kernelType(type, next));
    return `record<${fields.join(',')}>`;
  }
  const ports = {};
  const exceptions = {}, externalResults = {};
  for (const port of event.ports) {
    ports[port.id] = {
      endpoint: port.endpoint, effect: 'async-barrier', heap_effect: 'none', argument_observation: 'immutable_value',
      params: port.params.map(({name, type}) => ({name, type: kernelType(type)})),
      returns: kernelType(port.returns), throws: port.throws,
      origin: {path: 'Prepared.ts', excerpt: port.excerpt},
    };
    externalResults[port.id] = port.returns;
    for (const type of port.throws) exceptions[type] = 'unit';
  }
  return {
    schema: 'normalizer.kernel-input/1', source: 'Normalized.java',
    source_text: source, entries: [entry], exceptions, ports,
    entry_contexts: {[entry]: {
      function: event.function, returns: 'void', parameters: event.parameters,
      types: event.types, environment: event.environment,
      external_results: externalResults,
    }},
    functions: {run: {java: 'T.run()', params: [], returns: 'unit',
      origin: {path: 'Prepared.ts', excerpt: `function ${name}`},
      body: {op: 'settled', returns: 'unit', steps: event.steps}}},
  };
}
