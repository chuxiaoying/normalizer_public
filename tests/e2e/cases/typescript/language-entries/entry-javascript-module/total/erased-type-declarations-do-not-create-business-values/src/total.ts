type AccountAmount = number;
interface AuditShape { amount: AccountAmount }
export function total(value: number, base: number): number { return value + base; }
