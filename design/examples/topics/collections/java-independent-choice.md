# Hello World：抽取两个候选，保持每次选择的完整记录

[本主题](README.md)

**设计种子，尚未接入正式 CLI。** 以下语法与 PERS 只用来评审候选表示，不是当前已支持的输入或输出。

## 输入原始代码

数组、备用项及数组成员都不是 null。业务允许每次从非空集合任选一整条，无概率或遍历顺序要求。空集两次都使用调用者给定的备用项。

```java
import java.util.Arrays;
record Offer(String code, int price) {}
record Pick(String firstCode, int firstPrice, String secondCode) {}
class Offers {
    static Pick sample(Offer[] offers, Offer backup) {
        Offer first = Arrays.stream(offers).unordered().findAny().orElse(backup);
        Offer second = Arrays.stream(offers).unordered().findAny().orElse(backup);
        return new Pick(first.code(), first.price(), second.code());
    }
}
```

## AI 预处理结果

在两个 findAny 的原位置分别示意绑定一次候选的整成员选择语义：items 均取 offers，empty 均取 backup。保留构造 Pick 的 Java。完整的候选进入选择，两个调用点各有独立身份。[内部选择合同](../../../../implementation/contracts/normalized-choice.md)

```diff
--- a/src/Offers.java
+++ b/src/Offers.java
@@ -1,10 +1,11 @@
-import java.util.Arrays;
 record Offer(String code, int price) {}
 record Pick(String firstCode, int firstPrice, String secondCode) {}
 class Offers {
     static Pick sample(Offer[] offers, Offer backup) {
-        Offer first = Arrays.stream(offers).unordered().findAny().orElse(backup);
-        Offer second = Arrays.stream(offers).unordered().findAny().orElse(backup);
+        Offer first;
+        /* @normalizer-ir choose first from offers else backup */
+        Offer second;
+        /* @normalizer-ir choose second from offers else backup */
         return new Pick(first.code(), first.price(), second.code());
     }
 }
```

注释中的 choose 是原位交接示意：两个位置需要分别保留选择站点、候选集合和空集默认值。下面的 `T.member_choice` 是这个种子的候选公共表示；当前内核不会从该注释生成它。

## 待验收的理想 PERS

```yaml
schema: normalizer.pers/1

types:
  Offer: { fields: { code: text, price: number } }
  Pick: { fields: { firstCode: text, firstPrice: number, secondCode: text } }
entry: { function: src/Offers.java#Offers.sample, returns: Pick }
P:
  parameters:
    offers: { position: 0, type: 'Offer[]' }
    backup: { position: 1, type: Offer }
T:
  - { id: $t_0, op: member_choice, when: 'true', members: offers, empty: backup }
  - { id: $t_1, op: member_choice, when: 'true', members: offers, empty: backup }
R: |-
  { firstCode: $t_0.code, firstPrice: $t_0.price, secondCode: $t_1.code }
```

`$t_0.code` 和 `$t_0.price` 来自同一份报价。第二次选择绑定到 `$t_1`，可以与第一份相同，也可以不同。

**判别例**　候选为 (A,7)、(B,9) 时，(A,7,B) 和 (A,7,A) 都允许，(A,9,B) 不允许。
