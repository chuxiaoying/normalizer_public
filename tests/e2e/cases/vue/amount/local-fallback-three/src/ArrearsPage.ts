/** @N entryState */
const arrears = {
  status: { ready: false, loading: false },
  summary: { payable: 0 }
};

/** @N componentProp PayableCard#balance.amount */
export function amount(): number {
  const fallback = 3
  return arrears.status.ready ? arrears.summary.payable : fallback;
}

/** @N componentProp PayableCard#balance.caption */
export function caption(): string {
  return arrears.status.loading ? "加载中" : "应付金额";
}
