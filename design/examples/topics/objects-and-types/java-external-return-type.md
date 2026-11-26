# Hello World：外部对象作为入口返回值

## 设计意图

检验外部对象整体返回与多个独立标量，必须保留真实名义结构和回执来源。字段同形不代表类型或身份相同；失败路径不能产出虚构的返回对象。

## 输入原始代码

外部端口返回一个业务对象，入口把同一个对象直接返回给调用者。对象类型来自源码声明，调用结果仍是一次外部完成值。

```java
class Profile {
    int score;
}

class ProfileService {
    private final ProfileClient client;

    ProfileService(ProfileClient client) { this.client = client; }

    Profile load(int accountId) {
        return client.fetch(accountId);
    }
}

interface ProfileClient {
    Profile fetch(int accountId);
}
```

## AI 预处理结果

AI 在外部端口声明处增加 `@N.External` 标记并显式装配入口接收者。调用、对象类型和返回语句保持原生 Java，不把外部返回对象拆成新的 P 参数，也不替工程实现 `ProfileClient`。

**预处理要点**：外部对象的实现细节由边界合同封闭，预处理只补充 Normalizer 自身的边界身份，类型和返回关系交给 parser 与内核处理。

```diff
--- a/src/ProfileService.java
+++ b/src/ProfileService.java
@@ -1,17 +1,24 @@
+import normalizer.intrinsics.N;
+
 class Profile {
     int score;
 }

 class ProfileService {
+    @N.External
     private final ProfileClient client;

     ProfileService(ProfileClient client) { this.client = client; }

     Profile load(int accountId) {
         return client.fetch(accountId);
     }
 }

-interface ProfileClient {
-    Profile fetch(int accountId);
+class ProfileClient {
+    native Profile fetch(int accountId);
 }
+
+class Beans {
+    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
+}
```

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      score: number

entry:
  function: src/ProfileService.java#Beans.ENTRY.load
  returns: Profile

P:
  parameters:
    accountId:
      position: 0
      type: number

S:
  - call:
      external: src/ProfileService.java#Beans.ENTRY.client.fetch
      args:
        - accountId
      returns: Profile
      result: $e_0

R: $e_0
```

**PERS 验收要点**：外部返回值的业务类型属于调用合同。对象不需要先被拆成字段或复制进 P，调用完成值可以作为完整业务结果继续流转。
