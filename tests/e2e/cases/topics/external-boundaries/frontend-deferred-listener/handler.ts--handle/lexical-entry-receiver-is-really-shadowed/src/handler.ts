/** @N external */
declare const audit: { registered(): void }
/** @N external */
declare const notices: { notify(): void }

export function handle(notices: boolean): void {
    let pending: (() => string) | undefined
    function listen(callback: () => string) {
        pending = callback
        audit.registered()
    }
    listen(() => { return notices ? "deliver" : "stop" })
    if (notices) pending!()
}
