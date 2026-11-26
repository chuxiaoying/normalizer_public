# Hello World：集合中的自定义记录投影

## 设计意图

检验集合中真实类型的记录投影，对象身份不当作值折叠依据。字段路径和关系内的行类型必须对应；改变所选字段或过滤影响 R，声明里的无关字段不应进入依赖。记录的行数与数值字段求和是不同聚合；应用行过滤后逐条计数，空结果为零，不能借用物理表的字段来源。

## 输入原始代码

集合元素是自定义记录，入口聚合每条记录的 `score` 字段。

```java
class Profile {
    int score;
}

class ProfileScores {
    static int total(Profile[] profiles) {
        int total;
        /* @normalizer-ir
           total = sum from profiles as profile yield profile.score
        */
        return total;
    }
}
```

## AI 预处理结果

**原样通过。** 局部 IR 只替换集合聚合片段，保留记录声明和字段投影。

## 归一化最终结果

```yaml
schema: normalizer.pers/1
types:
  Profile: { fields: { score: number } }
entry: { function: src/ProfileScores.java#ProfileScores.total, returns: number }
P:
  parameters:
    profiles: { position: 0, type: 'Profile[]' }
R: COALESCE(SUM(SELECT profile.score FROM profiles AS profile), 0)
```

**验收要点**：集合元素的记录类型和字段来源必须同时进入 PERS，不能只保留一个无类型的 `score` 列。
