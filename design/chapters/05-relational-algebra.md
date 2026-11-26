# 第五章 · 关系代数：让 SQL 和集合运算一起化简

算一个账户的应付总额：SQL 先取出未结账单，Java 再从这些账单里挑正金额、逐笔累加。如果把查询和循环当成两个不透明的调用，归一化只能说「先取一批数据，再跑一段程序」。但这两段做的是同一件事的两半：**先挑哪些账单，再算这些账单的金额。**

关系代数让这两半用同一套数学表达。筛选就是留下符合条件的行，投影就是挑出字段或为每行计算一个值，连接就是按条件配对，聚合就是把多行算成一个结果。写在 SQL 里，还是写成 Java 的循环或集合调用，不决定它能不能参与化简。只要作用相同、需要观察的行为也相同，就可以用同一种操作表示。

## 一、把 SQL 和循环放进同一个算式

以下例子只读一次账单表。金额是非空的精确整数，逐行读取没有副作用，数据库故障不在这个例子的观察范围。原程序按账户和 `OPEN` 状态取账单，再按正金额筛选；每笔金额还先加上 `extra`，又减掉同一个 `extra`。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class ArrearsService {
    private final BillMapper mapper;

    ArrearsService(BillMapper mapper) {
        this.mapper = mapper;
    }

    int positiveTotal(int accountId, int extra) {
        int total = 0;
        for (BillRow row : mapper.openBills(accountId)) {
            if (row.amount > 0) {
                total += (row.amount + extra) - extra;
            }
        }
        return total;
    }
}

interface BillMapper {
    @Select("""
        SELECT amount
        FROM bill
        WHERE account_id = #{accountId} AND status = 'OPEN'
        """)
    List<BillRow> openBills(int accountId);
}

class BillRow {
    int amount;
}
```

预处理做两件事：在查询的位置写明**从哪张表取哪些行**，在循环的位置写明**对取得的行算什么**。哪怕看得出 `extra` 会抵消，也先照着源码交给工程，不替它算答案。

```diff
--- a/src/ArrearsService.java
+++ b/src/ArrearsService.java
@@ -1,31 +1,23 @@
 import java.util.List;
-import org.apache.ibatis.annotations.Select;

 class ArrearsService {
-    private final BillMapper mapper;
-
-    ArrearsService(BillMapper mapper) {
-        this.mapper = mapper;
-    }
-
     int positiveTotal(int accountId, int extra) {
         int total = 0;
-        for (BillRow row : mapper.openBills(accountId)) {
-            if (row.amount > 0) {
-                total += (row.amount + extra) - extra;
-            }
-        }
+
+        List<BillRow> rows;
+        /* @normalizer-ir
+           rows = from bill as b
+               where b.account_id == accountId and b.status == "OPEN"
+               yield { amount: b.amount }
+        */
+
+        /* @normalizer-ir
+           total += sum from rows as row
+               where row.amount > 0
+               yield (row.amount + extra) - extra
+        */
         return total;
     }
-}
-
-interface BillMapper {
-    @Select("""
-        SELECT amount
-        FROM bill
-        WHERE account_id = #{accountId} AND status = 'OPEN'
-        """)
-    List<BillRow> openBills(int accountId);
 }

 class BillRow {
```

这里的 `rows` 来自那一次查询，不是新的入口参数。循环遍历多少行不必提前知道，求和表达的就是对所有命中的行累加。

### 先从循环里找出四个动作

这个循环没有写出 `filter` 或 `sum`。要读的是变量怎样流动，而不是关键字长什么样：

| 循环里看到什么 | 转成什么 | 为什么 |
| --- | --- | --- |
| `for (BillRow row : …)` | `from rows as row` | 每轮处理原查询取到的一行，不重复查询。 |
| `if (row.amount > 0)` | `where row.amount > 0` | 不满足条件时什么也不做，累加器保持原值。 |
| `(row.amount + extra) - extra` | `yield (row.amount + extra) - extra` | 这是当行的贡献，只看当前行和固定输入，不读前几轮的结果。 |
| `int total = 0` 和 `total += …` | 从 0 开始求和 | `total` 只用于累加，既不决定下一项选不选，也不决定什么时候停。 |

拿一批实际数据走一遍：SQL 返回四笔未结账单，金额依次是 `[5, -2, 5, 0]`；`extra = 3`。前后两笔 5 是不同账单。

| 当前金额 | Java 循环结束这一轮后的 `total` | 关系式对这行做什么 |
| --- | ---: | --- |
| 第一笔 5 | 5 | 保留，产出 `(5 + 3) - 3 = 5` |
| -2 | 5 | 筛掉，没有贡献 |
| 第二笔 5 | 10 | 保留，再产出一个 5 |
| 0 | 10 | 筛掉，没有贡献 |

循环把「筛选、算一项、累加」揉在每一轮里；关系式把它们分开写，计算的还是同一批数据。如果把条件改成 `if (total < 6 && row.amount > 0)`，就不能照搬成独立的 `where`：对 `[5, 5, 5]`，第三笔会因前两轮已累到 10 而被跳过，原程序返回 10，不是 15。这类循环要保留前一轮的状态和次序。

### 同一段集合运算，Java 和 JavaScript 怎么交付

还是这批已经取到的 `rows`。把循环换成 Java Stream，或者把同样的逻辑写成 JavaScript 数组链：

**Java Stream**

```java
return rows.stream()
    .filter(row -> row.amount > 0)
    .mapToInt(row -> (row.amount + extra) - extra)
    .sum();
```

**JavaScript Array**

```javascript
return rows
    .filter(row => row.amount > 0)
    .map(row => (row.amount + extra) - extra)
    .reduce((total, amount) => total + amount, 0);
```

预处理不用去模拟库里的迭代器，也不用把两种链都改写成 `for`。它沿着调用链交付同一个计算：

| Java | JavaScript | 交给工程的含义 |
| --- | --- | --- |
| `filter(row -> row.amount > 0)` | `filter(row => row.amount > 0)` | 从 `rows` 筛出正金额行。 |
| `mapToInt(…)` | `map(…)` | 对每一行产生一个值：`(row.amount + extra) - extra`。相同的值不去重。 |
| `sum()` | `reduce((total, amount) => total + amount, 0)` | 从 0 开始加上每个值。空输入仍为 0。 |

把同一批数据代到两条链里，可以直接看见每一步的中间结果：

| 阶段 | 这组账单的结果 |
| --- | --- |
| 输入 `rows` | `[5, -2, 5, 0]`（这里只列金额） |
| `filter` | 两笔金额为 5 的账单，还是两行 |
| `mapToInt` / `map` | `[(5 + 3) - 3, (5 + 3) - 3] = [5, 5]` |
| `sum` / 从 0 开始 `reduce` | `0 + 5 + 5 = 10` |

两个版本都交成循环示例里的 `sum from rows as row where … yield …`。其中 `extra` 还在，`rows` 也还是原来的来源；等到工程化简时才消掉 `extra`、把筛选和 SQL 的条件合并。如果同样的函数接收的是前端已经拿到的数组，而不是数据库查询结果，集合操作照样能化简，但不能凭空补上一张表。

上面的等价有条件：这里的列表和数组都由普通记录组成，JS 数组没有空槽，回调只做字段访问和数值计算，不读索引、不改数组、不抛异常。按实际语言运行时比较，也要保证中间计算和累加在两端都能精确表示。尤其不能拿任意 JavaScript `Number` 套用加减抵消。如果回调会记日志，JS 的 `filter().map()` 先跑完过滤回调再进入映射，顺序 Stream 则可以逐项穿过各步。这种情况下不能把两条链当成同一个纯关系表达式。

## 二、逐层化简：先算行内，再合并行集

下面用接近 SQL 的记法展示演算，不是在重写应用要发给数据库的 SQL。记 `rows` 为已取得的查询结果；原循环从 0 开始，所以没有命中行时也必须得到 0。

| 步骤 | 改写前 | 改写后 | 为什么可以改 |
| --- | --- | --- | --- |
| 1. 化简行内表达式 | `(row.amount + extra) - extra` | `row.amount` | 是同一个不可变入参，两处都只读；在本例的精确整数上加减抵消。 |
| 2. 穿过字段投影 | `row.amount > 0` | `b.amount > 0` | `row.amount` 就是这批行的 `b.amount`，取字段没有副作用。 |
| 3. 合并两层筛选 | 先留本账户的未结账单，再留正金额 | 留下同时满足三项条件的账单 | 两层筛的是同一次查询的行，没有分页、去重或另一次读表隔在中间。 |
| 4. 保留空集结果 | 从 Java 初值 0 开始累加 | `COALESCE(SUM(...), 0)` | 没有匹配行时 SQL 式求和为空，不能丢掉原程序的 0。 |

得到的业务结果是：

```sql
SELECT COALESCE(SUM(b.amount), 0)
FROM bill AS b
WHERE b.account_id = :accountId
  AND b.status = 'OPEN'
  AND b.amount > 0
```

现在可以直接从结果看出：**金额取决于账户和账单，跟 `extra` 无关。** 这不是删掉入口参数，而是证明它不影响这个结果。如果数据库里同一账户有两笔正好都是 5 元的未结账单，两笔都参与，结果是 10，不是 5；再加一笔已结账单或一笔负金额账单，仍是 10。一个符合条件的账单都没有，结果是 0。

如果另一份实现一开始就把正金额筛选写进 SQL，或者先把列表 `filter` 再 `sum`，在同样的来源、空集和重复行约定下，归一化可以把它们化到同一个结果。关系代数不只是把流程换个写法，它让**不同实现里的同一个业务规则能被算成同一个表达式**。

## 三、换一种集合操作，哪些语义必须带过来

| Java / JavaScript 写法 | 交给工程的操作 | 转译时必须保留 |
| --- | --- | --- |
| Stream `filter` / Array `filter` | 筛选 | 谓词要保留原语义。SQL 对 `NULL` 的判断不能照抄成 JS 的真假转换。 |
| Stream `map` / Array `map` | 投影 | 每个输入项产生什么值，重复项算几次。源代码用到的索引也不能丢。 |
| Stream `flatMap` / Array `flatMap` | 逐项展开，再拼接 | 有匹配条件的展开可以是连接；不能看到 `flatMap` 就认作按键 JOIN。 |
| Stream `anyMatch` / Array `some` | 是否存在匹配项 | 空集为假；原来可以短路，不能新增本来不会执行的回调。 |
| 顺序 Stream `filter(...).findFirst()` / Array `find` | 按原顺序取第一个匹配项 | 无匹配时 Java 返回空 `Optional`，JS 返回 `undefined`。不是随便取一项，也不是对所有行回调完才挑第一项。 |
| Stream `distinct` / 数组经 `Set` 去重 | 按原相等规则去重 | 保留去重发生在哪一步、什么算同一项。不能把两个字段相同的 JS 对象自动当成同一对象。 |
| IntStream `sum` / Array `reduce` | 求和，或带初值的折叠 | 只有加总这样的特定合同才能归为求和；任意累加器不一定可以换顺序。 |
| Java `HashMap.put/getOrDefault` / JS `Map.set/has/get` | 按键覆盖，再按键是否存在取值 | 键存在却存着 `null`，与键不存在不同；JS 还可以显式存 `undefined`。键的相等规则也不能想当然。 |

两个短例说明转错会得到什么：

- **找首项。** 对 `[-1, 5, 2]` 找第一个正数，答案是 5。改成先取第一项再筛选，会得到没有结果；改成取最小正数，会得到 2。
- **按键查默认值。** 如果标题表还没有 `"main"`，就返回默认标题；如果用户明确把 `"main"` 写成了 `null`，就返回 `null`。Java 用 `getOrDefault("main", fallback)`；JS 可以写成 `titles.has("main") ? titles.get("main") : fallback`，不能写成 `titles.get("main") ?? fallback`。后者会把「明确清空」当成「没有设置」。

这些不是 API 名称的替换表。要先认出操作和它的合同，工程才能判断哪些关系式可以合并、重排或化简。

### 双重循环：看到的是配对，不是再求一次和

假设手上已有一批账单和一批折扣规则。下面的循环为同账户的每个「账单、规则」组合生成一行预览；它不修改原数组，也不发送请求：

```javascript
const matches = [];
for (const bill of bills) {
    for (const rule of rules) {
        if (bill.accountId === rule.accountId) {
            matches.push({ billId: bill.id, ruleId: rule.id });
        }
    }
}
return matches;
```

外层取账单，内层取规则，`if` 给出配对条件，`push` 给出输出字段。这就是**按账户做内连接，再投影出两个 ID**。例如账单 `B1`、`B2` 属于账户 7，`B3` 属于账户 8；规则 `R1`、`R2` 都属于账户 7。输出是 `[(B1,R1), (B1,R2), (B2,R1), (B2,R2)]`：同一账单可以有多个匹配，`B3` 一条都不产出。

这里返回的是数组，**顺序也是结果**。用连接来表达时，还要保留账单在外、规则在内的原始顺序；用 SQL 式记法，就得带上两个输入的位置并按它们排序：

```sql
SELECT b.id AS billId, r.id AS ruleId
FROM bills AS b
JOIN rules AS r ON r.account_id = b.account_id
ORDER BY b.input_pos, r.input_pos
```

这里假定两边的账户标识都是非空整数，`input_pos` 是项在原数组里的唯一位置。少了排序，`JOIN` 只规定哪些行配对，不规定数组元素的先后。如果原内层在首次匹配后就 `break`，那就应交成「每张账单按规则顺序取第一个匹配」，而不是枚举全部配对。

## 四、可以化简计算，不能搬动真实操作

关系代数也能描述 SQL 写入的目标表、命中条件和新字段值。但能化简条件，不等于能抹掉这次写入。比如「结清本账户未结账单」可以把冗余的筛选化简，不能把更新改成只计算影响了几行；该次更新的执行位置、真实受影响行数和失败行为仍要保留。

读取也是一样：上面的演算只合并**同一批账单上的筛选**。如果原程序先更新再查询，后一次读取可能看见更新后的数据，不能拿入口时的表来替它。归一化要消掉的是计算写法的差别，不是业务行为本身。
