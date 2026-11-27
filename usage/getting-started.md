# 第一次运行 Normalizer

以下命令从 Normalizer 仓库根目录运行，在 macOS 或 Linux 的 Shell 中执行。用 Git 克隆仓库后运行示例。CLI 需要 Bash、Rust 工具链（含 Cargo）和系统链接器。独立 TypeScript、Vue 和业务回归还需要 Node.js 24 或更高版本及 npm。首次调用会按锁文件编译内核、按需安装前端依赖，需要可用的本机缓存或网络。归一化不需要 JVM 或启动 Vue 服务。

```sh
cargo --version
implementation/tools/normalizer help
# 只在准备运行 TypeScript 或 Vue 时检查：
node --version
npm --version
```

## 1. Java：从源码直接得到 PERS

这个例子没有隐藏依赖，可以原样交付。下面的目录是输入工程，不是 Normalizer 仓库。

```sh
java_project=$(mktemp -d)
mkdir "$java_project/src"
cat > "$java_project/src/HelloWorld.java" <<'JAVA'
class HelloWorld {
    static int addOne(int value) { return value + 1; }
}
JAVA
implementation/tools/normalizer source "$java_project" 'src/HelloWorld.java#HelloWorld.addOne'
```

检查 `entry.function` 是上面给出的入口，`P.parameters.value.type` 为 `number`，`R` 为 `value + 1`。这是对未知入参的结果，命令不需要你给出某个具体的 `value`。

## 2. TypeScript：运行已准备的导出函数

原始 JavaScript 须先按[预处理 Skill](skills/normalizer-preprocess/SKILL.md)准备成严格 TypeScript。下面从仓库中现成的最小正例复制完整分析源码，只练习 CLI，不假装又做了一次预处理。

```sh
ts_project=$(mktemp -d)
cp -R tests/e2e/cases/typescript/language-entries/entry-javascript-module/total/original/src "$ts_project/"
implementation/tools/normalizer module "$ts_project" --entry 'src/total.ts#total'
```

核对[该例的完整预期](../tests/e2e/cases/typescript/language-entries/entry-javascript-module/total/original/case.yaml)。`case.yaml` 只用于验收，不是 CLI 输入。

## 3. Vue：运行生产入口与消费入口

从两个已准备的用例分别复制 `.vue` 原件和分析 `.ts`。这两个入口各自开始分析，不把前一个命令的输出喂给后一个。

```sh
producer=$(mktemp -d)
cp -R tests/e2e/cases/vue/onLoad/original/src "$producer/"
implementation/tools/normalizer scenario frontend-vo-production "$producer" \
  --entry 'src/ArrearsPage.ts#onLoad' --source src/ArrearsPage.vue

consumer=$(mktemp -d)
cp -R tests/e2e/cases/vue/caption/original/src "$consumer/"
implementation/tools/normalizer scenario frontend-vo-consumption "$consumer" \
  --entry 'src/ArrearsPage.ts#caption' --source src/ArrearsPage.vue
```

[生产预期](../tests/e2e/cases/vue/onLoad/original/case.yaml)可查看有序效果；[消费预期](../tests/e2e/cases/vue/caption/original/case.yaml)可查看状态读取与属性结果。自行准备 Vue 输入时，先看[生产与消费的 CLI 选择](cli.md#vue-场景)。

## 4. 辨认失败

对第一个 Java 工程选一个不存在的入口；要看的是非零退出、stderr 有诊断、stdout 没有 PERS：

```sh
if implementation/tools/normalizer source "$java_project" \
     'src/HelloWorld.java#HelloWorld.missing' \
     > "$java_project/pers.yaml" 2> "$java_project/diagnostic.log"; then
  printf '意外成功\n' >&2
  false
else
  cat "$java_project/diagnostic.log" >&2
  test ! -s "$java_project/pers.yaml" && grep -F '[NORM-ENTRY]' "$java_project/diagnostic.log" >/dev/null
fi
```

看[诊断参考](cli.md#诊断)决定先修工具链、入口还是分析副本。完成练习后，可移除本页创建的临时工程。
