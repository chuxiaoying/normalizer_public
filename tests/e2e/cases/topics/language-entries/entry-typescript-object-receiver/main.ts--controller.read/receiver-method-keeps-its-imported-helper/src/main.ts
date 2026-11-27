import { plusOne } from "./nested/helper.js";
export const controller = {
  base: 7,
  read(value: number): number { return plusOne(this.base + value); }
} as const;
