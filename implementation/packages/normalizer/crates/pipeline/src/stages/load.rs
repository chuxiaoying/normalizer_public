//! Load the closed UTF-8 Java source tree for the formal source entry.

use crate::error::PipelineError;
use simulator_parser::{parse, JavaProgram, ParseOptions};
use std::{fs, path::{Path, PathBuf}};

fn collect_java_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), PipelineError> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_java_paths(&path, paths)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("java") {
            paths.push(path);
        }
    }
    Ok(())
}

pub fn parse_source_directory(root: &Path) -> Result<JavaProgram, PipelineError> {
    let mut paths = Vec::new();
    collect_java_paths(root, &mut paths)?;
    if paths.is_empty() {
        return Err(PipelineError::Io("[SOURCE-EMPTY] no Java sources supplied".into()));
    }
    paths.sort();
    let mut program = JavaProgram::new();
    let options = ParseOptions::default();
    for path in paths {
        let file = path.to_str().unwrap_or_else(|| {
            panic!("[PIPELINE-NON-UTF8-PATH] source path is not valid UTF-8")
        });
        let bytes = fs::read(&path)?;
        let content = std::str::from_utf8(&bytes).map_err(|_| {
            PipelineError::Io(format!("[SOURCE-DECODE-FAILED] failed to decode {file} as UTF-8"))
        })?;
        let parsed = parse(content, file, &options).map_err(|error| {
            PipelineError::Resolve(crate::preprocess::required(
                "SOURCE-PARSE-FAILED", file, &error.to_string(),
                "修正源码或按 Skill 将不支持的语法改写为当前规范 Java 子集；不得跳过该文件。",
            ))
        })?;
        for class in parsed.classes() {
            program.push_class(class.clone());
        }
    }
    Ok(program)
}
