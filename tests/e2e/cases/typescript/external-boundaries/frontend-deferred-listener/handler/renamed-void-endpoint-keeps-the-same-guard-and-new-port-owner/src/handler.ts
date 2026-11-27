/** @N external */
declare const audit: { registered(): void }
/** @N external */
declare const notices: { inform(): void }

export function handle(deliver: boolean): void {
    let pending: (() => string) | undefined
    function listen(callback: () => string) {
        pending = callback
        audit.registered()
    }
    listen(() => { notices.inform(); return "callback-only" })
    if (deliver) pending!()
}
