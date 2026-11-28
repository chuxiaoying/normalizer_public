# Hello World：TypeScript 模块入口

## 设计意图

检验闭合 TypeScript 模块的标量、只读向量及有序文本 Map 合同。相同纯算术可与 Java 共用规则；稀疏、可变以及不同元素位置有各自的观察合同；Map 中的重复键先按实际覆盖语义求值，不能随便挑其中一个。

本种子选择同一模块中唯一定义且显式导出的 `total` 函数；同名私有函数不能充当导出入口。这是[AI救赎入口重组](../../../chapters/04-AI救赎.md#入口函数重组)中的直接函数入口，用例不对导出对象方法另定限制。交付物是严格 TypeScript 分析副本。原始 JS/JSDoc 只属于预处理输入；`.js`、`.mjs` 和 `.d.ts` 不能直接执行。没有显式、隐式或从调用推断出的 `any`，也不能用注释绕过检查；未被入口调用的辅助函数同样检查。 同一函数重复形参由 TypeScript 编译检查在内核前拒绝；不能把两次绑定的名称悄悄合并。把闭合纯函数换成读取模块级可变状态的函数、依赖多次取值的生成器，或对业务数值做布尔取反，均不是原例的闭合数值合同；不从普通返回值假造生成器完成，也不把数字的真假猜成业务布尔。

## 输入原始代码

调用合同限定两个输入为业务数值，不包含字符串拼接或宿主隐式类型转换。 两个 P 都来自调用方传入的必填形参：默认值不能悄悄替调用方补 P，布尔值也不能通过 JS 加法隐式转成数字。

```javascript
/** @param {number} value @param {number} base @returns {number} */
export function total(value, base) { return value + base; }
```

## AI 预处理结果

```diff
diff --git a/src/total.js b/src/total.ts
rename from src/total.js
rename to src/total.ts
--- a/src/total.js
+++ b/src/total.ts
@@ -1,2 +1 @@
-/** @param {number} value @param {number} base @returns {number} */
-export function total(value, base) { return value + base; }
+export function total(value: number, base: number): number { return value + base; }
```

**已交付明确类型。** 从调用合同交付两个数值参数和返回类型，不改计算或观察顺序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/total.ts#total, returns: number }
P:
  parameters:
    value: { position: 0, type: number }
    base: { position: 1, type: number }
R: base + value
```

## 已取得的只读 Vector 长度采用自然域代表

```javascript
/**
 * @param {ReadonlyArray<number>} values
 * @returns {number}
 */
export function choose(values) {
  return values.length > 0 ? 7 : 11;
}
```

```diff
diff --git a/src/vector-case.js b/src/vector-case.ts
rename from src/vector-case.js
rename to src/vector-case.ts
--- a/src/vector-case.js
+++ b/src/vector-case.ts
@@ -1,7 +1,3 @@
-/**
- * @param {ReadonlyArray<number>} values
- * @returns {number}
- */
-export function choose(values) {
+export function choose(values: readonly number[]): number {
   return values.length > 0 ? 7 : 11;
 }
```

**已交付明确类型。** 调用合同声明这是不可空、只读的数值 Vector；只读取有定义的长度，不改成可变数组操作。 因长度是自然数，正长度的补集恰为零。[长度的自然域合同](../../../chapters/05-关系代数.md#ra-11-有序内容与自然数基数)支持这个单点证明；本例在完整 CASE 中用等值守卫表示空分支，两个源码方向得到同一完整 PERS。长度下界也提供三种独立证明：`length >= 0` 恒真，`length !== -1` 恒真（负一不在自然域），`length <= 0` 则恰好等价于 `length === 0`。前两种应折叠返回值，第三种仍保留空与非空两条业务路径，并用同一个「零在前」的 CASE 表达。 同理，`length > -1` 恒真，`length !== 0` 与 `length > 0` 共用同一个完整 CASE。显式一元正号作用于精确整数字面量，例如 `+0`，不改变比较语义；不是对变量或对象做 JS 隐式数值转换。只有读入参自身的 `length` 才取得这项证据，不能把 `values.map.length` 或计算出的索引等其他属性读取当作向量长度。不把普通未知数也当作非负长度。可变的 `Array<number>` 声明也不构成只读向量证据。

```yaml
schema: normalizer.pers/1
entry: { function: src/vector-case.ts#choose, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
R: |-
  CASE
    WHEN 0 == LENGTH(values) THEN 11
    ELSE 7
  END
```

## 不可变构造保留元素位置

```javascript
/**
 * @param {boolean} enabled
 * @param {number} left
 * @param {number} right
 * @param {number} fixed
 * @returns {ReadonlyArray<number>}
 */
export function pair(enabled, left, right, fixed) {
  return [enabled ? left : right, fixed];
}
```

```diff
diff --git a/src/vector-construction.js b/src/vector-construction.ts
rename from src/vector-construction.js
rename to src/vector-construction.ts
--- a/src/vector-construction.js
+++ b/src/vector-construction.ts
@@ -1,10 +1,3 @@
-/**
- * @param {boolean} enabled
- * @param {number} left
- * @param {number} right
- * @param {number} fixed
- * @returns {ReadonlyArray<number>}
- */
-export function pair(enabled, left, right, fixed) {
+export function pair(enabled: boolean, left: number, right: number, fixed: number): readonly number[] {
   return [enabled ? left : right, fixed];
 }
```

**已交付明确类型。** 出口是不可变数值内容，不观察对象身份。两个元素及条件都已取得，构造不会抛异常，也不执行外部动作；构造里的 CASE 原位保留，避免复制相同的第二个元素及整个数组构造。

```yaml
schema: normalizer.pers/1
entry: { function: src/vector-construction.ts#pair, returns: 'number[]' }
P:
  parameters:
    enabled: { position: 0, type: bool }
    left: { position: 1, type: number }
    right: { position: 2, type: number }
    fixed: { position: 3, type: number }
R: "[(CASE WHEN enabled THEN left ELSE right END), fixed]"
```

## 有序只读 Map：可空输入只在非空时插入

三个输入都已取得，允许 null。返回值是有序的只读内容，不暴露 JavaScript Map 的可变身份。

```javascript
/**
 * @param {string|null} a
 * @param {string|null} b
 * @param {string|null} c
 * @returns {ReadonlyMap<string, string>}
 */
export function labels(a, b, c) {
  return new Map([
    ...(a === null ? [] : [['a', a]]),
    ...(b === null ? [] : [['b', b]]),
    ...(c === null ? [] : [['c', c]]),
  ]);
}
```

此合同要求一个原生 Map 构造和闭合的字面条目组装。数组返回值不是 Map；可空入参必须由自身的空值判断保护后才插入，不能无条件插入，也不能在该判断下用恰好同名的文本字面量替代入参。

```diff
diff --git a/src/ordered-map.js b/src/ordered-map.ts
rename from src/ordered-map.js
rename to src/ordered-map.ts
--- a/src/ordered-map.js
+++ b/src/ordered-map.ts
@@ -1,13 +1,7 @@
-/**
- * @param {string|null} a
- * @param {string|null} b
- * @param {string|null} c
- * @returns {ReadonlyMap<string, string>}
- */
-export function labels(a, b, c) {
-  return new Map([
-    ...(a === null ? [] : [['a', a]]),
-    ...(b === null ? [] : [['b', b]]),
-    ...(c === null ? [] : [['c', c]]),
+export function labels(a: string | null, b: string | null, c: string | null): ReadonlyMap<string, string> {
+  return new Map<string, string>([
+    ...(a === null ? [] : [['a', a] as const]),
+    ...(b === null ? [] : [['b', b] as const]),
+    ...(c === null ? [] : [['c', c] as const]),
   ]);
 }
```

**已交付明确类型。** 按 a、b、c 决定插入哪些键。省略键不等于把 null 插入 Map。每个结果保留真实的插入顺序，不能对键应用标量交换律；例如文本键 "10" 先于 "2" 插入时，不能按普通 JavaScript 对象的整数键规则重排，空文本也不等同于 null。按[有序映射的构造规则](../../../chapters/08-统一定形.md#cf-12-有序映射按有序成员构造)，每次插入都以之前的映射为基础，所以最外层判断是 c，内部依次是 b、a；这不是对成员换序。以下是八个完整结果域。输出采用首匹配 CASE：例如第二臂到达时已经排除三项全空，因而只检查 b、c 为空就必然证明 a 非空。同理，第四臂到达时，前三臂在 c 为空的域已经排除了 a 或 b 为空，只剩两项均在。后四臂由前三臂和第四臂已排除的 c 为空域取得 c 非空，不重复该条件。每个条件内可交换的合取按共享结构键排序。

```yaml
schema: normalizer.pers/1
entry: { function: src/ordered-map.ts#labels, returns: 'map<text,text>' }
P:
  parameters:
    a: { position: 0, type: 'text?' }
    b: { position: 1, type: 'text?' }
    c: { position: 2, type: 'text?' }
R: |-
  CASE
    WHEN a == null AND b == null AND c == null THEN MAP([])
    WHEN b == null AND c == null THEN MAP([["a", a]])
    WHEN a == null AND c == null THEN MAP([["b", b]])
    WHEN c == null THEN MAP([["a", a], ["b", b]])
    WHEN a == null AND b == null THEN MAP([["c", c]])
    WHEN b == null THEN MAP([["a", a], ["c", c]])
    WHEN a == null THEN MAP([["b", b], ["c", c]])
    ELSE MAP([["a", a], ["b", b], ["c", c]])
  END
```

本例交付的是已确认的原生 Map 构造器、三枚不同的字面量文本键和已取得的可空文本，不凭构造器同名认定自定义对象有相同语义。若同一原生 Map 的输入依次为 `[["a","old"],["b","only"],["a","new"]]`，先按原有覆盖合同构造，规范值为 `MAP([["a","new"],["b","only"]])`：更新 a 的值不移动 a 的位置。若构造成员的求值会读取或产生动作，则仍先按源码执行，不能以最后只留两个键为由跳过较早求值。耗尽定形预算报告未完成，不输出部分 PERS。

模块也可以包含只在类型检查中存在的声明，例如 `type AccountAmount = number` 和引用该别名的 `interface AuditShape`。它们不执行、不新增 P/E/S，也不改变原数值入口的 R；类型声明仍须通过源码的类型检查。

同长度的两条向量若在两个位置都不同，例如 `enabled ? [left, fixed] : [right, left]`，应保留一个外层 CASE，不能把同一次判断复制到两个槽。反之，`[enabled ? left : right, !enabled ? right : fixed]` 的两次判断使用互补守卫，必须按同一次真假决定取值：真得到 `[left, fixed]`，假得到 `[right, right]`。当槽内是已取得数值的纯运算时也可用同一证明；换成独立守卫则不能合成同一决定。

已知布尔守卫直接执行：把首槽守卫写成 `!false` 或 `!true`，R 分别是 `[left, fixed]` 和 `[right, fixed]`，已声明的入口参数仍留在 P。布尔字面量不是数字或宿主真值转换。

**布尔插入对照**　若改由已取得的 `p`、`q` 依次控制常量成员 `["a", "A"]`、`["b", "B"]`，则根据 [CF-12](../../../chapters/08-统一定形.md#cf-12-有序映射按有序成员构造)保留每个映射的成员顺序，根据 [CF-11](../../../chapters/08-统一定形.md#cf-11-纯布尔-case-从有效域选形)选 CASE：`WHEN p AND q THEN MAP([["a","A"],["b","B"]]) WHEN p THEN MAP([["a","A"]]) WHEN q THEN MAP([["b","B"]]) ELSE MAP([])`。原例的文本空值检查不属于以总布尔入参构造守卫的合同，不套用这条换序。

**向量槽内的精确运算。** `[enabled ? left + 1 : right + 2, fixed * 2]` 的第一槽保留选择，第二槽按已取得的 `fixed` 乘 2，不因为乘法处在数组字面量中就失去类型。若两个槽都由同一个 `enabled && ready`，或由同一个 `left < right` 决定，而且两臂只使用纯数值运算，则提升为一个外层 CASE；守卫只出现一次，向量内仍保留各槽的原顺序。

八个键若共享少数非空条件，仍按实际独立条件求值，不按键的个数提前截断。若改成八个各自独立的可空入参，矩阵展开可能耗尽预算。这是合法的只读 Map 输入；分析未完成时不丢掉后面的键，不要求改写正确的输入，也不交付部分 PERS。

原生 Map 的纯文本字面量条目不需要凭空加非空守卫。若前两项是 `["a", "old"]`、`["b", "only"]`，第三项只在替换文本非空时再写 `a`，两条路径上的 `a` 都留在第一个位置；差别只在它的值。键和值中的孤立 UTF-16 代理项按原数值保留。重复的同一代理项键照常覆盖，不等同于普通键或替换字符。

### 集合载体的身份来自实际类型

工程若自行声明 `type ReadonlyArray<T> = number` 或 `type ReadonlyMap<K,V> = number`，入口标注虽然碰巧采用标准库名字，实际入参和返回仍是 `number`。反过来，`type Values = ReadonlyArray<number>` 确实解析到标准库数组时，可保留 `number[]` 和其 `LENGTH`；`type Labels = ReadonlyMap<string,string>` 指向标准库类型时，可保留有序映射的完整值。归一化不靠名字猜类型，也不把工程内同名声明替换成标准库。

### 原生文本比较保留内容和可空分支

严格 TypeScript 的 `mode === "pickup"` 比较的是文本内容，不是对象身份。文本中孤立的 UTF-16 代理项保持原样，不用替换字符替代。若入口接收 `string | null`，`value === null ? "fallback" : value` 只在匹配 `null` 时返回兜底；空字符串仍返回原值。改成 `null !== value ? value : "fallback"` 也得到同一 CASE。窄化只适用于这次受检的绑定，不保证另一个可空输入也是文本。这些都是原生表达式，无需预处理为别的语言或标准库调用。
