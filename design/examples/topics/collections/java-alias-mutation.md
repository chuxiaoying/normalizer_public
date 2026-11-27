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

R 中扣的是写入前的有序首项，按[统一定形](../../../chapters/统一定形.md)记作 `FIRST(value FROM values)`；S 中的 `values[0]` 仍是精确的写入目标，不能改成读公式。

例如 values=[2,4]、next=7 时，返回 11，不是入口快照之和 6。数组为空时越界且无写回；没有正常返回。

### 捕获后继续：别名聚合读实际完成的写入

把首项写入换成对第二项的写入，并只在这条赋值外捕获下标越界，随后仍通过原别名求和。输入空数组或单元素数组时，失败的写入不产生 S，异常已处理，R 分别为 0 或原元素。输入至少两项时 S 写入第二项，R 必须从原总量扣除原第二项、再加新值，不可回读入口快照。例：输入 [2,4]、next=7，得到 R=9、调用者看到 [2,7]。

命中与捕获路径都继续观察同一数组。普通带源码方法也可自然地把成功处理与失败后的备用聚合拆成两个函数；这是另一种应等价的实现，不是要求 AI 把原生 try 改写为方法调用。先成功写回、后续抛出且被捕获的情形也不回滚已经完成的 S；若保护块里的写入本身有条件，不满足条件时不能凭空生成 S。

对照补齐三个不同的退出域。先写首项再显式抛出且匹配时，非空数组保留唯一一次 S，后续聚合看到新首项；空数组在尝试写入时已越界，两个异常均属于同一捕获类型，只在实际发生的那条路完成捕获。若增加布尔开关，关闭时的 R 恒为入口总量且没有写入；打开且第二项存在时，才把原第二项替换成 next。在已证明第二项存在的聚合分支里，可把实际读到的值写成与原第二项二选一的条件表达式；短数组不读取第二项。将处理器改成不能匹配越界的异常类时，短数组没有 R，真实越界仍出现在 throws。

### 多次写回的已完成前缀与同位覆盖

两次写回的 S 和最后聚合的 R 分别验收。先写首项 next，再经同一别名写第二项 next + 1，捕获越界后仍求和：长度为零时无写回，长度为一时只保留首项写回，长度至少为二时按原顺序保留两次写回。R 从入口总量出发，每个已写入的位置只扣除一次旧贡献，再加该位置实际最后写入的值；不存在的位置不读取。提高下标导致后一次失败，不能撤销已经完成的前缀。以 next=7 为例，[]、[2]、[2,4] 分别得到 0、7、15，S 分别有零、一次、两次写回。

若两句都写首项，非空数组仍有两次有序 S，但只有最后的 next + 1 能进入 R；不能累加两次变化，也不能因为第一项在聚合前被覆盖就从 S 中删掉第一次写入。输入 [2,4] 得到 12，写回顺序是先 7 后 8。
