# Hello World：无法证明终止的递归必须拒绝

## 输入原始代码

入口递归调用自身。虽然本例看起来有一个递减参数，但当前输入合同没有把递归降低为可执行的固定 fold 或已证明的递归语义。

```java
class Recursive {
    static int down(int value) {
        if (value <= 0) return 0;
        return down(value - 1) + 1;
    }
}
```

## AI 预处理结果

**拒绝。** AI 不截断递归深度，不把递归调用改成固定次数循环，也不凭借参数名猜测终止合同。需要补充可执行的递归语义或等价的有限 fold 定义。

```text
PREPROCESS_REQUIRED

down:
  reason: recursive call is outside the current finite-loop contract
  required: a closed recursive termination contract or an equivalent finite fold
```

**预处理 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 含有递归调用；new 在预处理阶段明确拒绝，没有删除递归分支或人为截断次数。 | 符号化执行不能在不知道递归深度时假装得到完整结果。无法证明终止时，拒绝比截断或补默认值可靠。 |

## 归一化最终结果

未生成 PERS。CLI 必须以非零状态返回 `PREPROCESS_REQUIRED`，stdout 为空，并指出 `Recursive.down`。

**PERS 结果 diff**

| 差异点（old → new） | 升级思想 |
|---|---|
| old 没有可执行的 PERS；new 只保留可回放的拒绝诊断。 | 递归终止合同属于进入内核前的准入条件，不能把不完整的递归执行包装成一个看似完整的 R。 |
