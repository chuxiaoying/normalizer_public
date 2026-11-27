/** @N entryState */
const arrears = {
  status: { ready: false, loading: false },
  summary: { payable: 0 }
};

/** @N componentProp PayableCard#balance.amount */
export function amount(): number {
  const summary = arrears["summary"]
return arrears.status.ready ? summary["payable"] : 0
;
}

/** @N componentProp PayableCard#balance.caption */
export function caption(): string {
  return arrears.status.loading ? "加载中" : "应付金额";
}
