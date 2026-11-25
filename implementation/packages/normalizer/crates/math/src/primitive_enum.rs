//! Primitive —— math 层代数根基类型（封闭枚举）。
//!
//! ## 用户教导（20260428114110 / 114150 / 122240 / 122330 / 122700 / 122815）
//!
//! 用户原话（核心）：
//! - "原生类型是要被 math 理解的，map 能和数组并列，是因为 map 本质上是 **tuple** 类型"
//! - "list 和 array 无异，而你却要区分类型，明显是想偷懒，想省事"
//! - "你为什么不愿意用我取的名字，tuple 和 array，非要用 map 和 list"
//! - "为什么 date 不能只留一个，非要在 math 概念里应付 java"
//! - "这里 number 不就够了，为何要照顾 java？"
//! - "这个 OK，但 void 不该在"（代数层 Void 不是"值"，是"没有值"）
//! - "我加入原生类型，是做好了付出的心理准备，而你反而把它当避风港了"
//!
//! ## 核心原则
//!
//! **math 层保持代数纯粹，不为 Java 打补丁**。
//!
//! - 数值差异（int/long/short/byte/double/float 精度）→ 统一 `Number`
//! - 时间差异（Date/LocalDate/Instant 等）→ 统一 `Date`
//! - Map 家族（HashMap/LinkedHashMap/TreeMap/...）→ 统一 `Tuple`
//! - List 家族（ArrayList/LinkedList/T[] 等）→ 统一 `Array`
//!
//! Java 层的多样性由 simulator 层的映射表消化到 math::Primitive。
//!
//! ## 历史五类标量与闭合语义值域
//!
//! | Primitive | 代数含义 | Java 层映射 |
//! |---|---|---|
//! | `Number` | 数值代数根基 | int/long/short/byte/double/float + Integer/Long/Short/Byte/Double/Float/Number |
//! | `String` | 字符串 | java.lang.String |
//! | `Boolean` | 布尔 | boolean / java.lang.Boolean |
//! | `Char` | 字符（单码元） | char / java.lang.Character |
//! | `Date` | 时间点 | Date/LocalDate/LocalDateTime/Instant/ZonedDateTime/OffsetDateTime |
//!
//! ordered-state-1 另使用 Composite 表示有完整定义的不可变值，ReferenceIdentity 表示容器中的 HeapRef 身份；两者均不从 Java 类名自动推断，也不授予对象库行为。
//!
//! ## Map/List 不属于 Primitive
//!
//! Map/List 是有独立状态和操作语义的集合值，不是标量 Primitive。
//! 执行器通过封闭的集合合同和 typed heap carrier 表达其行为；语义不从
//! `Map.java` / `List.java` 私有实现字段或循环展开中推断。
//!
//! ## 不在 enum（故意排除）
//!
//! - **Void**：用户明令 "void 不该在"。代数层 Void 不是"值"，是"没有值"。
//!   `register_func` 的 `return_type: Option<Primitive>` 中 `None` 天然表达"无返回"。
//! - **Object / 业务 Pojo**：用 HeapRef 表达，不是 primitive
//! - **泛型 T/E/K/V**：类型变量，不是 primitive
//! - **BigDecimal / BigInteger**：走 hack native 展开（用户明令）
//! - **Set 系列**：代数语义模糊，暂不纳入（用户未明确）
//!
//! ## 为什么 Number 一个 variant 够
//!
//! 用户 20260428122700 明令："这里 number 不就够了，为何要照顾 java？"
//!
//! - Java 的 int/long/short/byte 精度差异是 JVM 字长实现细节
//! - double/float 精度差异是 IEEE754 实现选择
//! - 业务语义上"都是数"，代数律对 Number 统一
//! - 列 6 个 variant = 跟着 Java 走 = 代数根基被 Java 打补丁
//!
//! ## 承担的代数律责任
//!
//! - **Number**：+ / - / * / / / % / < / <= / > / >= / == / !=
//! - **String**：concat / length / isBlank / charAt / valueOf
//! - **Date**：before / after / plus / minus / toString
//!
//! primitive 扩张不是避风港，是代数律实现责任扩张。

/// math 层代数根基类型。封闭枚举，不可扩展。
///
/// simulator 层通过 `Primitive::from_java_fqn` 将 Java 类名/原生类型名映射到此枚举。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Primitive {
    /// Closed identity tokens inside immutable values, represented by HeapRef.
    /// Never inferred from Object/Pojo types; does not expose object behavior.
    ReferenceIdentity,
    /// Immutable record/sequence/map values; never an opaque Java object.
    Composite,
    /// 数值代数根基（覆盖 Java 所有整数/浮点/Wrapper）
    Number,

    /// 字符串
    String,

    /// 布尔
    Boolean,

    /// 字符（单码元）
    Char,

    /// 时间点（统一一个代数概念，不列 LocalDate/LocalDateTime/Instant 等）
    Date,
}

impl Primitive {
    /// 从 Java 原生 primitive 名或 class FQN 映射到 math::Primitive。
    ///
    /// 返回 None 表示非 primitive（void / 业务 Pojo / Object / 泛型变量 / BigDecimal 等）。
    ///
    /// simulator 层负责归一：所有整数/浮点 → Number，HashMap → Tuple，ArrayList → Array，LocalDate → Date。
    ///
    /// ## void 的归宿
    ///
    /// `void` / `java.lang.Void` 返回 None —— 在 `register_func` 的
    /// `return_type: Option<Primitive>` 中用 `None` 表达"无返回"。
    pub fn from_java_fqn(s: &str) -> Option<Primitive> {
        match s {
            // --- 数值（Java 原生 + Wrapper 全部归一到 Number）---
            "int" | "long" | "short" | "byte" | "double" | "float" | "java.lang.Integer"
            | "java.lang.Long" | "java.lang.Short" | "java.lang.Byte" | "java.lang.Double"
            | "java.lang.Float" | "java.lang.Number" => Some(Primitive::Number),

            // --- 字符 ---
            "char" | "java.lang.Character" => Some(Primitive::Char),

            // --- 布尔 ---
            "boolean" | "java.lang.Boolean" => Some(Primitive::Boolean),

            // --- 字符串 ---
            "java.lang.String" => Some(Primitive::String),

            // --- Date 统一到一个 Primitive::Date ---
            "java.util.Date"
            | "java.sql.Date"
            | "java.sql.Timestamp"
            | "java.sql.Time"
            | "java.time.LocalDate"
            | "java.time.LocalDateTime"
            | "java.time.LocalTime"
            | "java.time.Instant"
            | "java.time.ZonedDateTime"
            | "java.time.OffsetDateTime"
            | "java.time.OffsetTime" => Some(Primitive::Date),

            // Map/List 家族保留为集合值，不映射到标量 Primitive。
            // 其它一律 None（void / 业务 Pojo / Object / 泛型 / BigDecimal 等）
            _ => None,
        }
    }

    /// 代数术语名（用于 Primitive Functions 归宿名前缀 / Display）。
    ///
    /// **全部用小写代数术语，不用 Java 类名**。
    pub fn algebra_name(&self) -> &'static str {
        match self {
            Primitive::Number => "number",
            Primitive::String => "string",
            Primitive::Boolean => "boolean",
            Primitive::Char => "char",
            Primitive::Date => "date",
            Primitive::Composite => "composite",
            Primitive::ReferenceIdentity => "identity",
        }
    }
}
