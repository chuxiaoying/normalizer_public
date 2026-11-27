/** @N external */
declare const audit: {registered(): void};

/** @N external */
declare const notices: {sent(): void};

/** @N entryState */
const ledger = {stage: 0};

export function dispatch(deliver: boolean): void {
  let pending: (() => string) | undefined;
  function listen(callback: () => string) {
    pending = callback;
    audit.registered();
  }
  listen(() => {
    ledger.stage = 1;
    notices.sent();
    ledger.stage = 2;
    return "callback-only";
  });
  if (false) pending!();
}
