export type Domain = number;

function increment(value: Domain): Domain { return value + 1; }
export { increment as default };
export function step(value: Domain): Domain { return value + 10; }
