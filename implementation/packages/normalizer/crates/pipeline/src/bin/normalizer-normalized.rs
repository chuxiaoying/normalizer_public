//! Closed normalized Java/project entry. Does not load semantic plugins.
use math::domains::decimal::DecimalResourceLimit;
use pipeline::parse_source_directory;

fn source_request(error: String, locator: &str) -> String {
    if locator.contains('#') && !locator.chars().any(char::is_control) {
        format!("{error}\nentry={locator}")
    } else {
        error
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--validate-standalone-closure"] {
        use std::io::Read;
        let mut input = Vec::new();
        std::io::stdin().take(65_537).read_to_end(&mut input).map_err(|e| e.to_string())?;
        return pipeline::kernel_input::standalone_closure::validate(&input);
    }
    match args.as_slice() {
        [mode, path, entry] if mode == "--project" => {
            print!("{}", pipeline::kernel_input::Loaded::project_pers(std::path::Path::new(path), entry)?);
        }
        [mode, root, entry] if mode == "--source" => {
            let path = std::path::Path::new(root);
            if !path.is_dir() {
                return Err("[NORM-SOURCE] expected existing directory".into());
            }
            let program = parse_source_directory(path).map_err(|e| source_request(e.to_string(), entry))?;
            let loaded = pipeline::kernel_input::Loaded::from_linked_source(program, path, entry)
                .map_err(|e| source_request(e, entry))?;
            print!("{}", loaded.source_pers(path, entry).map_err(|e| source_request(e, entry))?);
        }
        _ => return Err("invalid kernel invocation; use implementation/tools/normalizer help".into()),
    }
    Ok(())
}
fn main() {
    // A proven analysis limit aborts execution rather than publishing a
    // partial normal form. Keep unexpected panics on the ordinary error path.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if !panic.payload().is::<math::NormalizationLimit>() && !panic.payload().is::<DecimalResourceLimit>() {
            default_hook(panic);
        }
    }));
    match std::panic::catch_unwind(run) {
        Ok(Ok(())) => (),
        Ok(Err(e)) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
        Err(payload) => {
            let limit = payload.downcast_ref::<math::NormalizationLimit>().map(ToString::to_string)
                .or_else(|| payload.downcast_ref::<DecimalResourceLimit>().map(ToString::to_string));
            if let Some(limit) = limit {
                eprintln!("[NORM-ANALYSIS-INCOMPLETE] {limit}");
            } else {
                eprintln!("[NORM-ENGINE-FAILURE] no successful result produced");
            }
            std::process::exit(1);
        }
    }
}
