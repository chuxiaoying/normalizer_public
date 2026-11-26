# Hello World：external 与 SQL 关系

## 设计意图

区分真实端口、SQL 读取和有序 SQL 写入。SQL 过滤可引用同一次回执，先后两次反馈不可拼成一次；更换条件、受影响行数或失败路径应在相应段落体现。

## SELECT 与额度封顶

### 输入原始代码

服务先取客户配置，随后查询账单并对合计金额封顶。观察一次 Profile 调用、声明的 ProfileFetchError 和账单表数据。数据库可用性失败在观察范围之外。

```java
import java.util.List;
import org.apache.ibatis.annotations.Select;

class ProfileFetchError extends RuntimeException {}

class ArrearsService {
    private final ProfileClient profileClient;
    private final BillMapper billMapper;

    ArrearsService(
        ProfileClient profileClient,
        BillMapper billMapper
    ) {
        this.profileClient = profileClient;
        this.billMapper = billMapper;
    }

    int calculatePayable(int accountId, int today)
        throws ProfileFetchError {
        Profile profile = profileClient.fetchProfile(accountId);

        List<BillRow> openBills =
            billMapper.selectOpenBills(accountId);

        int total = openBills.stream()
            .filter(row -> row.dueDate <= today)
            .filter(row -> row.priority >= profile.minPriority)
            .map(row -> row.amount - row.waivedAmount)
            .filter(netAmount -> netAmount > 0)
            .mapToInt(netAmount -> netAmount)
            .sum();

        return total > profile.quota ? profile.quota : total;
    }
}

interface ProfileClient {
    Profile fetchProfile(int accountId) throws ProfileFetchError;
}

interface BillMapper {
    @Select("""
        SELECT account_id AS accountId,
               due_date AS dueDate,
               amount,
               waived_amount AS waivedAmount,
               priority
        FROM bill
        WHERE account_id = #{accountId}
          AND status = 'OPEN'
    """)
    List<BillRow> selectOpenBills(int accountId);
}

class Profile {
    int accountId;
    int minPriority;
    int quota;
}

class BillRow {
    int accountId;
    int dueDate;
    int amount;
    int waivedAmount;
    int priority;
}
```

### AI 预处理结果

以 `ArrearsBeans.SERVICE` 显式交付已装配的服务实例，给实际 ProfileClient 接收字段加入 `@N.External`，并保留原成功与失败接口合同。数据库行用工程声明的行类数组承载。把 Mapper SQL 的列别名、筛选和投影绑定到 openBills，再局部替换 Stream 聚合；原额度判断保留给内核。

```diff
--- a/src/ArrearsService.java
+++ b/src/ArrearsService.java
@@ -1,67 +1,59 @@
-import java.util.List;
-import org.apache.ibatis.annotations.Select;
+import normalizer.intrinsics.N;

 class ProfileFetchError extends RuntimeException {}

+class ArrearsBeans {
+    static final ArrearsService SERVICE = new ArrearsService(new ProfileClient());
+}
+
 class ArrearsService {
+    @N.External
     private final ProfileClient profileClient;
-    private final BillMapper billMapper;

-    ArrearsService(
-        ProfileClient profileClient,
-        BillMapper billMapper
-    ) {
+    ArrearsService(ProfileClient profileClient) {
         this.profileClient = profileClient;
-        this.billMapper = billMapper;
     }

     int calculatePayable(int accountId, int today)
         throws ProfileFetchError {
         Profile profile = profileClient.fetchProfile(accountId);

-        List<BillRow> openBills =
-            billMapper.selectOpenBills(accountId);
+        BillRow[] openBills;
+        /* @normalizer-ir
+           openBills = from bill as b
+               where b.account_id == accountId and b.status == "OPEN"
+               yield { accountId: b.account_id, dueDate: b.due_date, amount: b.amount,
+                       waivedAmount: b.waived_amount, priority: b.priority }
+        */

-        int total = openBills.stream()
-            .filter(row -> row.dueDate <= today)
-            .filter(row -> row.priority >= profile.minPriority)
-            .map(row -> row.amount - row.waivedAmount)
-            .filter(netAmount -> netAmount > 0)
-            .mapToInt(netAmount -> netAmount)
-            .sum();
+        int total;
+        /* @normalizer-ir
+           total = sum from openBills as row
+               where row.dueDate <= today
+                 and row.priority >= profile.minPriority
+               let netAmount = row.amount - row.waivedAmount
+               where netAmount > 0
+               yield netAmount
+        */

         return total > profile.quota ? profile.quota : total;
     }
 }

-interface ProfileClient {
-    Profile fetchProfile(int accountId) throws ProfileFetchError;
-}
-
-interface BillMapper {
-    @Select("""
-        SELECT account_id AS accountId,
-               due_date AS dueDate,
-               amount,
-               waived_amount AS waivedAmount,
-               priority
-        FROM bill
-        WHERE account_id = #{accountId}
-          AND status = 'OPEN'
-    """)
-    List<BillRow> selectOpenBills(int accountId);
+class ProfileClient {
+    native Profile fetchProfile(int accountId) throws ProfileFetchError;
 }

 class Profile {
     int accountId;
     int minPriority;
     int quota;
 }

 class BillRow {
     int accountId;
     int dueDate;
     int amount;
     int waivedAmount;
     int priority;
 }
```

**预处理要点**：外部交互前后取得的数据可能不同，读取位置属于业务因果。预处理不能把调用后的查询改成入口快照；局部 IR 的通用分工见[关系聚合基础例](../loops-and-termination/java-loop-fold.md)。

### 归一化最终结果

设计预期如下。

```yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      accountId: number
      minPriority: number
      quota: number

entry:
  function: src/ArrearsService.java#ArrearsBeans.SERVICE.calculatePayable
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number
    today:
      position: 1
      type: number

E:
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      due_date: number
      amount: number
      waived_amount: number
      priority: number
      status: text

S:
  - call:
      external: src/ArrearsService.java#ArrearsBeans.SERVICE.profileClient.fetchProfile
      args:
        - accountId
      returns: Profile
      result: $e_0

R: |-
  CASE
    WHEN $e_0.quota < COALESCE(
      SUM(
        SELECT bill.amount - bill.waived_amount
        FROM bill
        WHERE bill.account_id == accountId
          AND bill.due_date <= today
          AND $e_0.minPriority <= bill.priority
          AND bill.amount > bill.waived_amount
          AND bill.status == "OPEN"
      ),
      0
    ) THEN $e_0.quota
    ELSE COALESCE(
      SUM(
        SELECT bill.amount - bill.waived_amount
        FROM bill
        WHERE bill.account_id == accountId
          AND bill.due_date <= today
          AND $e_0.minPriority <= bill.priority
          AND bill.amount > bill.waived_amount
          AND bill.status == "OPEN"
      ),
      0
    )
  END

throws:
  - type: ProfileFetchError
    when: completion.is_throw($e_0, ProfileFetchError)
```

**PERS 验收要点**

- 外部端口封闭其内部实现，返回的业务数据仍应参与关系运算。各字段必须追溯到同一次调用，不能被改造成互不相关的新输入。
- 合计本身不含 CASE WHEN，重复出现也不产生 LET，不向 P/E 暴露折叠状态。

## INSERT 与返回行数

### 输入原始代码

金额大于零时向 `payable_log` 写入一行，返回受影响行数。本例明确单语句写入合同：普通 INSERT 原子成功时写入一行，失败抛出 `DatabaseWriteError` 且不提交本条语句的部分写入；没有触发器、冲突忽略或额外写入。这些是输入中的数据库边界合同，预处理必须完整交付。

```java
import org.apache.ibatis.annotations.Insert;
import org.apache.ibatis.annotations.Param;

class DatabaseWriteError extends RuntimeException {}

class PayableService {
    private final PayableMapper payableMapper;

    PayableService(PayableMapper payableMapper) {
        this.payableMapper = payableMapper;
    }

    int recordPayable(int accountId, int amount) throws DatabaseWriteError {
        if (amount <= 0) return 0;
        return payableMapper.insertPayable(accountId, amount);
    }
}

interface PayableMapper {
    @Insert("""
        INSERT INTO payable_log (account_id, amount)
        VALUES (#{accountId}, #{amount})
    """)
    int insertPayable(@Param("accountId") int accountId,
                      @Param("amount") int amount) throws DatabaseWriteError;
}
```

### AI 预处理结果

在原调用位置给出目标表、字段绑定和完成合同，保留 Java 中的金额判断与返回语句。删除只用于调用 Mapper 的字段、构造参数及接口；不把写库操作改叫普通 external。

```diff
--- a/src/PayableService.java
+++ b/src/PayableService.java
@@ -1,26 +1,16 @@
-import org.apache.ibatis.annotations.Insert;
-import org.apache.ibatis.annotations.Param;
-
 class DatabaseWriteError extends RuntimeException {}

 class PayableService {
-    private final PayableMapper payableMapper;
-
-    PayableService(PayableMapper payableMapper) {
-        this.payableMapper = payableMapper;
-    }
-
-    int recordPayable(int accountId, int amount) throws DatabaseWriteError {
+    static int recordPayable(int accountId, int amount) throws DatabaseWriteError {
         if (amount <= 0) return 0;
-        return payableMapper.insertPayable(accountId, amount);
+        int affected;
+        /* @normalizer-ir
+           affected = insert into payable_log
+               values { account_id: accountId, amount: amount }
+               returning affected_rows
+               atomic
+               throws DatabaseWriteError
+        */
+        return affected;
     }
 }
-
-interface PayableMapper {
-    @Insert("""
-        INSERT INTO payable_log (account_id, amount)
-        VALUES (#{accountId}, #{amount})
-    """)
-    int insertPayable(@Param("accountId") int accountId,
-                      @Param("amount") int amount) throws DatabaseWriteError;
-}
```

`atomic` 表示本条语句成功时提交全部写入，失败时不提交任何行；`affected_rows` 是该次操作正常完成时的行数。异常完成仍由原位置传播。内核依据单行 INSERT 的合同推导成功行数为一，AI 不替换原来的业务条件，也不直接计算入口返回值。

**预处理要点**：预处理必须穿透可见 SQL，把数据库操作的业务含义交给内核。只给方法换成外部端口名会丢失效果合同；源码中的金额判断仍留给内核分析。

### 归一化最终结果

`E` 声明写入目标表及其列，不表示本入口读取表中已有的行。`S` 的 `sql` 直接表达 INSERT：表名 `payable_log` 引用 E，值表达式 `accountId`、`amount` 引用 P。这里的 SQL 已完成名称绑定，框架占位符和 Mapper 调用已在预处理时消化。`when` 控制整条操作是否发生，`result` 绑定完成结果；原子性沿用输入合同，失败由 `throws` 表达。

```yaml
schema: normalizer.pers/1

entry:
  function: src/PayableService.java#PayableService.recordPayable
  returns: number

P:
  parameters:
    accountId: { position: 0, type: number }
    amount: { position: 1, type: number }

E:
  payable_log:
    kind: table
    source: payable_log
    columns:
      account_id: number
      amount: number

S:
  - sql: |-
      INSERT INTO payable_log (account_id, amount)
      VALUES (accountId, amount)
    returns: number
    result: $e_0
    when: amount > 0

R: |-
  CASE
    WHEN amount > 0 THEN 1
    ELSE 0
  END

throws:
  - type: DatabaseWriteError
    when: |-
      CASE
        WHEN amount > 0 THEN completion.is_throw($e_0, DatabaseWriteError)
        ELSE false
      END
```

**PERS 验收要点**

- 数据库写入由目标表和写入内容定义。SQL 本身能表达这些关系，融入 P/E 引用即可，不必另造表、列绑定的 YAML 语法；方法改名应保持业务含义，改表或换列必须可见。
- 完整效果合同既让正常结果可推导，也明确原子失败时没有部分写入。返回值成为常量不代表副作用消失，失败也不能冒充成功写入零行。

## UPDATE 与匹配条件

### 输入原始代码

将本账户所有 OPEN 账单的减免金额设为 `waivedAmount`，成功返回实际匹配的行数，即使字段值原本相同也计入；未匹配任何账单时返回零。本例按单条语句原子执行，失败抛出 `DatabaseWriteError` 且不提交部分写入，没有触发器或额外写入。参数、成功返回值和表字段遵循统一的默认非空类型合同。

```java
import org.apache.ibatis.annotations.Update;
import org.apache.ibatis.annotations.Param;

class DatabaseWriteError extends RuntimeException {}

class WaiverService {
    private final BillMapper billMapper;

    WaiverService(BillMapper billMapper) {
        this.billMapper = billMapper;
    }

    int setOpenBillWaiver(int accountId, int waivedAmount) throws DatabaseWriteError {
        return billMapper.updateOpenBills(accountId, waivedAmount);
    }
}

interface BillMapper {
    @Update("""
        UPDATE bill
        SET waived_amount = #{waivedAmount}
        WHERE account_id = #{accountId}
          AND status = 'OPEN'
    """)
    int updateOpenBills(@Param("accountId") int accountId,
                        @Param("waivedAmount") int waivedAmount) throws DatabaseWriteError;
}
```

### AI 预处理结果

在调用位置交付关系写入。`where` 决定哪些行参与更新，`set` 决定这些行的新字段值；二者不能随 Mapper 注解一起丢掉。

```diff
--- a/src/WaiverService.java
+++ b/src/WaiverService.java
@@ -1,27 +1,16 @@
-import org.apache.ibatis.annotations.Update;
-import org.apache.ibatis.annotations.Param;
-
 class DatabaseWriteError extends RuntimeException {}

 class WaiverService {
-    private final BillMapper billMapper;
-
-    WaiverService(BillMapper billMapper) {
-        this.billMapper = billMapper;
-    }
-
-    int setOpenBillWaiver(int accountId, int waivedAmount) throws DatabaseWriteError {
-        return billMapper.updateOpenBills(accountId, waivedAmount);
+    static int setOpenBillWaiver(int accountId, int waivedAmount) throws DatabaseWriteError {
+        int affected;
+        /* @normalizer-ir
+           affected = update bill as b
+               set { waived_amount: waivedAmount }
+               where b.account_id == accountId and b.status == "OPEN"
+               returning affected_rows
+               atomic
+               throws DatabaseWriteError
+        */
+        return affected;
     }
 }
-
-interface BillMapper {
-    @Update("""
-        UPDATE bill
-        SET waived_amount = #{waivedAmount}
-        WHERE account_id = #{accountId}
-          AND status = 'OPEN'
-    """)
-    int updateOpenBills(@Param("accountId") int accountId,
-                        @Param("waivedAmount") int waivedAmount) throws DatabaseWriteError;
-}
```

**预处理要点**：共性见[INSERT 样例](#insert-与返回行数)。

### 归一化最终结果

SQL 中的 `bill` 引用 E，`accountId`、`waivedAmount` 引用 P；`SET` 与 `WHERE` 共同描述更新内容和范围。更新的正常返回值取自本次操作的匹配行数。表中可能没有匹配行，也可能有多行，因此不能将它简化成固定值。

```yaml
schema: normalizer.pers/1

entry:
  function: src/WaiverService.java#WaiverService.setOpenBillWaiver
  returns: number

P:
  parameters:
    accountId: { position: 0, type: number }
    waivedAmount: { position: 1, type: number }

E:
  bill:
    kind: table
    source: bill
    columns:
      account_id: number
      waived_amount: number
      status: text

S:
  - sql: |-
      UPDATE bill
      SET waived_amount = waivedAmount
      WHERE bill.account_id = accountId
        AND bill.status = 'OPEN'
    returns: number
    result: $e_0

R: $e_0

throws:
  - type: DatabaseWriteError
    when: completion.is_throw($e_0, DatabaseWriteError)
```

**PERS 验收要点**：更新范围和更新内容是两项独立业务约束。去掉 OPEN 条件会扩大范围，改变 `SET` 的值会改变内容，两者都必须直接可检查；返回行数应对应这次更新实际选中的记录。
