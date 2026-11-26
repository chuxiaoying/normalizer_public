// Runtime publication proves exhaustiveness using its typed terminal
// alternatives, before serialization. The YAML-level checker can also
// independently certify terminal coverage for simple published guards; it
// intentionally does not guess the type or value of an arbitrary expression.
export function hasGuaranteedThrows(document) {
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
    const comparison = /^((?:[PE]\.)?[\w$]+(?:\.[\w$]+)*)\s*(<=|>=|==|!=|<|>)\s*(-?(?:0|[1-9]\d*)(?:\.\d+)?)$/.exec(condition);
    if (comparison && totalType(comparison[1]) === 'number') {
      const literal = comparison[3].replace(/(\.\d*?)0+$/, '$1').replace(/\.$/, '');
      const key = comparison[1] + ':' + (literal === '-0' ? '0' : literal);
      const domains = { '<': 1, '==': 2, '>': 4, '<=': 3, '>=': 6, '!=': 5 };
      const covered = (comparisons.get(key) ?? 0) | domains[comparison[2]];
      if (covered === 7) return true;
      comparisons.set(key, covered);
    }
  }
  return false;
}
