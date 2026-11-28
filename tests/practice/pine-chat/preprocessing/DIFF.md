# 会话工作台 · AI 预处理逐项 Diff

`-` 是 [原 App.vue](../frontend/src/App.vue) 的写法，`+` 是分析副本。每段只对照一个改动，不是要应用回原工程的补丁。先看 **CLI 已验收**，再看 **工作台候选稿**；后者保留原行为，但尚不能生成整页 PERS。每项改动按 [AI-05](../../../../design/chapters/04-AI救赎.md#ai-05-预处理停在哪儿) 和[场景归一](../../../../design/chapters/09-场景归一.md#规则区)说明为什么必要。

## 已验收：[Bindings.ts](analysis/src/Bindings.ts)

### 1. 把 Vue ref 还原为原实例已有的状态

```diff
-const query = ref('')
-const draft = ref('')
+/** @N entryState */
+const query = { value: '' }
+/** @N entryState */
+const draft = { value: '' }
```

**为什么改：** `ref` 的容器来自 Vue。这个切片只观察 `.value` 的读写，不观察响应式调度或容器自身；用原生字段接回同一个页面实例的绑定。初值只是实例刚创建时的状态，不是用户每次输入前都为空。已有值是 E，写回是 S。工作台候选的 `account`、`data`、`activeId` 等同理；`revision`、`disposed` 也保留，因为它们会决定回执能不能覆盖已有页面。

### 2. 把两个 v-model 分成写入事件和属性取值

```diff
-<div class="search"><input v-model="query" :placeholder="page==='chats'?'搜索会话和消息':'搜索联系人'" aria-label="搜索" /></div>
-<textarea v-model="draft" :disabled="!room.canSend || busy" rows="2" aria-label="消息正文"
-          placeholder="输入消息…" @keydown="composerKey"></textarea>
+export function onSearchValueChanged(value: string): void {
+  query.value = value
+}
+/** @N componentProp Input#global-search.value */
+export function searchValue(): string {
+  return query.value
+}
+export function onDraftValueChanged(value: string): void {
+  draft.value = value
+}
+/** @N componentProp Textarea#message-composer.value */
+export function draftValue(): string {
+  return draft.value
+}
```

**为什么改：** 原模板没把写入和取值暴露成函数。生产入口从 Vue **已交付字符串**的那刻开始，P 是该字符串，而不是原始 DOM 事件；消费入口同步无参，读回同一个字段。输入法尚未提交时不调用生产入口。输入 `"  hi  "` 会原样写入，不在这里做搜索、裁剪或发消息。草稿出口只在聊天分支且会话存在时调用；未挂载不求值。上面只对照 `v-model`，不把同一标签上的 `disabled` 等其他绑定算进这一对。

### 3. 把欢迎区标题绑定提成无参出口

```diff
-<h2>{{ loading?'正在连接':'选一个会话，聊点什么' }}</h2>
+/** @N componentProp H2#welcome.textContent */
+export function welcomeTitle(): string {
+  return loading.value ? '正在连接' : '选一个会话，聊点什么'
+}
```

**为什么改：** 标题表达式原来只由模板求值；现在让它成为独立的消费入口，条件和两段文案不变。原模板先排除通讯录、再排除有会话的分支；只有走到欢迎区才调用。一个已打开的会话不会因此得到空标题，也不会执行这个函数。

## 补充切片：把宿主绑定接回原生程序

```diff
-<button ... @click="switchPage('chats')">
-<button ... @click="switchPage('contacts')">
-<input v-model="query" :placeholder="page==='chats'?'搜索会话和消息':'搜索联系人'" ... />
+export function onChatsClick(): void { switchPage('chats') }
+export function onContactsClick(): void { switchPage('contacts') }
+/** @N componentProp Input#global-search.placeholder */
+export function searchPlaceholder(): string {
+  return page.value === 'chats' ? '搜索会话和消息' : '搜索联系人'
+}
```

两个参数值来自模板中的字面量，不来自事件，不新增 P。只把 `page` 的 Vue 容器还原成原实例的 `entryState`，`switchPage(next)` 原样保留。即使当前已在通讯录，再点一次也要清空搜索和两种提示。取值函数只搬出原模板的三元式。

```diff
-<button v-if="room.group" :aria-expanded="panel" @click="panel=!panel">群设置</button>
+/** @N entryState */
+const panel = { value: false }
+export function onGroupSettingsClick(): void { panel.value = !panel.value }
+/** @N componentProp Button#group-settings.aria-expanded */
+export function groupExpanded(): boolean { return panel.value }
```

模板的两个绑定分别导出，写、读都落在原群按钮所属页面的同一 `panel`。外层 `v-if` 仍限定实际调用：没有挂载就不调用，不把「未挂载」改成属性值 `false`。

## 尚未验收：闭合小入口里的两类拒绝

这些函数也在 [Bindings.ts](analysis/src/Bindings.ts)。它们通过了强类型检查，但不在上面五个已通过的入口里。

```diff
-{{ problem || notice }} <button aria-label="关闭提示" @click="problem=''; notice=''">×</button>
+export function onCloseNotice(): void {
+  problem.value = ''
+  notice.value = ''
+}
```

**为什么改：** 内联点击没有可调用入口，提取后保留先清错误、再清提示的两次写入。当前 CLI 装载这两次文本字面量写入时拒绝；不把空字串改成参数或布尔值换取通过。

```diff
-:disabled="busy || loading"
-:role="problem?'alert':'status'"
+/** @N componentProp Select#demo-account.disabled */
+export function accountDisabled(): boolean {
+  return busy.value || loading.value
+}
+/** @N componentProp Div#notice-banner.role */
+export function noticeRole(): string {
+  return problem.value ? 'alert' : 'status'
+}
```

**为什么改：** 让两个属性各有观察入口，保留原布尔短路和字符串真假。提示栏原先有 `v-if="problem || notice"`：只有通知时角色是 `status`，有错误时为 `alert`，两者都空时不调用。工具分别拒绝二元投影和非布尔类型的三元条件。

## 工作台候选：[ChatWorkspace.ts](workspace/src/ChatWorkspace.ts)

### 4. 暴露真实回调，不改等待方式

```diff
-async function send() {
+export async function send(): Promise<void> {
   if (!canSend.value || !room.value) return
   if (await command('send', {room:room.value.id, text:draft.value, target:mention.value})) {
     draft.value = ''; mention.value = ''
   }
 }
+interface ComposerKeyInput {
+  readonly key: string
+  readonly shiftKey: boolean
+  readonly isComposing: boolean
+}
+interface ComposerKeyDispatch {
+  preventDefault(): void
+}
+/** @N external */
+declare const composerKeyDispatch: ComposerKeyDispatch
+
-function composerKey(event: KeyboardEvent) {
+export function composerKey(event: ComposerKeyInput): void {
   if (event.key !== 'Enter' || event.shiftKey || event.isComposing) return
-  event.preventDefault()
+  composerKeyDispatch.preventDefault()
   void send()
 }
```

**为什么改：** `send` 是表单的真实应用回调；原模板的 `@submit.prevent` 由宿主先处理，不在入口里假造事件。键盘回调则自己判断 Enter、Shift 和组字，命中时先阻止默认行为，再启动发送并立即返回 `void`。宿主在回调入口交付事件的三个实际字段，并把端口绑定到同一次事件派发；端口不是新增参数，也不是声称默认动作一定成功取消的布尔值。两种入口不能合成一个都等待发送完成的函数。禁言、忙碌或无有效草稿仍不发请求；只有原命令判成功才清草稿。

### 5. 把列表行的闭包身份交代清楚，不新增 P

```diff
-@click="selectRoom(r.id)"
+/** @N entryState */
+declare const renderedRoom: Room
+export function onRoomClick(): Promise<void> { return selectRoom(renderedRoom.id) }
```

**为什么改：** `r` 来自这次模板渲染所建立的会话行闭包，不是点击事件随意传入的 ID。候选稿声明这条已绑定的行，仍在调用时取它的 `id` 并进入原函数。联系人、待撤回消息、成员、筛选选项也按原闭包绑定。切会话仍会按忙碌状态、是否换房、未读数决定能否清草稿和发已读请求。绑定声明目前不是已获 CLI 支持的入参格式。

### 6. 声明真实 HTTP 边界，保留两次等待和失败分支

```diff
+interface FetchResponse {
+  readonly ok: boolean
+  json(): Promise<unknown>
+}
+interface BrowserHttp {
+  /** @N rejects ExternalFailure */
+  get(url: string): Promise<FetchResponse>
+  /** @N rejects ExternalFailure */
+  post(url: string, method: 'POST', body: URLSearchParams): Promise<FetchResponse>
+}
+/** @N external */
+declare const http: BrowserHttp
-    const next = await reply<Snapshot>(await fetch('/api/view?' + new URLSearchParams({user: account.value})))
+    const next = await reply<Snapshot>(await http.get('/api/view?' + new URLSearchParams({user: account.value})))
-    const response = await fetch('/api/command', {
-      method:'POST',
-      body:new URLSearchParams({...args, user: account.value, action})
-    })
+    const response = await http.post('/api/command', 'POST', new URLSearchParams({...args, user: account.value, action}))
     const next = await reply<{room:string, view:Snapshot}>(response)
```

**为什么改：** 浏览器 Fetch 的实际实现不在分析工程里，GET、POST、表单编码与失败有原页面和 [Java HTTP 处理器](../backend/src/practice/chat/ChatServer.java)作为证据。网络拒绝不同于后端返回非 2xx；后者仍先读 JSON，再按 `ok` 和原错误字段处置。没有把 JSON 解析报错合并成网络失败，没有把 `response.json()` 记为第二次 HTTP 请求。原始 `unknown` 窄化、版本比较、卸载门控和 `finally` 清理都保留。正式 CLI 首先拒绝 `'POST'` 的字面量类型，不能据此说后续边界已能运行。

### 7. 拆生命周期，不把循环裁成一次

```diff
-onMounted(() => {
+export function onPollTick(): void {
+  if (document.visibilityState === 'visible') void refresh(true)
+}
+export function onMount(): void {
   void refresh()
-  timer = window.setInterval(() => { if (document.visibilityState === 'visible') void refresh(true) }, 1600)
-})
-onUnmounted(() => { disposed = true; if (timer !== undefined) window.clearInterval(timer) })
+  timer = window.setInterval(onPollTick, 1600)
+}
+export function onUnmount(): void {
+  disposed = true
+  if (timer !== undefined) window.clearInterval(timer)
+}
```

**为什么改：** Vue 原本注册的函数不是标准入口，要分别显式化挂载、定时器回调、卸载。定时器注册和取消没有被删除；后台不可见时仍不请求，不人为限定轮询总次数。原 Vue watcher 的 `flush:'post'` 仍是宿主条件：候选只交出滚动回调体 `onThreadLayoutChanged`，没有交出或验证整套 DOM 调度。

### 8. 解开 computed 包装，禁用按钮仍用原守卫

```diff
-const draftLength = computed(() => [...draft.value.trim()].length)
-const canSend = computed(() => !!room.value?.canSend && draftLength.value > 0 && draftLength.value <= 300 && !busy.value)
+const draftLength = { get value() { return [...draft.value.trim()].length } }
+const canSend = { get value() { return !!room.value?.canSend && draftLength.value > 0 && draftLength.value <= 300 && !busy.value } }
-<small>{{ draftLength }}/300</small><button class="primary" type="submit" :disabled="!canSend">发送</button>
+/** @N componentProp Button#send-message.disabled */
+export function sendDisabled(): boolean { return !canSend.value }
```

**为什么改：** Vue computed 不能作为零依赖工程交付。这里仅分析读取纯计算所得的值，不观察缓存身份或求值次数，因此在原读取处保留原表达式。草稿裁剪后数 Unicode 码点，300 个表情仍可发送，第 301 个才超限；不能改为 UTF-16 的字符串长度。文本区禁用是另一个出口，只看会话权限与忙碌状态。按钮未挂载时不调用该出口。按同样方式暴露了搜索占位语、提示正文、文本区禁用、编辑提示、群面板展开和邀请按钮禁用。

## 刻意没做的改动

- 不提前把会话搜索、排序、未读合计、权限守卫转换成不同算法；保留 `model.ts` 的原方法体。候选将相关本地源码并入同一个输入文件是打包选择，不是设计规定，也不是把本地函数标成外部。
- 不把模板行替成首行，不把时间格式改为 UTC，不把没挂载的属性替成空值。不把候选稿中未覆盖的列表、宿主调度和 DOM 规则标为完成。
- 不修改原 Vue/Java 应用、正式回归或内核。通过与拒绝的复现命令见[交付索引](README.md)。
