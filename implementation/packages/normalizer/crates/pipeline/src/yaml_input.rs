//! Validate YAML presentation before serde_yaml constructs application values.
//! YAML permits reused anchor names, but this dependency version can bind a
//! later alias to the wrong value. Refuse that input subset rather than silently
//! change data. Unique anchors remain supported; scalar contents are untouched.
use serde::de::DeserializeOwned;
use std::{collections::BTreeMap, ffi::CStr, marker::PhantomData, mem::MaybeUninit};
use unsafe_libyaml as yaml;

struct Scanner<'input> {
    // The parser stays at one address until deletion; it borrows `input`.
    raw: Box<MaybeUninit<yaml::yaml_parser_t>>,
    _input: PhantomData<&'input [u8]>,
}
impl<'input> Scanner<'input> {
    fn new(input: &'input [u8]) -> Result<Self, String> {
        let mut raw = Box::new(MaybeUninit::<yaml::yaml_parser_t>::uninit());
        // SAFETY: initialization writes into suitably aligned allocated memory;
        // the allocation stays live and stationary until Scanner::drop.
        unsafe {
            if yaml::yaml_parser_initialize(raw.as_mut_ptr()).fail {
                return Err("YAML scanner initialization failed".into());
            }
            yaml::yaml_parser_set_input_string(
                raw.as_mut_ptr(),
                input.as_ptr(),
                input.len() as u64,
            );
        }
        Ok(Self {
            raw,
            _input: PhantomData,
        })
    }
    fn check_anchor_names(&mut self) -> Result<(), String> {
        let mut anchors = BTreeMap::new();
        loop {
            let mut token = MaybeUninit::<yaml::yaml_token_t>::uninit();
            // SAFETY: `raw` is initialized, `input` outlives this scanner, and
            // scan writes one token on success. Every produced token is deleted
            // after copying only its kind, location, and owned anchor bytes.
            unsafe {
                let parser = self.raw.as_mut_ptr();
                if yaml::yaml_parser_scan(parser, token.as_mut_ptr()).fail {
                    let parser_state = &*parser;
                    let problem = if parser_state.problem.is_null() {
                        "invalid YAML".into()
                    } else {
                        CStr::from_ptr(parser_state.problem)
                            .to_string_lossy()
                            .into_owned()
                    };
                    let mark = parser_state.problem_mark;
                    return Err(format!(
                        "YAML scanner error at line {}, column {}: {problem}",
                        mark.line + 1,
                        mark.column + 1
                    ));
                }
                let mut token = token.assume_init();
                let kind = token.type_;
                let mark = token.start_mark;
                let duplicate = if kind == yaml::YAML_ANCHOR_TOKEN {
                    let name = CStr::from_ptr(token.data.anchor.value.cast())
                        .to_bytes()
                        .to_vec();
                    anchors
                        .insert(name.clone(), (mark.line + 1, mark.column + 1))
                        .map(|first| (name, first))
                } else {
                    None
                };
                yaml::yaml_token_delete(&mut token);
                if let Some((name, first)) = duplicate {
                    let name = String::from_utf8_lossy(&name);
                    return Err(format!("duplicate YAML anchor &{name} at line {}, column {} (first at line {}, column {}): YAML permits reused anchor names, but the current normalizer input subset does not support them; AI preprocessing must emit plain YAML or JSON without reused anchors", mark.line + 1, mark.column + 1, first.0, first.1));
                }
                if matches!(
                    kind,
                    yaml::YAML_DOCUMENT_START_TOKEN | yaml::YAML_DOCUMENT_END_TOKEN
                ) {
                    anchors.clear();
                }
                if kind == yaml::YAML_STREAM_END_TOKEN {
                    return Ok(());
                }
            }
        }
    }
}
impl Drop for Scanner<'_> {
    fn drop(&mut self) {
        // SAFETY: only successfully initialized parsers construct Scanner; all
        // produced tokens have been deleted, and parser deletion runs once.
        unsafe { yaml::yaml_parser_delete(self.raw.as_mut_ptr()) }
    }
}

/// Validate YAML presentation and deserialize directly into the requested
/// type.  Keeping this path separate from `serde_yaml::Value` avoids a full
/// intermediate tree for large normalized projects; the caller still gets
/// the same anchor safety check before application values are constructed.
pub(crate) fn parse_to<T: DeserializeOwned>(source: &str) -> Result<T, String> {
    Scanner::new(source.as_bytes())?.check_anchor_names()?;
    serde_yaml::from_str(source).map_err(|e| e.to_string())
}
