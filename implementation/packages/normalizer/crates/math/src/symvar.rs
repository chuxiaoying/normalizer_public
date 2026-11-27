//! Internal identities for parameters, observations, and scoped computation.
//! Only declared business inputs and real observations appear in PERS.
//! Recurrences, selection members, and task handles are not a public field.

/// Logical types admitted by ordinary scalar symbol constructors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarSymbolType {
    Integer,
    ExactDecimal,
    CalendarDate,
    Boolean,
    Text,
    NullableText,
    UntrustedSourceValue,
}

impl ScalarSymbolType {
    pub(crate) fn tag(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::ExactDecimal => crate::domains::decimal::VALUE_TAG,
            Self::Boolean => "boolean",
            Self::Text => "text",
            Self::NullableText => "text?",
            Self::UntrustedSourceValue => "ts_unknown",
            Self::CalendarDate => "date",
        }
    }
}

/// 符号变量 origin。
///
/// Every variant carries a checked scalar kind or a designated internal binding tag.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymVarOrigin {
    /// 入参符号，名字标识（如 "x", "req.name"）。
    ///
    /// `primitive_type` 示例：
    /// - 逻辑标量：`"integer"` / `"boolean"` / `"text"` 等。
    Parameter {
        name: String,
        primitive_type: String,
    },

    /// 外部寄存。binding_id 由外部发放；path 表示对该外部符号的结构化字段投影。
    /// 例如：E(binding=0, path=[]) 显示为 `$e_0`
    /// 例如：E(binding=0, path=["data","name"]) 显示为 `$e_0.data.name`
    ///
    /// `primitive_type` 是 Boundary 返回值的 primitive 类型。
    External {
        binding_id: u64,
        path: Vec<String>,
        primitive_type: String,
    },

    /// 临时绑定。binding_id 由外部发放。
    /// pending_task_handle 构造的 normalized.composite 只代表内部待完成任务，
    /// 不代表业务Pojo，也不允许作为P/E输入或外部响应。
    /// Debug-only labels such as `$t_5` are not published PERS bindings.
    Temporal {
        binding_id: u64,
        primitive_type: String,
    },
}

impl SymVarOrigin {
    pub(crate) fn primitive_type(&self) -> &str {
        match self {
            Self::Parameter { primitive_type, .. }
            | Self::External { primitive_type, .. }
            | Self::Temporal { primitive_type, .. } => primitive_type,
        }
    }
}
