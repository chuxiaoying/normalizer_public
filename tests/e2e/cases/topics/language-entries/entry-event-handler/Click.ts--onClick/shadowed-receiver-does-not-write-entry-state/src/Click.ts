/** @N entryState */
const counter = {count: 0};

export function onClick(reset: boolean, value: number): void {
  const counter = {count: 9};
  counter.count = reset ? 0 : value;
}
