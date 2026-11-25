//! 解析入口：Java 源码 → JavaProgram AST。
//!
//! 设计原则：
//! - 纯语法分析，不做类型推断/重载解析/框架识别
//! - 手写递归下降 parser，零外部依赖

use crate::ast::*;
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
    let mut p = Parser::new(source, file_hint);
    let mut prog = p.parse_program()?;
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
    pub(crate) src: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) file: String,
    pub(crate) package: String,
    /// 显式 import 的 FQN 列表（`import x.y.Z;`）——parser 只收集，不做 FQN 化
    pub(crate) imports: Vec<String>,
    /// 通配 import 的包前缀列表（`import x.y.*;` → 存 `"x.y"`）
    pub(crate) imports_wildcard: Vec<String>,
    /// 显式 static import 的完整成员名列表（`import static x.y.C.F;`）
    pub(crate) static_imports: Vec<String>,
    /// 通配 static import 的类型 FQN 列表（`import static x.y.C.*;` → 存 `"x.y.C"`）
    pub(crate) static_imports_wildcard: Vec<String>,
}

impl<'a> Parser<'a> {
    pub(crate) fn new(source: &'a str, file_hint: &str) -> Self {
        Self {
            src: source.as_bytes(),
            pos: 0,
            file: file_hint.to_string(),
            package: String::new(),
            imports: Vec::new(),
            imports_wildcard: Vec::new(),
            static_imports: Vec::new(),
            static_imports_wildcard: Vec::new(),
        }
    }

    pub(crate) fn err(&self, msg: &str) -> ParseError {
        let (mut line, mut col) = (1usize, 1usize);
        for &b in &self.src[..self.pos.min(self.src.len())] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        ParseError::Syntax {
            file: self.file.clone(),
            line,
            col,
            msg: msg.to_string(),
        }
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }
    pub(crate) fn peek_at(&self, o: usize) -> Option<u8> {
        self.src.get(self.pos + o).copied()
    }
    pub(crate) fn starts_with(&self, s: &str) -> bool {
        self.src[self.pos..].starts_with(s.as_bytes())
    }

    /// Leave semantic comments for the statement parser, even when malformed.
    pub(crate) fn local_ir_start(&self) -> Option<usize> {
        let comment = self.src.get(self.pos..)?.strip_prefix(b"/*")?;
        let offset = comment.iter().position(|byte| !byte.is_ascii_whitespace())?;
        let rest = comment[offset..].strip_prefix(b"@normalizer-ir")?;
        if rest.first().is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_') {
            return None;
        }
        Some(self.src.len() - rest.len())
    }

    pub(crate) fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(c) if c.is_ascii_whitespace() => {
                    self.pos += 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    while let Some(c) = self.peek() {
                        self.pos += 1;
                        if c == b'\n' {
                            break;
                        }
                    }
                }
                Some(b'/') if self.peek_at(1) == Some(b'*') => {
                    if self.local_ir_start().is_some() {
                        break;
                    }
                    self.pos += 2;
                    while self.pos + 1 < self.src.len() {
                        if self.src[self.pos] == b'*' && self.src[self.pos + 1] == b'/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
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
        Some(
            std::str::from_utf8(&self.src[start..self.pos])
                .ok()?
                .to_string(),
        )
    }

    pub(crate) fn read_qualified_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let first = self.read_ident()?;
        let mut out = first;
        loop {
            let save = self.pos;
            self.skip_ws();
            if self.peek() == Some(b'.') {
                self.pos += 1;
                self.skip_ws();
                match self.peek() {
                    Some(c) if c.is_ascii_alphabetic() || c == b'_' || c == b'$' => {
                        if let Some(part) = self.read_ident() {
                            out.push('.');
                            out.push_str(&part);
                            continue;
                        }
                    }
                    _ => {}
                }
                self.pos = save;
                break;
            } else {
                self.pos = save;
                break;
            }
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

    pub(crate) fn eat_keyword(&mut self, kw: &str) -> bool {
        self.skip_ws();
        let save = self.pos;
        if !self.starts_with(kw) {
            return false;
        }
        let next_pos = self.pos + kw.len();
        let after = self.src.get(next_pos).copied();
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

    pub(crate) fn skip_string_lit(&mut self) {
        self.pos += 1;
        while let Some(c) = self.peek() {
            if c == b'\\' {
                self.pos += 2;
                continue;
            }
            self.pos += 1;
            if c == b'"' {
                break;
            }
        }
    }

    pub(crate) fn skip_char_lit(&mut self) {
        self.pos += 1;
        while let Some(c) = self.peek() {
            if c == b'\\' {
                self.pos += 2;
                continue;
            }
            self.pos += 1;
            if c == b'\'' {
                break;
            }
        }
    }

    pub(crate) fn skip_balanced_braces(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'{') {
            return Err(self.err("expected '{'"));
        }
        let start = self.pos;
        let mut depth: i32 = 1;
        while let Some(c) = self.peek() {
            match c {
                b'{' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let body = std::str::from_utf8(&self.src[start..self.pos])
                            .unwrap_or_else(|e| {
                                panic!(
                                    "[PARSER-BODY-UTF8-FAIL] class body utf8 decode failed: {:?}",
                                    e
                                )
                            })
                            .to_string();
                        self.pos += 1;
                        return Ok(body);
                    }
                    self.pos += 1;
                }
                b'"' => {
                    self.skip_string_lit();
                }
                b'\'' => {
                    self.skip_char_lit();
                }
                b'/' if self.peek_at(1) == Some(b'/') => {
                    // line comment: skip to end of line
                    while let Some(c2) = self.peek() {
                        self.pos += 1;
                        if c2 == b'\n' {
                            break;
                        }
                    }
                }
                b'/' if self.peek_at(1) == Some(b'*') => {
                    // block comment (incl. JavaDoc): skip to */
                    self.pos += 2;
                    while self.pos + 1 < self.src.len() {
                        if self.src[self.pos] == b'*' && self.src[self.pos + 1] == b'/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        Err(self.err("unclosed '{'"))
    }

    pub(crate) fn skip_balanced_parens(&mut self) -> Result<String, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'(') {
            return Err(self.err("expected '('"));
        }
        let start = self.pos;
        let mut depth: i32 = 1;
        while let Some(c) = self.peek() {
            match c {
                b'(' => {
                    depth += 1;
                    self.pos += 1;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let body = std::str::from_utf8(&self.src[start..self.pos])
                            .unwrap_or_else(|e| panic!("[PARSER-METHOD-BODY-UTF8-FAIL] method body utf8 decode failed: {:?}", e))
                            .to_string();
                        self.pos += 1;
                        return Ok(body);
                    }
                    self.pos += 1;
                }
                b'"' => {
                    self.skip_string_lit();
                }
                b'\'' => {
                    self.skip_char_lit();
                }
                b'/' if self.peek_at(1) == Some(b'/') => {
                    while let Some(c2) = self.peek() {
                        self.pos += 1;
                        if c2 == b'\n' {
                            break;
                        }
                    }
                }
                b'/' if self.peek_at(1) == Some(b'*') => {
                    self.pos += 2;
                    while self.pos + 1 < self.src.len() {
                        if self.src[self.pos] == b'*' && self.src[self.pos + 1] == b'/' {
                            self.pos += 2;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
        Err(self.err("unclosed '('"))
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

    fn parse_class_type_params(&mut self) -> Result<Vec<String>, ParseError> {
        self.skip_ws();
        if self.peek() != Some(b'<') {
            return Ok(Vec::new());
        }

        self.pos += 1;
        let mut params = Vec::new();
        loop {
            self.skip_ws();
            let name = self
                .read_ident()
                .ok_or_else(|| self.err("expected type parameter name"))?;
            params.push(name);

            let mut depth: i32 = 0;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some(b'<') => {
                        depth += 1;
                        self.pos += 1;
                    }
                    Some(b'>') if depth == 0 => {
                        self.pos += 1;
                        return Ok(params);
                    }
                    Some(b'>') => {
                        depth -= 1;
                        self.pos += 1;
                    }
                    Some(b',') if depth == 0 => {
                        self.pos += 1;
                        break;
                    }
                    Some(b'"') => self.skip_string_lit(),
                    Some(b'\'') => self.skip_char_lit(),
                    Some(_) => {
                        self.pos += 1;
                    }
                    None => return Err(self.err("unclosed '<'")),
                }
            }
        }
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
            let mut args: Vec<(String, String)> = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'(') {
                let inner = self.skip_balanced_parens()?;
                for part in split_top_level_commas(&inner) {
                    let t = part.trim();
                    if let Some(eq) = t.find('=') {
                        args.push((t[..eq].trim().to_string(), t[eq + 1..].trim().to_string()));
                    } else if !t.is_empty() {
                        args.push(("value".to_string(), t.to_string()));
                    }
                }
            }
            out.push(AnnotationRef { name, args });
        }
        Ok(out)
    }

    fn parse_program(&mut self) -> Result<JavaProgram, ParseError> {
        let mut classes = Vec::new();

        self.skip_ws();
        if self.eat_keyword("package") {
            let name = self
                .read_qualified_ident()
                .ok_or_else(|| self.err("expected package name"))?;
            self.package = name;
            self.skip_ws();
            self.eat_char(b';');
        }

        // 收集 import 语句为 FQN 列表（普通 import 和通配 import 分开）。
        // parser 只保留原始语法信息，FQN 化决策由 resolver 在类型推断收口点完成。
        // `import static` 是合法 Java 语法；它不参与类型 FQN 化，不能因为
        // source-root 下无关文件含 static import 就阻断目标 entry 的解析。
        loop {
            self.skip_ws();
            if self.eat_keyword("import") {
                self.skip_ws();
                let is_static = self.eat_keyword("static");
                self.skip_ws();
                let mut fqn = String::new();
                let mut is_wildcard = false;
                // 读 qualified ident，允许结尾 .*
                while let Some(c) = self.peek() {
                    if c == b';' {
                        break;
                    }
                    if c == b'*' {
                        // 末尾可能是 `.*`——去掉最后的 `.`（fqn 最后字符）
                        if fqn.ends_with('.') {
                            fqn.pop();
                        }
                        is_wildcard = true;
                        self.pos += 1;
                    } else if c.is_ascii_whitespace() {
                        self.pos += 1;
                    } else {
                        fqn.push(c as char);
                        self.pos += 1;
                    }
                }
                self.eat_char(b';');
                if !fqn.is_empty() {
                    if is_static {
                        if is_wildcard {
                            self.static_imports_wildcard.push(fqn);
                        } else {
                            self.static_imports.push(fqn);
                        }
                    } else if is_wildcard {
                        self.imports_wildcard.push(fqn);
                    } else {
                        self.imports.push(fqn);
                    }
                }
            } else {
                break;
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

    fn parse_class(&mut self) -> Result<JavaClass, ParseError> {
        let annotations = self.read_annotations()?;

        loop {
            self.skip_ws();
            let mut consumed = false;
            for kw in &[
                "public",
                "private",
                "protected",
                "static",
                "final",
                "abstract",
                "sealed",
                "strictfp",
            ] {
                if self.eat_keyword(kw) {
                    consumed = true;
                    break;
                }
            }
            if !consumed {
                break;
            }
        }

        self.skip_ws();
        let is_interface = if self.eat_keyword("class") {
            false
        } else if self.eat_keyword("interface") {
            true
        } else if self.eat_keyword("enum") {
            return Err(self.err("enum requires explicit preprocessing"));
        } else {
            return Err(self.err("expected class or interface declaration"));
        };

        let simple_name = self
            .read_ident()
            .ok_or_else(|| self.err("expected class name"))?;

        let type_params = self.parse_class_type_params()?;

        let (supertype, interfaces) = self.parse_class_hierarchy(is_interface)?;

        self.skip_ws();
        let body_text = self.skip_balanced_braces()?;

        let mut class = JavaClass {
            source_file: Some(self.file.clone()),
            package: self.package.clone(),
            imports: self.imports.clone(),
            imports_wildcard: self.imports_wildcard.clone(),
            static_imports: self.static_imports.clone(),
            static_imports_wildcard: self.static_imports_wildcard.clone(),
            simple_name,
            type_params,
            annotations,
            fields: Vec::new(),
            methods: Vec::new(),
            inner_classes: Vec::new(),
            supertype,
            interfaces,
            is_interface,
        };

        parse_class_body(&body_text, &mut class, &self.package, &self.file)?;

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
    package: &str,
    file: &str,
) -> Result<(), ParseError> {
    let mut p = Parser::new(body, file);
    p.package = package.to_string();
    p.imports = class.imports.clone();
    p.imports_wildcard = class.imports_wildcard.clone();
    p.static_imports = class.static_imports.clone();
    p.static_imports_wildcard = class.static_imports_wildcard.clone();

    loop {
        p.skip_ws();
        if p.peek().is_none() {
            break;
        }

        if p.eat_char(b';') {
            continue;
        } // Java permits empty member declarations.
        let annotations = p.read_annotations()?;

        let mut is_static = false;
        let mut is_final = false;
        let mut is_volatile = false;
        let mut is_native = false;
        let mut native_modifier_conflict = false;
        loop {
            p.skip_ws();
            if p.eat_keyword("public")
                || p.eat_keyword("private")
                || p.eat_keyword("protected")
                || p.eat_keyword("synchronized")
            {
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
            if p.eat_keyword("volatile") {
                is_volatile = true;
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
        let nested_kind = if p.eat_keyword("class") {
            Some(false)
        } else if p.eat_keyword("interface") {
            Some(true)
        } else if p.eat_keyword("enum") {
            return Err(p.err("enum requires explicit preprocessing"));
        } else {
            None
        };
        if let Some(nested_is_interface) = nested_kind {
            let nested_simple = p
                .read_ident()
                .ok_or_else(|| p.err("expected nested class name"))?;
            let nested_type_params = p.parse_class_type_params()?;

            let (nested_supertype, nested_interfaces) =
                p.parse_class_hierarchy(nested_is_interface)?;

            p.skip_ws();
            let nested_body = p.skip_balanced_braces()?;
            let mut nested_class = JavaClass {
                source_file: Some(file.to_owned()),
                package: package.to_string(),
                imports: class.imports.clone(),
                imports_wildcard: class.imports_wildcard.clone(),
                static_imports: class.static_imports.clone(),
                static_imports_wildcard: class.static_imports_wildcard.clone(),
                simple_name: format!("{}.{}", class.simple_name, nested_simple),
                type_params: nested_type_params,
                annotations: annotations.clone(),
                fields: Vec::new(),
                methods: Vec::new(),
                inner_classes: Vec::new(),
                supertype: nested_supertype,
                interfaces: nested_interfaces,
                is_interface: nested_is_interface,
            };
            parse_class_body(
                &nested_body,
                &mut nested_class,
                package,
                file,
            )?;
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
            let simple_first = first.rsplit('.').next().unwrap_or(&first);
            let constructor_simple_name = class
                .simple_name
                .rsplit('.')
                .next()
                .unwrap_or(&class.simple_name);
            if p.peek() == Some(b'(')
                && simple_first == constructor_simple_name
                && !first.contains('.')
            {
                if is_native { return Err(p.err("[JAVA-NATIVE-DECLARATION] constructor cannot be native")); }
                let params_text = p.skip_balanced_parens()?;
                let (params, is_varargs, param_annotations) = parse_params(&params_text)?;
                p.skip_ws();
                let throws = parse_throws(&mut p)?;
                p.skip_ws();
                let body = if p.peek() == Some(b'{') {
                    let text = p.skip_balanced_braces()?;
                    Some(crate::body_parser::parse_method_body_text(
                        &text,
                        file,
                        package,
                        class.imports.clone(),
                        class.imports_wildcard.clone(),

                    )?)
                } else {
                    p.eat_char(b';');
                    None
                };
                class.methods.push(JavaMethod {
                    throws,
                    name: first,
                    type_params: Vec::new(),
                    params,
                    param_annotations,
                    return_type: JavaType::Void,
                    is_static: false,
                    is_constructor: true,
                    is_native: false,
                    is_varargs,
                    annotations: annotations.clone(),
                    body,
                });

                continue;
            }
        }

        p.pos = save;
        let method_type_params = p.parse_class_type_params()?;
        let java_type = match parse_type(&mut p) {
            Some(t) => t,
            None => {
                return Err(p.err("[MEMBER-TYPE-UNPARSED] cannot discard an unparsed class member"))
            }
        };

        p.skip_ws();
        let member_name = p
            .read_ident()
            .ok_or_else(|| p.err("[MEMBER-NAME-UNPARSED] expected member name"))?;

        p.skip_ws();
        if p.peek() == Some(b'(') {
            if is_reserved_identifier(&member_name) {
                return Err(p.err(&format!("[JAVA-RESERVED-METHOD-NAME] {}.{}: reserved keyword used as a Java method name", class.full_name(), member_name)));
            }
            if is_native && class.is_interface() {
                return Err(p.err("[JAVA-NATIVE-DECLARATION] interface method cannot be native"));
            }
            if is_native && (native_modifier_conflict || is_volatile) {
                return Err(p.err("[JAVA-NATIVE-DECLARATION] incompatible native method modifier"));
            }
            let params_text = p.skip_balanced_parens()?;
            let (params, is_varargs, param_annotations) = parse_params(&params_text)?;
            p.skip_ws();
            let throws = parse_throws(&mut p)?;
            p.skip_ws();
            if p.eat_keyword("default") {
                if is_native {
                    return Err(p.err("[JAVA-NATIVE-DECLARATION] native method cannot have a default value"));
                }
                while let Some(c) = p.peek() {
                    if c == b';' {
                        break;
                    }
                    if c == b'(' {
                        let _ = p.skip_balanced_parens()?;
                    } else if c == b'{' {
                        let _ = p.skip_balanced_braces()?;
                    } else {
                        p.pos += 1;
                    }
                    p.skip_ws();
                }
            }
            p.skip_ws();
            let body = if is_native {
                if !p.eat_char(b';') {
                    return Err(p.err("[JAVA-NATIVE-DECLARATION] native method requires a semicolon, not a method body"));
                }
                None
            } else if p.peek() == Some(b'{') {
                let text = p.skip_balanced_braces()?;
                Some(crate::body_parser::parse_method_body_text(
                    &text,
                    file,
                    package,
                    class.imports.clone(),
                    class.imports_wildcard.clone(),

                )?)
            } else {
                p.eat_char(b';');
                None
            };
            class.methods.push(JavaMethod {
                throws,
                name: member_name,
                type_params: method_type_params,
                params,
                param_annotations,
                return_type: java_type,
                is_static,
                is_constructor: false,
                is_native,
                is_varargs,
                annotations,
                body,
            });

        } else {
            if is_native { return Err(p.err("[JAVA-NATIVE-DECLARATION] field cannot be native")); }
            p.skip_ws();
            // Java 语义：interface 字段隐式 public static final。
            // Parser 不建 public flag，但 static/final 必须在 AST 中体现，否则 class init
            // 看不到 `interface Result { int CODE = 100; }` 这种真实常量字段。
            let field_is_static = is_static || class.is_interface;
            let field_is_final = is_final || class.is_interface;

            // 解析字段 initializer 表达式。
            // PR-ClassLoader: 字段 initializer 解析（static 和 instance 统一）
            //
            // Java 语义：
            // - instance 字段 initializer 在实例化时（ctor body 前）执行，可访问 this
            // - static 字段 initializer 在类加载时执行，处于 static context，无 enclosing this
            //
            // 执行入口分别由 initialize_instance_fields / ensure_class_initialized 负责。
            let initializer = if p.eat_char(b'=') {
                p.skip_ws();
                let expr = p.parse_initializer_expr(&java_type)?;
                p.skip_ws();
                Some(expr)
            } else {
                None
            };
            p.eat_char(b';');
            class.fields.push(JavaField {
                name: member_name,
                java_type,
                is_static: field_is_static,
                is_final: field_is_final,
                is_volatile,
                annotations,
                initializer,
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
    for prim in &[
        "boolean", "byte", "short", "int", "long", "float", "double", "char",
    ] {
        if p.eat_keyword(prim) {
            let mut t = JavaType::Primitive(prim.to_string());
            p.skip_ws();
            while p.peek() == Some(b'[') {
                p.pos += 1;
                p.skip_ws();
                if p.eat_char(b']') {
                    t = JavaType::Array(Box::new(t));
                } else {
                    return None;
                }
                p.skip_ws();
            }
            return Some(t);
        }
    }
    let name = p.read_qualified_ident()?;
    let mut t = JavaType::Class(name);
    p.skip_ws();
    if p.peek() == Some(b'<') {
        p.pos += 1;
        let mut args: Vec<JavaType> = Vec::new();
        loop {
            p.skip_ws();
            if p.peek() == Some(b'?') {
                p.pos += 1;
                p.skip_ws();
                if p.eat_keyword("extends") || p.eat_keyword("super") {
                    let _ = parse_type(p);
                }
                args.push(JavaType::Class("?".to_string()));
            } else if let Some(inner) = parse_type(p) {
                args.push(inner);
            }
            p.skip_ws();
            if p.eat_char(b',') {
                continue;
            }
            if p.peek() == Some(b'>') {
                p.pos += 1;
                break;
            }
            break;
        }
        let class_name = if let JavaType::Class(n) = t {
            n
        } else {
            String::new()
        };
        t = JavaType::Generic(class_name, args);
    }
    p.skip_ws();
    while p.peek() == Some(b'[') {
        p.pos += 1;
        p.skip_ws();
        if p.eat_char(b']') {
            t = JavaType::Array(Box::new(t));
        } else {
            return None;
        }
        p.skip_ws();
    }
    Some(t)
}

/// 解析参数列表，返回 (参数列表, 最后一个参数是否 varargs)。
///
/// Java 语义：varargs 只能是方法的最后一个参数。parser 将 `T...` 转换为 `T[]`
/// 但同时返回 is_varargs 位以供下游（JavaMethod / MethodSignature）使用。
type ParsedParameters = (Vec<(JavaType, String)>, bool, std::collections::BTreeMap<String, Vec<AnnotationRef>>);

fn parse_params(inner: &str) -> Result<ParsedParameters, ParseError> {
    let mut annotations = std::collections::BTreeMap::new();
    let parts = split_top_level_commas(inner);
    let mut out = Vec::new();
    let mut last_is_varargs = false;
    let total_parts = parts.len();
    for (idx, part) in parts.iter().enumerate() {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cleaned = strip_leading_annotations(trimmed);
        let cleaned = cleaned.trim_start_matches("final ").trim().to_string();
        if let Some((type_str, name)) = split_type_and_name(&cleaned) {
            let is_varargs = type_str.contains("...");
            let base_type_str = type_str.replace("...", "").trim().to_string();
            let mut pp = Parser::new(&base_type_str, "<params>");
            if let Some(mut t) = parse_type(&mut pp) {
                if is_varargs {
                    t = JavaType::Array(Box::new(t));
                    // 只有最后一个参数才能是 varargs（Java 语义约束）
                    if idx == total_parts - 1 {
                        last_is_varargs = true;
                    }
                }
                let mut declaration = Parser::new(trimmed, "<parameter>");
                let attributes = declaration.read_annotations()?;
                if !attributes.is_empty() { annotations.insert(name.clone(), attributes); }
                out.push((t, name));
            }
        }
    }
    Ok((out, last_is_varargs, annotations))
}

fn split_type_and_name(s: &str) -> Option<(String, String)> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' {
            i -= 1;
        } else {
            break;
        }
    }
    if i >= bytes.len() {
        return None;
    }
    let name = std::str::from_utf8(&bytes[i..]).ok()?.to_string();
    let type_str = std::str::from_utf8(&bytes[..i]).ok()?.trim().to_string();
    if type_str.is_empty() || name.is_empty() {
        return None;
    }
    Some((type_str, name))
}

fn strip_leading_annotations(s: &str) -> String {
    let mut out = s.to_string();
    loop {
        let trimmed = out.trim_start();
        if trimmed.starts_with('@') {
            let bytes = trimmed.as_bytes();
            let mut i = 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            let rest = &trimmed[i..];
            let rest = rest.trim_start();
            if rest.starts_with('(') {
                let mut depth = 0i32;
                let mut j = 0usize;
                let b = rest.as_bytes();
                while j < b.len() {
                    match b[j] {
                        b'(' => {
                            depth += 1;
                        }
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                out = rest[j..].trim_start().to_string();
            } else {
                out = rest.to_string();
            }
        } else {
            break;
        }
    }
    out
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth_paren: i32 = 0;
    let mut depth_angle: i32 = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'<' => depth_angle += 1,
            b'>' => depth_angle -= 1,
            b',' if depth_paren == 0 && depth_angle == 0 => {
                out.push(std::mem::take(&mut current));
                i += 1;
                continue;
            }
            _ => {}
        }
        current.push(c as char);
        i += 1;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
