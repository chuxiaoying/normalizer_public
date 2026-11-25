# 场景规范源码的跨语言共享合同

本文定义理想态下 AI 预处理结果与多语言 parser 的关系。目标是让 Java、TypeScript 和其他语言在进入统一 IR 前使用同一套可观察语义，而不是让每种语言各自发明一套扩展含义。

## 先对具体值执行，再做符号执行

规范源码中的常量是内核直接接收的业务值，不是需要预处理消掉的语法。支持的类型首先要能执行具体值，其次才验证未知输入上的符号表达式。AI 不得因为值已知就预算业务结果，也不得把源码常量改成入参来绕过执行缺口。

文本直接保留引号及字符内容。比如 `NormalizedText.trim("  item  ")` 的常量由内核求值为 `"item"`；Java 的 null、空文本、Unicode 空白和非断行空格不会被混同。普通 Java 字符串的标准单字符与八进制转义精确保留；输入层未处理的 Java Unicode 源文件转义须先做无损字符转换，不能悄悄删除反斜杠。参见 [Java 词法合同](https://docs.oracle.com/javase/specs/jls/se21/html/jls-3.html#jls-3.10.7)。单个 Java 字符常量在数值表达式里使用它的无符号 UTF-16 码元，不把补充字符当作一个 Java char。数值域和外部观察使用各自合同，不能凭近似、默认值或忽略效果让常量用例通过。

Java 十进制的最小 long 用一元负号与相应边界字面量表示；不接受同一正数写法，也不能把下划线开头的同形标识符当成字面量。该边界字面量按[整数词法合同](https://docs.oracle.com/javase/specs/jls/se21/html/jls-3.html#jls-3.10.1)识别，再在精确业务整数域计算，不把宿主字面量的位宽当成运算结果上限。Java 二进制、八进制和十六进制整数字面量先根据有无 long 后缀按 32 或 64 位解释符号，不自动扩大过宽字面量；其后的业务运算仍不回绕。

Java 的 `+` 用于两个已知文本时，按从左到右的源码顺序连接；有文本操作数时，具体的 `null` 按 Java 的文本转换规则成为 `"null"`，不当作空串。这条具体值规则不声明未知对象的隐式转换或一般符号拼接支持。

## 核心决定

AI 预处理后的扩展语义是跨语言共享的。Java parser、TypeScript parser 和其他语言 parser 都必须理解同一套扩展语义合同，并把同一语义降低为同一种统一 IR。

宿主语言的原生语法仍由各自 parser 负责。Java 的 class、static、List 和注解，TypeScript 的 function、Promise、模板和组件语法，都在语言 parser 内部被消化。它们不会进入内核，也不会形成不同的 PERS 定义。

```text
Java / TypeScript / 其他原始代码
  → 各语言 AI 预处理
  → 带共享扩展语义的场景规范源码
  → 各语言 parser + 共享扩展语义 parser
  → 统一 IR
  → 归一化内核
  → PERS
```

扩展语义的“语义”和“结构”共享。宿主语言中的分号、括号、变量声明和函数外壳可以保持各自风格，只要它们都能解析为同一个扩展节点和同一个统一 IR 结构。

## 两个语法层

### 宿主语言层

宿主语言层负责识别语言本身的代码结构：

- Java 的类、方法、参数、局部变量和控制流。
- TypeScript 的函数、对象、数组、异步表达式和模块。
- 其他语言的函数、对象、集合和控制结构。

这一层可以存在语言差异。它的职责是把语言表达转换为 parser 内部的宿主 AST。

### 共享扩展层

共享扩展层负责表达跨语言都能观察到的业务语义：

- `external` 调用和完成值绑定。
- 入口状态根、事件参数和入口完成类型。
- 关系根、关系字段、谓词、投影和聚合。
- 集合过滤、映射、连接、分组、排序、并集、交集和差集。
- CASE-WHEN 和纯条件选择。
- 有序状态写入、VO 读取、VO 写入和其他可观察副作用。
- 异常完成、失败清理和边界顺序。
- 入口、参数和观察范围。

共享扩展层是同一份语义合同。Java parser 和 TypeScript parser 可以在宿主语言外壳中使用同一组扩展结构，也可以把语言原生结构识别后直接降低到同一扩展节点。

### 入口状态根与事件参数

前端 VO 生产入口需要区分事件参数和已有页面状态。理想场景规范源码使用 `entry_state` 声明状态根，再用普通入口参数声明事件输入：

```text
entry_state vo: ArrearsVo

entry onLoad(today: int) -> void {
    accountId := vo.query.accountId
    vo.write(loading, true)
    ...
}
```

`today` 是入口参数，进入统一 IR 的 `P`。`vo` 是入口已有的可变状态对象，不作为普通 `P`；实际读取的 `vo.query.accountId` 进入 `E`，对 `vo` 的字段写入进入有序 `S`。如果入口没有事件参数，`P` 可以为空。源码层的 `void` 在内核输入合同中可以映射为内部 `unit` 类型，但最终 PERS 沿用公共的无返回值表示，不把这个内部名称泄漏给使用者。把 `vo` 作为 `onLoad(vo, today)` 的普通形参只能在场景合同明确它就是业务输入时成立，不能用一个普通形参掩盖 VO 状态根的身份。

共享场景源码可以声明 external 的返回类型和异常类型：

```text
external profileClient.fetchProfile(accountId: int)
    -> Profile throws ProfileFetchError
```

这条声明只建立边界合同，不实现调用。`ProfileFetchError` 必须来自原始源码或宿主 client 的正式类型合同。没有异常来源时，预处理不能凭空生成异常类型，也不能把 callee 名称当成异常类型。

## 同一扩展语义的跨语言形态

下面两个片段的宿主语言外壳不同，扩展语义相同：

```java
int calculate(int accountId, int today) {
    Profile profile = external.profileClient.fetchProfile(accountId);
    Bills openBills = from bill as b
        where b.accountId == accountId
          and b.status == "OPEN";
    return sum from openBills as row
        where row.dueDate <= today
        yield row.amount;
}
```

```typescript
function calculate(accountId: number, today: number): number {
  const profile = external.profileClient.fetchProfile(accountId)
  const openBills = from bill as b
    where b.accountId == accountId
      and b.status == "OPEN"
  return sum from openBills as row
    where row.dueDate <= today
    yield row.amount
}
```

两个 parser 必须得到相同的语义：

```text
入口 calculate(accountId, today)
→ external.profileClient.fetchProfile(accountId)
→ 只读关系 bill
→ accountId/status 谓词
→ dueDate 谓词
→ amount 投影
→ sum
```

它们最终产生同一统一 IR 和同一 PERS。PERS 不携带 Java 或 TypeScript 的语言标签，也不因为宿主语言不同而产生两套业务结果格式。

## parser 的职责边界

每种语言 parser 都有三项职责：

1. 识别宿主语言结构，并完成语言绑定。
2. 识别或接收共享扩展结构，并校验扩展节点的参数、作用域和类型。
3. 把宿主语义和共享扩展语义共同降低为统一 IR。

parser 不得把同一个扩展概念按语言改名后送入内核。例如：

```text
Java parser: java_rpc_call
TypeScript parser: ts_fetch
```

这两个名字不能成为两种内核语义。它们都应降低为同一个 `external` 调用结构，并保留 endpoint、参数、完成值、异常和顺序。

同理，Java Stream 的 `filter`、TypeScript Array 的 `filter` 和场景规范源码的关系过滤，都必须在语义等价时降低为同一个关系过滤节点。方法名属于宿主语言表面，过滤语义属于共享扩展层。

上文的 `external.profileClient.fetchProfile` 是共享边界语义的示意。首批规范源码在 Java 字段声明上使用 `@External`，在前端端口声明上使用 `@external`，调用处保留原生的 `profileClient.fetchProfile(...)`。解析器根据声明绑定端口并保留调用路径。完整的标记与注释验收范围见[业务 PERS 设计](business-pers.md)。如果预处理确实把源码调用映射到稳定的服务端点，映射必须同时保留原始 callee、目标端点和可复核的来源依据；没有依据时不得生成新的命名空间。

预处理结果和最终 PERS 使用同一套 P/E 来源定义。`P` 用入口签名、参数位置或 props 名称逐项定义输入，`E` 用状态根、字段路径、数据库表命名空间、表名和列名定义已有来源。PERS 的公共 YAML 形态见[PERS 符号定义与源码绑定](pers-source-linked.md)。S 中的 external 调用显式绑定 `result` 完成值，不需要在 E 区重复声明。

## AI 预处理的职责边界

AI 预处理可以因语言不同而不同。Java 预处理需要消化 Bean 装配、Mapper 和注解，TypeScript 预处理需要消化组件、模板、双向绑定和宿主调度器。它们的输出必须遵守同一个共享扩展语义合同：

- 入口必须明确。
- external 必须明确。
- 关系来源必须明确。
- 状态、对象身份、异常和顺序必须保留。
- 未闭合的行为必须显式暴露。
- 业务语义不能依赖 parser 之外的配置文件才能成立。

AI 预处理可以改写宿主代码的组织方式，但不能为某种语言制造内核专属旁路。它的目标是把原始语言代码改写成共享扩展层可以理解的场景规范源码。

## 统一 IR 的不变量

同一业务语义从不同语言进入时，统一 IR 必须满足以下不变量：

- 同一 external 调用拥有同一边界结构。
- 同一关系根、列、谓词、投影和聚合拥有同一关系结构。
- 同一副作用拥有同一顺序、完成和异常结构。
- 同一对象字段拥有同一身份和读取关系。
- 同一入口拥有同一参数和观察范围。
- 语言特有的类名、方法名、模板名和框架名不进入内核语义。

如果某语言 parser 不能完成这种降低，场景应在进入内核前失败并说明缺失语义。它不能把未知调用变成默认 external、把未知集合变成空集合，或把未知字段变成 null。

## PERS 的统一性

PERS 是共享扩展语义和统一 IR 的最终业务表达。它不区分来源语言：

- Java 和 TypeScript 的纯计算都使用同样的 `P`、`R` 和 CASE 形态。
- Java 和 TypeScript 的 external 都使用同样的 `S` 和 `$e_n` 绑定形态。
- Java SQL 和前端数组关系都使用同样的关系表达式和集合代数。
- VO 生产和 VO 消费的入口差异在进入统一 IR 前被适配，PERS 只表达统一后的入口、边界、状态和结果。

这样，归一化内核只需要理解一份统一语义，PERS 也只需要维护一份结果合同。

## 一维业务整数数组的局部关系

直接 Java 源入口的 `int[]` 等整数数组按已绑定、非空引用的 `number[]` 输入处理；元素属于精确业务整数域，空数组是合法值。固定宽度转换和可空数组引用不由此规则推断。类型化工程入口原有的固定宽度载体合同不变。

- `sum from values as v yield expression` 使用相同关系绑定和逐元素表达式；空数组贡献零。
- `first from values[k:] else fallback` 在长度大于非负索引 `k` 时取该元素，否则返回显式默认值。省略切片就是 `k=0`。它不是无序表上的首行选择。
- `count [distinct] from values as v [where predicate]` 在过滤后计数。`where true` 保留所有项，`where false` 不保留任何项，因此计数为零。普通计数保留重复项；去重计数只合并元素值相同的项。SQL 表上的可空列不沿用数组元素非空的证明。
- `reduce values from initial as (acc, v) yield acc + term` 和相应的减法可归约为初值加减逐元素求和，前提是 term 不依赖 acc，且初值和每个 term 都是闭合的精确整数值。这不是任意递推或浮点、定宽运算的交换规则。

### 适配器的交接格式

`module` 与 `scenario` 把各自读到的规范源码转为同一份有类型的内核输入。适配器生成的内部工程数据为 JSON，内核经过同一加载和类型校验路径。内部格式不提供第二种面向用户的规范输入入口，最终 PERS 仍是 YAML。

### Java 声明与无源码调用

源码树里的 Java 声明先经过语法检查，即使声明的函数不会被入口调用。保留字不能作方法名；例如 `return` 非法，`returnValue` 合法。语法错误必须指向原文件及非法声明，不可以删除声明来通过入口检查。 参数同样逐项解析；不把缺名字、重复名字、尾逗号或非末尾可变参数静默剔除成另一份签名。 导入声明必须完整，固定源码合同使用显式类型导入，不推测通配或静态导入带入的绑定。 语法要求的分号不得省略；不把漏写分号的字段或语句当成完整源码。

Java 原始的 Unicode 转义先于注释和记号识别（见 [JLS §3.3](https://docs.oracle.com/javase/specs/jls/se25/html/jls-3.html#jls-3.3)）。当前规范源码要求预处理先展开符合条件的原始转义；直接入口在扫描注释之前拒绝它，绝不把可能改变语义的文本当作普通注释。字符串中不构成原始转义的双反斜杠序列仍按字符串字面量处理。 LF、CR 和 CRLF 都能结束行注释；CRLF 在诊断中只记一行（见 [JLS §3.4](https://docs.oracle.com/javase/specs/jls/se25/html/jls-3.html#jls-3.4)）。

在当前 Java 源码合同中，受支持的注解只作标记，不接收参数。带空括号或只含注释的括号仍是空标记；孤立逗号及其他实际内容不得被剔除为“无参数”。

合法的 `native` 声明本身不是调用，也不是外部合同。未执行到它时，不能凭声明产生 S 或逼迫 AI 补造函数体。若模拟执行确实到达没有已声明合法 external 合同的无源码调用，内核按无源码边界 Panic，不产生 PERS。常量不可达分支不执行其中的调用。见[非法声明与未调用对照](../../design/examples/series-2/java-invalid-declaration.md)。

### 数组原地写回和别名

数组引用来自一个已声明的 P 或 EntryState E 根；局部别名共享这个根，P/E 声明的是入口快照。当前支持无副作用地址表达式的非负整数字面量下标。对简单赋值先求右值；若右值正常返回且下标越界，抛出 `java.lang.ArrayIndexOutOfBoundsException`，不产生这一次写回，不撤销此前完成的操作。复合赋值先经下标读取，失败时不求后续右值。每次完成的写回按顺序保留在 S；读下标和受支持的逐元素精确整数循环看到最后一次适用的写入。原地写回目前只接受一个对外数组根，不能假定两个独立入参或状态根互不别名。其他局部关系查询不能在写后偷偷读入口快照。标量数组聚合使用同一种输出 `SELECT value FROM values`，不因存在写回而改变隐式元素绑定。

### TypeScript 局部元组和 rest

独立 `.ts` 通过与 `.js` 相同的 `module` 入口，以原始 TypeScript AST 和类型检查为证据；不执行模块或抹去元组声明。可分析的局部元组由可见 helper 返回文字元组，保留各条件分支和每个槽的位置。必占槽持有 `undefined` 时仍占一个位置；缺失可选槽和显式 `undefined` 在读取中可得到相同值，但布局和 rest 个数不能提前合并。字符串 `"undefined"` 不是未定义值。当前局部元组只允许闭合纯观察：判断未定义、对可选记录访问已检查的 Boolean 标志、从解构位置求 rest 长度。内核求值各条路径，局部元组不作为 PERS 出口或外部记录。见[回执元组种子](../../design/examples/series-2/typescript-tuple-slots.md)。

### TypeScript 局部监听和同步调用

可见、未逃逸的本地回调可以通过有类型的注册方法保存到局部槽。赋值本身没有 S；只在源码实际调用回调时展开其可见函数体。局部返回值未被入口使用时，不改变入口的返回类型。当前闭合模块接受声明为 `@external` 的无实参、正常完成的 `void` 同步操作，不把同步调用仿造成任务等待。每次实际调用使用独立回执并记录原顺序和守卫。未标记 external 的声明不授予边界合同；实际调用则在共享内核 Panic，常量不可达的调用不执行。见[登记与派发种子](../../design/examples/series-2/frontend-deferred-listener.md)。

### 局部栈的逆序匹配

闭合的 Java 局部栈可以先按输入顺序逐项 push，再逐项读取另一个输入的任务编号，空栈或栈顶编号不符就抛出同一种无载荷异常，成功才 pop。预处理必须交付三段连贯的局部语义（push、带前置检查的 pop、观察是否为空），保留共用栈和本来的数据来源，不能交付已经计算好的真假。当前入口只接收未写入的同一名义记录类型数组，并比较该类型已声明的直接整数字段；不将此规则扩大到任意栈或有外部效果的循环。

栈不逃逸，两个失败检查的类型相同，故定位到共同长度以内的第一个不一致位置或关闭次数过多都产生相同异常。位置对照按向前和向后两个方向读取，不能改成集合相等。长度相同且没有异常，最终栈才为空；两组长度相同不代表一定成功。见[反向关闭种子](../../design/examples/series-2/java-lifo-release.md)。

### 有序表首值与 SQL NULL

已声明类型的表行可先通过 `from ... yield { ... }` 绑定，再以 `first ... order by ... yield value else fallback` 选择非空数值列的首个值。排序项按声明顺序保存；默认值只补无匹配行，不能替换存在行的 NULL 值。关系 `IS NULL` 和 `IS NOT NULL` 作用于已绑定的列。`count from` 计数匹配行，包括可空列为 NULL 的行，其结果与对非空常量 1 求和并为无匹配行补零等价。

### 类型化表的组计数与内连接计数

先用带行类型的 `from ... yield {...}` 声明各表。换分组键或连接键不改变表合同。当前 `count groups from rows as r by r.k1, r.k2` 只接受未筛选的表绑定，以声明列的完整键元组划分组，只为每个不同元组计数一次；不能分别去重各列。没有行时不存在任何组，返回 0。

`count from rows1 as a join rows2 as b on ... [where ...]` 统计符合条件的行对，保留重复匹配的乘数；不为未匹配的行产生占位结果。当前入口要求源绑定都是未经筛选的、列名与物理列一致的非空列声明。ON 是依赖左右两行的布尔比较，WHERE 可用合取逐项过滤任一行或入口参数。不声明的列不会从比较式里反推。内核用相关的精确整数聚合执行，PERS 用 JOIN 形式表达相同的行对集合。

### 源码中的独立矩形计数和有界迭代

两个或更多从零开始、每次加一的原生整型循环，只有当各层上界都是互不依赖的已绑定整数、唯一更新是给独立的局部累加器加常量、且没有其他可观察行为时，才可按各层长度的乘积归约。任一层为空则保留入口初值。原生 `<` 和 `<=` 分别对应排除和包含端点。条件是合取的计数 CASE 保持多行显示。

`fold [lower, upper) as i when i < skip continue when i > stop break yield i` 使用独立的索引和已初始化的累加器。此窄形式要求下界和两个阈值为整数常量，上界也可为已绑定整数。只有已到达且未被 continue 跳过、也未越过 break 的索引进入累加。内核保留每个已提交前缀，不会截掉已接受的元素；若可证明前缀超过当前符号展开预算，则报告不支持，而不是伪造截断结果。

### Java finally 的完成顺序

在已闭合的终端 try/catch 中，finally 在待返回值或待抛异常形成后执行。最终块正常结束时恢复原完成结果；若自身发生显式返回或已声明异常，则覆盖原完成。写入和调用仍按真实发生顺序进入 S，一次 finally 不能被复写成两次无条件效果。当前不接收带自动关闭资源的 try，也不接收单个 catch 中用竖线连接多个异常类型。解析时明确拒绝，不跳过资源初始化、关闭或任意异常类型。

## 二维业务整数数组的相关聚合

直接 Java 源入口的 `int[][]` 按非空引用、内层也是非空引用的 `number[][]` 输入处理；外层和任意内层都可以为空，内层长度无需相同。用 `sum from groups as grp yield sum from grp as item [where predicate] yield expression` 明确内外绑定。内层只消费当前外层元素，逐内层元素计算过滤与精确整数投影；空聚合贡献零，重复元素重复贡献。不得把内层重绑到整个外层输入，亦不得用 P 参数代替同名的行绑定。输出保留两层聚合，不以无数据合同的扁平化或伪造数据替代。

## 源码可见的接口分派与同步 void 边界

普通接口入参不提供实现，即使同文件列了几个实现类也不能从中猜一个。闭合分派使用实际构造的对象与当次条件，执行该对象自己的兼容方法体；条件分派当前要求各候选在其分支上正常返回，且没有向外部产生效果。未走到的实现不参与结果。若打开的是普通对象入参上的无源码方法，内核 Panic，不输出 PERS。已标记 `@External` 的同步 void 端口则给每次调用单独的完成回执，不能因没有返回值而删除调用或变更顺序。

## 精确十进制数值

直接 Java 源用 `java.math.BigDecimal` 值载体交付精确十进制，不推断 `double` 的二进制精度、溢出或显示语义。显式 `round(value, scale, HALF_UP)` 在正好一半时向远离零的方向；`HALF_EVEN` 在正好一半时选择末位为偶数的候选。位数由每次声明给出；复制数值不舍入。局部精确乘积先以任意精度整数系数计算，再舍入一次。PERS 的 `ROUND(value, scale, "MODE")` 保留模式；缺少模式时拒绝推断，不把某个种子的选择设成全局默认。输出是数值，不强制补齐文本小数位。

## CLI 入口的统一约定

正式入口与参数以[CLI 参考](../../usage/cli.md)为准。Java 源目录用 `source SOURCE_DIR Class.method`，独立 JS 用 `module PROJECT_ROOT ENTRY_FILE FUNCTION`，Vue 用 `scenario SCENE PROJECT_ROOT --entry ENTRY_FILE:FUNCTION --source ORIGINAL.vue`。不同入口都沿各自可达范围进入同一 PERS 合同；内部规范工程不是新的 CLI 入口。

## 实现时必须遵守的顺序

后续实现必须按以下顺序建设：

1. 先固定共享扩展语义及其统一 IR 映射。
2. 再为每种语言实现宿主语法解析和扩展结构接入。
3. 用同一业务案例分别验证 Java、TypeScript 或其他语言的 parser 输出是否相同。
4. 最后把统一 IR 交给同一个归一化内核和同一个 PERS 断言体系。

不能先为 Java 写一套扩展，再把它复制成 TypeScript 版本。这样会在进入内核前形成语义分叉，后续 PERS 也会出现语言专属变体。
