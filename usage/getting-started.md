# 第一次运行 Normalizer

以下命令适用于 macOS 或 Linux。已有检出就直接进入仓库根目录；还没检出时运行：

```sh
git clone https://github.com/chuxiaoying/normalizer_public.git
cd normalizer_public
```

Java 示例只需要 Bash、Rust/Cargo 和系统链接器（仓库 CI 使用 Rust 1.90）。TypeScript（含前端 VO 场景）和业务回归还需要 Node.js 24 或更高版本及 npm。首次执行分析会用 Cargo 按锁文件构建内核，前端入口首次使用时还会按需安装 npm 依赖。请确保能访问依赖仓库，或本机已经有相应缓存。不需要 JDK，也不用启动前端应用。

```sh
cargo --version
rustc --version
implementation/tools/normalizer help
```

只在准备运行 TypeScript（含前端 VO 场景）或整套回归时，再检查：

```sh
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

从仓库用例复制已准备好的 TypeScript 模块，直接运行 CLI。自己的工程按[输入手册](kernel-input.md)准备。

```sh
ts_project=$(mktemp -d)
cp -R tests/e2e/cases/topics/language-entries/entry-javascript-module/total.ts--total/original/src "$ts_project/"
implementation/tools/normalizer module "$ts_project" --entry 'src/total.ts#total'
```

核对[该例的完整预期](../tests/e2e/cases/topics/language-entries/entry-javascript-module/total.ts--total/original/case.yaml)。`case.yaml` 只用于验收，不是 CLI 输入。

## 3. 前端 VO：运行生产入口与消费入口

从两个已准备的用例分别复制纯净 TypeScript 分析源码。生产命令选择 `onLoad`，消费命令选择 `caption`。各自传入对应的分析目录。

这里各跑一个已准备入口。对真实页面，先[选定生产或消费以及入口范围](skills/normalizer-preprocess/SKILL.md#前端整页归一化)，再逐入口运行。

**生产：查询数据并写入页面状态。**

```sh
producer=$(mktemp -d)
cp -R tests/e2e/cases/topics/language-entries/frontend-vo-production-vue/frontend-vo-production--onLoad/original/src "$producer/"
implementation/tools/normalizer scenario frontend-vo-production "$producer" \
  --entry 'src/ArrearsPage.ts#onLoad'
```

**消费：计算卡片上的说明文字。**

```sh
consumer=$(mktemp -d)
cp -R tests/e2e/cases/topics/language-entries/frontend-vo-consumption-vue/frontend-vo-consumption--caption/original/src "$consumer/"
implementation/tools/normalizer scenario frontend-vo-consumption "$consumer" \
  --entry 'src/ArrearsPage.ts#caption'
```

[生产预期](../tests/e2e/cases/topics/language-entries/frontend-vo-production-vue/frontend-vo-production--onLoad/original/case.yaml)可查看有序效果；[消费预期](../tests/e2e/cases/topics/language-entries/frontend-vo-consumption-vue/frontend-vo-consumption--caption/original/case.yaml)可查看状态读取与属性结果。自行准备前端分析模块时，先看[生产与消费的 CLI 选择](cli.md#前端-vo-场景)。

## 4. 辨认失败

在运行第 1 节的同一个终端里，使用刚创建的 `java_project`，把入口换成一个不存在的方法，将输出保存到分析工程外的临时目录。预期是非零退出、标准错误有诊断、标准输出没有 PERS：

```sh
run_output=$(mktemp -d)
if implementation/tools/normalizer source "$java_project" \
     'src/HelloWorld.java#HelloWorld.missing' \
     > "$run_output/pers.yaml" 2> "$run_output/diagnostic.log"; then
  printf '意外成功：请查看 %s\n' "$run_output/pers.yaml" >&2
  false
else
  status=$?
  printf '退出码：%s\n' "$status"
  cat "$run_output/diagnostic.log" >&2
  test ! -s "$run_output/pers.yaml" && grep -F '[NORM-ENTRY]' "$run_output/diagnostic.log" >/dev/null
fi
```

看[诊断参考](cli.md#诊断)决定先修工具链、入口还是分析副本。完成练习后，可移除本页创建的临时工程。
