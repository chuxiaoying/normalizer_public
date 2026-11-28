//! Public CLI presentation boundary. Legacy stage messages are decoded only
//! here, never used to decide source-language control flow.
use serde::{Deserialize, Serialize};

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tools/diagnostics/catalog.tsv"
));

#[derive(Clone, Copy)]
struct Definition<'a> {
    code: &'a str,
    kind: &'a str,
    stage: &'a str,
    summary: &'a str,
    action: &'a str,
    design: &'a str,
}

fn definitions() -> impl Iterator<Item = Definition<'static>> {
    CATALOG.lines().filter_map(|line| {
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut fields = line.split('|');
        Some(Definition {
            code: fields.next().expect("diagnostic code"),
            kind: fields.next().expect("diagnostic kind"),
            stage: fields.next().expect("diagnostic stage"),
            summary: fields.next().expect("diagnostic summary"),
            action: fields.next().expect("diagnostic action"),
            design: fields.next().expect("diagnostic reference"),
        })
    })
}

fn definition(code: &str) -> Option<Definition<'static>> {
    definitions().find(|candidate| candidate.code == code)
}

fn leading(raw: &str) -> Option<(&str, &str)> {
    let (code, message) = raw.strip_prefix('[')?.split_once(']')?;
    Some((code, message.trim_start()))
}

fn strip_known_envelope(line: &str) -> (&str, &str) {
    if let Some(rest) = line.strip_prefix("SCENARIO_MODULE_INPUT:") {
        return ("NORM-SCENARIO-INPUT", rest.trim_start());
    }
    let mut pair = leading(line).unwrap_or(("", line));
    if pair.0 == "PIPELINE-RESOLVE" {
        if let Some(inner) = leading(pair.1).filter(|inner| definition(inner.0).is_some()) {
            pair = inner;
        }
    }
    pair
}

#[derive(Serialize)]
struct Diagnostic {
    schema: &'static str,
    code: &'static str,
    kind: &'static str,
    stage: &'static str,
    summary: &'static str,
    action: &'static str,
    design: &'static str,
    result: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cause_code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_code: Option<String>,
    details: Vec<String>,
}

fn location(message: &str) -> Option<&str> {
    let text = message.split_once("location=")?.1;
    let end = [" target=", " problem="]
        .iter()
        .filter_map(|delimiter| text.find(delimiter))
        .min()
        .unwrap_or(text.len());
    let value = text[..end].trim();
    (!value.is_empty()).then_some(value)
}

#[derive(Deserialize)]
struct PrivateFailure {
    code: String,
    message: String,
    location: Option<String>,
    target: Option<String>,
    tool_code: Option<String>,
}

fn record(
    def: Definition<'static>,
    location: Option<String>,
    target: Option<String>,
    cause_code: Option<&'static str>,
    details: Vec<String>,
) -> Diagnostic {
    Diagnostic {
        schema: "normalizer.diagnostic/1",
        code: def.code,
        kind: def.kind,
        stage: def.stage,
        summary: def.summary,
        action: def.action,
        design: def.design,
        result: "not-produced",
        location,
        target,
        cause_code,
        tool_code: None,
        details,
    }
}

fn diagnose(raw: &str) -> Diagnostic {
    let mut lines = raw
        .trim()
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with("[normalizer:"));
    let first = lines.next().unwrap_or_default().trim_start();
    if let Some(frame) = first
        .strip_prefix("@normalizer-diagnostic ")
        .and_then(|value| serde_json::from_str::<PrivateFailure>(value).ok())
    {
        if let Some(def) = definition(&frame.code) {
            let mut diagnostic =
                record(def, frame.location, frame.target, None, vec![frame.message]);
            diagnostic.tool_code = frame.tool_code;
            return diagnostic;
        }
    }
    let (code, mut first_detail) = strip_known_envelope(first);
    let def = definition(code).unwrap_or_else(|| {
        let io = first.starts_with("ENOENT:")
            || first.starts_with("EACCES:")
            || first.starts_with("ENOTDIR:")
            || first.starts_with("EISDIR:")
            || first.starts_with("ENOSPC:")
            || first.starts_with("EPERM:");
        let fallback = if io && first.contains("mkdtemp ") {
            "NORM-TEMP-IO"
        } else if io {
            "NORM-INPUT-IO"
        } else {
            "NORM-ENGINE-FAILURE"
        };
        first_detail = first;
        definition(fallback).expect("fallback diagnostic")
    });
    // Older semantically tagged errors keep their detail. The renderer owns
    // the only public kind field; do not echo the legacy prefix twice.
    if let Some((_, detail)) = first_detail
        .strip_prefix("kind=")
        .and_then(|rest| rest.split_once(' '))
    {
        first_detail = detail;
    }
    let mut details: Vec<_> = std::iter::once(first_detail)
        .chain(lines)
        .filter(|line| {
            !line.trim().is_empty()
                && !line.starts_with("result=")
                && (!line.starts_with("note: run with `RUST_BACKTRACE")
                    || std::env::var_os("NORMALIZER_DIAGNOSTIC_DEBUG").is_some())
        })
        .collect();
    if def.code == "NORM-ENGINE-FAILURE"
        && first.starts_with("thread ")
        && std::env::var_os("NORMALIZER_DIAGNOSTIC_DEBUG").is_none()
    {
        details.clear();
    }
    let cause_code = details.iter().find_map(|line| {
        // Recognize a leading cause, not arbitrary bracketed source text.
        let line = line.strip_prefix("problem=").unwrap_or(line);
        let (code, _) = leading(line)?;
        definition(code)
            .filter(|cause| cause.code != def.code)
            .map(|cause| cause.code)
    });
    record(
        def,
        location(first_detail).map(str::to_owned),
        None,
        cause_code,
        details.into_iter().map(str::to_owned).collect(),
    )
}

pub(super) fn emit(code: &str, message: &str, location: Option<&str>, target: Option<&str>) {
    // The Rust kernel and Node adapter pass the same private evidence format.
    // Only render() emits the public contract.
    let frame = serde_json::json!({
        "code": code,
        "message": message,
        "location": location,
        "target": target,
    });
    eprintln!("@normalizer-diagnostic {frame}");
}

pub(super) fn render(raw: &str, json: bool) -> String {
    let d = diagnose(raw);
    if json {
        return format!(
            "{}\n",
            serde_json::to_string(&d).expect("serialize diagnostic")
        );
    }
    let mut rendered = format!("[{}] kind={} stage={}\n", d.code, d.kind, d.stage);
    if let Some(cause) = d.cause_code {
        rendered.push_str(&format!("cause={cause}\n"));
    }
    if let Some(tool_code) = d.tool_code.as_deref() {
        rendered.push_str(&format!("tool_code={tool_code}\n"));
    }
    if let Some(target) = d.target.as_deref() {
        rendered.push_str(&format!("target={target}\n"));
    }
    if let Some(location) = d.location.as_deref() {
        if !d.details.iter().any(|line| line.contains("location=")) {
            rendered.push_str(&format!("location={location}\n"));
        }
    }
    for line in d.details {
        rendered.push_str(&line);
        rendered.push('\n');
    }
    rendered.push_str(&format!(
        "summary={}\nnext={}\ndesign={}\nresult={}\n",
        d.summary, d.action, d.design, d.result
    ));
    rendered
}

pub(super) fn markdown() -> String {
    let mut text = String::from(
        "# CLI 错误码\n\n由唯一[诊断目录](../tools/diagnostics/catalog.tsv)生成，可运行 `implementation/tools/normalizer errors` 查看。设计编号见[设计章节](../../design/README.md#设计章节)；工程接口不是业务语义。\n\n| 错误码 | 类别 | 阶段 | 怎么读 | 下一步 | 依据 |\n| --- | --- | --- | --- | --- | --- |\n",
    );
    for d in definitions() {
        text.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            d.code, d.kind, d.stage, d.summary, d.action, d.design
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for d in definitions() {
            assert!(seen.insert(d.code) && !d.action.is_empty() && !d.design.is_empty());
        }
    }

    #[test]
    fn intentional_abort_is_not_an_engine_failure() {
        let d = diagnose("[NORM-OUT-OF-SOURCE] kind=ABORT location=src/main.ts:3:10 target=raw.trim: Panic: not-support");
        assert_eq!(
            (d.code, d.kind, d.location.as_deref()),
            ("NORM-OUT-OF-SOURCE", "ABORT", Some("src/main.ts:3:10"))
        );
        assert!(d.details[0].contains("not-support"));
    }

    #[test]
    fn private_failure_is_typed_even_with_profiling() {
        let d = diagnose(
            r#"[normalizer:parse#1] parse
@normalizer-diagnostic {"code":"NORM-INPUT-IO","message":"cannot read input","location":"src/main.ts","target":"entry"}"#,
        );
        assert_eq!(d.code, "NORM-INPUT-IO");
        assert_eq!(d.location.as_deref(), Some("src/main.ts"));
        assert_eq!(d.target.as_deref(), Some("entry"));
        assert_eq!(d.details, ["cannot read input"]);
    }

    #[test]
    fn wrapped_source_error_keeps_one_public_diagnostic() {
        let d = diagnose("[PIPELINE-RESOLVE] [SOURCE-LINK-FAILED] kind=PREPROCESS_REQUIRED location=project\nproblem=[NORM-IMPORT] missing import");
        assert_eq!(d.code, "SOURCE-LINK-FAILED");
        assert!(d.details[1].contains("[NORM-IMPORT]"));
    }
}
