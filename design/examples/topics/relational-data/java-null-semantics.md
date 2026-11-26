# Hello World：SQL NULL 的关系计数语义

## 设计意图

行数下界只能决定它实际推出的比较。未知行数仍可能是零或正数，不能用「非负」折掉等零或与正阈值的判断；将阈值放左边不改变可证明性，取得行数之前也不能使用这条路径证明。

取阈值 3：行数 0、3、4 应分别把 `count < 3` 判成真、假、假，把 `count > 3` 判成假、假、真，把 `count >= 3` 判成假、真、真。因此三种比较都要保留整个计数表达式；`3 > count` 则按比较换边定形到与 `count < 3` 相同的结果。表、账户参数和 NULL 过滤不变。

过滤键也可以是请求对象的字段。即使 Java 把 `request.accountId` 存进局部变量，归一化的 P 仍是原请求，SQL 源仍是账单表；不能把局部别名当成额外入口或数据库列。

区分 SQL 里 null 行值、空集及聚合不计空值。命中一条字段为 null 的行不等于没命中；只在无行时使用缺省，不能用通用空值替换抹平。 已取得关系的行数不小于零，因此负数守卫不可达；零仍可观察，已知空集上的等零守卫必须成立。SQL NULL 关键字大小写不改变谓词。对已经取得的非负行数，`<= -1` 恒假，`!= -1` 与 `>= 0` 恒真；对它加上符号未知的偏移后，不再直接继承非负下界。

## 输入原始代码

入口统计本账户中 memo 为 SQL NULL 的账单行数。空集合返回零。

```java
import normalizer.intrinsics.N;

class BillMemoRow {
    int accountId;
    @N.Nullable String memo;
}
class NullMemoService {
    static int missingMemoCount(int accountId) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { accountId: b.account_id, memo: b.memo }
        */
        int count;
        /* @normalizer-ir
           count = count from bills as row where row.memo IS NULL
        */
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** 工程声明的行类以数组承载查询结果，不在分析副本依赖 JDK 集合类。 局部关系语义保留 SQL `IS NULL`，没有把 NULL 转成 Java 普通 false，也没有把空集合误认为一条空记录。

**预处理要点**：SQL NULL 的取得和比较规则属于关系合同，必须在预处理交接中显式保留，不能按宿主语言布尔值猜测。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/NullMemoService.java#NullMemoService.missingMemoCount
  returns: number

P:
  parameters:
    accountId: { position: 0, type: number }

E:
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      memo: text?

R: |-
  COALESCE(
    SUM(
      SELECT 1
      FROM bill
      WHERE bill.account_id == accountId
        AND bill.memo IS NULL
    ),
    0
  )
```

**PERS 验收要点**：关系 NULL、业务可空字段和空集合是三个不同层次，不能合并成一个宿主 null 判断。

同账户的一条 NULL memo 和一条空字符串 memo 分别满足 `IS NULL` 和 `IS NOT NULL`，两种计数各为 1；空账户计数为 0。这里统计满足条件的行，不能改为 `COUNT(memo)`，后者会跳过 NULL。

## 选中的首条记录为空值不是没有记录

```java
import normalizer.intrinsics.N;
class BillMemoRow {
    int id;
    @N.Nullable String memo;
}
class NullMemoService {
    static boolean firstRowHasNullMemo(int accountId) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { id: b.id, memo: b.memo }
        */
        @N.Nullable BillMemoRow match;
        /* @normalizer-ir
           match = first from bills as row
               order by row.id
               yield row else null
        */
        if (match != null) return match.memo == null;
        return false;
    }
}
```

**原样通过。** 判断必须同时满足“确实有记录”和“有序首条的 memo 为 null”。关系已取得不等于首条记录已取得；后者只在存在性保护臂内。按[宿主保护 CASE](../../../secondary/CASE%20的取得域与等价化简.md#宿主保护-case)保留这个取得域，不展开为可能把首项判断提前的 AND。`FIRST` 的空集结果是 null，但不能代替独立的存在性判断。输入没有记录返回 false；首条 memo 确实为 null 返回 true；首条非 null 即便后续某条为 null 也返回 false。

```yaml
schema: normalizer.pers/1
entry: { function: src/NullMemoService.java#NullMemoService.firstRowHasNullMemo, returns: bool }
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill:
    kind: table
    source: bill
    columns: { account_id: number, id: number, memo: 'text?' }
R: |-
  CASE
    WHEN EXISTS (SELECT 1 FROM bill WHERE bill.account_id == accountId) THEN FIRST(bill.memo FROM bill WHERE bill.account_id == accountId ORDER BY bill.id) == null
    ELSE false
  END
```

## 显式缺省只用于没有首条记录

```java
import normalizer.intrinsics.N;
class BillMemoRow {
    int id;
    @N.Nullable String memo;
}
class NullMemoService {
    static boolean fallbackStillPreservesNull(int accountId) {
        BillMemoRow[] bills;
        /* @normalizer-ir
           bills = from bill as b
               where b.account_id == accountId
               yield { id: b.id, memo: b.memo }
        */
        @N.Nullable BillMemoRow match;
        /* @normalizer-ir
           match = first from bills as row
               order by row.id
               yield row else null
        */
        @N.Nullable String value = "missing";
        if (match != null) {
            value = match.memo;
        }
        return value == null;
    }
}
```

**原样通过。** 取得首条记录后，即使其 memo 为 null，也已经确实做了赋值；非空缺省只用于没有首条记录。对可空字段，`COALESCE(首项, 缺省)` 会把两种情况混在一起，不能作为规范输出。缺省即使是空字符串也不是 null；反过来只观察是否存在时，NULL 或 NOT NULL 筛选保留，但不需读取首行字段或为存在性排序。直接观察首条非 null 的字段仍须先确认存在，不能把无行判为非空。这里最终只问 value 是否为 null，因此无行时 `"missing" == null` 折叠为 false，和上例在同一域归到同一个 R；不提前取得首条记录。

```yaml
schema: normalizer.pers/1
entry: { function: src/NullMemoService.java#NullMemoService.fallbackStillPreservesNull, returns: bool }
P:
  parameters:
    accountId: { position: 0, type: number }
E:
  bill:
    kind: table
    source: bill
    columns: { account_id: number, id: number, memo: 'text?' }
R: |-
  CASE
    WHEN EXISTS (SELECT 1 FROM bill WHERE bill.account_id == accountId) THEN FIRST(bill.memo FROM bill WHERE bill.account_id == accountId ORDER BY bill.id) == null
    ELSE false
  END
```

把缺省对照的最后一步改为 `value != null` 时，无行会读到非空文字缺省并返回真；有行而首条 memo 为空则返回假；有行且首条 memo 非空则返回真。仍先确认该行存在，不能把“没有行”和“选中了 null”合并成同一个值。

对同一已取得的过滤计数，`count > -1` 恒真，`1 + count == 0` 恒假：行数最小是 0，`1 + count` 最小是 1。R 可归约为相应常量，但不能删掉原表声明或把未知符号偏移当成 1。关系谓词 `is not null` 的小写拼写不改变谓词，也不把缺行当成有空值。
