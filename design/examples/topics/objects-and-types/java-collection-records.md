# Hello World：集合中的自定义记录投影

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
