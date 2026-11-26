// Compile a synchronous component projection to the shared symbolic kernel.
// Each binding denotes a read of a plain field during this render observation.
import { accessSync, constants, readFileSync, writeFileSync } from 'node:fs';
import { extname, join } from 'node:path';
import { readModuleEvent } from '../frontend/module-event.mjs';
import { readModuleProjection } from '../frontend/module-projection.mjs';
import { projectSources } from '../frontend/project-source.mjs';
import { vueScenario } from './registry.mjs';

const carriers = {
  bool: ['boolean', 'bool'], text: ['String', 'text?'],
  number: ['int', 'number'], void: ['void', 'unit'],
};

function compileProjection(projection) {
  const reads = new Map();
  const bindings = new Map();
  const environment = Object.create(null);

  function read(target) {
    const [binding, ...fields] = target;
    const alias = bindings.get(binding);
    const statePath = [...(alias?.statePath ?? [binding]), ...fields];
    const [root, ...path] = statePath;
    const key = JSON.stringify(statePath);
    const label = path.reduce((prefix, part) =>
      /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(part) ? prefix + '.' + part : prefix + '[' + JSON.stringify(part) + ']', root);
    const state = projection.states.get(root);
    let type = state.fields;
    for (const name of path) {
      if (!type || typeof type !== 'object' || !Object.hasOwn(type, name)) throw new Error(`SCENARIO_MODULE_INPUT: missing type for ${label}`);
      type = type[name];
    }
    if (typeof type === 'object') return { statePath };
    if (!reads.has(key)) {
      reads.set(key, { name: `state_${reads.size}`, path: ['E', root, ...path], type });
      environment[root] ??= { kind: 'state', source: state.source, fields: Object.create(null) };
      let fields = environment[root].fields;
      for (const name of path.slice(0, -1)) fields = fields[name] ??= Object.create(null);
      fields[path.at(-1)] = type;
    }
    return { type, body: { op: 'var', name: reads.get(key).name } };
  }

  function compile(expr) {
    if (expr.op === 'read') return read(expr.target);
    if (expr.op === 'var') {
      const binding = bindings.get(expr.name);
      if (binding?.statePath || projection.states.has(expr.name)) return read([expr.name]);
      if (!binding) throw new Error(`SCENARIO_MODULE_INPUT: unbound projection local ${expr.name}`);
      return { type: binding.type, body: { op: 'var', name: binding.name } };
    }
    if (expr.op === 'literal') {
      const type = { number: 'number', boolean: 'bool', string: 'text' }[expr.type];
      if (type && (type !== 'number' || Number.isSafeInteger(expr.value))) {
        return { type, body: { op: 'const', type: carriers[type][1], value: expr.value } };
      }
    }
    if (expr.op === 'select') {
      const test = compile(expr.condition), yes = compile(expr.whenTrue), no = compile(expr.whenFalse);
      if (test.type === 'bool' && yes.type && yes.type === no.type) {
        return { type: yes.type, body: { op: 'if', cond: test.body, then: yes.body, else: no.body } };
      }
    }
  }

  const steps = [];
  for (const local of projection.locals) {
    const value = compile(local.value);
    if (value.statePath) {
      // The projection contract admits plain immutable records for the duration
      // of this observation. An alias retains the original field's E source.
      bindings.set(local.name, value);
      continue;
    }
    const { type, body } = value;
    const name = `projection_${bindings.size}`;
    steps.push({ op: 'let', name, value: body });
    bindings.set(local.name, { type, name });
  }
  const returned = compile(projection.expression);
  const body = steps.length
    ? { op: 'action', returns: carriers[returned.type][1], steps: [...steps, { op: 'return', value: returned.body }] }
    : returned.body;
  return { type: returned.type, body, reads: reads.values(), environment };
}

function projectionContract(modulePath, moduleText, name) {
  const projection = readModuleProjection(modulePath, moduleText, name);
  const { type, body, reads, environment } = compileProjection(projection);
  const params = [], javaParams = [], declarations = [], bindings = {};
  for (const { name, path, type } of reads) {
    const [carrier, domain] = carriers[type];
    params.push({ name, type: domain });
    javaParams.push(carrier);
    declarations.push(`${carrier} ${name}`);
    bindings[name] = path;
  }
  const returns = carriers[type][1];
  const javaReturn = carriers[type][0];
  const source = `class T { static native ${javaReturn} run(${declarations.join(', ')}); }\n`;
  const entry = 'T.run';
  const project = {
    schema: 'normalizer.kernel-input/1',
    source: 'Normalized.java', source_text: source,
    entries: [entry],
    entry_contexts: { [entry]: {
      function: projection.function, returns: type, parameters: {}, environment,
      bindings,
    } },
    functions: { run: {
      java: `T.run(${javaParams.join(',')})`, params, returns,
      origin: { path: 'Prepared.ts', excerpt: `function ${name}` }, body,
    } },
  };
  return project;
}

function eventContract(modulePath, moduleText, name, physicalPath) {
  const event = readModuleEvent(modulePath, moduleText, name, physicalPath);
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

export function buildModuleProject({ scene, root, file, name, original, directory }) {
  const build = {event: eventContract, projection: projectionContract}[vueScenario(scene)];
  if (!build || extname(original) !== '.vue' || extname(file) !== '.ts') {
    throw new Error('SCENARIO_MODULE_INPUT: expected a supported scenario with its Vue source and prepared .ts module');
  }
  const source = projectSources(root);
  const component = source(original);
  const module = source(file);
  accessSync(component.path, constants.R_OK);
  const moduleText = readFileSync(module.path, 'utf8');
  const project = build(module.name, moduleText, name, module.declared);
  const [entry] = project.entries;
  writeFileSync(join(directory, 'Normalized.java'), project.source_text);
  writeFileSync(join(directory, 'Prepared.ts'), moduleText);
  writeFileSync(join(directory, 'Original.vue'), readFileSync(component.path));
  const projectPath = join(directory, 'project.json');
  writeFileSync(projectPath, JSON.stringify(project, null, 2) + '\n');
  return { projectPath, entry };
}
