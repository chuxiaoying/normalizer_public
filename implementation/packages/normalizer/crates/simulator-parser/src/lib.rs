//! # simulator-parser
//!
//! Java 源码 → AST。这是 simulator 的第一阶段：纯词法与语法分析。
//!
//! ## 设计原则
//!
//! - **纯语法**：不做任何语义推断（不推类型、不解析重载、不识别框架注解）。
//!   语义推断是 resolver 的职责
//! - **产出不透明**：`JavaProgram` / `JavaClass` / `JavaMethod` 字段私有，
//!   外部只能通过 pub API 查询，不能直接构造/修改
//! - **Lombok 等代码生成**：由引擎外的 AI 预处理准备明确源码；不在解析后执行插件
//!
//! ## 公开 API
//!
//! - `parse(source, opts) → Result<JavaProgram>`
//! - `JavaProgram::classes() → impl Iterator<Item=&JavaClass>`
//! - `JavaClass` / `JavaMethod` / `JavaStatement` / `JavaExpr` 的只读访问器
//!
//! ## 不做的事
//!
//! - 不做类型推断（Var<T>、泛型、重载）
//! - 不解析框架注解含义（@Autowired、@Bean 的含义由预处理明确）
//! - 不读文件（由 pipeline 读取后送入字符串）

pub mod ast;
pub mod java_unicode;
mod parser;

pub use ast::{
    AnnotationRef, CatchClause, JavaBinOp, JavaClass, JavaExpr, JavaField, JavaLit, JavaMethod,
    JavaProgram, JavaStatement, JavaType, JavaUnaryOp,
};
pub use parser::{parse, ParseError, ParseOptions};
pub(crate) mod body_parser;

#[cfg(test)]
mod lexical_regressions {
    use super::*;

    #[test]
    fn reference_cast_keeps_identifier_operand() {
        for name in ["_token", "$token"] {
            let source = format!(
                "class Lexical {{ static String read(Object {name}) {{ return (String){name}; }} }}"
            );
            let program = parse(&source, "Lexical.java", &ParseOptions::default()).unwrap();
            let method = &program.find_class("Lexical").unwrap().methods()[0];
            assert!(matches!(
                method.body().unwrap(),
                [JavaStatement::Return(Some(JavaExpr::Cast {
                    target_type: JavaType::Class(target),
                    operand,
                }))] if target == "String" && matches!(operand.as_ref(), JavaExpr::Ident(actual) if actual == name)
            ));
        }
    }

    #[test]
    fn nested_array_initializer_keeps_expression_and_shorthand_elements() {
        let program = parse(
            "class Lexical { static int[][] read(int[] row) { return new int[][]{row, {2, 3}}; } }",
            "Lexical.java",
            &ParseOptions::default(),
        ).unwrap();
        let method = &program.find_class("Lexical").unwrap().methods()[0];
        let [JavaStatement::Return(Some(JavaExpr::ArrayInitializer {
            element_type,
            elements,
        }))] = method.body().unwrap() else {
            panic!("expected an array initializer in the return expression");
        };
        assert_eq!(*element_type, JavaType::Array(Box::new(JavaType::Primitive("int".into()))));
        assert!(matches!(
            elements.as_slice(),
            [JavaExpr::Ident(row), JavaExpr::ArrayInitializer { element_type, elements }]
                if row == "row"
                    && *element_type == JavaType::Primitive("int".into())
                    && matches!(elements.as_slice(), [JavaExpr::Lit(JavaLit::Int(2)), JavaExpr::Lit(JavaLit::Int(3))])
        ));
    }

    fn syntax_failure(source: &str, fragment: &str) {
        let error = parse(source, "Entrance.java", &ParseOptions::default())
            .expect_err("unsupported or malformed source must not produce an AST");
        let ParseError::Syntax { file, line, col, msg } = error;
        assert_eq!(file, "Entrance.java");
        assert!(line >= 1 && col >= 1);
        assert!(msg.contains(fragment), "{source:?}: {msg}");
    }

    #[test]
    fn passive_record_keeps_declared_nominal_component_schema() {
        let program = parse(
            "record BatchRow(int sequence, boolean active) { /* passive */ } class Entry {}",
            "Entrance.java", &ParseOptions::default(),
        ).unwrap();
        let record = program.find_class("BatchRow").unwrap();
        assert!(record.is_record());
        assert_eq!(record.source_file(), Some("Entrance.java"));
        assert!(record.methods().is_empty());
        assert!(record.fields().iter().all(|field| field.is_final() && !field.is_static()));
        assert_eq!(record.fields()[0].name(), "sequence");
        assert_eq!(*record.fields()[1].java_type(), JavaType::Primitive("boolean".into()));
        assert!(!program.find_class("Entry").unwrap().is_record());
        syntax_failure("record Row(int x, int x) {}", "[JAVA-RECORD]");
        syntax_failure("record Row(int x) { public int x() { return 0; } }", "[JAVA-RECORD]");
    }

    #[test]
    fn incomplete_unit_reports_the_original_parse_failure() {
        syntax_failure("class Incomplete", "expected '{'");
        syntax_failure(r#"class Incomplete { String field = "unfinished"#, "unclosed '{'");
        syntax_failure("class WrongConstructor { Other() {} }", "[MEMBER-NAME-UNPARSED]");
    }

    #[test]
    fn malformed_expressions_are_not_reinterpreted_as_legal_java() {
        syntax_failure("class Entrance { static int read() { int n = {1}; return n; } }", "expected expression");
        syntax_failure("class Entrance { static boolean read(boolean a) { return a ?? false; } }", "expected ';' after return value");
        syntax_failure("class Entrance { static int read() { return .; } }", "expected expression");
    }

    #[test]
    fn unsupported_enum_is_not_dropped_from_the_compilation_unit() {
        syntax_failure(
            "class Entrance { static int read() { return 7; } } enum Mode { NORMAL }",
            "enum requires explicit preprocessing",
        );
    }

    #[test]
    fn explicit_this_has_only_an_instance_receiver() {
        syntax_failure(
            "class Entrance { static int read() { if (true) { return this.helper(); } return 0; } static int helper() { return 7; } }",
            "[JAVA-STATIC-THIS]",
        );
        syntax_failure(
            "class Entrance { int read() { int initial = 1, this = 7; return initial; } }",
            "[JAVA-RESERVED-LOCAL-NAME]",
        );
        let source = "class Entrance {
            int value;
            Entrance(int value) { this.value = value; }
            int instance() { return this.value; }
            static int read() { return Entrance.helper(); }
            static int helper() { return 7; }
        }";
        let program = parse(source, "Entrance.java", &ParseOptions::default()).unwrap();
        let class = program.find_class("Entrance").unwrap();
        assert!(class.methods().iter().any(JavaMethod::is_constructor));
        let instance = class.methods().iter().find(|method| method.name() == "instance").unwrap();
        assert!(matches!(
            instance.body().unwrap(),
            [JavaStatement::Return(Some(JavaExpr::FieldAccess { target, field }))]
                if field == "value" && matches!(target.as_ref(), JavaExpr::Ident(name) if name == "this")
        ));
        let read = class.methods().iter().find(|method| method.name() == "read").unwrap();
        assert!(matches!(
            read.body().unwrap(),
            [JavaStatement::Return(Some(JavaExpr::MethodCall { target: Some(owner), method, args }))]
                if method == "helper" && args.is_empty()
                    && matches!(owner.as_ref(), JavaExpr::Ident(name) if name == "Entrance")
        ));
    }
}
