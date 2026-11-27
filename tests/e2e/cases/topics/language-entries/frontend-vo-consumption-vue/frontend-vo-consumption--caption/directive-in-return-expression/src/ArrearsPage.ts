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
  return arrears.status.loading ? /* @normalizer-ir extra = unknown */ "加载中" : "应付金额";
}
