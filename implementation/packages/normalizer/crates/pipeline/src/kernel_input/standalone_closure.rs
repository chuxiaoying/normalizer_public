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

pub fn validate(input: &[u8]) -> Result<(), String> {
    let error = |problem| format!("[NORM-STANDALONE-CLOSURE] {problem}");
    if input.len() > 65_536 { return Err(error("input too large")); }
    let entry: Entry = serde_json::from_slice(input).map_err(|_| error("invalid standalone entry closure"))?;
    let identifier = |name: &str| {
        let mut bytes = name.bytes();
        bytes.next().is_some_and(|x| x.is_ascii_alphabetic() || x == b'_' || x == b'$')
            && bytes.all(|x| x.is_ascii_alphanumeric() || x == b'_' || x == b'$')
    };
    let mut params = BTreeSet::new();
    if entry.schema != "normalizer.standalone-closure/1"
        || !entry.file.ends_with(".js")
        || entry.file.chars().any(char::is_control)
        || !std::path::Path::new(&entry.file).components().all(|c| matches!(c, std::path::Component::Normal(_)))
        || !identifier(&entry.entry) || !identifier(&entry.call.member)
        || entry.parameters.iter().any(|name| !identifier(name) || !params.insert(name.as_str()))
        || !params.contains(entry.call.receiver.as_str())
    {
        return Err(error("call must name a member on a distinct declared module parameter"));
    }
    let phase = if entry.call.awaited { "await" } else { "call" };
    let location = format!("{}#{}:{phase}", entry.file, entry.entry);
    let target = format!("{}.{}", entry.call.receiver, entry.call.member);
    crate::preprocess::out_of_source(&location, &target)
}
