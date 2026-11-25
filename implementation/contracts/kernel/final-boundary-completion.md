# 同一次边界调用的正常与异常观察

有声明异常的同步端口，S 中一次 `boundary_call` 的 `e` 指向一个互斥且完备的完成值：`Return(value)` 或某个声明的 `Throw(exception, payload)`。同一调用不会为正常值与异常另造互不相关的观察身份。没有声明异常的端口保留原来的响应值格式。

例如返回 `text?`、可能抛出载荷类型为 `text?` 的 `Failure`，该 S 记录额外声明：

```yaml
completion:
  returns: text?
  throws:
    - exception: Failure
      payload_type: text?
```

`completion.is_throw($e_0, "Failure")` 是总布尔表达式；`completion.value($e_0)` 只在正常完成域定义；`completion.payload($e_0, "Failure")` 只在对应异常域定义。这些是已定义的完成值代数投影，不是延迟执行端口或解释程序。R 仍只描述正常返回域，throws 记录对应异常条件与载荷。后继调用只能在此前调用正常完成域发生；Catch 从实际抛出点承接已经发生的状态写入和调用记录。

在业务 PERS 中，`S.call.result` 绑定的 `$e_0` 在正常取值位置直接表示本次调用的返回值，包括数值、文本和对象。重试分支写作 `WHEN completion.is_return($e_0) THEN $e_0`。成功条件与失败条件仍然观察同一次调用的完成状态。

引用响应保留环境身份，可能与输入或其他响应别名，不能当成新分配对象。原 `reference_value` 合同规定响应引用的字段属于同一个环境堆；`heap_effect: none` 不改变该堆。字段 E 的 `at: entry` 指向这个堆基准，前序写入仍按别名条件覆盖；取得该字段观察的顺序在响应之后，条件保留调用正常域和引用非空域。未认证的任意表达式不能作为新增堆根。

当前生产认证范围为同步、无堆变更、无回调、闭合参数/返回类型及明确声明的异常载荷类型。`argument_fields`、异步和回调组合需要各自完整语义，不由本合同隐式接纳。

## 同步参数字段写回的目标合同

同步、无回调、不保留参数的 `argument_fields` 端口沿用一次 S 调用和一个完成值。该完成值包含正常或异常分支，并携带同一次调用已经完成的可选字段写回。调用前 checkpoint 和实参内容先按原模拟堆求值。写回只改变调用完成后的状态，异常也保留已经完成的写回。

下面是 QOBR 的声明示意。槽号从 0 开始，绑定该次 S.args 的位置。字段类型来自模拟堆声明。公开实参仍按各槽的业务内容投影。

```yaml
completion:
  returns: ref?
  throws:
    - exception: java.lang.RuntimeException
      payload_type: unit
  argument_fields:
    mutations:
      - slot: 0
        fields: {success: bool, code: 'ref?'}
      - slot: 1
        fields: {instId: 'ref?'}
      - slot: 2
        fields: {chargeInst: 'ref?'}
    aliases: []
    return_slots: [0]
```

`mutations` 只授权本次实参根的直接字段。写回地址由调用前的对象身份和字段确定。对同一对象同一字段，每次完成最多携带一个最终值，别名读共享该值。多槽同时授权该字段时，以第一个授权槽表示同一个更新。同根别名槽查询同一个更新的存在位和值。缺写时沿用调用前状态，写 null 时覆盖为 null。null 槽没有可写字段。

`aliases` 为可能同根的槽对提供调用前身份条件，每项使用 `slots: [lower, higher]` 与 `cond`。无可能共享的槽对可省略。原 QOBR 业务入口的 Resp、Req、Model 是不同 Java 类，且 Resp 由本地创建，三个根不共享。通用 Probe/Snapshot 的参数是结构型 Object，按实际输入身份表达别名条件。调用实参的内容相等不构成根别名证据。

正常引用返回是可空环境引用或 `return_slots` 中一个实际非空参数的原身份。后者按槽的原引用身份关联本次调用，局部新对象的身份不进入公开 R 或 S。多个授权槽共享返回对象时，每个槽的身份谓词都为真，首个匹配槽和其他匹配槽投影同一对象。所有授权槽的身份谓词都为假时使用环境引用分支。环境引用若与未列入返回槽的外部参数别名，后续字段读取仍先查同一份写回，再查调用前状态。

- `completion.argument_written(e, slot, "field")` 在调用完成后判断字段是否实际写回，适用于 Return 和 Throw。
- `completion.argument_value(e, slot, "field")` 只在对应存在位为真时取得原模拟字段值。
- `completion.returns_argument(e, slot)` 只在正常完成时判断该槽是否被返回。所有此类选择为 false 后，`completion.value(e)` 表示正常的可空环境引用。
- 调用自身的条件、实参、别名关系和 before checkpoint 都不能依赖该次完成值。对后续观察，先检查调用确实发生，再按各投影的有效域用 CASE 保护。
- 对已发生的外部对象写回，最终 S 保留原目标，字段值按出口合同投影。新建的本地 Resp 字段初始化和写入仅影响模拟状态、调用时实参或最终 R，无公开分配记录。对非空 String 字段，先确认写回存在且非空，再从该 String 的原堆内容投影完整 UTF-16 单元。

当前测试守卫认证的直接字段类型为 bool、i32、i64、text? 和 ref?。同一字段可经不同别名槽授权时，各槽声明相同的模拟字段类型。生产状态归约尚未接通该合同。
