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

#[test]
fn nested_static_class_entry_is_not_interpreted_as_a_field() {
    let project = Project::write("nested", r#"
        class Over { static class Nested { static int run(int value) { return value + 1; } } }
    "#);
    let out = project.run("Over.Nested.run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let pers: serde_yaml::Value = serde_yaml::from_slice(&out.stdout).unwrap();
    assert_eq!(pers["entry"]["function"].as_str(), Some("src/Over.java#Over.Nested.run"));
    assert_eq!(pers["R"].as_str(), Some("value + 1"));
}

#[test]
fn independently_closed_singleton_does_not_change_static_entry() {
    let project = Project::write("singleton", r#"
        import normalizer.intrinsics.N;
        class Over {
            static final Over INSTANCE = new Over();
            @N.EntryState int value;
            static int run() { return 7; }
            int read() { return value; }
        }
    "#);
    for (invocation, expected) in [("Over.run", "7"), ("Over.INSTANCE.read", "value")] {
        let out = project.run(invocation);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let pers: serde_yaml::Value = serde_yaml::from_slice(&out.stdout).unwrap();
        assert_eq!(pers["R"].as_str(), Some(expected));
        assert_eq!(pers["E"].is_null(), invocation == "Over.run");
    }
}

#[test]
fn unused_static_field_does_not_hide_startup_code() {
    let project = Project::write("startup", r#"
        class Over {
            static final Over INSTANCE = new Over();
            Over() { throw new IllegalArgumentException(); }
            static int run() { return 7; }
        }
    "#);
    let out = project.run("Over.run");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("NORM-FIELD-DECLARATION"));
}
