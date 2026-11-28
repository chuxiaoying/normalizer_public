//! 预处理缺口诊断。内核遇到未闭合输入时必须明确拒绝。

pub const SKILL: &str = "usage/skills/normalizer-preprocess/SKILL.md";

pub(crate) fn required(code: &str, location: &str, problem: &str, action: &str) -> String {
    format!("[{code}] kind=PREPROCESS_REQUIRED location={location}\nproblem={problem}\nskill={SKILL}\naction={action}\nresult=not-produced")
}

/// Expected analysis abort; it is not a Rust engine fault or a source-language throw.
#[derive(Debug)]
pub struct AnalysisAbort {
    pub location: String,
    pub target: String,
}

/// A reachable execution target has neither a source/semantic definition
/// nor a legal external boundary contract. Never return a symbolic substitute.
#[cold]
#[track_caller]
pub(crate) fn out_of_source(at: &str, target: &str) -> ! {
    std::panic::panic_any(AnalysisAbort {
        location: at.to_owned(),
        target: target.to_owned(),
    })
}
