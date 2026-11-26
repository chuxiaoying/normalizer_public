# Hello World：关系连接与未匹配行

## 设计意图

检验连接条件、记录来源与未匹配记录的处理。相同值来自不同表仍要沿真实别名追踪；修改连接键或未匹配保留策略必须改变结果。

## 输入原始代码

统计指定账户对应的 OPEN 账单数量。账户没有匹配账单时返回零。

```java
import java.util.List;

class AccountJoinRow { int id; }
class BillJoinRow { int account_id; String status; }
class JoinService {
    static int openBillCount(int accountId) {
        List<AccountJoinRow> accounts;
        /* @normalizer-ir
           accounts = from account as row yield { id: row.id }
        */
        List<BillJoinRow> bills;
        /* @normalizer-ir
           bills = from bill as row yield { account_id: row.account_id, status: row.status }
        */
        int count;
        /* @normalizer-ir
           count = count from accounts as a
               join bills as b on a.id == b.account_id
               where a.id == accountId and b.status == "OPEN"
        */
        return count;
    }
}
```

## AI 预处理结果

**原样通过。** 局部关系 IR 显式交付连接键、账户过滤、账单过滤和空结果计数。AI 不把 JOIN 改成两个互不相关的查询。

**预处理要点**：JOIN 的匹配条件是业务规则的一部分，不能只按表名拼接，也不能在预处理阶段丢掉未匹配行语义。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/JoinService.java#JoinService.openBillCount
  returns: number

P:
  parameters:
    accountId: { position: 0, type: number }

E:
  account:
    kind: table
    source: account
    columns: { id: number }
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      status: text

R: |-
  COALESCE(
    SUM(
      SELECT 1
      FROM account AS a
      JOIN bill AS b ON a.id == b.account_id
      WHERE a.id == accountId
        AND b.status == "OPEN"
    ),
    0
  )
```

如果账单记录为 `(1, OPEN), (1, OPEN), (1, CLOSED), (2, OPEN)`，且账户 1 存在，则三种用例（原例、改筛 CLOSED、偏移连接键）各得 2、1、1。另一个没有匹配账单的账户返回 0。重复账单仍是两条，不能去重。

**PERS 验收要点**：连接条件和未匹配结果必须进入关系代数，不能让内核按某个 Java Mapper 名称猜测连接方式。
