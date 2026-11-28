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

原始 JavaScript 须先按[强类型准入规则](../design/chapters/04-AI救赎.md#ai-04-强类型准入)准备成严格 TypeScript。下面从仓库中现成的最小正例复制完整分析源码，只练习 CLI，不假装又做了一次预处理。

```sh
ts_project=$(mktemp -d)
cp -R tests/e2e/cases/topics/language-entries/entry-javascript-module/total.ts--total/original/src "$ts_project/"
implementation/tools/normalizer module "$ts_project" --entry 'src/total.ts#total'
```

核对[该例的完整预期](../tests/e2e/cases/topics/language-entries/entry-javascript-module/total.ts--total/original/case.yaml)。`case.yaml` 只用于验收，不是 CLI 输入。

## 3. 前端 VO：运行生产入口与消费入口

从两个已准备的用例分别复制纯净 TypeScript 分析源码。原始 Vue 只用于种子里展示预处理对照，不进入运行工程。生产看 `onLoad` 怎样读取数据和写入页面状态；消费看某个组件属性怎样从已有状态得到展示值。两次分析相互独立，不把生产命令的输出喂给消费命令。

这里各跑一个已准备入口。对真实页面，先[选定生产或消费以及入口范围](README.md#用户说把这张欠费单页面归一化)，再逐入口运行。

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

在运行第 1 节的同一个终端里，使用刚创建的 `java_project`，把入口换成一个不存在的方法。预期是非零退出、标准错误有诊断、标准输出没有 PERS：

```sh
if implementation/tools/normalizer source "$java_project" \
     'src/HelloWorld.java#HelloWorld.missing' \
     > "$java_project/pers.yaml" 2> "$java_project/diagnostic.log"; then
  printf '意外成功：请查看 %s\n' "$java_project/pers.yaml" >&2
  false
else
  status=$?
  printf '退出码：%s\n' "$status"
  cat "$java_project/diagnostic.log" >&2
  test ! -s "$java_project/pers.yaml" && grep -F '[NORM-ENTRY]' "$java_project/diagnostic.log" >/dev/null
fi
```

以上是上手检查，不代替按设计对完整 PERS 做独立验收。看[诊断参考](cli.md#诊断)决定先修工具链、入口还是分析副本。完成练习后，可移除本页创建的临时工程。
