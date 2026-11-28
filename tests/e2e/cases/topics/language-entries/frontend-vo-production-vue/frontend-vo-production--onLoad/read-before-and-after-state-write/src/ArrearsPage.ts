/** @N entryState */
const arrears = { total: 0, previous: 0, after: 0 };

export async function onLoad(delta: number): Promise<void> {
  const before = arrears.total;
  arrears.total = before + delta;
  arrears.previous = before;
  arrears.after = arrears.total;
}
