import * as N from "@normalizer/stdlib";

/** @N entryState */
const draft = { units: 0, ready: false };

export function addOne(): void {
    draft.units += 1;
    draft.ready = true;
}

/** @N componentProp OrderCard#summary.units */
export function cardUnits(): number {
    return draft.ready ? draft.units : 0;
}
