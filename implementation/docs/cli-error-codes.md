# CLI 错误码

由唯一[诊断目录](../tools/diagnostics/catalog.tsv)生成，可运行 `implementation/tools/normalizer errors` 查看。设计编号见[设计章节](../../design/README.md#设计章节)；工程接口不是业务语义。

| 错误码 | 类别 | 阶段 | 怎么读 | 下一步 | 依据 |
| --- | --- | --- | --- | --- | --- |
| `NORM-USAGE` | USAGE | dispatch | 命令、选项或参数不正确 | 运行 normalizer help，先核对模式、参数顺序和入口引号 | 工程接口 |
| `NORM-TOOLCHAIN` | ENVIRONMENT | bootstrap | CLI 工具链或依赖准备失败 | 核对 Cargo、Node、npm、锁文件和缓存；修好环境后重试 | 工程接口 |
| `NORM-BOOTSTRAP-IO` | ENVIRONMENT | bootstrap | CLI 无法建立本次输出缓冲 | 检查临时目录权限与可用空间 | 工程接口 |
| `NORM-INPUT-IO` | INPUT | load | 无法访问分析工程中的输入 | 核对工程根目录、文件实际路径、权限和编码 | AI-11 |
| `NORM-TEMP-IO` | ENVIRONMENT | adapter | 无法准备内部临时分析工程 | 检查 TMPDIR、临时目录权限和可用空间 | 工程接口 |
| `NORM-RESULT-TRANSPORT` | ENVIRONMENT | transport | 结果通道或内核子进程传输失败 | 核对输出接收端、进程状态和可用资源；不要读取不完整结果 | 工程接口 |
| `NORM-INTERRUPTED` | INTERRUPTED | transport | 分析进程被中断 | 核对信号或外部超时；只能在完整成功后读取结果 | 工程接口 |
| `NORM-ENGINE-FAILURE` | ENGINE_FAILURE | internal | 引擎或 CLI 出现未归类故障 | 提交版本、入口、最小工程和诊断；不要改写输入来掩盖问题 | CF-02 |
| `NORM-INPUT-MANIFEST` | INPUT | manifest | 依赖清单无法读取、解析或未满足零包依赖 | 查看清单位置和具体条目；按真实合同闭合交付物 | AI-11 |
| `NORM-SOURCE` | INPUT | load | 分析工程根目录无效 | 传分析工程根目录，入口仍使用工程相对路径 | AI-03 |
| `SOURCE-ALIAS` | INPUT | load | 不同声明路径指向同一个源码文件 | 清理歧义链接，保留源码与入口的唯一身份 | PERS-04 |
| `SOURCE-DECODE-FAILED` | INPUT | load | 源码不能按 UTF-8 解码 | 确认源码编码并按 UTF-8 提供 | AI-11 |
| `SOURCE-EMPTY` | INPUT | load | 输入目录中没有将要分析的源码 | 核实根目录、文件扩展名和准备好的交付物 | AI-03 |
| `SOURCE-OUTSIDE-ROOT` | INPUT | load | 实际源码路径越出工程根目录 | 将真实交付源码放入分析工程，不用越界链接替代 | AI-01 |
| `SOURCE-PATH-FAILED` | INPUT | load | 无法确定分析源码的实际路径 | 核对路径存在性、链接目标和访问权限 | AI-11 |
| `SOURCE-READ-FAILED` | INPUT | load | 源码不能作为普通文件读取 | 核对权限、文件类型和路径 | AI-11 |
| `SOURCE-SYMLINK-DIRECTORY` | INPUT | load | 不接收目录软链接 | 交付明确封闭的项目目录，保持来源身份 | AI-01 |
| `SOURCE-WALK-FAILED` | INPUT | load | 遍历源码目录失败 | 检查目录权限和目录结构 | AI-11 |
| `PIPELINE-IO` | INPUT | load | 源码加载产生 I/O 错误 | 查看具体读入失败，避免误作业务异常 | AI-11 |
| `PIPELINE-RESOLVE` | PREPROCESS_REQUIRED | admission | 源码解析或链接阶段未完成 | 查看实际解析或链接原因并核对原工程 | AI-11 |
| `SOURCE-PARSE-FAILED` | PREPROCESS_REQUIRED | admission | 源码解析或静态语法检查未通过 | 按源码位置检查语法；合法输入被误拒则提交最小复现 | AI-11 |
| `SOURCE-LINK-FAILED` | PREPROCESS_REQUIRED | admission | 源码绑定或链接不闭合 | 查实际导入、类型和声明来源；不猜同名定义 | AI-01 |
| `NORM-IMPORT` | INPUT | admission | 导入或类型引用不符合闭合合同 | 沿绑定核查真实源码或正式内置定义；不把宿主方法自动认作已支持 | AI-01 |
| `NORM-ENTRY` | PREPROCESS_REQUIRED | admission | 入口不能按提供的定位发起调用 | 核实根目录、实际导出或 Java 接收者与签名 | AI-03 |
| `NORM-ENTRY-ASSEMBLY` | PREPROCESS_REQUIRED | admission | 入口装配或构造事实不足 | 恢复真实接收实例及实际装配，不创建替代对象 | AI-03 |
| `NORM-ENTRY-STATE` | PREPROCESS_REQUIRED | admission | 入口已有状态的声明或身份不能解析 | 核对原对象及已有状态标记，不挪进形参 | PERS-01 |
| `NORM-STATE-DECLARATION` | PREPROCESS_REQUIRED | admission | 状态声明不满足可追溯合同 | 核对真实状态根、字段和类型 | PERS-01 |
| `NORM-SOURCE-NAME` | PREPROCESS_REQUIRED | admission | 源码名称或绑定不符合可识别的入口合同 | 核对诊断指出的绑定和可见范围 | AI-03 |
| `NORM-SOURCE-TYPE` | PREPROCESS_REQUIRED | admission | 源码类型、值域或赋值没有通过静态检查 | 保留实际类型与分支守卫；不要用 any 或断言掩盖 | AI-04 |
| `NORM-SOURCE-ANNOTATION` | PREPROCESS_REQUIRED | admission | 源码标记不适用或合同不完整 | 对照标记的实际声明、身份和适用位置 | AI-11 |
| `NORM-DUPLICATE-CLASS` | PREPROCESS_REQUIRED | admission | 工程内类型声明冲突 | 核对重复类型的源码与真实绑定 | AI-01 |
| `NORM-AMBIGUOUS-OWNER` | PREPROCESS_REQUIRED | admission | 成员声明所有者不唯一 | 保留实际接收者和源码所有者，不凭同名挑选 | PERS-04 |
| `NORM-CLASS-SHAPE` | PREPROCESS_REQUIRED | admission | 当前源码对象形状无法分析 | 核对源码类型和实现范围；合法形状误拒请提交复现 | SE-02 |
| `NORM-FIELD-DECLARATION` | PREPROCESS_REQUIRED | admission | 字段声明缺失、冲突或不符合类型规则 | 核对实际字段声明及初始化 | AI-11 |
| `NORM-METHOD-SHAPE` | PREPROCESS_REQUIRED | admission | 方法的声明形状不满足当前分析合同 | 保留源码；核对入口、签名和当前适配能力 | AI-03 |
| `NORM-STATIC-INITIALIZER` | PREPROCESS_REQUIRED | admission | 静态初始化的行为无法闭合 | 恢复真实初始化，不能略过具有业务影响的求值 | SE-01 |
| `NORM-MISSING-BODY` | PREPROCESS_REQUIRED | admission | 选定方法或其声明缺少要求的实现 | 提供可核实源码；外部调用仅按已知合同声明 | AI-05 |
| `NORM-OVERLOAD` | PREPROCESS_REQUIRED | admission | 重载无法按输入定位唯一匹配 | 使用源码参数类型明确实际重载 | PERS-04 |
| `NORM-OVERLOADED-CALL` | PREPROCESS_REQUIRED | admission | 调用的重载没有获得唯一绑定 | 核对真实实参与静态类型，不能凭名字推断 | PERS-04 |
| `NORM-UNBOUND-CALL` | PREPROCESS_REQUIRED | admission | 不能证明调用绑定 | 查找真实声明或已确认外部合同，不编造方法体 | AI-05 |
| `NORM-CALL-TARGET` | PREPROCESS_REQUIRED | admission | 调用目标不符合已确定的绑定合同 | 根据原接收者、方法和实际参数定位 | AI-03 |
| `NORM-INTRINSIC-SHADOW` | PREPROCESS_REQUIRED | admission | 源码绑定遮蔽或冒用了官方内置操作 | 核对官方来源，不把同名对象当作内置操作 | AI-02 |
| `NORM-EXTERNAL-TYPE` | PREPROCESS_REQUIRED | admission | 外部端口的请求或回执类型合同不可解析 | 核实外部实参和回执的真实合同 | AI-08 |
| `NORM-EXTERNAL-FIELD` | PREPROCESS_REQUIRED | admission | 端口字段不是可证的外部边界 | 标记实际接收对象，不凭字段名认定外部 | AI-08 |
| `NORM-EXTERNAL-METHOD` | PREPROCESS_REQUIRED | admission | 外部方法的可调用合同不完整 | 核实方法签名、来源和实参 | AI-08 |
| `NORM-EXTERNAL-EXCEPTION` | PREPROCESS_REQUIRED | admission | 外部失败类型或标记不合合同 | 只声明已有依据的失败，不猜测或吞掉异常 | AI-09 |
| `NORM-EXTERNAL-POLL` | PREPROCESS_REQUIRED | execution | 外部轮询缺少可分析的边界或等价规则 | 保留调用、停止条件和失败；补可证明边界或报告缺口 | AI-05 |
| `NORM-EXCEPTION-BASE` | PREPROCESS_REQUIRED | admission | 异常基类未获得可解析的声明 | 核对异常继承和真实捕获范围 | PERS-03 |
| `NORM-EXCEPTION-CYCLE` | PREPROCESS_REQUIRED | admission | 异常继承包含循环 | 修正声明，不改变原来的捕获顺序 | AI-11 |
| `NORM-EXCEPTION-DECLARATION` | PREPROCESS_REQUIRED | admission | 异常声明不完整或形状非法 | 提供可解析、符合原合同的类型声明 | PERS-03 |
| `NORM-EXCEPTION-SHADOW` | PREPROCESS_REQUIRED | admission | 异常名称被另一个声明遮蔽 | 核对声明身份，不凭异常名推断相同 | PERS-04 |
| `NORM-CATCH-TYPE` | PREPROCESS_REQUIRED | admission | catch 条件类型不满足来源合同 | 核对原异常类型和可捕获域 | PERS-03 |
| `NORM-CATCH-ORDER` | PREPROCESS_REQUIRED | admission | catch 顺序或可达关系不合法 | 按源语言规则保持实际命中优先级 | PERS-07 |
| `NORM-CATCH-BODY` | PREPROCESS_REQUIRED | admission | catch 内逻辑未能按源码分析 | 保留处理器及异常前发生的动作 | SE-02 |
| `NORM-EXPLICIT-THROW` | PREPROCESS_REQUIRED | admission | 抛出语句的声明或表达式无法处理 | 核对原异常实际值与类型；不要把业务抛出一律改为 CLI 错误 | PERS-03 |
| `NORM-LOOP` | PREPROCESS_REQUIRED | execution | 循环缺少可证明的边界或等价执行形式 | 保留原循环与效果，补证明或报告缺口，不截断 | AI-05 |
| `NORM-RECURSION` | PREPROCESS_REQUIRED | execution | 递归或调用路径没有可证明的结束条件 | 保留源码并查明递归边界，不猜部分结果 | SE-02 |
| `NORM-RESOURCE-CLOSURE` | PREPROCESS_REQUIRED | execution | 源码的资源或关闭行为未得到等价表示 | 保留清理、次序和失败，不直接省略 | AI-06 |
| `NORM-NUMBER-LITERAL` | PREPROCESS_REQUIRED | admission | 指定数值字面量不能按当前输入合同接收 | 检查原始数值和精确类型，不用近似值替代 | BF-02 |
| `NORM-DECIMAL-OP` | PREPROCESS_REQUIRED | execution | 精确小数操作或舍入条件缺少定义 | 核对实际舍入模式和输入合同，不引入隐式舍入 | BF-02 |
| `NORM-EXPRESSION` | PREPROCESS_REQUIRED | execution | 表达式未得到完整的源码执行解释 | 保留输入并明确指出不支持的操作或实现缺口 | SE-02 |
| `NORM-STATEMENT` | PREPROCESS_REQUIRED | execution | 语句未得到完整的源码执行解释 | 保留该语句及其实际效果，报告实现缺口 | SE-02 |
| `NORM-RECORD-ACCESS` | PREPROCESS_REQUIRED | execution | 记录字段取值来源或有效域无法证明 | 核对实际字段及其访问守卫 | PERS-01 |
| `NORM-RECORD-CONSTRUCTOR` | PREPROCESS_REQUIRED | execution | 记录构造无法保持实际字段与求值 | 核对真实构造和字段绑定 | SE-01 |
| `NORM-UNSUPPORTED-LOCAL-IR` | PREPROCESS_REQUIRED | adapter | 扩展语句未获得已定义的解释 | 核对扩展语法、位置和绑定；不得把指令当普通注释 | RA-02 |
| `NORM-OUT-OF-SOURCE` | ABORT | execution | 走到的调用无源码、内置定义或合法外部合同 | 查真实绑定和执行路径；补实证、合法合同或保留缺口，不交部分 PERS | AI-01,AI-05,SE-02 |
| `NORM-ANALYSIS-INCOMPLETE` | ANALYSIS_INCOMPLETE | analysis | 分析或证明在预算内未完成 | 记录预算、入口和最小复现；不要截断输入或发布候选结果 | SE-02,CF-03 |
| `NORM-CYCLIC-CONTENT-EXIT` | ABORT | output | 返回内容存在当前 PERS 无法表达的循环 | 保留身份和循环，交输出合同负责人判断；不要拆环改语义 | PERS-03,CF-06 |
| `NORM-BUSINESS-EXPRESSION` | OUTPUT_UNSUPPORTED | output | 业务表达式不能安全交付为规范 PERS | 保留原语义，核实规范表示或提交复现 | CF-06 |
| `NORM-BUSINESS-IDENTITY` | OUTPUT_UNSUPPORTED | output | PERS 的业务绑定身份冲突 | 核对真实来源、取得身份和命名空间 | PERS-04,PERS-08 |
| `NORM-BUSINESS-TYPE` | OUTPUT_UNSUPPORTED | output | 业务结果类型缺少受支持的规范表示 | 核对业务类型合同，不擅自替换为其他值域 | BF-03 |
| `NORM-INVARIANT` | ENGINE_FAILURE | internal | 内核结构不满足自身不变量 | 提交诊断和最小工程；不可继续下游处理 | CF-02 |
| `NORM-PROJECT` | PREPROCESS_REQUIRED | carrier | 内核输入装载或验证失败 | 查详细原因和原入口；若内部载体有误则报适配器缺陷，不修改临时 Java | AI-11 |
| `NORM-STANDALONE-CLOSURE` | ENGINE_FAILURE | carrier | 内部独立模块调用信封校验失败 | 提交原 TypeScript 输入和诊断，勿改内部信封 | 工程接口 |
| `NORM-TS-SYNTAX` | PREPROCESS_REQUIRED | admission | TypeScript 解析器拒绝输入的语法 | 按位置和 TS 编号核对原句；合法输入被误拒时保留源码并报告 | AI-11 |
| `NORM-TS-TYPECHECK` | PREPROCESS_REQUIRED | admission | TypeScript 静态检查未通过 | 按位置和 TS 编号核对类型、闭合声明或赋值；不要放宽检查或加载宿主库 | AI-01,AI-11 |
| `NORM-TS-ANY` | PREPROCESS_REQUIRED | admission | 强类型准入拒绝显式、隐式或解析出的 any | 追到真实类型来源；可按原语义保留 unknown 及其检查，不凭断言消除 any | AI-04 |
| `NORM-TS-CHECK-DIRECTIVE` | PREPROCESS_REQUIRED | admission | 源码指令试图压制检查或改变封闭的声明集合 | 恢复固定准入检查；按原合同核对真实声明，不引入宿主类型库 | AI-01,AI-11 |
| `NORM-TS-MODULE` | ADAPTER | adapter | 独立 TypeScript 的入口或当前适配失败 | 看具体绑定或能力原因；合规代码被误拒时报告缺口 | AI-03,AI-05 |
| `NORM-SCENARIO-INPUT` | ADAPTER | adapter | 场景入口、标记或支持范围未满足 | 核对准备好的 TS 和选定场景；投影必须同步无参 | AI-10,AI-11 |
| `NORM-ADAPTER-UNSUPPORTED` | ADAPTER | adapter | 适配阶段不能为此结构建立内部执行表示 | 保留合法输入并报告实现缺口；不称为已执行到的无源码调用 | AI-05,SE-02 |
