/** @N entryState */
const arrears = { caption: "", previous: "" };

export async function onLoad(next: string): Promise<void> {
  const before = arrears.caption;
  arrears.caption = next;
  arrears.previous = before;
}
