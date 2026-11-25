# Hello World：外部对象成功返回与字段投影

## 输入原始代码

外部客户端返回 Profile 对象，入口读取其中的 score 字段作为业务结果。外部失败向入口传播。

```java
class Profile {
    int score;
}

class ProfileService {
    private final ProfileClient client;

    ProfileService(ProfileClient client) {
        this.client = client;
    }

    int score(int accountId) {
        return client.fetch(accountId).score;
    }
}

interface ProfileClient {
    Profile fetch(int accountId) throws ProfileUnavailable;
}

class ProfileUnavailable extends RuntimeException {}
```

## AI 预处理结果

AI 只在外部对象声明处增加 Normalizer 自身的标记，调用、字段读取和异常声明保持 Java 原生结构。

```diff
--- a/src/ProfileService.java
+++ b/src/ProfileService.java
@@ -1,8 +1,11 @@
+import normalizer.annotations.External;
+
 class Profile {
     int score;
 }
 
 class ProfileService {
+    @External
     private final ProfileClient client;
 
     ProfileService(ProfileClient client) {
         this.client = client;
     }
```

Normalizer 只识别 `@External`，不按 ProfileClient 的来源库和方法名做特判。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      score: number

entry:
  function: src/ProfileService.java#ProfileService.score
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

R: $e_0.score

S:
  - call:
      external: client.fetch
      args:
        - accountId
      returns: Profile
      result: $e_0

throws:
  - type: ProfileUnavailable
    when: completion.is_throw($e_0, ProfileUnavailable)
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只有外部对象字段声明，new 只增加 `@External` 和对应导入。 | 外部边界身份由 Normalizer 标记提供，工程不理解具体库。 |
| old 与 new 都保留对象调用、字段读取和异常声明。 | 预处理补充边界身份，不提前把外部对象投影成标量，也不吞掉异常。 |

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 只看到 `client.fetch(...).score`，new 在 S 中记录对象完成值，在 R 中读取其 `score`，并在 throws 中保留失败类型。 | 外部对象的边界调用、类型结构、字段投影和异常传播必须共同进入业务结果。 |
