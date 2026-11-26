# Hello World：集合别名修改与聚合

## 设计意图

对照两个名字指向同一个集合与两个内容相同的集合。后续聚合必须观察实际突变；只重命名别名不变，换成副本会改变改动是否可见。

## 输入原始代码

values 非空引用但允许空数组；调用者可看见写回。P 中 values 表示入口快照，聚合读取通过 alias 访问的写后值。

```java
class AliasCollection {
    static int total(int[] values, int next) {
        int[] alias = values;
        values[0] = next;
        int total = 0;
        for (int value : alias) total += value;
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** 保留有源码依据的计算、类型和观察顺序。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/AliasCollection.java#AliasCollection.total, returns: number }
P:
  parameters:
    values: { position: 0, type: 'number[]' }
    next: { position: 1, type: number }
S:
  - write:
      target: 'values[0]'
      value: next
      when: LENGTH(values) > 0
R: COALESCE(SUM(SELECT value FROM values), 0) - FIRST(value FROM values) + next
throws:
  - { type: java.lang.ArrayIndexOutOfBoundsException, when: 'LENGTH(values) == 0' }
```

R 中扣的是写入前的有序首项，按统一正则规范记作 `FIRST(value FROM values)`；S 中的 `values[0]` 仍是精确的写入目标，不能改成读公式。

例如 values=[2,4]、next=7 时，返回 11，不是入口快照之和 6。数组为空时越界且无写回；没有正常返回。
