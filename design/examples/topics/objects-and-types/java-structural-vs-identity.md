# Hello World：对象结构相等与引用身份相等

## 设计意图

对照同字段值的独立分配、同一对象的两个别名以及有条件取得的引用。结构相同不等于同一身份；互换纯比较两边也不改原求值顺序。

本种子用同一组 `Box` 输入对照两种不同的业务判断。字段值相同不代表两个对象是同一个实例。

已声明非空的借用对象与一个已存为 null 的局部 Object 比较，结果仍然确定；不能仅识别比较式里的字面量 null。

## 结构相等的输入原始代码

```java
class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right) {
        return left == right;
    }
}
```

## 结构相等的 AI 预处理结果

**原样通过。** 字段比较是原生 Java 的纯表达式。AI 不把字段比较改成引用比较。

**预处理要点**：对象字段的结构关系和对象身份是两种独立语义，预处理不能因为参数类型相同就替换比较方式。

## 结构相等的归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  Box:
    fields:
      value: number

entry:
  function: src/Identity.java#Identity.sameValue
  returns: bool

P:
  parameters:
    left: { position: 0, type: Box }
    right: { position: 1, type: Box }

R: left.value == right.value
```

## 引用身份的输入原始代码

```java
class Box {
    int value;
}

class Identity {
    static boolean sameValue(Box left, Box right) {
        return left.value == right.value;
    }

    static boolean sameIdentity(Box left, Box right) {
        return left == right;
    }
}
```

## 引用身份的 AI 预处理结果

**原样通过。** `left == right` 是 Java 原生的引用身份判断。AI 不把它化成字段比较，也不替工程猜测两个对象的内容是否相等。

**预处理要点**：引用身份必须沿对象分配和别名关系进入统一 IR，不能用结构字段相等代替身份合同。

## 引用身份的归一化最终结果

交换比较两端或改变两个命名参数的声明先后，不改变身份关系与 R 中的唯一排序。声明变化仍要在 P 的 position 中体现。比如从左到右的形参变成 `(Box right, Box left)`，R 仍按公开路径写为 `left == right`，不能按内部引用的构造顺序写成反向。

```yaml
schema: normalizer.pers/1

types:
  Box:
    fields:
      value: number

entry:
  function: src/Identity.java#Identity.sameIdentity
  returns: bool

P:
  parameters:
    left: { position: 0, type: Box }
    right: { position: 1, type: Box }

R: left == right
```

**PERS 验收要点**：PERS 必须保留比较对象的层级。相同字段内容可以来自两个不同对象，身份判断不能被结构化简化吞掉。

## 多个身份判断的顺序也与引用分配顺序无关

每个对象引用都是已经取得的输入。这里保留两个真实身份判断，不把不同对象合并；先各自定形，再比较两个总布尔项的完整结构。两项的成本、运算符和合同相同；递归比较左操作数时，前几级仍打平，才按同一类别的公开参数名判出 `amber` 在 `yellow` 前。参数声明从 zebra 开始不改变这个结果。

```java
class Box { int value; }
class Identity {
    static boolean samePairs(Box zebra, Box yellow, Box blue, Box amber) {
        return zebra == yellow && blue == amber;
    }
}
```

**原样通过。** 不改源码求值次序；符号取得后的纯布尔结果才可排序。

```yaml
schema: normalizer.pers/1
types:
  Box: { fields: { value: number } }
entry: { function: src/Identity.java#Identity.samePairs, returns: bool }
P:
  parameters:
    zebra: { position: 0, type: Box }
    yellow: { position: 1, type: Box }
    blue: { position: 2, type: Box }
    amber: { position: 3, type: Box }
R: amber == blue AND yellow == zebra
```

### null、局部分配与借入引用的对照

新对象和 null 比较时，等号为假、不等号为真；把 null 放左边或右边不改变结果。可空入口引用则不能折成常量，`null == left` 规范成 `left == null`，`left != null` 保留反向判断。这些源码对照不新增 S 或异常。

引用身份来自实际分配或借入的来源，仅凭编号不同不能证明两份引用都来自新分配。以[用来源证明引用身份](../../../chapters/03-数学化简.md#ma-07-用来源证明引用身份)裁决。

结构观察也应区分成员名和内核操作：借用 Box 中声明的整数属性 `length` 仍从对应的 Box 入参读取，两个字段按其原拥有者比较；名字不能触发数组基数计算。
