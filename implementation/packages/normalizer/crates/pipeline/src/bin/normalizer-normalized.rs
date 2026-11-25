//! Closed normalized Java/project entry. Does not load semantic plugins.
use pipeline::parse_source_directory;

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
            let project = pipeline::kernel_input::Loaded::load_for_entry(std::path::Path::new(path))?;
            print!("{}", project.project_pers(entry)?);
        }
        [mode, root, entry] if mode == "--source" => {
            let path = std::path::Path::new(root);
            if !path.is_dir() {
                return Err("[NORM-SOURCE] expected existing directory".into());
            }
            let program = parse_source_directory(path).map_err(|e| e.to_string())?;
            let loaded = pipeline::kernel_input::Loaded::from_parsed_source(program, entry)?;
            print!("{}", loaded.source_pers(path, entry)?);
        }
        _ => return Err("invalid kernel invocation; use implementation/tools/normalizer help".into()),
    }
    Ok(())
}
fn main() {
    match std::panic::catch_unwind(run) {
        Ok(Ok(())) => (),
        Ok(Err(e)) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("[NORM-ENGINE-FAILURE] no successful result produced");
            std::process::exit(1);
        }
    }
}
