export const controller = {
  base: 7,
  read(value: number): number { return this.base + value; }
} as const;
export const other = {
  base: 13,
  read(value: number): number { return this.base + value; }
} as const;
