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
    const comparison = /^(?:(LENGTH)\(((?:[PE]\.)?[\w$]+)\)|((?:[PE]\.)?[\w$]+(?:\.[\w$]+)*))\s*(<=|>=|==|!=|<|>)\s*(-?(?:0|[1-9]\d*)(?:\.\d+)?)$/.exec(condition);
    if (!comparison) continue;
    const [, length, lengthRoot, numberRoot, operator, boundary] = comparison;
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
