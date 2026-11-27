/** @N entryState */
const counter = {count: 0};

export function onClick(reset: boolean, value: number): void {
  counter.extra = reset ? 0 : value;
}
