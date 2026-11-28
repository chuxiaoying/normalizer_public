/** @N entryState */
const arrears = { ready: false, total: 0 };

export async function onLoad(delta: number): Promise<void> {
  arrears.total = arrears.ready ? delta : 0;
}
