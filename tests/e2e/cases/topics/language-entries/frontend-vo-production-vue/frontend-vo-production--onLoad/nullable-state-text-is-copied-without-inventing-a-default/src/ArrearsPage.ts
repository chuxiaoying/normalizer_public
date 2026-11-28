/** @N entryState */
const arrears: { caption: string | null; previous: string | null } = {
  caption: null,
  previous: null,
};

export async function onLoad(): Promise<void> {
  arrears.previous = arrears.caption;
}
