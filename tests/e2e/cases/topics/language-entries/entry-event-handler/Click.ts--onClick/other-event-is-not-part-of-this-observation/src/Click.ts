/** @N entryState */
const counter = {count: 0};

export function onClick(reset: boolean, value: number): void {
  counter.count = reset ? 0 : value;
}

/** @N entryState */
const hover = {count: 0};

export function onHover(): void { hover.count = 99; }
