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
  const busy = arrears.status.loading
  const loading = "加载中", title = "应付金额"
  return busy ? loading : title;
}
