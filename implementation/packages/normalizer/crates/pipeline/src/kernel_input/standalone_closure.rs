//! Closure validation for a definite call lowered from standalone module
//! source. A standalone entry supplies no external boundary contract;
//! the type or name of an ordinary input does not provide a call body.
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    schema: String,
    file: String,
    entry: String,
    parameters: Vec<String>,
    call: Call,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    receiver: String,
    member: String,
    awaited: bool,
}

// This envelope identifies a parsed JavaScript call for a diagnostic. These
// names are not being admitted as typed PERS or Java carrier identifiers.
fn source_identifier(name: &str) -> bool {
    !name.is_empty() && name.bytes().enumerate().all(|(index, byte)|
        matches!(byte, b'_' | b'$') || byte.is_ascii_alphabetic()
            || (index != 0 && byte.is_ascii_digit()))
}

pub fn validate(input: &[u8]) -> Result<(), String> {
    let error = |problem| format!("[NORM-STANDALONE-CLOSURE] {problem}");
    if input.len() > 65_536 { return Err(error("input too large")); }
    let entry: Entry = serde_json::from_slice(input).map_err(|_| error("invalid standalone entry closure"))?;
    let mut params = BTreeSet::new();
    if entry.schema != "normalizer.standalone-closure/1"
        || !(entry.file.ends_with(".js") || entry.file.ends_with(".mjs"))
        || entry.file.chars().any(char::is_control)
        || !std::path::Path::new(&entry.file).components().all(|c| matches!(c, std::path::Component::Normal(_)))
        || !source_identifier(&entry.entry) || !source_identifier(&entry.call.member)
        || entry.parameters.iter().any(|name| !source_identifier(name) || !params.insert(name.as_str()))
        || !params.contains(entry.call.receiver.as_str())
    {
        return Err(error("call must name a member on a distinct declared module parameter"));
    }
    let phase = if entry.call.awaited { "await" } else { "call" };
    let location = format!("{}#{}:{phase}", entry.file, entry.entry);
    let target = format!("{}.{}", entry.call.receiver, entry.call.member);
    crate::preprocess::out_of_source(&location, &target)
}

#[cfg(test)]
mod tests {
    use super::validate;
    use serde_json::{json, Value};

    fn envelope() -> Value {
        json!({
            "schema": "normalizer.standalone-closure/1",
            "file": "src/input.js", "entry": "entry",
            "parameters": ["input", "other"],
            "call": { "receiver": "input", "member": "read", "awaited": true }
        })
    }

    fn assert_rejected(input: Value) {
        let result = validate(&serde_json::to_vec(&input).unwrap());
        assert_eq!(
            result.unwrap_err(),
            "[NORM-STANDALONE-CLOSURE] call must name a member on a distinct declared module parameter"
        );
    }

    #[test]
    fn closure_budget_precedes_decoding() {
        assert_eq!(
            validate(&vec![b' '; 65_537]).unwrap_err(),
            "[NORM-STANDALONE-CLOSURE] input too large"
        );
    }

    #[test]
    fn closure_schema_and_source_provenance_are_admitted_separately() {
        let mut wrong_version = envelope();
        wrong_version["schema"] = json!("normalizer.standalone-closure/2");
        assert_rejected(wrong_version);

        let mut control_in_path = envelope();
        control_in_path["file"] = json!("src/input\tname.js");
        assert_rejected(control_in_path);

        let mut parent_in_path = envelope();
        parent_in_path["file"] = json!("src/../input.js");
        assert_rejected(parent_in_path);
    }

    #[test]
    fn closure_checks_distinct_formals_and_receiver_identity() {
        let mut duplicate = envelope();
        duplicate["parameters"] = json!(["input", "input"]);
        assert_rejected(duplicate);

        let mut undeclared = envelope();
        undeclared["call"]["receiver"] = json!("third");
        assert_rejected(undeclared);
    }

    #[test]
    fn closure_requires_an_admitted_module_path_and_each_identifier() {
        for filename in ["/src/input.js", "src/input.ts"] {
            let mut wrong_file = envelope();
            wrong_file["file"] = json!(filename);
            assert_rejected(wrong_file);
        }

        let mut wrong_entry = envelope();
        wrong_entry["entry"] = json!("2entry");
        assert_rejected(wrong_entry);

        let mut wrong_member = envelope();
        wrong_member["call"]["member"] = json!("a.b");
        assert_rejected(wrong_member);

        let mut wrong_formal = envelope();
        wrong_formal["parameters"] = json!(["input", "not-a-binding"]);
        assert_rejected(wrong_formal);
    }

    #[test]
    fn a_parsed_mjs_call_with_dollar_identifiers_keeps_the_no_source_diagnostic() {
        let mut valid = envelope();
        valid["file"] = json!("src/input.mjs");
        valid["entry"] = json!("$entry");
        valid["parameters"] = json!(["$input", "other"]);
        valid["call"]["receiver"] = json!("$input");
        valid["call"]["member"] = json!("$read");
        let wire = serde_json::to_vec(&valid).unwrap();
        let panic = std::panic::catch_unwind(|| validate(&wire))
            .expect_err("the no-source call explicitly aborts");
        let message = panic.downcast_ref::<String>().unwrap();
        assert!(message.contains(
            "[NORM-OUT-OF-SOURCE] kind=ABORT location=src/input.mjs#$entry:await target=$input.$read"
        ));
    }
}
