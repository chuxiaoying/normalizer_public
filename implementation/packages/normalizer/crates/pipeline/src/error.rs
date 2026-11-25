//! pipeline 的集中错误类型。
//!
//! 每个子步产出 `Result<T, PipelineError>`；错误通过正式 CLI 报告。

#[derive(Debug)]
pub enum PipelineError {
    /// 源码加载/读取 IO 错误
    Io(String),
    /// resolve 阶段错误（字符串承载底层错误文本；不得吞错或降级）
    Resolve(String),

}

impl std::fmt::Display for PipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineError::Io(msg) => write!(f, "[PIPELINE-IO] {}", msg),
            PipelineError::Resolve(msg) => write!(f, "[PIPELINE-RESOLVE] {}", msg),
        }
    }
}

impl std::error::Error for PipelineError {}
