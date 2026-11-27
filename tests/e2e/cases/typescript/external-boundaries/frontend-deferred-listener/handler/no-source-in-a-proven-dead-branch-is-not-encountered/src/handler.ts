/** @N external */
declare const audit: { registered(): void }
declare const notices: { notify(): void }

export function handle(deliver: boolean): void {
    let pending: (() => string) | undefined
    function listen(callback: () => string) {
        pending = callback
        audit.registered()
    }
    listen(() => { notices.notify(); return "callback-only" })
    if (false) pending!()
}
