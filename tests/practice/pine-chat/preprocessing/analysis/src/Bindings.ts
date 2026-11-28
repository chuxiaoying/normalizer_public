// One selected App.vue instance. This is the host-to-application binding, not a raw DOM event.
/** @N entryState */
const query = { value: '' }
/** @N entryState */
const draft = { value: '' }
/** @N entryState */
const problem = { value: '' }
/** @N entryState */
const notice = { value: '' }
/** @N entryState */
const busy = { value: false }
/** @N entryState */
const loading = { value: true }

// App.vue: the global search input's v-model, after Vue has delivered the string.
export function onSearchValueChanged(value: string): void {
  query.value = value
}

/** @N componentProp Input#global-search.value */
export function searchValue(): string {
  return query.value
}

// App.vue: the composer textarea's v-model, after Vue has delivered the string.
export function onDraftValueChanged(value: string): void {
  draft.value = value
}

/** @N componentProp Textarea#message-composer.value */
export function draftValue(): string {
  return draft.value
}

// App.vue: @click on the banner's “关闭提示” button, only dispatched while mounted.
export function onCloseNotice(): void {
  problem.value = ''
  notice.value = ''
}

/** @N componentProp Select#demo-account.disabled */
export function accountDisabled(): boolean {
  return busy.value || loading.value
}

/** @N componentProp Div#notice-banner.role */
export function noticeRole(): string {
  return problem.value ? 'alert' : 'status'
}

/** @N componentProp H2#welcome.textContent */
export function welcomeTitle(): string {
  return loading.value ? '正在连接' : '选一个会话，聊点什么'
}
