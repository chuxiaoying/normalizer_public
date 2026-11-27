# Hello World：通过两个别名写入与捕获对象字段

## 设计意图

编辑者和读取者持有同一个入口状态对象。写左字段后，从另一引用读取两个字段，再把旧值和新值装进本地结果对象。随后还会写一次原状态的右字段。PERS 必须同时保留原状态来源、两次实际写入，以及结果对象在捕获时的四个值，不能把后来的右字段写入倒灌进已捕获的结果。

## 输入原始代码

```java
import normalizer.intrinsics.N;

class StatePair {
    int left;
    int right;
}

class StateSnapshot {
    int leftBefore;
    int leftAfter;
    int rightBefore;
    int rightAfter;

    StateSnapshot(int leftBefore, int leftAfter, int rightBefore, int rightAfter) {
        this.leftBefore = leftBefore;
        this.leftAfter = leftAfter;
        this.rightBefore = rightBefore;
        this.rightAfter = rightAfter;
    }
}

class SnapshotEntry {
    @N.EntryState StatePair pair;

    StateSnapshot run(int next, int later) {
        StatePair edit = this.pair;
        StatePair view = this.pair;
        int leftBefore = view.left;
        int rightBefore = view.right;
        edit.left = next;
        StateSnapshot result = new StateSnapshot(leftBefore, view.left, rightBefore, view.right);
        edit.right = later;
        return result;
    }
}

class Beans {
    static final SnapshotEntry ENTRY = new SnapshotEntry();
    static {
        ENTRY.pair = new StatePair();
    }
}
```

## AI 预处理结果

**原样通过。** 入口状态和接收者已经声明。构造结果对象、解析两个别名、按实际顺序读写字段都由内核处理。不要额外复制入口状态或预先折叠读写。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
types:
  StateSnapshot:
    fields:
      leftBefore: number
      leftAfter: number
      rightBefore: number
      rightAfter: number
entry:
  function: src/SnapshotEntry.java#Beans.ENTRY.run
  returns: StateSnapshot
P:
  parameters:
    next:
      position: 0
      type: number
    later:
      position: 1
      type: number
E:
  pair:
    kind: state
    source: src/SnapshotEntry.java#Beans.ENTRY.pair
    fields:
      left: number
      right: number
S:
- write:
    target: pair.left
    value: next
- write:
    target: pair.right
    value: later
R: |-
  {
    leftBefore: pair.left,
    leftAfter: next,
    rightBefore: pair.right,
    rightAfter: pair.right
  }
```

**判别例**：如果原状态为 (3, 5)，入参为 (7, 9)，结果为 (3, 7, 5, 5)，状态最后是 (7, 9)。把结果构造移到第二次写之后，只有结果的 `rightAfter` 改为 9；把第一次写改到右字段，只有右字段在捕获时看到 7。对有条件的左字段写入，S 与结果中新的左字段值共享该条件。
