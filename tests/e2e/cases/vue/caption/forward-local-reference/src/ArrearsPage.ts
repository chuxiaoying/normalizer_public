/** @N entryState */
const arrears = {
  status: { ready: false, loading: false },
  summary: { payable: 0 }
};

/** @N componentProp PayableCard#balance.amount */
export function amount(): number {
  return arrears.status.ready ? arrears.summary.payable : 0;
}

/** @N componentProp PayableCard#balance.caption */
export function caption(): string {
  const early = later
  const later = "查询中"
  return arrears.status.loading ? early : "应付金额";
}
