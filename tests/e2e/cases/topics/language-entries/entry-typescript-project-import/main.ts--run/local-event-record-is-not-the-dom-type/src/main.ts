interface KeyboardEvent { key: string }
export function run(event: KeyboardEvent): boolean { return event.key === "Enter"; }
