# Hello World：外部对象成功返回与字段投影

## 设计意图

区分同一次外部对象回执的不同字段、失败与真实的可空数据。字段须依附原回执，成功前不可读取；不同回执的同名字段不能伪装成同一来源。先成功取得一个标量或同类型对象，不能替后发、可能失败的取得提供字段或成功证明；其异常条件仍指向后一个回执。不同回执的 score 相减保留两项及其方向。 外部端口不声明失败时，以连续十一笔不同入参的调用作对照：交换求和中的两次字段引用不改变执行。表达式按数字回执编号排列，2 在 10 之前；S 按源码顺序保留全部十一笔调用。

派生验收把 `Profile.score` 声明为只读精确整数。一次取得的同一回执上，`reply.score - reply.score` 的正常返回是 `0`；这不消除取得回执的 S，也不改变调用可能抛出的异常和它的回执引用。两次独立调用或两个不同字段不能套用同项消去。

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

AI 在外部对象声明处增加 `@N.External` 并显式装配入口接收者，调用、字段读取和异常声明保持 Java 原生结构。

```diff
--- a/src/ProfileService.java
+++ b/src/ProfileService.java
@@ -1,21 +1,28 @@
+import normalizer.intrinsics.N;
+
 class Profile {
     int score;
 }

 class ProfileService {
+    @N.External
     private final ProfileClient client;

     ProfileService(ProfileClient client) {
         this.client = client;
     }

     int score(int accountId) {
         return client.fetch(accountId).score;
     }
 }

-interface ProfileClient {
-    Profile fetch(int accountId) throws ProfileUnavailable;
+class ProfileClient {
+    native Profile fetch(int accountId) throws ProfileUnavailable;
 }

 class ProfileUnavailable extends RuntimeException {}
+
+class Beans {
+    static final ProfileService ENTRY = new ProfileService(new ProfileClient());
+}
```

Normalizer 只识别 `@N.External`，不按 ProfileClient 的来源库和方法名做特判。

`Beans.ENTRY` 定位已装配实例；`native` 端口声明交付原签名，按[外部端口合同](../../../chapters/04-AI救赎.md#ai-08-外部端口要有真实合同)处理。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Profile:
    fields:
      score: number

entry:
  function: src/ProfileService.java#Beans.ENTRY.score
  returns: number

P:
  parameters:
    accountId:
      position: 0
      type: number

R: $e_0.score

S:
  - call:
      external: src/ProfileService.java#Beans.ENTRY.client.fetch
      args:
        - accountId
      returns: Profile
      result: $e_0

throws:
  - type: ProfileUnavailable
    when: completion.is_throw($e_0, ProfileUnavailable)
```

**预处理要点**

- 外部边界身份由 Normalizer 标记提供，工程不理解具体库。
- 预处理补充边界身份，不提前把外部对象投影成标量，也不吞掉异常。

**PERS 验收要点**：外部对象的边界调用、类型结构、字段投影和异常传播必须共同进入业务结果。

**相消不抹掉取得过程**　只调用一次 fetch，把正常回执 score 先绑定到局部，再计算 `score - score`：正常域的 R 恒为零，但 S 仍保留同一请求和回执，服务失败仍从相同回执的异常完成产生。相反，把独立调用的两个 score 相减不能只凭字段同名归零。