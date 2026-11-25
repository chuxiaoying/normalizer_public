# Hello World：对象参数与多字段对象

## 输入原始代码

入口接收 `SummaryRequest`，计算并返回 `Summary` 的三个字段。按[统一业务类型合同](../../../implementation/contracts/pers/schema.yaml)，普通类型默认非空，因此本例的请求对象非空；允许空值的入口须显式声明 `SummaryRequest?`。

```java
class Summary {
    int raised;
    int selected;
    int delta;

    Summary(int raised, int selected, int delta) {
        this.raised = raised;
        this.selected = selected;
        this.delta = delta;
    }
}

class SummaryRequest {
    int value;
    int limit;
    int fallback;
}

class Calculator {
    static Summary summarize(SummaryRequest request) {
        int raised = request.value + 1;
        int selected = raised > request.limit ? raised : request.fallback;
        int delta = selected - raised;
        return new Summary(raised, selected, delta);
    }
}
```

## AI 预处理结果

**原样通过。** 输入字段、三元表达式和返回对象的构造器都有源码定义。上面的 Java 即为 Normalizer 的输入。

**预处理要点**：源码已有的业务类型应继续作为字段与结构的依据，预处理不应先抹掉类型，再用另一套字段合同重建它。

## 归一化最终结果

```yaml
schema: normalizer.pers/1

types:
  SummaryRequest:
    fields:
      value: number
      limit: number
      fallback: number
  Summary:
    fields:
      raised: number
      selected: number
      delta: number

entry:
  function: src/Calculator.java#Calculator.summarize
  returns: Summary

P:
  parameters:
    request:
      position: 0
      type: SummaryRequest

R: |-
  {
    raised: request.value + 1,
    selected: CASE
      WHEN request.limit < request.value + 1 THEN request.value + 1
      ELSE request.fallback
    END,
    delta: CASE
      WHEN request.limit < request.value + 1 THEN 0
      ELSE request.fallback - (request.value + 1)
    END
  }
```

`P.parameters.request.type` 引用源码中的 `SummaryRequest`，`entry.returns` 引用源码中的 `Summary`。`types` 给出这两个类型的字段及其业务类型。一个 `R` 给出返回对象的 `raised`、`selected` 和 `delta` 如何由输入算得。PERS 保留字段之间的业务关系，`Summary` 构造函数、字段赋值顺序和 Java 对象布局不成为额外的 PERS 层级。本例的构造器只将参数赋给三个字段，没有独立的业务副作用；原方法也未通过对象身份参与判断，因此最终表达可以直接给出返回内容。

**PERS 验收要点**

- 输入归属由来源决定。调用者传入的复合值仍是一个业务输入，不因访问字段就变成系统已有状态。
- 自定义类型在最终产物中应有定义和源码依据，让业务类型的名称与字段结构一同可读。
- 复合返回应直接表达各字段之间的业务关系，避免构造过程和重复记录遮蔽这些关系。
- 可空性属于统一业务类型合同：普通类型默认非空，`?` 才允许空值。显式可空时必须保留相应的返回或异常行为。
