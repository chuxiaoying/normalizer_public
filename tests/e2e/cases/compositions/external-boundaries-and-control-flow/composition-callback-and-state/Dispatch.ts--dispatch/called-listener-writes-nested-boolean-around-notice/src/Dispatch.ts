/** @N external */
declare const audit: {registered(): void};

/** @N external */
declare const notices: {sent(): void};

/** @N entryState */
const ledger = {status: {ready: false}};

export function dispatch(deliver: boolean): void {
  let pending: (() => string) | undefined;
  function listen(callback: () => string) {
    pending = callback;
    audit.registered();
  }
  listen(() => {
    ledger.status.ready = true;
    notices.sent();
    ledger.status.ready = false;
    return "callback-only";
  });
  if (deliver) pending!();
}
