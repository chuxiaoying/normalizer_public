type Labels = ReadonlyMap<string, string>;
export function labels(): Labels {
  return new Map<string, string>([["k", "v"]]);
}
