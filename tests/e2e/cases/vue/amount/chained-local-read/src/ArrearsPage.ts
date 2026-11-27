/** @N entryState */
const arrears = {
  status: { ready: false, loading: false },
  summary: { payable: 0 }
};

/** @N componentProp PayableCard#balance.amount */
export function amount(): number {
  const ready = arrears.status.ready
  const value = arrears.summary.payable
  const visible = ready ? value : 0
  return visible;
}

/** @N componentProp PayableCard#balance.caption */
export function caption(): string {
  return arrears.status.loading ? "加载中" : "应付金额";
}
