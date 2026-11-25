//! Closed value kinds registered in the math algebra.
//! Host declarations can describe a value only after their language adapter
//! has established its semantics. A familiar class name is not a library
//! implementation or an external boundary contract.

/// math 层代数根基类型。封闭枚举，不可扩展。
///
/// Symbols admit established scalar encodings, not arbitrary host objects.
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
}

impl Primitive {
    /// 从 Java 原生 primitive 名或 class FQN 映射到 math::Primitive。
    ///
    /// 返回 None 表示非 primitive（void / 业务 Pojo / Object / 泛型变量 / BigDecimal 等）。
    ///
    /// This does not provide host operations, boxing, or new source types.
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
            Primitive::Composite => "composite",
            Primitive::ReferenceIdentity => "identity",
        }
    }
}
