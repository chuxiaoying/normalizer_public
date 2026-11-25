# Hello World：对象结构相等与引用身份相等

本种子用同一组 `Box` 输入对照两种不同的业务判断。字段值相同不代表两个对象是同一个实例。

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

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留 `left.value == right.value`。 | 对象字段的结构关系和对象身份是两种独立语义，预处理不能因为参数类型相同就替换比较方式。 |

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

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 与 new 都保留 `left == right`。 | 引用身份必须沿对象分配和别名关系进入统一 IR，不能用结构字段相等代替身份合同。 |

## 引用身份的归一化最终结果

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

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| 结构入口的 R 是 `left.value == right.value`，身份入口的 R 是 `left == right`。 | PERS 必须保留比较对象的层级。相同字段内容可以来自两个不同对象，身份判断不能被结构化简化吞掉。 |
