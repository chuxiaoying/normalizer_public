# Hello World：关系删除与受影响行数

## 设计意图

检验删除的谓词、影响行数和成功／失败回执分别保留。零行受影响不等于调用未执行；改变数据库条件应改变 S.sql，不能把删除伪装成纯读取。

文本筛选的转义只改变 SQL 字面量拼写，不改变待比较的业务字符串；例如带一个撇号的值要仍是一个值，不能截断或改写条件。

## 输入原始代码

场景规范源码把删除操作交付在原调用位置。它删除指定账户的 CLOSED 账单，并返回本次匹配的行数。数据库失败时声明 `DatabaseWriteError`，原子删除不提交部分行。

```java
class DatabaseWriteError extends RuntimeException {}

class CleanupService {
    int removeClosed(int accountId) {
        int affected;
        /* @normalizer-ir
           affected = delete from bill as b
               where b.account_id == accountId and b.status == "CLOSED"
               returning affected_rows
               atomic throws DatabaseWriteError
        */
        return affected;
    }
}
```

## AI 预处理结果

**原样通过。** 这段源码已经是保持 Java 合法的局部关系语义交付。AI 不把删除改成普通 external，也不把受影响行数预先写成常量。

**预处理要点**：SQL 写入的目标关系、条件、返回行数和原子性必须作为完整效果合同交给内核，不能只保留 Mapper 方法名。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

entry:
  function: src/CleanupService.java#CleanupService.removeClosed
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
      status: text

S:
  - sql: |-
      DELETE FROM bill
      WHERE bill.account_id = accountId
        AND bill.status = 'CLOSED'
    returns: number
    result: $e_0

R: $e_0
throws:
  - { type: DatabaseWriteError, when: 'completion.is_throw($e_0, DatabaseWriteError)' }
```

**PERS 验收要点**：删除是有序关系写入，正常返回值与副作用来自同一次操作。不能把删除简化为查询，也不能把影响行数固定为一。
