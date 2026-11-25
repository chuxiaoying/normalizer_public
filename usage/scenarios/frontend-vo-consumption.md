# Vue consumption 场景

```sh
implementation/tools/normalizer scenario frontend-vo-consumption PROJECT_ROOT --entry src/page.mjs:render --source src/page.vue
```

提交原始 Vue 组件与预处理模块，模块保留真实业务入口、宿主状态及读取时点。完整输入、预处理和输出见[场景案例](../../design/examples/series-1/frontend-vo-consumption-vue.md)。参数与验收见[场景入口](../scenario-entrypoints.md)。
