# 使用 Normalizer

**第一次用？先跑通一个例子，再分析自己的项目。** Normalizer 从选定的程序入口提取业务逻辑，输出 PERS：描述入参、环境、正常返回、业务异常和可观察副作用的 YAML。它符号化描述真实调用和写入，不启动原应用，不向真实服务发请求。

**如果你是替用户使用 Normalizer 的 agent，从[用户意图撮合 Skill](skills/normalizer-preprocess/SKILL.md)开始。** 里面用用户原话示范怎样选入口、何时需要商量，以及拿到结果后怎样回答。

1. **先试用。** 按[新手入门](getting-started.md)运行 Java 或 TypeScript 的最小例子，也可以试前端 VO 生产和消费。所有示例已经准备好，不需要先做 AI 预处理。
2. **分析自己的项目。** 按[输入清单](kernel-input.md)准备分析工程，需要预处理的入口按[场景归一](../design/chapters/09-场景归一.md#规则区)查设计规则。
3. **运行并读结果。** 按[CLI 手册](cli.md)选择命令。成功时读标准输出中的 PERS，字段含义查[《PERS登场》](../design/chapters/07-PERS登场.md)；失败时从[诊断](cli.md#诊断)定位原因。

## 用户说「把这张欠费单页面归一化」

做哪一侧、选哪些入口，可以从页面里的查询、金额和还款按钮谈起。具体问法和对应做法见[欠费单页面的撮合示例](skills/normalizer-preprocess/SKILL.md#前端整页归一化)。

运行整个仓库的正式回归，见[测试区](../tests/README.md)。
