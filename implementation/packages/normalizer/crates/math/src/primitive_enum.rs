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

}

impl Primitive {
    /// Lower an internal scalar symbol tag, after the language adapter has
    /// provided its evidence. Fixed-width carriers remain distinct tags;
    /// they do not acquire implicit Java class or runtime library behavior.
    pub(crate) fn from_symbol_tag(tag: &str) -> Option<Self> {
        match tag {
            "integer" | "i32" | "i64" | crate::domains::decimal::VALUE_TAG => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "text" => Some(Self::String),
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
            Primitive::Composite => "composite",
            Primitive::ReferenceIdentity => "identity",
        }
    }
}
