export function labels(): ReadonlyMap<string, string> {
  return new Map<string, string>([['\uD800', 'old'], ['a', 'only'], ['\uD800', '\uDFFF']]);
}
