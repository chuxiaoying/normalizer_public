//! Admission checks on the submitted project, before language adaptation.
use roxmltree::{Document, Node};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn npm(text: &str) -> Result<(), String> {
    let value: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
    let manifest = value.as_object().ok_or("expected a package.json object")?;
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(declared) = manifest.get(section) {
            let deps = declared
                .as_object()
                .ok_or_else(|| format!("{section}: expected an object"))?;
            if let Some(name) = deps.keys().next() {
                return Err(format!("{section}.{name}"));
            }
        }
    }
    for section in ["bundledDependencies", "bundleDependencies"] {
        if let Some(declared) = manifest.get(section) {
            // A boolean selects all or none of the dependencies already checked above.
            if declared.is_boolean() {
                continue;
            }
            let names = declared
                .as_array()
                .ok_or_else(|| format!("{section}: expected an array or boolean"))?;
            if let Some(name) = names.first() {
                return Err(format!("{section}: {name}"));
            }
        }
    }
    Ok(())
}

fn child<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|item| item.has_tag_name(name))
}

fn value<'a>(node: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(node, name)?.text().map(str::trim)
}

fn read_manifest(root: &Path, path: &Path) -> Result<String, String> {
    let target = path.canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(root) || !target.is_file() {
        return Err("expected a manifest file inside the input directory".into());
    }
    fs::read_to_string(target).map_err(|e| e.to_string())
}

fn check_dependencies(project: Node<'_, '_>) -> Result<(), String> {
    if !project.has_tag_name("project") {
        return Err("expected a Maven project".into());
    }
    for dep in project
        .descendants()
        .filter(|n| n.has_tag_name("dependency"))
    {
        if dep
            .parent_element()
            .is_some_and(|p| p.tag_name().name() == "dependencies")
            && !dep
                .ancestors()
                .any(|n| n.has_tag_name("configuration") || n.has_tag_name("properties"))
        {
            return Err(format!(
                "Maven dependency {}:{}",
                value(dep, "groupId").unwrap_or("?"),
                value(dep, "artifactId").unwrap_or("?")
            ));
        }
    }
    Ok(())
}

fn maven(mut text: String, mut path: PathBuf, root: &Path) -> Result<(), String> {
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(path.canonicalize().map_err(|e| e.to_string())?) {
            return Err("Maven parent cycle".into());
        }
        let doc = Document::parse(&text).map_err(|e| e.to_string())?;
        let project = doc.root_element();
        check_dependencies(project)?;
        let Some(parent) = child(project, "parent") else {
            return Ok(());
        };
        // Maven falls back to its repositories if the relative parent is absent or mismatched.
        let relative = child(parent, "relativePath")
            .map(|p| p.text().unwrap_or("").trim())
            .unwrap_or("../pom.xml");
        let mut target = path.parent().unwrap().join(relative);
        if relative.is_empty() {
            return Err("Maven parent must be present in the input directory".into());
        }
        if target.is_dir() {
            target.push("pom.xml");
        }
        let content =
            read_manifest(root, &target).map_err(|e| format!("Maven parent {relative}: {e}"))?;
        let parent_doc = Document::parse(&content).map_err(|e| e.to_string())?;
        let local = parent_doc.root_element();
        for key in ["groupId", "artifactId", "version"] {
            let actual = value(local, key).or_else(|| {
                if key == "artifactId" {
                    None
                } else {
                    value(child(local, "parent")?, key)
                }
            });
            if actual.is_none() || actual != value(parent, key) {
                return Err(format!(
                    "Maven parent {relative}: {key} does not match the submitted parent"
                ));
            }
        }
        text = content;
        path = target;
    }
}

pub fn check(directory: &Path) -> Result<(), String> {
    // Let the language adapter diagnose a missing input root.
    if !directory.is_dir() {
        return Ok(());
    }
    let root = directory.canonicalize().map_err(|e| e.to_string())?;
    let mut directories = vec![root.clone()];
    while let Some(dir) = directories.pop() {
        let mut paths = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        paths.sort();
        for path in paths {
            let name = path.file_name().and_then(|n| n.to_str());
            if matches!(name, Some("pom.xml" | "package.json")) {
                read_manifest(&root, &path)
                    .and_then(|text| {
                        if name == Some("pom.xml") {
                            maven(text, path.clone(), &root)
                        } else {
                            npm(&text)
                        }
                    })
                    .map_err(|why| {
                        format!(
                            "[NORM-INPUT-MANIFEST] {}: {why}. 分析工程须零包依赖，见第四章 AI-11。",
                            path.strip_prefix(&root).unwrap().display()
                        )
                    })?;
            } else if path.symlink_metadata().map_err(|e| e.to_string())?.is_dir() {
                directories.push(path);
            }
        }
    }
    Ok(())
}
