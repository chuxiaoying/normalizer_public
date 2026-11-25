# Hello World：SQL NULL 的关系计数语义

## 输入原始代码

入口统计本账户中 memo 为 SQL NULL 的账单行数。空集合返回零。

```java
import java.util.List;
import normalizer.annotations.Nullable;

class BillMemoRow {
    int accountId;
    @Nullable String memo;
}
class NullMemoService {
    static int missingMemoCount(int accountId) {
        List<BillMemoRow> bills;
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

**原样通过。** 局部关系语义保留 SQL `IS NULL`，没有把 NULL 转成 Java 普通 false，也没有把空集合误认为一条空记录。

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
