# 任务身份、观察前缀与纯完成值（扩展目标）

本文的 `task.value`、参数化 `pure_value` 与任意任务前缀格式是设计目标，不是当前生产 CLI 的通用输出。当前可运行的任务和事件场景以 [Vue 生产种子](../../../design/examples/topics/language-entries/frontend-vo-production-vue.md)及其正式回归为准。

最终异步 PERS 的 R 仍返回任务值，不把将来得到的标量充当同步返回。数学构造 `task.value(ref.task_local(N), $t_K)` 将本次入口调用中新鲜的任务身份与同文档中的已认证完成值定义配对。编号只在本次入口调用内有效；同一任务所有引用相等，不同任务及不同入口调用的身份互异。定义引用由内核登记产生，不能用未知函数名或字符串伪造。

T 的 `op: pure_value` 定义一个完整类型的纯数学函数。当前任务定义恰有形式参数 `observation.prefix:i64`，定义域为非负整数。前缀 0 是入口同步段结束；前缀 k>0 是本入口第 k 个就绪微任务完成后。该编号描述边界观察次序，不是输出的程序计数器。S 的状态记录 `order:[k]` 在此前缀发生；观察前缀 n 包含 order<=n 的效果。同步写入与后续写入不能合并成单一最终状态而丢失同步出口。

任务完成值为 `record<settled:bool,completion:Q>`。Q 复用已有带标签的完整完成记录：`exception=0` 时 `state` 是正常结果；非零标签与 `task_completion.throws` 声明的异常类型、载荷字段对应。`settled=false` 时整个 completion 投影没有任务结果含义，其字段仅满足纯记录完整类型；读取结果必须证明 settled=true，读取正常值还必须证明 exception=0，异常载荷必须证明对应标签。不能把未完成任务中占位字段误当作返回结果。

待登记的 deferred 观察顺序判别例：同步写 value=0，第一次 resolve(5) 赢得完成，第二次 resolve(7) 不改变它，但后续同步写 value=1 仍发生。等待者即使等到已完成任务也只入 FIFO 微任务队列；入口返回时等待者尚未完成，seen 尚未写，前缀 1 才 seen=1 且任务正常完成值为 5。返回身份保持同一任务。state=null 时入口同步抛错，无正常返回任务，也没有状态写入。

编译器内部会遍历已验证的续体并传播符号路径、共享字段值与词法捕获；这些队列、Behavior 节点和遍历状态均不输出到 T。该目标首先限制在闭合、本地任务的有限静态续体及标量结果。外部完成顺序、callback、循环 adoption、跨暂停 catch/scope、带循环的异步行为、需要事件域 E 的入口读取，以及尚未投影的 detached rejection/任务字段逃逸均明确拒绝。无法归约或预算耗尽不会被改成已完成、永久 pending 或正常 null。不能从此例推断任意任务最终完成或所有异步场景固定只需一个微任务。

闭合永久 Pending 只在本地任务没有完成、没有外部任务或回调来源，且所有已经就绪的编译队列已耗尽时产生。这是闭合方程的固定点，不是超时或未知路径的占位。将来应独立测试先完成再等待仍排队、同一同步前缀下两条互斥状态写入，以及任务身份先写共享字段再覆盖仍明确拒绝；历史写入不会因最终字段覆盖而绕过逃逸检查。

同一编译任务身份可以出现在互斥输入分支中，是否作为入口返回值必须逐分支判定。`已settled gate → waiter → 同步state.value写`在state=null时入口先抛错，但已排队waiter随后仍会拒绝；不能因为同一任务在state!=null分支会被返回，就删除null分支的未返回拒绝。该完整源输入在缺少 detached 完成值投影时必须明确拒绝；无heap的已settled等待另有完整正例，不能用它替换这个尚未支持的源场景。


## 本地任务 adoption

本地 `resolve(otherTask)` 和 async 函数返回任务采用同一身份的完成值传播。必须区分 `claimed` 与 `settled`：第一次 resolve(otherTask) 立即占用解决权，即使 otherTask 仍 Pending，随后 resolve(7) 或 reject 也不能覆盖；不能仅用“是否已经完成”来判断第二次调用是否有效。

编译器内部保留 Promise 的 thenable assimilation job 与后续兑现 reaction 两步，再唤醒等待该任务的续体；这些内部步骤只决定观察前缀中的纯方程和 S 次序，均不会作为任务指令输出。闭合源 `resolveAdopted(gate); resolveAdopted(7); ... resolveGate(5)` 中，返回 waiter 的完整定义为前缀 0、1、2 尚未完成，前缀 3 起正常结果 5；同步字段 value=9 位于前缀 0，后续 seen=9 位于前缀 3。task_adopt 形式得到同样的方程。拒绝路径保留原异常标签和完整载荷，不伪造正常结果。

未来应以独立完整文档核对首次解决权、async 返回采用、拒绝传播。独立 Node 对照只能在原 native then 的调用与 reaction 内同步记录标签，再记录源 waiter 的恢复：它不插入额外 await/queueMicrotask 探针，故不会把测试自身微任务混入三个源 job 的坐标。最后等待结果的观察者不计入这些标签。

通过 await/adoption 接续的源任务拒绝会转移到其后继任务；只有已认证接续关系才能消除内部源拒绝的独立输出义务。未返回且没有接续的拒绝仍明确拒绝，跨分支反例必须保留。自采用或循环采用目前明确 unsupported，不用永久 Pending 代替其尚未实现的完整语义。外部任务参数的 `originalAdopt` 仍是独立待完成目标，不能用本地新例冒称其已通过。
