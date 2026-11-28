// Preprocessed candidate for the workspace's original App.vue instance.
// The browser-backed paths below deliberately preserve source constructs that
// the current scenario adapter does not yet admit. See the handoff README.
interface FetchResponse {
  readonly ok: boolean
  json(): Promise<unknown>
}
interface BrowserHttp {
  /** @N rejects ExternalFailure */
  get(url: string): Promise<FetchResponse>
  /** @N rejects ExternalFailure */
  post(url: string, method: 'POST', body: URLSearchParams): Promise<FetchResponse>
}
/** @N external */
declare const http: BrowserHttp

type Action = 'request' | 'accept' | 'decline' | 'block' | 'unblock' | 'open' | 'send' |
  'recall' | 'read' | 'pin' | 'mute' | 'mode' | 'promote' | 'demote' | 'invite' | 'remove' | 'leave'
interface Person { id: string; name: string }
interface Contact extends Person { status: 'BLOCKED' | 'FRIEND' | 'INCOMING' | 'OUTGOING' | 'STRANGER'; canInvite: boolean }
interface Member extends Person { role: 'OWNER' | 'ADMIN' | 'MEMBER' }
interface Message {
  id: number; from: string; name: string; text: string; at: string; recalled: boolean; mention: string; canRecall: boolean
}
interface Room {
  id: string; title: string; group: boolean; peer: string; owner: string; admin: boolean;
  mode: 'OPEN' | 'ADMINS_ONLY'; members: Member[]; messages: Message[];
  unread: number; mentions: number; pinned: boolean; muted: boolean; canSend: boolean; sendReason: string
}
interface Snapshot { people: Person[]; rooms: Room[]; contacts: Contact[] }
const contactLabels: Record<Contact['status'], string> = {
  BLOCKED: '已拉黑', FRIEND: '好友', INCOMING: '好友申请待处理', OUTGOING: '等待对方处理', STRANGER: '尚未添加'
}
function displayMessage(message: Message, account: string) {
  if (message.recalled) return message.from === account ? '你撤回了一条消息' : message.name + '撤回了一条消息'
  return message.text
}
function unreadLabel(room: Room) {
  if (room.mentions) return '@ ' + room.mentions
  if (!room.unread) return ''
  if (room.muted) return '·'
  return room.unread > 99 ? '99+' : String(room.unread)
}
function roomList(rooms: Room[], filter: 'all' | 'unread' | 'mentions', query: string) {
  const needle = query.trim().toLocaleLowerCase('zh-CN')
  return rooms.filter(room => {
    if (filter === 'unread' && !room.unread) return false
    if (filter === 'mentions' && !room.mentions) return false
    if (!needle || room.title.toLocaleLowerCase('zh-CN').includes(needle)) return true
    return room.messages.some(message => !message.recalled && message.text.toLocaleLowerCase('zh-CN').includes(needle))
  }).sort((a, b) => Number(b.pinned) - Number(a.pinned) ||
    (b.messages.at(-1)?.id ?? -1) - (a.messages.at(-1)?.id ?? -1) || a.id.localeCompare(b.id))
}

const empty: Snapshot = { people: [], rooms: [], contacts: [] }
/** @N entryState */
const account = { value: 'lin' }
/** @N entryState */
const data: { value: Snapshot } = { value: empty }
/** @N entryState */
const page: { value: 'chats' | 'contacts' } = { value: 'chats' }
/** @N entryState */
const activeId = { value: '' }
/** @N entryState */
const filter: { value: 'all' | 'unread' | 'mentions' } = { value: 'all' }
/** @N entryState */
const query = { value: '' }
/** @N entryState */
const draft = { value: '' }
/** @N entryState */
const mention = { value: '' }
/** @N entryState */
const panel = { value: false }
/** @N entryState */
const invitee = { value: '' }
/** @N entryState */
const notice = { value: '' }
/** @N entryState */
const problem = { value: '' }
/** @N entryState */
const busy = { value: false }
/** @N entryState */
const loading = { value: true }

/** @N entryState */
let revision = 0
/** @N entryState */
let fetching = false
/** @N entryState */
let disposed = false
/** @N entryState */
let timer: number | undefined

// The template ref points at the original thread element, not a new element.
// This browser binding is outside the currently runnable input set.
declare const thread: { value: HTMLElement | undefined }
const room = { get value() { return data.value.rooms.find(r => r.id === activeId.value) } }
const self = { get value() { return data.value.people.find(p => p.id === account.value) } }
const rooms = { get value() { return roomList(data.value.rooms, filter.value, query.value) } }
const contacts = { get value() { return data.value.contacts.filter(c => c.name.includes(query.value.trim())) } }
const waiting = { get value() { return data.value.contacts.filter(c => c.status === 'INCOMING').length } }
const unread = { get value() { return data.value.rooms.reduce((n, r) => n + (r.muted ? 0 : r.unread), 0) } }
const draftLength = { get value() { return [...draft.value.trim()].length } }
const inviteOptions = { get value() { return data.value.contacts.filter(c => c.status === 'FRIEND' &&
  !room.value?.members.some(member => member.id === c.id)) } }
const canSend = { get value() { return !!room.value?.canSend && draftLength.value > 0 && draftLength.value <= 300 && !busy.value } }
const composerHint = { get value() { return !room.value?.canSend ?
  room.value?.sendReason === 'ADMINS_ONLY' ? '当前仅管理员可以发言' : '当前不能向对方发送消息' :
  draftLength.value > 300 ? '消息不能超过 300 个字符' : 'Enter 发送 · Shift+Enter 换行' } }
const actionLabels: Partial<Record<Action,string>> = {
  request:'已发送好友申请', accept:'已添加好友', decline:'已拒绝申请', block:'已拉黑',
  unblock:'已解除拉黑', recall:'消息已撤回', invite:'已邀请成员', remove:'已移出成员',
  leave:'已退出群聊', promote:'已设为管理员', demote:'已撤销管理员',
}
async function reply<T>(response: FetchResponse): Promise<T> {
  const raw: unknown = await response.json()
  if (!response.ok) {
    const message = typeof raw === 'object' && raw !== null && 'message' in raw ? raw.message : undefined
    throw new Error(typeof message === 'string' ? message : '请求未完成')
  }
  return raw as T
}
async function refresh(quiet = false) {
  if ((fetching || busy.value) && quiet) return
  const stamp = ++revision
  fetching = true
  try {
    const next = await reply<Snapshot>(await http.get('/api/view?' + new URLSearchParams({user: account.value})))
    if (stamp !== revision || disposed) return
    data.value = next
    if (room.value?.unread && page.value === 'chats' && document.visibilityState === 'visible') {
      await command('read', {room: room.value.id}, true)
    } else if (activeId.value && !room.value) {
      activeId.value = ''
      panel.value = false
    }
  } catch (err) {
    if (!quiet && stamp === revision) problem.value = errorMessage(err)
  } finally {
    if (stamp === revision) loading.value = false
    fetching = false
  }
}
function errorMessage(err: unknown) { return err instanceof Error ? err.message : '请求未完成' }

async function command(action: Action, args: Record<string,string> = {}, quiet = false) {
  if (busy.value) return false
  const stamp = ++revision
  busy.value = true
  if (!quiet) { problem.value = ''; notice.value = '' }
  try {
    const response = await http.post('/api/command', 'POST', new URLSearchParams({...args, user: account.value, action}))
    const next = await reply<{room:string, view:Snapshot}>(response)
    if (revision !== stamp || disposed) return false
    data.value = next.view
    if (actionLabels[action]) notice.value = actionLabels[action]!
    if (action === 'open' || action === 'accept') {
      page.value = 'chats'
      filter.value = 'all'
      query.value = ''
      await selectRoom(next.room, false)
    } else if (action === 'leave') {
      activeId.value = ''
      panel.value = false
    }
    return true
  } catch (err) {
    if (revision === stamp && !quiet) problem.value = errorMessage(err)
    return false
  } finally {
    busy.value = false
    loading.value = false
  }
}
async function selectRoom(id: string, mark = true) {
  if (busy.value && mark) return
  if (activeId.value !== id) {
    draft.value = ''; mention.value = ''; panel.value = false; invitee.value = ''
  }
  activeId.value = id
  if (mark && room.value?.unread) await command('read', {room:id}, true)
}
function switchAccount() {
  activeId.value = ''; panel.value = false; invitee.value = ''; draft.value = ''; mention.value = ''
  notice.value = ''; problem.value = ''; query.value = ''; filter.value = 'all'; page.value = 'chats'
  data.value = { ...empty, people: data.value.people }
  loading.value = true
  void refresh()
}
function switchPage(next: 'chats' | 'contacts') {
  page.value = next; query.value = ''; notice.value = ''; problem.value = ''
}
export async function send(): Promise<void> {
  if (!canSend.value || !room.value) return
  if (await command('send', {room:room.value.id, text:draft.value, target:mention.value})) {
    draft.value = ''; mention.value = ''
  }
}
interface ComposerKeyInput {
  readonly key: string
  readonly shiftKey: boolean
  readonly isComposing: boolean
}

interface ComposerKeyDispatch {
  preventDefault(): void
}

// Bound to the event that supplied this invocation's ComposerKeyInput.
/** @N external */
declare const composerKeyDispatch: ComposerKeyDispatch

export function composerKey(event: ComposerKeyInput): void {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return
  composerKeyDispatch.preventDefault()
  void send()
}
function inRoom(action: Action, target = '') {
  if (room.value) return command(action, {room:room.value.id, target})
}
export async function invite(): Promise<void> { if (invitee.value && await inRoom('invite', invitee.value)) invitee.value = '' }
function contactAction(action: Action, c: Contact) { return command(action, {target:c.id}) }
function recall(m: Message) { if (room.value) return command('recall', {room:room.value.id, id:String(m.id)}) }
function label(r: Room) { return unreadLabel(r) }
function time(value: string) {
  return new Intl.DateTimeFormat('zh-CN', {hour:'2-digit', minute:'2-digit', hour12:false}).format(new Date(value))
}

// Per-render bindings, not event formals. Each declaration denotes the
// original v-for closure's observed row or option, retaining its identity.
// The enclosing template guards still decide whether it exists.
/** @N entryState */
declare const renderedRoom: Room
/** @N entryState */
declare const renderedContact: Contact
/** @N entryState */
declare const renderedMessage: Message
/** @N entryState */
declare const renderedMember: Member
/** @N entryState */
declare const renderedFilter: 'all' | 'unread' | 'mentions'

// Host ingress contracts. Template modifiers own the event preprocessing.
export function onPollTick(): void {
  if (document.visibilityState === 'visible') void refresh(true)
}
export function onMount(): void {
  void refresh()
  timer = window.setInterval(onPollTick, 1600)
}
export function onUnmount(): void {
  disposed = true
  if (timer !== undefined) window.clearInterval(timer)
}
export function onThreadLayoutChanged(): void {
  if (thread.value) thread.value.scrollTop = thread.value.scrollHeight
}
export function onReconnectClick(): Promise<void> { return refresh() }
export function onAccountValueChanged(value: string): void { account.value = value }
export function onAccountChange(): void { switchAccount() }
export function onChatsClick(): void { switchPage('chats') }
export function onContactsClick(): void { switchPage('contacts') }
export function onRoomClick(): Promise<void> { return selectRoom(renderedRoom.id) }
export function onFilterClick(): void { filter.value = renderedFilter }
export function onSearchValueChanged(value: string): void { query.value = value }
export function onDraftValueChanged(value: string): void { draft.value = value }
export function onMentionChanged(value: string): void { mention.value = value }
export function onInviteeChanged(value: string): void { invitee.value = value }
export function onCloseNotice(): void { problem.value = ''; notice.value = '' }
export function onBackClick(): void { activeId.value = ''; panel.value = false }
export function onGroupSettingsClick(): void { panel.value = !panel.value }
export function onPinClick() { return inRoom('pin') }
export function onMuteClick() { return inRoom('mute') }
export function onModeClick() { return inRoom('mode') }
export function onLeaveClick() { return inRoom('leave') }
export function onRemoveClick() { return inRoom('remove', renderedMember.id) }
export function onRoleClick() { return inRoom(renderedMember.role === 'ADMIN' ? 'demote' : 'promote', renderedMember.id) }
export function onRecallClick() { return recall(renderedMessage) }
export function onAcceptClick() { return contactAction('accept', renderedContact) }
export function onDeclineClick() { return contactAction('decline', renderedContact) }
export function onDirectClick() { return contactAction('open', renderedContact) }
export function onBlockClick() { return contactAction('block', renderedContact) }
export function onUnblockClick() { return contactAction('unblock', renderedContact) }
export function onRequestClick() { return contactAction('request', renderedContact) }

// Consumption starts after the host has selected the observed binding;
// the README gives the precise mount domain for each conditional property.
/** @N componentProp Input#global-search.value */
export function searchValue(): string { return query.value }
/** @N componentProp Input#global-search.placeholder */
export function searchPlaceholder(): string {
  return page.value === 'chats' ? '搜索会话和消息' : '搜索联系人'
}
/** @N componentProp Select#demo-account.disabled */
export function accountDisabled(): boolean { return busy.value || loading.value }
/** @N componentProp Div#notice-banner.role */
export function noticeRole(): string { return problem.value ? 'alert' : 'status' }
/** @N componentProp Div#notice-banner.textContent */
export function noticeText(): string { return problem.value || notice.value }
/** @N componentProp Textarea#message-composer.value */
export function draftValue(): string { return draft.value }
/** @N componentProp Textarea#message-composer.disabled */
export function draftDisabled(): boolean {
  const selected = room.value!
  return !selected.canSend || busy.value
}
/** @N componentProp Button#send-message.disabled */
export function sendDisabled(): boolean { return !canSend.value }
/** @N componentProp Small#composer-hint.textContent */
export function draftHint(): string { return composerHint.value }
/** @N componentProp Button#group-settings.aria-expanded */
export function groupExpanded(): boolean { return panel.value }
/** @N componentProp Button#invite-member.disabled */
export function inviteDisabled(): boolean { return busy.value || !invitee.value }

/** @N componentProp H2#welcome.textContent */
export function welcomeTitle(): string {
  return loading.value ? '正在连接' : '选一个会话，聊点什么'
}
