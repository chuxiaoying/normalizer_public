export const controller = {
  base: 7,
  read(value: number): number { return this.base + value; }
} as const;
