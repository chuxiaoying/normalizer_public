//! SymVar origin —— 符号变量的来源标签。
//!
//! ## 三种 origin（math 层的抽象，不含 Java 含义）
//!
//! - **Parameter(P)**：输入参数符号。来源是 normalize 分析的对象的入参
//! - **External(E)**：外部寄存符号。由外部系统（如 boundary 调用）产生，持有 binding id + 可选路径
//! - **Temporal(T)**：临时封印符号。由 math 自己或 simulator 封印（如深度限制），持有 binding id
//!
//! binding_id 是 `u64`（math 层无语义），由外部（simulator 的 binding table）发放并保证唯一。
//!
//! ## P0 架构断言：primitive_type 必填（20260427）
//!
//! 用户指示（20260427161604）："所有 P 和 E 的符号变量，都是 primitive（含 string 这些）？
//! 这个需要确保，不能用一个 struct pojo 当做符号。这个断言要确认做到。"
//!
//! 三个变体都必填 `primitive_type: String`——Java 原生 primitive（int/long/boolean/...）
//! 或归一化 primitive（String/Wrapper/Date 等）。业务 Pojo 必须用 HeapRef 表达。
//! 内部 pending_task_handle 可携带专用任务句柄，不能作为公开 T 定义；纯递推结果另有精确整数构造器。
//!
//! param/sym_e/sym_t 做 assert 验证（release mode 也生效），非 primitive panic。

/// 符号变量 origin。
///
/// 所有变体都携带类型；P/E 仅为 primitive，T 另允许已定义的不可变复合值。
/// 业务 Pojo 不能作 SymVar，必须用 HeapRef。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymVarOrigin {
    /// 入参符号，名字标识（如 "x", "req.name"）。
    ///
    /// `primitive_type` 示例：
    /// - Java 原生：`"int"` / `"long"` / `"boolean"` / `"double"` / ...
    /// - 归一化 primitive：`"java.lang.String"` / `"integer"` 等
    Parameter {
        name: String,
        primitive_type: String,
    },

    /// 外部寄存。binding_id 由外部发放；path 表示对该外部符号的结构化字段投影。
    /// 例如：E(binding=0, path=[]) 显示为 `$e_0`
    /// 例如：E(binding=0, path=["data","name"]) 显示为 `$e_0.data.name`
    ///
    /// `primitive_type` 是 Boundary 返回值的 primitive 类型。
    /// Boundary 返回 Pojo 的场景不能走 SymVar，必须走 HeapRef（方向1独立 PR 处理）。
    External {
        binding_id: u64,
        path: Vec<String>,
        primitive_type: String,
    },

    /// 临时绑定。binding_id 由外部发放。
    /// pending_task_handle 构造的 normalized.composite 只代表内部待完成任务，
    /// 不代表业务Pojo，也不允许作为P/E输入或外部响应。
    /// 例如：T(binding=5) 显示为 `$t_5`
    Temporal {
        binding_id: u64,
        primitive_type: String,
    },
}
