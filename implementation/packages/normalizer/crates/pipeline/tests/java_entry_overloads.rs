use std::{fs, path::PathBuf, process::{Command, Output}};

struct Project(PathBuf);

impl Project {
    fn write(case: &str, code: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("normalizer-overloads-{}-{case}", std::process::id()));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/Over.java"), code).unwrap();
        Self(dir)
    }

    fn run(&self, invocation: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_normalizer-normalized"))
            .arg("--source").arg(&self.0).arg(format!("src/Over.java#{invocation}"))
            .output().unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn explicit_java_types_select_distinct_overloads() {
    let project = Project::write("selection", r#"
        class Over {
            static int run(int value) { return marker() + 1; }
            static int run(long value) { return marker() + 2; }
            static int marker() { return 100; }
        }
    "#);
    for (invocation, expected) in [("Over.run(int)", "101"), ("Over.run(long)", "102"), ("Over.marker", "100")] {
        let out = project.run(invocation);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let pers: serde_yaml::Value = serde_yaml::from_slice(&out.stdout).unwrap();
        assert_eq!(pers["entry"]["function"].as_str().unwrap(), format!("src/Over.java#{invocation}"));
        assert_eq!(pers["R"].as_str(), Some(expected));
    }
    for (invocation, diagnostic) in [("Over.run", "ambiguous entry method"), ("Over.run(number)", "source parameter signature")] {
        let out = project.run(invocation);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains(diagnostic));
    }
}

#[test]
fn method_body_overload_calls_do_not_guess_a_declaration() {
    let project = Project::write("call-site", r#"
        class Over {
            static int run(int value) { return 1; }
            static int run(long value) { return 2; }
            static int choose() { return Over.run(3); }
        }
    "#);
    let out = project.run("Over.choose");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("NORM-OVERLOADED-CALL"));
}
