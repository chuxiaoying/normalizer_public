function compute(): number { return 7; }
export const controller = {
  base: compute(),
  read(value: number): number { return this.base + value; }
} as const;
