# Hello World：嵌套有限循环聚合

## 输入原始代码

两个区间分别由 rows、columns 决定；任一为空，循环体执行零次。计数使用精确业务整数。

```java
class NestedLoop {
    static int count(int rows, int columns) {
        int total = 0;
        for (int row = 0; row < rows; row++) {
            for (int column = 0; column < columns; column++) total = total + 1;
        }
        return total;
    }
}
```

## AI 预处理结果

保留两层原生循环。内核证明矩形计数范围和常量累加的等价关系，不在预处理中提前求出归一化乘积。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
entry: { function: src/NestedLoop.java#NestedLoop.count, returns: number }
P:
  parameters:
    rows: { position: 0, type: number }
    columns: { position: 1, type: number }
R: |-
  CASE
    WHEN columns > 0 AND rows > 0 THEN columns * rows
    ELSE 0
  END
```

例如 rows=2、columns=3 时结果为 6。变体分别改变总数的业务倍率和外层区间的上端点。
