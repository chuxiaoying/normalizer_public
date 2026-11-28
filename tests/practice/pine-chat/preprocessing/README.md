# 松果聊天 · 会话工作台的 AI 预处理

原件：[App.vue](../frontend/src/App.vue)、[model.ts](../frontend/src/model.ts)、[HTTP 合同](../backend/src/practice/chat/ChatServer.java)、[业务服务](../backend/src/practice/chat/ChatService.java)。工作台是松果聊天的单页应用，包含会话、通讯录、群设置和消息编辑。原应用不改动。要逐处看原码、改写和依据，见[预处理逐项 Diff](DIFF.md)。

这里分三个根目录：

- [analysis/src/Bindings.ts](analysis/src/Bindings.ts)：可单独送入正式 CLI 的、按绑定切出的最小闭合工程。同一生产/消费对使用同一份 VO 声明。此处不是整个页面已归一化的结论。
- [navigation/src/Navigation.ts](navigation/src/Navigation.ts)：页签和群设置的独立切片，保留页签原有联合类型和本地函数调用；不影响原最小切片的验收。
- [workspace/src/ChatWorkspace.ts](workspace/src/ChatWorkspace.ts)：覆盖工作台核心流程的严格 TS 预处理稿。保留发送、已读、好友和群命令、生命周期、回执及选择后的状态投影；当前场景适配器拒收。它不是已通过的分析副本。列表逐项、日期显示和 DOM 观察仍有后文列出的宿主/运算缺口。

## 可交付的入口和出口

从仓库根执行，根目录不包含 Vue、包依赖、HTML、预期或产物：

```sh
root=tests/practice/pine-chat/preprocessing/analysis
implementation/tools/normalizer scenario frontend-vo-production "$root" --entry 'src/Bindings.ts#onSearchValueChanged'
implementation/tools/normalizer scenario frontend-vo-consumption "$root" --entry 'src/Bindings.ts#searchValue'
implementation/tools/normalizer scenario frontend-vo-production "$root" --entry 'src/Bindings.ts#onDraftValueChanged'
implementation/tools/normalizer scenario frontend-vo-consumption "$root" --entry 'src/Bindings.ts#draftValue'
implementation/tools/normalizer scenario frontend-vo-consumption "$root" --entry 'src/Bindings.ts#welcomeTitle'
```

| 原绑定 / 进入条件 | 生产或消费 | 独立推导的业务观察 |
| --- | --- | --- |
| 全局搜索 `input v-model="query"`；Vue 已经确定要交付新的字符串 | `onSearchValueChanged(value)` | P 是这次交付的字符串；S 写原实例的 `query.value` 一次；R 为 `void`。不将去空格、转小写、过滤或查远端挪进这个监听。 |
| 同一个搜索框的显示值；该元素在两页都存在 | `searchValue()` | 无 P、无 S，读取 E 中的 `query.value` 并原样返回，包括前后空格。 |
| 聊天编辑框 `textarea v-model="draft"`；已挂载且 Vue 已交付字符串 | `onDraftValueChanged(value)` | P 是文本；S 只更新 `draft.value`；R 为 `void`。不等于提交消息。 |
| 同一个编辑框的显示值；`page !== 'contacts'` 且当前会话存在 | `draftValue()` | 无 P、无 S，返回已有 `draft.value`。没有选中会话时不存在该绑定，不执行该入口。 |
| 欢迎区标题；`page !== 'contacts'` 且没有当前会话 | `welcomeTitle()` | 无 P、无 S；已有 `loading` 为真得到「正在连接」，否则得到「选一个会话，聊点什么」。隐藏欢迎区不等于返回空字符串。 |

输入的观察点在 Vue 的文本绑定交付之后，不是在浏览器开始一次原始 `InputEvent` 时。输入法组字等 Vue 尚未交付值的事件不调用该入口。消费入口由原模板调用域限定；上层不挂载时不调用 supplier，返回值里不造 `null`、空文本或 `false` 作为「没有观察」的替代。

## 补充切片：切换页签、展开群设置

这些入口在 `navigation/src/Navigation.ts`。每组入口在这一根内共用 VO；`query`、`notice`、`problem` 对应与原切片相同的 App.vue 槽位，但两个根的 PERS 标识不能自动合并。

| 原绑定 / 观察点 | 入口 | 预期业务观察 |
| --- | --- | --- |
| 顶部两个页签按钮，宿主已经派发点击 | 生产 `onChatsClick()` / `onContactsClick()` | 无 P。沿原 `switchPage`，S 依次写 `page.value`、清空 `query.value`、`notice.value`、`problem.value`，R 为 `void`。点当前页也照常清空。保留 `switchPage(next)` 的原签名和函数体；只有模板绑定的两个固定实参被显式接回。 |
| 同一个搜索框的占位文案，两页均挂载 | 消费 `searchPlaceholder()` | 无 P、无 S。读同一个 `page.value`，返回对应文案。不读取或清洗搜索文字。 |
| 群设置按钮；处在会话页且选中的会话是群聊 | 生产 `onGroupSettingsClick()` / 消费 `groupExpanded()` | 前者无 P，读取并翻转 `panel.value`，R 为 `void`；后者无 P、无 S，读同一槽并返回模板实际绑定的布尔值（宿主序列化为 DOM 属性之前）。没有按钮时两者都不在调用域，不能用 `false` 代替没有观察。 |

定向查验用 `root=tests/practice/pine-chat/preprocessing/navigation` 和 `--entry 'src/Navigation.ts#<导出名>'`。源码保留原页签联合类型；当前场景适配器的状态装载只接受有限几种字段类型，不能把 `'chats' | 'contacts'` 当作普通 `string` 交付来迁就它。表中是从原件推导的预期，不是手填的 CLI 产物。

## 工作台生产预处理稿

| 原入口 | 稿中入口 | 不能丢的行为 |
| --- | --- | --- |
| 表单 `@submit.prevent="send"` | `send` | 模板先阻止原生表单提交，再调用原 `async` handler。无权限、空白/超长或忙碌不发请求；许可时把当次账号、会话、原草稿和提及写入 POST 表单。保留回执后的版本/销毁检查、失败提示、正常/异常路径的 busy/loading 清理。只有命令成功才清空草稿和提及。 |
| 编辑框按键 | `composerKey` | 非 Enter、Shift+Enter 和组字不发送；命中时先 `preventDefault()`，发起 `void send()` 后按原方式返回，绝不替换成等待发送完成的入口。 |
| 列表会话点击 | `onRoomClick` | 沿 `selectRoom` 原函数；忙碌时不切换，换房间才清草稿/提及/面板/邀请项；只有标记为已读且未读数非零才 POST。会话 ID 取自当时模板捕获的行，不是假设用户传入的 P。 |
| 通讯录、撤回、群角色/禁言/邀请/退出 | 各 `on…Click` 和 `invite` | 模板选择的行或成员保留绑定身份；只在原分支调用对应命令。接受好友后切到新私聊，退群清空选择，过期回执不覆盖现有快照。 |
| 账号绑定、挂载、轮询、卸载、重新连接 | `onAccountValueChanged`、`onAccountChange`、`onMount`、`onPollTick`、`onUnmount`、`onReconnectClick` | 账号绑定与 change 回调是不同连接点；保留刷新、可见性门控、1600ms 注册、注销与过期回执门控，不将无限轮询当成一次有界调用。挂载、计时器每次触发、卸载分别观察。 |

**HTTP 边界。** `http.get`、`http.post` 分别定位真实的 `GET /api/view?user=…` 和 `POST /api/command`。POST 的 `URLSearchParams` 保持原字段共存和编码合同。网络拒绝有真实依据，声明为 `ExternalFailure`；HTTP 非 2xx 仍先收到响应，按原程序等待 `json()`、再检查 `ok` 和失败消息。响应体读取可独立失败，并属于同一个响应，不得当成第二个 HTTP 请求，也不能合并两个 await 以消除可能发生的交错。保留宽类型 `unknown` 检查和原捕获行为，不编造后端成功数据。

## 工作台消费预处理稿

- `searchPlaceholder` 原样使用页签条件；`accountDisabled` 原样使用 `busy || loading`，不是会话状态的另一份副本。
- `noticeRole` / `noticeText` 仅在 `problem || notice` 为真时求值。失败文案优先；关闭提示按原来顺序清 `problem` 再清 `notice`，下一次渲染不挂载提示。
- `draftDisabled` / `sendDisabled` / `draftHint` 只在聊天分支且存在会话时求值。按钮禁用取反的是原 `canSend`，保留会话权限、trim 后 Unicode 码点计数、300 临界值和忙碌条件；不能改成 UTF-16 `length`。文本区的禁用条件**不检查草稿长度**。
- `groupExpanded` 另外要求群聊；`inviteDisabled` 还要求面板打开且是群主。条件不满足时不产生属性观察。
- 会话/联系人列表、未读与提及徽标、消息正文和角色文案的原表达式保留在原件和本稿的相关计算中，不作为已交付的无参属性入口。尤其是模板 `v-for` 属性要绑定真实行身份，语言区 `Intl` 时间显示须有宿主时区/格式合同；不能取首行、把本地时间改为 UTC，或用列表长度当列表内容。

## 改动许可与核对

| 设计章 | 本交付的处理 |
| --- | --- |
| 符号执行 | 保留原生分支、窄化、局部函数、await 与异常，不把严格 TS 的 `unknown` 改成具体 JSON。 |
| 多重宇宙 | 不合并 busy、无权限、HTTP 拒绝、过期回执、成功等路径。相同 UI 文案不证明相同动作。 |
| 数学化简 | 不在预处理折叠 composer 守卫、计数、过滤或布尔式，不用结果反填源程序。 |
| AI救赎 | 将 Vue ref 脱糖为同一实际实例上的字段，把模板/生命周期绑定显式化。纯 computed 的值表达式沿原读取点计算；这里的消费者不观察 computed 缓存对象身份。把真实网络边界类型化；不把本地辅助函数标成外部。 |
| 关系代数 | 原有数组操作、排序和搜索合同保留，未把 `find`、`sort`、locale 比较或码点计数硬改成不等价的指令。 |
| 业务至上 | 不删会影响请求次数的可见性、定时器、版本与销毁门控，也不把用户可见的格式、长短边界或错误默认为等价。 |
| PERS登场 | 现有状态是 E，真实事件实参是 P；外部请求和已有状态写入按时点进 S；回执是其原请求的响应；生产完成与属性返回各自独立。 |
| 统一定形 | 源码保留执行次序；不由预处理选 CASE 表达方向、不手写输出代替 CLI。 |
| 场景归一 | 生产从宿主交给应用的回调开始，消费从具体一次渲染的绑定开始；用具名、同步、无参 supplier 读同一实例。 |

两个工程的同名字段回指同一原件绑定，但 CLI 会输出各自源码定位，**不要把不同根产物中的 E 标识自动合并**。可串看的是同根同绑定，例如写、读 `draft.value` 的一对。

## 当前实现缺口

以下是可复验的首个拒绝点，不代表修掉它就能完整分析。保留候选源码，未改内核或正式回归。

| 根目录 / 入口 | 首个拒绝及含义 |
| --- | --- |
| `analysis` / 生产 `onCloseNotice` | 内部值装载报 `missing field value`；原赋值是两次写空字符串。不能为测试通过改写成空参数或布尔清除。 |
| `analysis` / 消费 `accountDisabled`、`noticeRole` | 前者拒绝二元投影；后者拒绝字符串 truthiness 的三元守卫。两者都保留原表达式。 |
| `workspace` / 生产 `send` | 当前外部端口签名解析不接受实际方法字面量类型 `'POST'`。响应解析、局部异步调用和复杂 VO 也要单独验收。 |
| `workspace` / 生产 `composerKey`、`onRoomClick` | 前者所在同步入口模块含真实异步端口；后者是非 async 的回调，但返回原 `selectRoom` 的 Promise，当前路由要求同步入口显式 `void`。不为过准入重写完成类型。 |
| `workspace` / 消费 `sendDisabled`、`searchPlaceholder` | 当前投影入口不能接纳工作台的数据初始化，连符号执行还没到。这不能证实复合守卫或列表相关语义已经支持。 |

运行拒绝复现时，把上面命令末尾的场景、根和导出名替换为表中入口；`workspace` 使用 `src/ChatWorkspace.ts#…`。本轮不运行或改动正式回归。
