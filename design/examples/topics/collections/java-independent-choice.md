# Hello World：抽取两个候选，保持每次选择的完整记录

## 设计意图

检验相同候选条件下两次选择仍独立，一次结果的多个字段共享同一选择。重命名本地记录不改变身份；把读取移到第二次选择必须改变 T 依赖。

[本主题](README.md)

本例的合同限于有声明、非空的候选与备用记录；归一化观察内容，不承诺 Java 引用身份。

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

在两个 findAny 的原位置分别绑定一次已有的 choose 语义：items 均取 offers，empty 均取 backup。保留原生 record、Pick 的三个结果字段及原构造顺序。完整的候选进入选择，两个调用点各有独立身份。[内部选择合同](../../../../implementation/contracts/normalized-choice.md)

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
+        /* @normalizer-ir first = choose from offers else backup */
+        Offer second;
+        /* @normalizer-ir second = choose from offers else backup */
         return new Pick(first.code(), first.price(), second.code());
     }
 }
```

注释中的 choose 是原位关系指令：两个源码位置分别代表独立的选择位置、候选和缺省值；下方 `T.member_choice` 定义整记录选择。只允许已声明的标量字段投影；不能把选中的记录当作可写引用。

## 归一化最终结果

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
  {
    firstCode: $t_0.code,
    firstPrice: $t_0.price,
    secondCode: $t_1.code
  }
```

`$t_0.code` 和 `$t_0.price` 来自同一份报价。第二次选择绑定到 `$t_1`，可以与第一份相同，也可以不同。

**判别例**　候选为 (A,7)、(B,9) 时，(A,7,B) 和 (A,7,A) 都允许，(A,9,B) 不允许。

增加布尔字段 `available` 时，`Offer` 的公开形状包含 `available: bool`，而没有读取它的 `Pick` 结果保持不变。不因字段类型变化而重排或合并两个选择。

备用记录经由局部别名 `Offer same = backup` 仍是同一个已取得的入口值。首个选择若从 `same` 取得备用值，公开 T 仍写 `empty: backup`，不新增参数或第三次选择。
