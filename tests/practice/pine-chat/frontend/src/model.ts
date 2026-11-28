export type Action = 'request' | 'accept' | 'decline' | 'block' | 'unblock' | 'open' | 'send' |
  'recall' | 'read' | 'pin' | 'mute' | 'mode' | 'promote' | 'demote' | 'invite' | 'remove' | 'leave'
export interface Person { id: string; name: string }
export interface Contact extends Person { status: 'BLOCKED' | 'FRIEND' | 'INCOMING' | 'OUTGOING' | 'STRANGER'; canInvite: boolean }
export interface Member extends Person { role: 'OWNER' | 'ADMIN' | 'MEMBER' }
export interface Message {
  id: number; from: string; name: string; text: string; at: string; recalled: boolean; mention: string; canRecall: boolean
}
export interface Room {
  id: string; title: string; group: boolean; peer: string; owner: string; admin: boolean;
  mode: 'OPEN' | 'ADMINS_ONLY'; members: Member[]; messages: Message[];
  unread: number; mentions: number; pinned: boolean; muted: boolean; canSend: boolean; sendReason: string
}
export interface Snapshot { people: Person[]; rooms: Room[]; contacts: Contact[] }
export const contactLabels: Record<Contact['status'], string> = {
  BLOCKED: '已拉黑', FRIEND: '好友', INCOMING: '好友申请待处理', OUTGOING: '等待对方处理', STRANGER: '尚未添加'
}
export function displayMessage(message: Message, account: string) {
  if (message.recalled) return message.from === account ? '你撤回了一条消息' : message.name + '撤回了一条消息'
  return message.text
}
export function unreadLabel(room: Room) {
  if (room.mentions) return '@ ' + room.mentions
  if (!room.unread) return ''
  if (room.muted) return '·'
  return room.unread > 99 ? '99+' : String(room.unread)
}
export function roomList(rooms: Room[], filter: 'all' | 'unread' | 'mentions', query: string) {
  const needle = query.trim().toLocaleLowerCase('zh-CN')
  return rooms.filter(room => {
    if (filter === 'unread' && !room.unread) return false
    if (filter === 'mentions' && !room.mentions) return false
    if (!needle || room.title.toLocaleLowerCase('zh-CN').includes(needle)) return true
    return room.messages.some(message => !message.recalled && message.text.toLocaleLowerCase('zh-CN').includes(needle))
  }).sort((a, b) => Number(b.pinned) - Number(a.pinned) ||
    (b.messages.at(-1)?.id ?? -1) - (a.messages.at(-1)?.id ?? -1) || a.id.localeCompare(b.id))
}
