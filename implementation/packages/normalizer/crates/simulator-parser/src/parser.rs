//! 解析入口：Java 源码 → JavaProgram AST。
//!
//! 设计原则：
//! - 纯语法分析，不做类型推断/重载解析/框架识别
//! - 手写递归下降 parser，零外部依赖

use crate::ast::*;
use crate::java_unicode::{self, Surrogates};
/// 解析选项。
///
/// Parser 只负责 Java 语言语法；代码生成必须由引擎外的预处理完成。
#[derive(Debug, Default)]
pub struct ParseOptions {}

/// 解析错误。
#[derive(Debug, Clone)]
pub enum ParseError {
    Syntax {
        file: String,
        line: usize,
        col: usize,
        msg: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Syntax {
                file,
                line,
                col,
                msg,
            } => {
                write!(f, "[PARSE] {}:{}:{}: {}", file, line, col, msg)
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// 解析一个 Java 源码文件，返回包含类定义的 JavaProgram。
pub fn parse(
    source: &str,
    file_hint: &str,
    _opts: &ParseOptions,
) -> Result<JavaProgram, ParseError> {
    let translated = java_unicode::translate(source)
        .map_err(|(offset, msg)| Parser::new(source, file_hint).err_at(offset, msg))?;
    let input = translated.as_ref().map_or(source, |t| t.text.as_str());
    let surrogates = translated.as_ref().map_or_else(Surrogates::default, |t| t.surrogates());
    let mut p = Parser::new(input, file_hint).with_surrogates(surrogates);
    let parsed = p.parse_program();
    let mut prog = p.lexical_error.take().map_or(parsed, Err).map_err(|error| {
        let Some(t) = &translated else { return error };
        let ParseError::Syntax { file, line, col, msg } = error;
        if file != file_hint { return ParseError::Syntax { file, line, col, msg } }
        Parser::new(source, file_hint).err_at(t.raw_offset(line, col), &msg)
    })?;
    flatten_inner_classes(&mut prog);
    Ok(prog)
}

fn flatten_inner_classes(program: &mut JavaProgram) {
    let mut flattened = Vec::new();
    for class in &mut program.classes {
        collect_inner_classes(class, &mut flattened);
    }
    program.classes.extend(flattened);
}

fn collect_inner_classes(class: &mut JavaClass, out: &mut Vec<JavaClass>) {
    let mut inners = class.take_inner_classes();
    for inner in &mut inners {
        collect_inner_classes(inner, out);
    }
    out.extend(inners);
}

pub(crate) struct Parser<'a> {
    pub(crate) source: &'a str,
    pub(crate) surrogates: Surrogates<'a>,
    pub(crate) pos: usize,
    lexical_error: Option<ParseError>,
    pub(crate) file: String,
    /// 显式 import 的 FQN 列表（`import x.y.Z;`）——parser 只收集，不做 FQN 化
    pub(crate) imports: Vec<String>,
    pub(crate) has_unbound_imports: bool,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(source: &'a str, file_hint: &str) -> Self {
        Self {
            source,
            surrogates: Surrogates::default(),
            pos: 0,
            lexical_error: None,
            file: file_hint.to_string(),
            imports: Vec::new(),
            has_unbound_imports: false,
        }
    }

    pub(crate) fn with_surrogates(mut self, surrogates: Surrogates<'a>) -> Self {
        self.surrogates = surrogates;
        self
    }

    fn location(&self, offset: usize) -> (usize, usize) {
        let (mut line, mut col) = (1usize, 1usize);
        let mut previous_cr = false;
        for &b in &self.source.as_bytes()[..offset.min(self.source.len())] {
            previous_cr = java_unicode::advance_position(b, previous_cr, &mut line, &mut col);
        }
        (line, col)
    }

    pub(crate) fn err_at(&self, offset: usize, msg: &str) -> ParseError {
        let (line, col) = self.location(offset);
        ParseError::Syntax { file: self.file.clone(), line, col, msg: msg.to_string() }
    }

    pub(crate) fn err(&self, msg: &str) -> ParseError {
        self.err_at(self.pos, msg)
    }

    // Class and method fragments are parsed with local offsets. Reattach an
    // error to its original source position at every enclosing slice.
    pub(crate) fn rebase_slice(&self, offset: usize, error: ParseError) -> ParseError {
        let ParseError::Syntax { file, line, col, msg } = error;
        let (start_line, start_col) = self.location(offset);
        ParseError::Syntax {
            file, line: start_line + line - 1,
            col: if line == 1 { start_col + col - 1 } else { col }, msg,
        }
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.pos).copied()
    }
    pub(crate) fn peek_at(&self, o: usize) -> Option<u8> {
        self.source.as_bytes().get(self.pos + o).copied()
    }
    pub(crate) fn starts_with(&self, s: &str) -> bool {
        self.source.as_bytes()[self.pos..].starts_with(s.as_bytes())
    }

    /// Leave semantic comments for the statement parser, even when malformed.
    pub(crate) fn local_ir_start(&self) -> Option<usize> {
        let comment = self.source.as_bytes().get(self.pos..)?.strip_prefix(b"/*")?;
        let offset = comment.iter().position(|byte| !byte.is_ascii_whitespace())?;
        let rest = comment[offset..].strip_prefix(b"@normalizer-ir")?;
        if rest.first().is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
            return None;
        }
        Some(self.source.len() - rest.len())
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            self.pos += 1;
            if matches!(c, b'\r' | b'\n') { break; }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), ParseError> {
        let start = self.pos;
        self.pos += 2;
        while self.pos + 1 < self.source.len() {
            if self.source.as_bytes()[self.pos] == b'*' && self.source.as_bytes()[self.pos + 1] == b'/' {
                self.pos += 2;
                return Ok(());
            }
            self.pos += 1;
        }
        self.pos = self.source.len();
        Err(self.err_at(start, "[JAVA-COMMENT] unclosed block comment"))
    }

    pub(crate) fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.pos += 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    self.skip_line_comment();
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    if self.local_ir_start().is_some() {
                        break;
                    }
                    if let Err(error) = self.skip_block_comment() {
                        self.lexical_error = Some(error);
                        break;
                    }
                }
                _ => break,
            }
        }
    }

    pub(crate) fn read_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == b'_' || c == b'$' => {
                self.pos += 1;
            }
            _ => return None,
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
                self.pos += 1;
            } else {
                break;
            }
        }
        Some(self.source[start..self.pos].to_owned())
    }

    pub(crate) fn read_qualified_ident(&mut self) -> Option<String> {
        let mut out = self.read_ident()?;
        loop {
            let save = self.pos;
            if self.eat_char(b'.') {
                if let Some(part) = self.read_ident() {
                    out.push('.');
                    out.push_str(&part);
                    continue;
                }
            }
            self.pos = save;
            break;
        }
        Some(out)
    }

    pub(crate) fn eat_char(&mut self, c: u8) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(crate) fn require_delimiter(&mut self, token: u8, expected: &str) -> Result<(), ParseError> {
        if self.eat_char(token) { return Ok(()); }
        Err(self.err(expected))
    }

    pub(crate) fn require_semicolon(&mut self, position: &str) -> Result<(), ParseError> {
        if self.eat_char(b';') { return Ok(()); }
        Err(self.err(&format!("[JAVA-SEMICOLON] expected ';' after {position}")))
    }

    pub(crate) fn eat_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let save = self.pos;
        if !self.starts_with(kw) {
            return false;
        }
        let next_pos = self.pos + kw.len();
        let after = self.source.as_bytes().get(next_pos).copied();
        match after {
            Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' => {
                self.pos = save;
                false
            }
            _ => {
                self.pos = next_pos;
                true
            }
        }
    }

    fn skip_quoted(&mut self, delimiter: u8) {
        self.pos += 1;
        while let Some(c) = self.peek() {
            self.pos += 1;
            match c {
                b'\\' => self.pos += 1,
                c if c == delimiter => break,
                _ => {}
            }
        }
    }

    pub(crate) fn skip_balanced_braces(&mut self) -> Result<&'a str, ParseError> {
        self.require_delimiter(b'{', "expected '{'")?;
        self.skip_balanced_body(b'{', b'}', "unclosed '{'")
    }

    // Both callers have consumed their ASCII opener in the decoded source.
    fn skip_balanced_body(
        &mut self, opening: u8, closing: u8, unclosed: &str,
    ) -> Result<&'a str, ParseError> {
        let start = self.pos;
        let mut depth = 1i32;
        while let Some(c) = self.peek() {
            match c {
                c if c == opening => {
                    depth += 1;
                    self.pos += 1;
                }
                c if c == closing => {
                    depth -= 1;
                    if depth == 0 {
                        let body = &self.source[start..self.pos];
                        self.pos += 1;
                        return Ok(body);
                    }
                    self.pos += 1;
                }
                b'"' | b'\'' => self.skip_quoted(c),
                b'/' if self.peek_at(1) == Some(b'/') => self.skip_line_comment(),
                b'/' if self.peek_at(1) == Some(b'*') => self.skip_block_comment()?,
                _ => self.pos += 1,
            }
        }
        Err(self.err(unclosed))
    }

    fn parse_nominal_base(&mut self) -> Result<String, ParseError> {
        let name = self.read_qualified_ident()
            .ok_or_else(|| self.err("expected nominal supertype"))?;
        self.skip_ws();
        if self.peek() == Some(b'<') {
            return Err(self.err("generic supertype requires explicit preprocessing"));
        }
        Ok(name)
    }

    fn parse_class_hierarchy(
        &mut self,
        is_interface: bool,
    ) -> Result<(Option<String>, Vec<String>), ParseError> {
        let supertype = if !is_interface && self.eat_keyword("extends") {
            Some(self.parse_nominal_base()?)
        } else {
            None
        };
        let interfaces_keyword = if is_interface { "extends" } else { "implements" };
        let mut interfaces = Vec::new();
        if self.eat_keyword(interfaces_keyword) {
            loop {
                interfaces.push(self.parse_nominal_base()?);
                if !self.eat_char(b',') {
                    break;
                }
            }
        }
        Ok((supertype, interfaces))
    }

    fn require_closed_declaration(&mut self) -> Result<(), ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'<') {
            return Err(self.err("[JAVA-TYPE-PARAMETERS] generic declarations require explicit preprocessing"));
        }
        Ok(())
    }

    pub(crate) fn read_annotations(&mut self) -> Result<Vec<AnnotationRef>, ParseError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.peek() != Some(b'@') {
                break;
            }
            self.pos += 1;
            let name = self
                .read_qualified_ident()
                .ok_or_else(|| self.err("expected annotation name"))?;
            self.skip_ws();
            let has_arguments = if self.peek() == Some(b'(') {
                self.pos += 1;
                let inner = self.skip_balanced_body(b'(', b')', "unclosed '('")?;
                let mut contents = Parser::new(inner, &self.file);
                contents.skip_ws();
                contents.peek().is_some()
            } else {
                false
            };
            out.push(AnnotationRef { name, has_arguments });
        }
        Ok(out)
    }

    fn parse_program(&mut self) -> Result<JavaProgram, ParseError> {
        let mut classes = Vec::new();

        self.skip_ws();
        let package_at = self.pos;
        if self.eat_keyword("package") {
            return Err(self.err_at(package_at, "[JAVA-PACKAGE] package declarations require explicit preprocessing"));
        }

        // Keep the exact ordinary imports. The closed input profile only
        // needs the presence, not the strings, of static or wildcard imports.
        while self.eat_keyword("import") {
            let is_static = self.eat_keyword("static");
            let name = self.read_qualified_ident()
                .ok_or_else(|| self.err("[JAVA-IMPORT] expected a qualified import name"))?;
            let wildcard = if self.eat_char(b'.') {
                self.require_delimiter(b'*', "[JAVA-IMPORT] expected wildcard after trailing dot")?;
                true
            } else { false };
            if !wildcard && !name.contains('.') {
                return Err(self.err("[JAVA-IMPORT] expected a qualified import name"));
            }
            self.require_delimiter(b';', "[JAVA-IMPORT] expected semicolon")?;
            if is_static || wildcard {
                self.has_unbound_imports = true;
            } else {
                self.imports.push(name);
            }
        }

        loop {
            self.skip_ws();
            if self.peek().is_none() {
                break;
            }
            if !self.eat_char(b';') {
                classes.push(self.parse_class()?);
            }
        }

        Ok(JavaProgram { classes })
    }

    fn take_class_kind(&mut self) -> Result<Option<bool>, ParseError> {
        if self.eat_keyword("class") {
            Ok(Some(false))
        } else if self.eat_keyword("interface") {
            Ok(Some(true))
        } else if self.eat_keyword("enum") {
            Err(self.err("enum requires explicit preprocessing"))
        } else {
            Ok(None)
        }
    }

    fn parse_class(&mut self) -> Result<JavaClass, ParseError> {
        let annotations = self.read_annotations()?;
        let modifiers = [
            "public", "private", "protected", "static", "final", "abstract", "sealed", "strictfp",
        ];
        while modifiers.iter().any(|keyword| self.eat_keyword(keyword)) {}
        let is_interface = self.take_class_kind()?
            .ok_or_else(|| self.err("expected class or interface declaration"))?;

        let simple_name = self
            .read_ident()
            .ok_or_else(|| self.err("expected class name"))?;

        self.require_closed_declaration()?;

        let (supertype, interfaces) = self.parse_class_hierarchy(is_interface)?;

        self.skip_ws();
        let body_at = self.pos + 1; // after the opening brace
        let body_text = self.skip_balanced_braces()?;

        let mut class = JavaClass {
            source_file: Some(self.file.clone()),
            imports: self.imports.clone(),
            has_unbound_imports: self.has_unbound_imports,
            simple_name,

            annotations,
            fields: Vec::new(),
            methods: Vec::new(),
            inner_classes: Vec::new(),
            supertype,
            interfaces,
            is_interface,
        };

        parse_class_body(body_text, &mut class, &self.file, self.surrogates.subspan(body_at)).map_err(|error| self.rebase_slice(body_at, error))?;

        Ok(class)
    }
}

fn parse_throws(p: &mut Parser<'_>) -> Result<Vec<String>, ParseError> {
    if !p.eat_keyword("throws") {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    loop {
        p.skip_ws();
        names.push(p.read_qualified_ident().ok_or_else(|| p.err("expected exception type after throws"))?);
        p.skip_ws();
        if !p.eat_char(b',') {
            break;
        }
    }
    Ok(names)
}

/// Whether a token cannot serve as an ordinary Java identifier. Contextual
/// keywords are not blanket reserved; literals and the single underscore are.
pub fn is_reserved_identifier(name: &str) -> bool {
    matches!(name,
        "_" | "abstract" | "assert" | "boolean" | "break" | "byte" | "case" | "catch" |
        "char" | "class" | "const" | "continue" | "default" | "do" | "double" | "else" |
        "enum" | "extends" | "final" | "finally" | "float" | "for" | "goto" | "if" |
        "implements" | "import" | "instanceof" | "int" | "interface" | "long" | "native" |
        "new" | "package" | "private" | "protected" | "public" | "return" | "short" |
        "static" | "strictfp" | "super" | "switch" | "synchronized" | "this" | "throw" |
        "throws" | "transient" | "try" | "void" | "volatile" | "while" | "true" | "false" | "null")
}

fn parse_class_body(
    body: &str,
    class: &mut JavaClass,
    file: &str,
    surrogates: Surrogates<'_>,
) -> Result<(), ParseError> {
    let mut p = Parser::new(body, file).with_surrogates(surrogates);

    loop {
        p.skip_ws();
        if p.peek().is_none() {
            break;
        }

        let annotations = p.read_annotations()?;

        let mut is_static = false;
        let mut is_final = false;
        let mut is_native = false;
        let mut native_modifier_conflict = false;
        let mut synchronized_at = None;
        loop {
            p.skip_ws();
            if p.eat_keyword("public")
                || p.eat_keyword("private")
                || p.eat_keyword("protected")
            {
                continue;
            }
            let modifier_at = p.pos;
            if p.eat_keyword("synchronized") {
                synchronized_at = Some(modifier_at);
                continue;
            }
            if p.eat_keyword("abstract") || p.eat_keyword("default")
                || p.eat_keyword("strictfp") || p.eat_keyword("transient")
            {
                native_modifier_conflict = true;
                continue;
            }
            if p.eat_keyword("native") {
                if is_native { return Err(p.err("[JAVA-NATIVE-DECLARATION] duplicate native modifier")); }
                is_native = true;
                continue;
            }
            if p.eat_keyword("static") {
                is_static = true;
                continue;
            }
            if p.eat_keyword("final") {
                is_final = true;
                continue;
            }
            break;
        }

        p.skip_ws();
        if let Some(at) = synchronized_at.filter(|_| !is_native) {
            return Err(p.err_at(at, "[JAVA-MONITOR] synchronized method requires explicit preprocessing"));
        }
        if let Some(nested_is_interface) = p.take_class_kind()? {
            if is_native {
                return Err(p.err("[JAVA-NATIVE-DECLARATION] a nested type cannot be native"));
            }
            let nested_simple = p
                .read_ident()
                .ok_or_else(|| p.err("expected nested class name"))?;
            p.require_closed_declaration()?;

            let (nested_supertype, nested_interfaces) =
                p.parse_class_hierarchy(nested_is_interface)?;

            p.skip_ws();
            let nested_at = p.pos + 1;
            let nested_body = p.skip_balanced_braces()?;
            let mut nested_class = JavaClass {
                source_file: Some(file.to_owned()),
                imports: class.imports.clone(),
                has_unbound_imports: class.has_unbound_imports,
                simple_name: format!("{}.{}", class.simple_name, nested_simple),
                annotations: annotations.clone(),
                fields: Vec::new(),
                methods: Vec::new(),
                inner_classes: Vec::new(),
                supertype: nested_supertype,
                interfaces: nested_interfaces,
                is_interface: nested_is_interface,
            };
            parse_class_body(
                nested_body,
                &mut nested_class,
                file,
                p.surrogates.subspan(nested_at),
            ).map_err(|error| p.rebase_slice(nested_at, error))?;
            class.inner_classes.push(nested_class);
            continue;
        }

        if p.peek() == Some(b'{') {
            return Err(p.err("[INITIALIZER-BLOCK-UNSUPPORTED] initializer block cannot be discarded; preprocess explicit initialization with original order and exceptions (implementation/skills/normalizer-preprocess/SKILL.md)"));
        }

        let save = p.pos;
        let first_ident = p.read_qualified_ident();

        if let Some(first) = first_ident.clone() {
            p.skip_ws();
            let constructor_simple_name = class
                .simple_name
                .rsplit('.')
                .next()
                .unwrap_or(&class.simple_name);
            if p.peek() == Some(b'(') && first == constructor_simple_name {
                if is_native { return Err(p.err("[JAVA-NATIVE-DECLARATION] constructor cannot be native")); }
                let (params, param_annotations) = parse_params(&mut p)?;
                p.skip_ws();
                let throws = parse_throws(&mut p)?;
                p.skip_ws();
                let body = if p.peek() == Some(b'{') {
                    let body_at = p.pos + 1;
                    let text = p.skip_balanced_braces()?;
                    Some(crate::body_parser::parse_method_body_text(
                        text,
                        file,
                        p.surrogates.subspan(body_at),
                    ).map_err(|error| p.rebase_slice(body_at, error))?)
                } else {
                    return Err(p.err("[JAVA-CONSTRUCTOR] expected constructor body"));
                };
                class.methods.push(JavaMethod {
                    throws,
                    name: first,
                    params,
                    param_annotations,
                    return_type: JavaType::Void,
                    is_static: false,
                    is_constructor: true,
                    is_native: false,
                    annotations: annotations.clone(),
                    body,
                });

                continue;
            }
        }

        p.pos = save;
        p.require_closed_declaration()?;
        let java_type = match parse_type(&mut p) {
            Some(t) => t,
            None => {
                return Err(p.err("[MEMBER-TYPE-UNPARSED] cannot discard an unparsed class member"))
            }
        };

        p.skip_ws();
        let member_at = p.pos;
        let member_name = p
            .read_ident()
            .ok_or_else(|| p.err("[MEMBER-NAME-UNPARSED] expected member name"))?;

        p.skip_ws();
        if p.peek() == Some(b'(') {
            if is_reserved_identifier(&member_name) {
                return Err(p.err_at(member_at, &format!("[JAVA-RESERVED-METHOD-NAME] {}.{}: reserved keyword used as a Java method name", class.full_name(), member_name)));
            }
            if is_native && class.is_interface() {
                return Err(p.err("[JAVA-NATIVE-DECLARATION] interface method cannot be native"));
            }
            if is_native && native_modifier_conflict {
                return Err(p.err("[JAVA-NATIVE-DECLARATION] incompatible native method modifier"));
            }
            let (params, param_annotations) = parse_params(&mut p)?;
            p.skip_ws();
            let throws = parse_throws(&mut p)?;
            p.skip_ws();
            let default_at = p.pos;
            if p.eat_keyword("default") {
                if is_native {
                    return Err(p.err_at(default_at, "[JAVA-NATIVE-DECLARATION] native method cannot have a default value"));
                }
                return Err(p.err_at(default_at, "[JAVA-DECLARATION-DEFAULT] method default value requires explicit preprocessing"));
            }
            p.skip_ws();
            let body = if is_native {
                if !p.eat_char(b';') {
                    return Err(p.err("[JAVA-NATIVE-DECLARATION] native method requires a semicolon, not a method body"));
                }
                None
            } else if p.peek() == Some(b'{') {
                let body_at = p.pos + 1;
                let text = p.skip_balanced_braces()?;
                Some(crate::body_parser::parse_method_body_text(
                    text,
                    file,
                    p.surrogates.subspan(body_at),
                ).map_err(|error| p.rebase_slice(body_at, error))?)
            } else {
                p.require_semicolon("bodyless method declaration")?;
                None
            };
            class.methods.push(JavaMethod {
                throws,
                name: member_name,
                params,
                param_annotations,
                return_type: java_type,
                is_static,
                is_constructor: false,
                is_native,
                annotations,
                body,
            });

        } else {
            if is_native { return Err(p.err("[JAVA-NATIVE-DECLARATION] field cannot be native")); }
            if is_reserved_identifier(&member_name) {
                return Err(p.err_at(member_at, "[JAVA-RESERVED-FIELD-NAME] reserved keyword used as a Java field name"));
            }
            p.skip_ws();
            // Java 语义：interface 字段隐式 public static final。
            // Parser 不建 public flag，但 static/final 必须在 AST 中体现，否则 class init
            // 看不到 `interface Result { int CODE = 100; }` 这种真实常量字段。
            let field_is_static = is_static || class.is_interface;
            let field_is_final = is_final || class.is_interface;

            // Validate the initializer syntax, but retain only its presence:
            // the source contract must reject it, never silently execute or drop it.
            let has_initializer = p.eat_char(b'=');
            if has_initializer {
                p.parse_initializer_expr(&java_type)?;
            }
            p.require_semicolon("field declaration")?;
            class.fields.push(JavaField {
                name: member_name,
                java_type,
                is_static: field_is_static,
                is_final: field_is_final,
                annotations,
                has_initializer,
            });
        }
    }

    Ok(())
}

pub(crate) fn parse_type(p: &mut Parser) -> Option<JavaType> {
    p.skip_ws();
    if p.eat_keyword("void") {
        return Some(JavaType::Void);
    }
    let ty = parse_type_atom(p, false)?;
    parse_array_type_suffix(p, ty)
}

// Empty dimension pairs in type/declarator syntax are distinct from the
// sized dimensions consumed by the array creation expression.
pub(crate) fn parse_array_type_suffix(p: &mut Parser, mut ty: JavaType) -> Option<JavaType> {
    while p.eat_char(b'[') {
        if !p.eat_char(b']') {
            return None;
        }
        ty = JavaType::Array(Box::new(ty));
    }
    Some(ty)
}

pub(crate) fn parse_creation_type(p: &mut Parser) -> Option<JavaType> {
    // In `new T[n]`, the caller consumes the sized dimension. An
    // empty generic list is the diamond only before a constructor call.
    parse_type_atom(p, true)
}

fn parse_type_atom(p: &mut Parser, allow_diamond: bool) -> Option<JavaType> {
    p.skip_ws();
    for prim in [
        "boolean", "byte", "short", "int", "long", "float", "double", "char",
    ] {
        if p.eat_keyword(prim) {
            return Some(JavaType::Primitive(prim.to_owned()));
        }
    }

    let name = p.read_qualified_ident()?;
    if !p.eat_char(b'<') {
        return Some(JavaType::Class(name));
    }
    if allow_diamond && p.eat_char(b'>') {
        p.skip_ws();
        return (p.peek() == Some(b'(')).then(|| JavaType::Generic(name, Vec::new()));
    }

    let mut args = Vec::new();
    loop {
        args.push(parse_type(p)?);
        if !p.eat_char(b',') {
            return p.eat_char(b'>').then(|| JavaType::Generic(name, args));
        }
    }
}

/// Parse an ordinary non-variadic method parameter list.
type ParsedParameters = (Vec<(JavaType, String)>, std::collections::BTreeMap<String, Vec<AnnotationRef>>);

fn parse_params(p: &mut Parser<'_>) -> Result<ParsedParameters, ParseError> {
    let invalid = |p: &Parser<'_>, reason: &str| p.err(&format!("[JAVA-PARAMETER] {reason}"));
    p.pos += 1; // Both declaration dispatches have matched the opening parenthesis.
    let mut params = Vec::new();
    let mut annotations = std::collections::BTreeMap::new();
    if p.eat_char(b')') { return Ok((params, annotations)); }
    let mut names = std::collections::BTreeSet::new();
    loop {
        let mut attributes = p.read_annotations()?;
        if p.eat_keyword("final") { attributes.extend(p.read_annotations()?); }
        let mut ty = parse_type(p).filter(|ty| *ty != JavaType::Void)
            .ok_or_else(|| invalid(p, "expected a parameter type"))?;
        p.skip_ws();
        let variadic_at = p.starts_with("...").then_some(p.pos);
        if variadic_at.is_some() { p.pos += 3; }
        p.skip_ws();
        let at = p.pos;
        let name = p.read_ident().ok_or_else(|| invalid(p, "expected a parameter name"))?;
        if is_reserved_identifier(&name) || !names.insert(name.clone()) {
            return Err(p.err_at(at, "[JAVA-PARAMETER] reserved or duplicate parameter name"));
        }
        if let Some(at) = variadic_at {
            if p.eat_char(b')') {
                return Err(p.err_at(at, "[JAVA-PARAMETER] variadic parameters require explicit preprocessing"));
            }
            return Err(invalid(p, "varargs must be the final parameter"));
        }
        ty = parse_array_type_suffix(p, ty)
            .ok_or_else(|| invalid(p, "expected a closing array bracket"))?;
        if !attributes.is_empty() { annotations.insert(name.clone(), attributes); }
        params.push((ty, name));
        if p.eat_char(b')') { return Ok((params, annotations)); }
        if !p.eat_char(b',') { return Err(invalid(p, "expected a comma or closing parenthesis")); }
    }
}
