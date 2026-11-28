<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Message, Room, Snapshot, Contact, Action } from './model'
import { contactLabels, displayMessage, roomList, unreadLabel } from './model'

const empty: Snapshot = { people: [], rooms: [], contacts: [] }
const account = ref('lin')
const data = ref<Snapshot>(empty)
const page = ref<'chats' | 'contacts'>('chats')
const activeId = ref('')
const filter = ref<'all' | 'unread' | 'mentions'>('all')
const query = ref('')
const draft = ref('')
const mention = ref('')
const panel = ref(false)
const invitee = ref('')
const notice = ref('')
const problem = ref('')
const busy = ref(false)
const loading = ref(true)
const thread = ref<HTMLElement>()
let revision = 0
let fetching = false
let timer: number | undefined
let disposed = false
const room = computed(() => data.value.rooms.find(r => r.id === activeId.value))
const self = computed(() => data.value.people.find(p => p.id === account.value))
const rooms = computed(() => roomList(data.value.rooms, filter.value, query.value))
const contacts = computed(() => data.value.contacts.filter(c => c.name.includes(query.value.trim())))
const waiting = computed(() => data.value.contacts.filter(c => c.status === 'INCOMING').length)
const unread = computed(() => data.value.rooms.reduce((n, r) => n + (r.muted ? 0 : r.unread), 0))
const draftLength = computed(() => [...draft.value.trim()].length)
const inviteOptions = computed(() => data.value.contacts.filter(c => c.status === 'FRIEND' &&
  !room.value?.members.some(member => member.id === c.id)))
const canSend = computed(() => !!room.value?.canSend && draftLength.value > 0 && draftLength.value <= 300 && !busy.value)
const composerHint = computed(() => !room.value?.canSend ?
  room.value?.sendReason === 'ADMINS_ONLY' ? '当前仅管理员可以发言' : '当前不能向对方发送消息' :
  draftLength.value > 300 ? '消息不能超过 300 个字符' : 'Enter 发送 · Shift+Enter 换行')
const actionLabels: Partial<Record<Action,string>> = {
  request:'已发送好友申请', accept:'已添加好友', decline:'已拒绝申请', block:'已拉黑',
  unblock:'已解除拉黑', recall:'消息已撤回', invite:'已邀请成员', remove:'已移出成员',
  leave:'已退出群聊', promote:'已设为管理员', demote:'已撤销管理员',
}
async function reply<T>(response: Response): Promise<T> {
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
    const next = await reply<Snapshot>(await fetch('/api/view?' + new URLSearchParams({user: account.value})))
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
    const response = await fetch('/api/command', {
      method:'POST',
      body:new URLSearchParams({...args, user: account.value, action})
    })
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
async function send() {
  if (!canSend.value || !room.value) return
  if (await command('send', {room:room.value.id, text:draft.value, target:mention.value})) {
    draft.value = ''; mention.value = ''
  }
}
function composerKey(event: KeyboardEvent) {
  if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return
  event.preventDefault()
  void send()
}
function inRoom(action: Action, target = '') {
  if (room.value) return command(action, {room:room.value.id, target})
}
async function invite() { if (invitee.value && await inRoom('invite', invitee.value)) invitee.value = '' }
function contactAction(action: Action, c: Contact) { return command(action, {target:c.id}) }
function recall(m: Message) { if (room.value) return command('recall', {room:room.value.id, id:String(m.id)}) }
function label(r: Room) { return unreadLabel(r) }
function time(value: string) {
  return new Intl.DateTimeFormat('zh-CN', {hour:'2-digit', minute:'2-digit', hour12:false}).format(new Date(value))
}
watch(() => [room.value?.id, room.value?.messages.length], () => {
  if (thread.value) thread.value.scrollTop = thread.value.scrollHeight
}, {flush:'post'})
onMounted(() => {
  void refresh()
  timer = window.setInterval(() => { if (document.visibilityState === 'visible') void refresh(true) }, 1600)
})
onUnmounted(() => { disposed = true; if (timer !== undefined) window.clearInterval(timer) })
</script>

<template>
  <main class="shell" :class="{ 'conversation-open': !!room && page === 'chats' }">
    <aside class="sidebar">
      <header class="brand">
        <span class="mark" aria-hidden="true">松</span>
        <div><p class="eyebrow">NORMALIZER · 测试实战</p><h1>松果聊天</h1></div>
      </header>
      <label class="account">
        <span class="avatar own">{{ self?.name.slice(-1) || '林' }}</span>
        <span><small>演示账号</small>
          <select v-model="account" aria-label="演示账号" :disabled="busy || loading" @change="switchAccount">
            <option v-if="!data.people.length" value="lin">林澄</option>
            <option v-for="p in data.people" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </span>
      </label>
      <nav class="tabs" aria-label="主导航">
        <button :class="{selected:page==='chats'}" @click="switchPage('chats')">会话 <span v-if="unread">{{ unread }}</span></button>
        <button :class="{selected:page==='contacts'}" @click="switchPage('contacts')">通讯录 <span v-if="waiting">{{ waiting }}</span></button>
      </nav>
      <div class="search"><input v-model="query" :placeholder="page==='chats'?'搜索会话和消息':'搜索联系人'" aria-label="搜索" /></div>
      <template v-if="page==='chats'">
        <div class="filters" aria-label="会话筛选">
          <button v-for="choice in (['all','unread','mentions'] as const)" :key="choice"
                  :class="{selected:filter===choice}" @click="filter=choice">{{ {all:'全部',unread:'未读',mentions:'提到我'}[choice] }}</button>
        </div>
        <div class="rooms">
          <button v-for="r in rooms" :key="r.id" class="room" :class="{selected:r.id===activeId}" :disabled="busy"
                  @click="selectRoom(r.id)">
            <span class="avatar" :class="{group:r.group}">{{ r.group?'群':r.title.slice(-1) }}</span>
            <span class="roomtext">
              <strong>{{ r.title }}<small v-if="r.pinned">置顶</small></strong>
              <small>{{ r.messages.length ? displayMessage(r.messages[r.messages.length-1]!, account) : '暂无消息' }}</small>
            </span>
            <span v-if="label(r)" class="badge" :class="{muted:r.muted && !r.mentions}">{{ label(r) }}</span>
          </button>
          <p v-if="!rooms.length" class="empty-list">{{ loading ? '正在连接…' : '没有匹配的会话' }}</p>
        </div>
      </template>
      <p v-else class="side-note">在通讯录处理好友申请、开启私聊或管理拉黑。</p>
      <footer class="side-foot">LOCAL DEMO <span>仅本地 · 重启重置</span></footer>
    </aside>

    <section class="workspace">
      <div v-if="problem || notice" class="notice" :class="{error:problem}" :role="problem?'alert':'status'">
        {{ problem || notice }} <button aria-label="关闭提示" @click="problem=''; notice=''">×</button>
      </div>
      <template v-if="page==='contacts'">
        <header class="titlebar"><div><p class="eyebrow">CONNECTIONS</p><h2>通讯录</h2></div></header>
        <div class="contact-list">
          <p v-if="!contacts.length" class="empty-list">没有匹配的联系人</p>
          <article v-for="c in contacts" :key="c.id" class="contact">
            <span class="avatar">{{ c.name.slice(-1) }}</span><div><strong>{{ c.name }}</strong><small>{{ contactLabels[c.status] }}</small></div>
            <div class="contact-actions">
              <template v-if="c.status==='INCOMING'">
                <button :disabled="busy" class="primary" @click="contactAction('accept', c)">同意</button>
                <button :disabled="busy" @click="contactAction('decline', c)">拒绝</button>
              </template>
              <template v-else-if="c.status==='FRIEND'">
                <button :disabled="busy" class="primary" @click="contactAction('open', c)">发消息</button>
                <button :disabled="busy" @click="contactAction('block', c)">拉黑</button>
              </template>
              <button v-else-if="c.status==='BLOCKED'" :disabled="busy" @click="contactAction('unblock', c)">解除拉黑</button>
              <button v-else-if="c.canInvite" :disabled="busy" @click="contactAction('request', c)">添加好友</button>
            </div>
          </article>
        </div>
      </template>

      <template v-else-if="room">
        <header class="titlebar">
          <button class="back" @click="activeId=''; panel=false">‹ 会话</button>
          <div><p class="eyebrow">{{ room.group ? 'GROUP CHAT' : 'DIRECT MESSAGE' }}</p>
            <h2>{{ room.title }}<small v-if="room.group">{{ room.members.length }} 人</small></h2></div>
          <div class="tools">
            <button :disabled="busy" @click="inRoom('pin')">{{ room.pinned?'取消置顶':'置顶' }}</button>
            <button :disabled="busy" @click="inRoom('mute')">{{ room.muted?'取消免打扰':'免打扰' }}</button>
            <button v-if="room.group" :aria-expanded="panel" @click="panel=!panel">群设置</button>
          </div>
        </header>
        <div class="chatbody">
          <div ref="thread" class="thread" role="log" aria-label="消息记录">
            <p v-if="room.group && room.mode==='ADMINS_ONLY'" class="system">群内仅管理员可以发言</p>
            <p v-if="!room.messages.length" class="system">你们已成为好友，发条消息打招呼吧。</p>
            <article v-for="m of room.messages" :key="m.id" class="message" :class="{mine:m.from===account, recalled:m.recalled}">
              <span v-if="m.recalled" class="system">{{ displayMessage(m, account) }}</span>
              <template v-else>
                <span class="avatar">{{ m.name.slice(-1) }}</span>
                <div class="message-content">
                  <small>{{ m.name }} · {{ time(m.at) }}<b v-if="m.mention===account"> · 提到你</b></small>
                  <p class="bubble">{{ m.mention ? '@' + (data.people.find(p => p.id===m.mention)?.name || m.mention) + ' ' : '' }}{{ m.text }}</p>
                  <button v-if="m.canRecall" class="plain" :disabled="busy" @click="recall(m)">撤回</button>
                </div>
              </template>
            </article>
          </div>
          <aside v-if="panel && room.group" class="group-panel">
            <h3>群设置</h3>
            <p>{{ room.mode==='OPEN'?'所有成员可发言':'仅群主和管理员可发言' }}</p>
            <button v-if="room.admin" :disabled="busy" @click="inRoom('mode')">{{ room.mode==='OPEN'?'开启禁言':'恢复发言' }}</button>
            <ul>
              <li v-for="m in room.members" :key="m.id">
                <strong>{{ m.name }}</strong> <small>{{ {OWNER:'群主',ADMIN:'管理员',MEMBER:'成员'}[m.role] }}</small>
                <div v-if="room.owner===account && m.id!==account" class="member-actions">
                  <button :disabled="busy" @click="inRoom(m.role==='ADMIN'?'demote':'promote', m.id)">{{ m.role==='ADMIN'?'撤销管理':'设为管理' }}</button>
                  <button :disabled="busy" @click="inRoom('remove', m.id)">移出</button>
                </div>
              </li>
            </ul>
            <div v-if="room.owner===account" class="invite">
              <select v-model="invitee" aria-label="选择要邀请的好友">
                <option value="">选择好友</option><option v-for="c in inviteOptions" :key="c.id" :value="c.id">{{ c.name }}</option>
              </select>
              <button :disabled="busy || !invitee" @click="invite">邀请入群</button>
            </div>
            <button v-else :disabled="busy" @click="inRoom('leave')">退出群聊</button>
          </aside>
        </div>
        <form class="composer" @submit.prevent="send">
          <textarea v-model="draft" :disabled="!room.canSend || busy" rows="2" aria-label="消息正文"
                    placeholder="输入消息…" @keydown="composerKey"></textarea>
          <div class="composer-bottom">
            <small>{{ composerHint }}</small>
            <select v-if="room.group" v-model="mention" aria-label="提到群成员" :disabled="!room.canSend || busy">
              <option value="">不提醒成员</option><option v-for="m in room.members.filter(x => x.id!==account)" :key="m.id" :value="m.id">@{{ m.name }}</option>
            </select>
            <small>{{ draftLength }}/300</small><button class="primary" type="submit" :disabled="!canSend">发送</button>
          </div>
        </form>
      </template>
      <div v-else class="welcome">
        <span class="mark" aria-hidden="true">松</span><p class="eyebrow">PINE CHAT</p>
        <h2>{{ loading?'正在连接':'选一个会话，聊点什么' }}</h2>
        <p>切换左侧演示账号，就能看到同一条消息在发送方与接收方各自的状态。</p>
        <button v-if="problem" @click="refresh()">重新连接</button>
      </div>
    </section>
  </main>
</template>
