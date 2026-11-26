//! 方法体解析：Java 语句/表达式 → AST。
//!
//! 设计原则：
//! - 手写递归下降 + 表达式优先级爬升（precedence climbing）
//! - 解析失败 → ParseError，不静默跳过
//! - 复用 parser.rs 的 Parser struct 和 helper（通过 pub(crate)）

use crate::ast::*;
use crate::java_unicode::Surrogates;
use crate::parser::{parse_array_type_suffix, parse_creation_type, parse_type, ParseError, Parser};

pub(crate) fn parse_method_body_text(
    body_text: &str,
    file: &str,
    surrogates: Surrogates<'_>,
) -> Result<Vec<JavaStatement>, ParseError> {
    let mut p = Parser::new(body_text, file).with_surrogates(surrogates);
    p.parse_statements_until_end()
}

impl<'a> Parser<'a> {
    /// 解析语句列表直到输入结束（用于方法体顶层）。
    pub(crate) fn parse_statements_until_end(&mut self) -> Result<Vec<JavaStatement>, ParseError> {
        let mut out = Vec::new();
        loop {
            self.skip_ws();
            if self.peek().is_none() {
                break;
            }
            let stmt = self.parse_statement(&mut out)?;
            out.push(stmt);
        }
        Ok(out)
    }

    fn parse_braced_statements(&mut self) -> Result<Vec<JavaStatement>, ParseError> {
        self.skip_ws();
        let body_at = self.pos + 1;
        let text = self.skip_balanced_braces()?;
        parse_method_body_text(text, &self.file, self.surrogates.subspan(body_at)).map_err(|error| self.rebase_slice(body_at, error))
    }

    /// Return the last statement from this source position. A local declaration
    /// with commas inserts earlier declarators into the same statement list.
    fn parse_statement(&mut self, earlier: &mut Vec<JavaStatement>) -> Result<JavaStatement, ParseError> {
        self.skip_ws();

        if let Some(start) = self.local_ir_start() {
            let end = self.source.as_bytes()[start..].windows(2).position(|part| part == b"*/")
                .map(|offset| start + offset).ok_or_else(|| self.err("unterminated @normalizer-ir comment"))?;
            let source = &self.source[start..end];
            let directive = normalizer_ir::local::parse(source).map_err(|error| self.err(&format!("invalid @normalizer-ir: {error}")))?;
            self.pos = end + 2;
            return Ok(JavaStatement::LocalIr(directive));
        }

        // Java empty statement: a standalone `;` is a legal no-op.
        if self.eat_char(b';') {
            return Ok(JavaStatement::Block(Vec::new()));
        }

        // 块语句 { ... }
        if self.peek() == Some(b'{') {
            return Ok(JavaStatement::Block(self.parse_braced_statements()?));
        }

        // 关键字分发
        if self.eat_keyword("if") {
            return self.parse_if();
        }
        if self.eat_keyword("while") {
            return self.parse_while();
        }
        if self.eat_keyword("for") {
            return self.parse_for_or_foreach();
        }
        if self.eat_keyword("return") {
            return self.parse_return();
        }
        if self.eat_keyword("throw") {
            return self.parse_throw();
        }
        if self.eat_keyword("try") {
            return self.parse_try();
        }
        let synchronized_at = self.pos;
        if self.eat_keyword("synchronized") {
            return Err(self.err_at(synchronized_at, "[JAVA-MONITOR] synchronized block requires explicit preprocessing"));
        }
        for keyword in ["switch", "do", "assert"] {
            if self.eat_keyword(keyword) {
                return Err(self.err(&format!("{keyword} requires explicit preprocessing")));
            }
        }
        let jump_at = self.pos;
        for keyword in ["break", "continue"] {
            if self.eat_keyword(keyword) {
                self.pos = jump_at;
                return Err(self.err(&format!(
                    "[JAVA-LOOP-CONTROL] raw {keyword} requires explicit loop preprocessing"
                )));
            }
        }
        // 尝试变量声明：Type name ... 或 Type name = expr;
        let save = self.pos;
        if let Some((first, mut rest)) = self.try_parse_vardecl()? {
            if let Some(last) = rest.pop() {
                earlier.push(first);
                earlier.extend(rest);
                return Ok(last);
            }
            return Ok(first);
        }
        self.pos = save;

        // 否则是表达式语句（含赋值）
        let expr = self.parse_expression()?;
        self.skip_ws();
        // 赋值表达式在 statement 位置仍归一为 JavaStatement::Assign，
        // 让 loop lift / 静态分析等 statement-level 语义继续看到赋值信号。
        if let JavaExpr::Assign { target, value } = expr {
            self.require_semicolon("assignment")?;
            return Ok(JavaStatement::Assign {
                target: *target,
                value: *value,
            });
        }
        self.require_semicolon("expression statement")?;
        Ok(JavaStatement::ExprStmt(expr))
    }

    fn compound_assign_ops() -> &'static [(&'static str, JavaBinOp)] {
        &[
            ("+=", JavaBinOp::Add),
            ("-=", JavaBinOp::Sub),
            ("*=", JavaBinOp::Mul),
            ("/=", JavaBinOp::Div),
            ("%=", JavaBinOp::Mod),
            ("&=", JavaBinOp::BitAnd),
            ("|=", JavaBinOp::BitOr),
            ("^=", JavaBinOp::BitXor),
            ("<<=", JavaBinOp::Shl),
            (">>=", JavaBinOp::Shr),
            (">>>=", JavaBinOp::UShr),
        ]
    }

    fn is_at_compound_assign_op(&mut self) -> bool {
        self.skip_ws();
        Self::compound_assign_ops()
            .iter()
            .any(|(sym, _)| self.starts_with(sym))
    }

    fn try_compound_assign_op(&mut self) -> Option<JavaBinOp> {
        self.skip_ws();
        for (sym, op) in Self::compound_assign_ops() {
            if self.starts_with(sym) {
                self.pos += sym.len();
                return Some(*op);
            }
        }
        None
    }

    fn parse_if(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'(') {
            return Err(self.err("expected '(' after 'if'"));
        }
        let condition = self.parse_expression()?;
        self.skip_ws();
        if !self.eat_char(b')') {
            return Err(self.err("expected ')' after if condition"));
        }
        let then_body = self.parse_single_or_block()?;
        self.skip_ws();
        let else_body = if self.eat_keyword("else") {
            Some(self.parse_single_or_block()?)
        } else {
            None
        };
        Ok(JavaStatement::If {
            cond: condition,
            then_block: then_body,
            else_block: else_body,
        })
    }

    fn parse_while(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'(') {
            return Err(self.err("expected '(' after 'while'"));
        }
        let condition = self.parse_expression()?;
        self.skip_ws();
        if !self.eat_char(b')') {
            return Err(self.err("expected ')' after while condition"));
        }
        let body = self.parse_single_or_block()?;
        Ok(JavaStatement::While {
            cond: condition,
            body,
        })
    }

    fn parse_for_or_foreach(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'(') {
            return Err(self.err("expected '(' after 'for'"));
        }
        // 尝试识别 for-each: Type name : expr
        let save = self.pos;
        if let Some(fe) = self.try_parse_foreach_head()? {
            let body = self.parse_single_or_block()?;
            return Ok(JavaStatement::ForEach {
                var_type: fe.0,
                var_name: fe.1,
                iterable: fe.2,
                body,
            });
        }
        self.pos = save;

        // 普通 for: (init; cond; update) body
        let init: Option<Box<JavaStatement>> = {
            self.skip_ws();
            if self.peek() == Some(b';') {
                self.pos += 1;
                None
            } else {
                // init 可以是 vardecl 或 expression
                let stmt = if let Some((v, rest)) = self.try_parse_vardecl()? {
                    if !rest.is_empty() {
                        return Err(self.err("[JAVA-FOR-INITIALIZER] only one induction declaration is supported"));
                    }
                    v
                } else {
                    let e = self.parse_expression()?;
                    self.skip_ws();
                    self.require_semicolon("for initializer")?;
                    JavaStatement::ExprStmt(e)
                };
                Some(Box::new(stmt))
            }
        };
        self.skip_ws();
        let condition = if self.peek() == Some(b';') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.skip_ws();
        self.require_semicolon("for condition")?;
        self.skip_ws();
        let update = if self.peek() == Some(b')') {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.skip_ws();
        if !self.eat_char(b')') {
            return Err(self.err("expected ')' after for clauses"));
        }
        let body = self.parse_single_or_block()?;
        Ok(JavaStatement::For {
            init,
            cond: condition,
            update,
            body,
        })
    }

    fn try_parse_foreach_head(
        &mut self,
    ) -> Result<Option<(JavaType, String, JavaExpr)>, ParseError> {
        let save = self.pos;
        // 跳过可选 final
        self.skip_ws();
        self.eat_keyword("final");
        let var_type = match parse_type(self) {
            Some(t) => t,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        let var_name = match self.read_ident() {
            Some(n) => n,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        if !self.eat_char(b':') {
            self.pos = save;
            return Ok(None);
        }
        let iterable = self.parse_expression()?;
        self.skip_ws();
        if !self.eat_char(b')') {
            return Err(self.err("expected ')' after for-each"));
        }
        Ok(Some((var_type, var_name, iterable)))
    }

    fn parse_return(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b';') {
            self.pos += 1;
            return Ok(JavaStatement::Return(None));
        }
        let expr = self.parse_expression()?;
        self.skip_ws();
        self.require_semicolon("return value")?;
        Ok(JavaStatement::Return(Some(expr)))
    }

    fn parse_throw(&mut self) -> Result<JavaStatement, ParseError> {
        let expr = self.parse_expression()?;
        self.skip_ws();
        self.require_semicolon("throw expression")?;
        Ok(JavaStatement::Throw(expr))
    }

    fn parse_try(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            return Err(self.err("try-with-resources is not supported in normalized Java"));
        }
        let try_body = self.parse_braced_statements()?;

        let mut catches = Vec::new();
        loop {
            self.skip_ws();
            if !self.eat_keyword("catch") {
                break;
            }
            self.skip_ws();
            if !self.eat_char(b'(') {
                return Err(self.err("expected '(' after 'catch'"));
            }
            self.skip_ws();
            self.eat_keyword("final");
            let exception_class = match parse_type(self) {
                Some(JavaType::Class(name)) => name,
                _ => return Err(self.err("catch requires an exception class name")),
            };
            self.skip_ws();
            if self.peek() == Some(b'|') {
                return Err(self.err("multi-catch is not supported in normalized Java"));
            }
            let var_name = self
                .read_ident()
                .ok_or_else(|| self.err("expected catch var name"))?;
            self.skip_ws();
            if !self.eat_char(b')') {
                return Err(self.err("expected ')' after catch clause"));
            }
            let body = self.parse_braced_statements()?;
            catches.push(CatchClause {
                exception_class,
                var_name,
                body,
            });
        }

        self.skip_ws();
        let finally = if self.eat_keyword("finally") {
            Some(self.parse_braced_statements()?)
        } else {
            None
        };

        Ok(JavaStatement::Try {
            body: try_body,
            catches,
            finally_block: finally,
        })
    }

    fn parse_single_or_block(&mut self) -> Result<Vec<JavaStatement>, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'{') {
            self.parse_braced_statements()
        } else {
            let mut body = Vec::new();
            let stmt = self.parse_statement(&mut body)?;
            if matches!(stmt, JavaStatement::VarDecl { .. }) {
                return Err(self.err("[JAVA-LOCAL-SCOPE] a controlled local declaration requires braces"));
            }
            body.push(stmt);
            Ok(body)
        }
    }

    /// 尝试解析变量声明 `Type name (= expr)?;` 或 `Type n1 = e1, n2 = e2;`。
    /// 失败时回退位置并返回 None。
    /// Parse an initializer expression for a variable declaration.
    ///
    /// Java allows an array initializer shorthand `{a, b, c}` only in variable
    /// declaration initializers and in annotation element values. When the
    /// declared type is an array and we see `{`, synthesize `new T[]{a,b,c}`
    /// (semantically equivalent to the shorthand form). This keeps parser
    /// accepting legal Java syntax without affecting normalized semantics.
    pub(crate) fn parse_initializer_expr(
        &mut self,
        declared_ty: &JavaType,
    ) -> Result<JavaExpr, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'{') {
            if let JavaType::Array(elem) = declared_ty {
                return self.parse_array_initializer(elem);
            }
        }
        self.parse_expression()
    }

    fn try_parse_vardecl(&mut self) -> Result<Option<(JavaStatement, Vec<JavaStatement>)>, ParseError> {
        let save = self.pos;
        self.skip_ws();
        let annotation_at = self.pos;
        let mut annotations = self.read_annotations()?;
        if self.eat_keyword("final") {
            annotations.extend(self.read_annotations()?);
        }
        if annotations.iter().any(|annotation| annotation.name() != "java.lang.SuppressWarnings") {
            return Err(self.err_at(annotation_at, "[JAVA-LOCAL-ANNOTATION] unsupported local variable annotation"));
        }
        let base_ty = match parse_type(self) {
            Some(t) => t,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        let name = match self.read_ident() {
            Some(n) => n,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        let ty = match parse_array_type_suffix(self, base_ty.clone()) {
            Some(t) => t,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        // 后续必须是 `=`、`;` 或 `,`——否则可能是 `Type.method()` 被误判
        match self.peek() {
            Some(b'=') | Some(b';') | Some(b',') => {}
            _ => {
                self.pos = save;
                return Ok(None);
            }
        }

        let init = if self.eat_char(b'=') {
            Some(self.parse_initializer_expr(&ty)?)
        } else {
            None
        };
        self.skip_ws();

        // Additional declarators execute in the same scope and source order.
        let mut extras: Vec<JavaStatement> = Vec::new();
        while self.eat_char(b',') {
            self.skip_ws();
            let n2 = self
                .read_ident()
                .ok_or_else(|| self.err("expected var name in multi-decl"))?;
            self.skip_ws();
            let ty2 = parse_array_type_suffix(self, base_ty.clone())
                .ok_or_else(|| self.err("expected ']' in array declarator"))?;
            self.skip_ws();
            let i2 = if self.eat_char(b'=') {
                Some(self.parse_initializer_expr(&ty2)?)
            } else {
                None
            };
            extras.push(JavaStatement::VarDecl {
                name: n2,
                java_type: ty2,
                initializer: i2,
            });
            self.skip_ws();
        }
        self.require_semicolon("local variable declaration")?;

        let main = JavaStatement::VarDecl {
            name,
            java_type: ty,
            initializer: init,
        };
        Ok(Some((main, extras)))
    }

    // ============ 表达式解析（优先级爬升）============

    /// 表达式入口（从最低优先级开始）。
    pub(crate) fn parse_expression(&mut self) -> Result<JavaExpr, ParseError> {
        self.parse_assignment()
    }

    /// 赋值表达式 `target = value`（右结合，优先级低于三元）。
    fn parse_assignment(&mut self) -> Result<JavaExpr, ParseError> {
        let target = self.parse_ternary()?;
        self.skip_ws();
        if self.peek() == Some(b'=') && self.peek_at(1) != Some(b'=') {
            self.pos += 1;
            let value = self.parse_assignment()?;
            return Ok(JavaExpr::Assign {
                target: Box::new(target),
                value: Box::new(value),
            });
        }
        if let Some(op) = self.try_compound_assign_op() {
            let rhs = self.parse_assignment()?;
            let value = JavaExpr::BinOp {
                op,
                left: Box::new(target.clone()),
                right: Box::new(rhs),
            };
            return Ok(JavaExpr::Assign {
                target: Box::new(target),
                value: Box::new(value),
            });
        }
        Ok(target)
    }

    /// 三元 `c ? a : b`（右结合）。
    fn parse_ternary(&mut self) -> Result<JavaExpr, ParseError> {
        let cond = self.parse_binary(1)?;
        self.skip_ws();
        if self.peek() == Some(b'?') && self.peek_at(1) != Some(b'?') {
            self.pos += 1;
            let then_e = self.parse_expression()?; // 右结合
            self.skip_ws();
            if !self.eat_char(b':') {
                return Err(self.err("expected ':' in ternary"));
            }
            let else_e = self.parse_expression()?;
            return Ok(JavaExpr::Ternary {
                cond: Box::new(cond),
                then_expr: Box::new(then_e),
                else_expr: Box::new(else_e),
            });
        }
        Ok(cond)
    }

    /// 二元运算（优先级爬升）。
    fn parse_binary(&mut self, min_prec: u8) -> Result<JavaExpr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            self.skip_ws();
            let operator = self.pos;
            if self.eat_keyword("instanceof") {
                self.pos = operator;
                return Err(self.err("[JAVA-UNSUPPORTED-TYPE-TEST] instanceof requires explicit preprocessing"));
            }
            let (op, prec, len) = match self.peek_binop() {
                Some(t) => t,
                None => break,
            };
            if prec < min_prec {
                break;
            }
            self.pos += len;
            let right = self.parse_binary(prec + 1)?;
            left = JavaExpr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// 查看当前位置的二元运算符（不消费）。返回 (op, precedence, byte_len)。
    fn peek_binop(&mut self) -> Option<(JavaBinOp, u8, usize)> {
        self.skip_ws();
        // `a += b` / `a <<= b` 是语句级复合赋值，不是表达式里的
        // `a + (= b)`；表达式解析必须停在复合赋值符号前，留给
        // parse_statement 的 try_compound_assign_op 处理。
        if self.is_at_compound_assign_op() {
            return None;
        }
        // 多字节运算符优先
        if self.starts_with("||") {
            return Some((JavaBinOp::Or, 3, 2));
        }
        if self.starts_with("&&") {
            return Some((JavaBinOp::And, 4, 2));
        }
        if self.starts_with("==") {
            return Some((JavaBinOp::Eq, 8, 2));
        }
        if self.starts_with("!=") {
            return Some((JavaBinOp::Ne, 8, 2));
        }
        if self.starts_with("<=") {
            return Some((JavaBinOp::Le, 9, 2));
        }
        if self.starts_with(">=") {
            return Some((JavaBinOp::Ge, 9, 2));
        }
        if self.starts_with(">>>") {
            return Some((JavaBinOp::UShr, 10, 3));
        }
        if self.starts_with("<<") {
            return Some((JavaBinOp::Shl, 10, 2));
        }
        if self.starts_with(">>") {
            return Some((JavaBinOp::Shr, 10, 2));
        }
        // 单字节运算符
        match self.peek() {
            Some(b'|') => Some((JavaBinOp::BitOr, 5, 1)),
            Some(b'^') => Some((JavaBinOp::BitXor, 6, 1)),
            Some(b'&') => Some((JavaBinOp::BitAnd, 7, 1)),
            Some(b'<') => Some((JavaBinOp::Lt, 9, 1)),
            Some(b'>') => Some((JavaBinOp::Gt, 9, 1)),
            Some(b'+') => Some((JavaBinOp::Add, 11, 1)),
            Some(b'-') => Some((JavaBinOp::Sub, 11, 1)),
            Some(b'*') => Some((JavaBinOp::Mul, 12, 1)),
            Some(b'/') => Some((JavaBinOp::Div, 12, 1)),
            Some(b'%') => Some((JavaBinOp::Mod, 12, 1)),
            _ => None,
        }
    }

    /// 一元运算 + cast。
    fn parse_unary(&mut self) -> Result<JavaExpr, ParseError> {
        self.skip_ws();
        // 前置 ++ / --
        if self.starts_with("++") {
            self.pos += 2;
            let inner = self.parse_unary()?;
            return Ok(JavaExpr::UnaryOp {
                op: JavaUnaryOp::PreInc,
                operand: Box::new(inner),
            });
        }
        if self.starts_with("--") {
            self.pos += 2;
            let inner = self.parse_unary()?;
            return Ok(JavaExpr::UnaryOp {
                op: JavaUnaryOp::PreDec,
                operand: Box::new(inner),
            });
        }
        match self.peek() {
            Some(b'!') => {
                self.pos += 1;
                let inner = self.parse_unary()?;
                return Ok(JavaExpr::UnaryOp {
                    op: JavaUnaryOp::Not,
                    operand: Box::new(inner),
                });
            }
            Some(b'~') => {
                return Err(self.err("[JAVA-BITWISE] raw bitwise complement requires explicit preprocessing"));
            }
            Some(b'-') => {
                if let Some(lit) = self.try_parse_negative_min_literal() {
                    return Ok(lit);
                }
                self.pos += 1;
                let inner = self.parse_unary()?;
                return Ok(JavaExpr::UnaryOp {
                    op: JavaUnaryOp::Neg,
                    operand: Box::new(inner),
                });
            }
            Some(b'+') => {
                self.pos += 1;
                return self.parse_unary();
            } // 一元 + 恒等
            _ => {}
        }
        // cast 或括号表达式或 primary
        if self.peek() == Some(b'(') {
            if let Some(casted) = self.try_parse_cast()? {
                return Ok(casted);
            }
        }
        self.parse_postfix()
    }

    /// The decimal int and long lower-bound magnitudes are legal only as the
    /// direct token operand of unary minus. They are not positive literals.
    fn try_parse_negative_min_literal(&mut self) -> Option<JavaExpr> {
        let save = self.pos;
        debug_assert_eq!(self.peek(), Some(b'-'));
        self.pos += 1;
        self.skip_ws();
        let start = self.pos;
        if matches!(self.peek(), Some(b'2' | b'9')) {
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'_') {
                self.pos += 1;
            }
            let digits = &self.source.as_bytes()[start..self.pos];
            if digits.last().is_some_and(u8::is_ascii_digit) {
                let long = matches!(self.peek(), Some(b'L' | b'l'));
                let next = if long { self.peek_at(1) } else { self.peek() };
                if !next.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'$' | b'.')) {
                    let magnitude = if long { b"9223372036854775808".as_slice() } else { b"2147483648".as_slice() };
                    if digits.iter().copied().filter(|c| *c != b'_').eq(magnitude.iter().copied()) {
                        let literal = if long {
                            self.pos += 1;
                            JavaLit::Long(i64::MIN)
                        } else {
                            JavaLit::Int(i32::MIN)
                        };
                        return Some(JavaExpr::Lit(literal));
                    }
                }
            }
        }
        self.pos = save;
        None
    }

    /// 尝试解析 cast `(Type) expr`。失败返回 None 并回退位置。
    fn try_parse_cast(&mut self) -> Result<Option<JavaExpr>, ParseError> {
        let save = self.pos;
        self.pos += 1; // parse_unary has matched the opening parenthesis.
        self.skip_ws();
        // 保守判定：primitive 关键字后跟 ')'，一定是 cast
        let is_primitive = [
            "boolean", "byte", "short", "int", "long", "float", "double", "char",
        ]
        .iter()
        .any(|&kw| {
            if !self.starts_with(kw) {
                return false;
            }
            let after = self.source.as_bytes().get(self.pos + kw.len()).copied();
            !matches!(after, Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
        });
        if is_primitive {
            let ty = parse_type(self).ok_or_else(|| self.err("expected primitive type in cast"))?;
            self.skip_ws();
            if !self.eat_char(b')') {
                return Err(self.err("expected ')' after cast type"));
            }
            let expr = self.parse_unary()?;
            return Ok(Some(JavaExpr::Cast {
                target_type: ty,
                operand: Box::new(expr),
            }));
        }

        // 非 primitive：尝试 parse_type，再看是否紧跟 `)` 和表达式起始
        let ty = match parse_type(self) {
            Some(t) => t,
            None => {
                self.pos = save;
                return Ok(None);
            }
        };
        self.skip_ws();
        if self.peek() != Some(b')') {
            self.pos = save;
            return Ok(None);
        }
        self.pos += 1;
        self.skip_ws();
        // cast 后必须跟表达式起始字符：ident / lit / ! / ~ / - / + / ( / new
        let is_expr_start = match self.peek() {
            Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' => true,
            Some(b'"') | Some(b'\'') | Some(b'!') | Some(b'~') | Some(b'(') => true,
            Some(b'-') | Some(b'+') => true,
            _ => false,
        };
        if !is_expr_start {
            self.pos = save;
            return Ok(None);
        }
        // 额外保守：cast 目标类型必须是 Class 或 Generic，避免 (a+b) 被误判
        let is_valid_cast_type = match &ty {
            JavaType::Class(n) => {
                // 首字母大写或含点，视为类型名
                n.chars().next().map_or(false, |c| c.is_ascii_uppercase()) || n.contains('.')
            }
            JavaType::Generic(_, _) | JavaType::Array(_) => true,
            _ => true,
        };
        if !is_valid_cast_type {
            self.pos = save;
            return Ok(None);
        }
        let expr = self.parse_unary()?;
        Ok(Some(JavaExpr::Cast {
            target_type: ty,
            operand: Box::new(expr),
        }))
    }

    /// 后缀链：`.field` / `.method(args)` / `[idx]` / `++` / `--`。
    fn parse_postfix(&mut self) -> Result<JavaExpr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'.') => {
                    self.pos += 1;
                    self.skip_ws();
                    let name = self
                        .read_ident()
                        .ok_or_else(|| self.err("expected identifier after '.'"))?;
                    self.skip_ws();
                    if self.peek() == Some(b'(') {
                        let args = self.parse_arg_list()?;
                        expr = JavaExpr::MethodCall {
                            target: Some(Box::new(expr)),
                            method: name,
                            args,
                        };
                    } else {
                        expr = JavaExpr::FieldAccess {
                            target: Box::new(expr),
                            field: name,
                        };
                    }
                }
                Some(b'[') => {
                    self.pos += 1;
                    let idx = self.parse_expression()?;
                    self.skip_ws();
                    if !self.eat_char(b']') {
                        return Err(self.err("expected ']' after array index"));
                    }
                    expr = JavaExpr::ArrayAccess {
                        target: Box::new(expr),
                        index: Box::new(idx),
                    };
                }
                _ => {
                    if self.starts_with("++") {
                        self.pos += 2;
                        expr = JavaExpr::UnaryOp {
                            op: JavaUnaryOp::PostInc,
                            operand: Box::new(expr),
                        };
                    } else if self.starts_with("--") {
                        self.pos += 2;
                        expr = JavaExpr::UnaryOp {
                            op: JavaUnaryOp::PostDec,
                            operand: Box::new(expr),
                        };
                    } else {
                        break;
                    }
                }
            }
        }
        Ok(expr)
    }

    /// 基础单元：字面量 / 标识符 / 括号 / new。
    fn parse_primary(&mut self) -> Result<JavaExpr, ParseError> {
        self.skip_ws();

        // 括号表达式
        if self.peek() == Some(b'(') {
            self.pos += 1;
            let inner = self.parse_expression()?;
            self.skip_ws();
            if !self.eat_char(b')') {
                return Err(self.err("expected ')'"));
            }
            return Ok(inner);
        }

        // new 表达式
        if self.eat_keyword("new") {
            return self.parse_new_expr();
        }

        // A qualified receiver such as this.field is not a constructor
        // invocation. Do not lower this(...) or super(...) as ordinary
        // methods, since their initialization order is not supported here.
        let receiver_at = self.pos;
        for keyword in ["this", "super"] {
            if self.eat_keyword(keyword) {
                self.skip_ws();
                if self.peek() == Some(b'(') {
                    self.pos = receiver_at;
                    return Err(self.err(&format!(
                        "[JAVA-CONSTRUCTOR-DELEGATION] {keyword}(...) requires explicit constructor preprocessing"
                    )));
                }
                return Ok(JavaExpr::Ident(keyword.into()));
            }
        }

        // 字面量
        if let Some(lit) = self.try_parse_literal()? {
            return Ok(JavaExpr::Lit(lit));
        }

        // 标识符（可能跟 `(args)` 形成 method call）
        if let Some(name) = self.read_ident() {
            self.skip_ws();
            if self.peek() == Some(b'(') {
                let args = self.parse_arg_list()?;
                return Ok(JavaExpr::MethodCall {
                    target: None,
                    method: name,
                    args,
                });
            }
            return Ok(JavaExpr::Ident(name));
        }

        Err(self.err("expected expression"))
    }

    fn parse_new_expr(&mut self) -> Result<JavaExpr, ParseError> {
        let ty = parse_creation_type(self)
            .ok_or_else(|| self.err("[JAVA-CREATION-TYPE] expected a complete type after 'new'"))?;
        self.skip_ws();
        if self.eat_char(b'[') {
            if self.eat_char(b']') {
                let mut element_type = ty;
                while self.eat_char(b'[') {
                    if !self.eat_char(b']') {
                        return Err(self.err("expected empty dimension before array initializer"));
                    }
                    element_type = JavaType::Array(Box::new(element_type));
                }
                return self.parse_array_initializer(&element_type);
            }
            let length = Box::new(self.parse_expression()?);
            if !self.eat_char(b']') {
                return Err(self.err("expected ']' in new array"));
            }
            return Ok(JavaExpr::ArrayAllocation { element_type: ty, length });
        }

        // 构造器调用 new Type(args)
        if self.peek() == Some(b'(') {
            let args = self.parse_arg_list()?;
            self.skip_ws();
            if self.peek() == Some(b'{') {
                return Err(self.err("anonymous class requires explicit preprocessing"));
            }
            return Ok(JavaExpr::New {
                java_type: ty,
                args,
            });
        }

        Err(self.err("expected '(' or '[' after 'new Type'"))
    }

    fn parse_array_initializer(&mut self, element_type: &JavaType) -> Result<JavaExpr, ParseError> {
        if !self.eat_char(b'{') {
            return Err(self.err("expected '{' for array initializer"));
        }
        let mut elements = Vec::new();
        while !self.eat_char(b'}') {
            let value = match element_type {
                JavaType::Array(inner) if self.peek() == Some(b'{') =>
                    self.parse_array_initializer(inner)?,
                _ => self.parse_expression()?,
            };
            elements.push(value);
            if self.eat_char(b',') {
                continue;
            }
            if !self.eat_char(b'}') {
                return Err(self.err("expected ',' or '}' in array initializer"));
            }
            break;
        }
        Ok(JavaExpr::ArrayInitializer { element_type: element_type.clone(), elements })
    }

    fn parse_arg_list(&mut self) -> Result<Vec<JavaExpr>, ParseError> {
        self.pos += 1; // Callers have already matched the opening parenthesis.
        let mut args = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            let e = self.parse_expression()?;
            args.push(e);
            self.skip_ws();
            if self.eat_char(b',') {
                continue;
            }
            if self.eat_char(b')') {
                break;
            }
            return Err(self.err("expected ',' or ')' in arg list"));
        }
        Ok(args)
    }

    fn try_parse_literal(&mut self) -> Result<Option<JavaLit>, ParseError> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => {
                let s = self.read_string_literal()?;
                Ok(Some(JavaLit::Str(s)))
            }
            Some(b'\'') => {
                let c = self.read_char_literal()?;
                Ok(Some(JavaLit::Char(c)))
            }
            Some(c) if c.is_ascii_digit() => Ok(Some(self.read_number_literal()?)),
            Some(b'.') if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => {
                Ok(Some(self.read_number_literal()?))
            }
            _ => {
                if self.eat_keyword("true") {
                    return Ok(Some(JavaLit::Bool(true)));
                }
                if self.eat_keyword("false") {
                    return Ok(Some(JavaLit::Bool(false)));
                }
                if self.eat_keyword("null") {
                    return Ok(Some(JavaLit::Null));
                }
                Ok(None)
            }
        }
    }

    fn read_literal_escape(&mut self) -> Result<char, ParseError> {
        let escaped = self.peek().ok_or_else(|| self.err("unexpected end in literal escape"))?;
        self.pos += 1;
        match escaped {
            b'b' => Ok('\u{0008}'),
            b'f' => Ok('\u{000c}'),
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b's' => Ok(' '),
            b't' => Ok('\t'),
            b'\\' => Ok('\\'),
            b'"' => Ok('"'),
            b'\'' => Ok('\''),
            leading @ b'0'..=b'7' => {
                let mut value = leading - b'0';
                let remaining = if leading <= b'3' { 2 } else { 1 };
                for _ in 0..remaining {
                    let Some(digit @ b'0'..=b'7') = self.peek() else { break };
                    self.pos += 1;
                    value = value * 8 + (digit - b'0');
                }
                Ok(char::from(value))
            }
            _ => Err(self.err("invalid Java literal escape")),
        }
    }

    fn read_literal_unit(&mut self) -> Result<char, ParseError> {
        if self.surrogates.at(self.pos).is_some() {
            return Err(self.err("[JAVA-UNICODE-REPRESENTATION] isolated UTF-16 code unit has no current scalar text representation"));
        }
        let first = self.peek().ok_or_else(|| self.err("unexpected end in Java literal"))?;
        match first {
            b'\n' | b'\r' => Err(self.err("line terminator in Java literal")),
            b'\\' => {
                self.pos += 1;
                self.read_literal_escape()
            }
            ascii if ascii.is_ascii() => {
                self.pos += 1;
                Ok(char::from(ascii))
            }
            utf8 => {
                let end = self.pos + utf8.leading_ones() as usize;
                let value = self.source.get(self.pos..end)
                    .and_then(|text| text.chars().next())
                    .ok_or_else(|| self.err("invalid UTF-8 in Java literal"))?;
                self.pos = end;
                Ok(value)
            }
        }
    }

    fn read_string_literal(&mut self) -> Result<String, ParseError> {
        self.pos += 1; // try_parse_literal has already matched the opening quote.
        let mut out = String::new();
        loop {
            if self.peek() == Some(b'"') {
                self.pos += 1;
                return Ok(out);
            }
            out.push(self.read_literal_unit()?);
        }
    }

    fn read_char_literal(&mut self) -> Result<u16, ParseError> {
        self.pos += 1; // try_parse_literal has already matched the opening quote.
        if self.peek() == Some(b'\'') {
            return Err(self.err("expected one Java character"));
        }
        let value = if let Some(unit) = self.surrogates.at(self.pos) {
            self.pos += char::REPLACEMENT_CHARACTER.len_utf8();
            unit
        } else {
            let c = self.read_literal_unit()?;
            u16::try_from(u32::from(c))
                .map_err(|_| self.err("Java character literal must contain one UTF-16 code unit"))?
        };
        if self.peek() != Some(b'\'') {
            return Err(self.err("Java character literal must contain one UTF-16 code unit"));
        }
        self.pos += 1;
        Ok(value)
    }

    fn scan_numeric_digits(&mut self, hex: bool) {
        while self.peek().is_some_and(|c| {
            c == b'_' || if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }
        }) {
            self.pos += 1;
        }
    }

    // Only admit digit separators between digits, not against a prefix,
    // radix point, exponent sign or type suffix. An empty part is optional.
    fn checked_ieee_digits(&self, from: usize, part: &'static str) -> Result<bool, ParseError> {
        let digits = &self.source.as_bytes()[from..self.pos];
        if digits.first() == Some(&b'_') || digits.last() == Some(&b'_') {
            return Err(self.err(part));
        }
        Ok(!digits.is_empty())
    }

    fn read_decimal_literal(
        &mut self, start: usize, digits_start: usize, hex: bool,
    ) -> Result<JavaLit, ParseError> {
        let has_whole = self.checked_ieee_digits(digits_start, "invalid numeric literal significand")?;
        let whole_end = self.pos;
        let mut fraction = whole_end..whole_end;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let fraction_start = self.pos;
            self.scan_numeric_digits(hex);
            self.checked_ieee_digits(fraction_start, "invalid numeric literal significand")?;
            fraction = fraction_start..self.pos;
        }
        if !has_whole && fraction.is_empty() {
            return Err(self.err("invalid numeric literal significand"));
        }

        let exponent = if hex { matches!(self.peek(), Some(b'p' | b'P')) }
            else { matches!(self.peek(), Some(b'e' | b'E')) };
        let mut power = 0;
        if exponent {
            self.pos += 1;
            let power_start = self.pos;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let from = self.pos;
            self.scan_numeric_digits(false);
            if !self.checked_ieee_digits(from, "invalid numeric literal exponent")? {
                return Err(self.err("invalid numeric literal exponent"));
            }
            power = self.source[power_start..self.pos].replace('_', "").parse::<i32>()
                .map_err(|_| self.err_at(start, "exact decimal exponent exceeds source resource limits"))?;
        } else if hex {
            return Err(self.err("hexadecimal floating literal is missing binary exponent"));
        }

        let single = matches!(self.peek(), Some(b'f' | b'F'));
        if single || matches!(self.peek(), Some(b'd' | b'D')) {
            self.pos += 1;
        }
        if self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$') {
            return Err(self.err("invalid numeric literal suffix"));
        }
        // Same bound as the explicit decimal IR spelling. Never allocate an
        // arbitrarily expanded value just from an exponent written in source.
        if self.pos - start > 4096 {
            return Err(self.err_at(start, "exact decimal literal exceeds 4096 characters"));
        }
        let fraction = self.source[fraction].replace('_', "");
        let scale = fraction.len() as i32 * if hex { 4 } else { 1 };
        let Some(power) = power.checked_sub(scale).filter(|p| (-4096..=4096).contains(p)) else {
            return Err(self.err_at(start, "exact decimal exponent exceeds source resource limits"));
        };
        let digits = self.source[digits_start..whole_end].replace('_', "") + &fraction;
        Ok(JavaLit::Decimal { digits, radix: if hex { 16 } else { 10 }, power, single })
    }

    fn read_number_literal(&mut self) -> Result<JavaLit, ParseError> {
        let start = self.pos;
        let mut is_long = false;
        let radix = match (self.peek(), self.peek_at(1)) {
            (Some(b'0'), Some(b'x' | b'X')) => 16,
            (Some(b'0'), Some(b'b' | b'B')) => 2,
            _ => 10,
        };

        if radix != 10 {
            self.pos += 2;
        }
        let digits_start = self.pos;
        self.scan_numeric_digits(radix == 16);
        let exponent_marker = match radix {
            16 => matches!(self.peek(), Some(b'p' | b'P')),
            10 => matches!(self.peek(), Some(b'e' | b'E')),
            _ => false,
        };
        let suffix = radix == 10 && matches!(self.peek(), Some(b'f' | b'F' | b'd' | b'D'));
        if exponent_marker || suffix || (radix != 2 && self.peek() == Some(b'.')) {
            return self.read_decimal_literal(start, digits_start, radix == 16);
        }

        if radix != 10
            && (self.pos == digits_start || self.source.as_bytes()[digits_start] == b'_')
        {
            return Err(self.err("expected digit after integer radix prefix"));
        }
        let end = self.pos;
        // An integer token can take only the long suffix.
        if matches!(self.peek(), Some(b'L' | b'l')) {
            is_long = true;
            self.pos += 1;
        }
        if self.source.as_bytes().get(end - 1) == Some(&b'_')
            || self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
        {
            return Err(self.err("invalid numeric literal suffix"));
        }

        let text = &self.source[start..end];
        let cleaned: String = text.chars().filter(|&c| c != '_').collect();

        if radix != 10 || (cleaned.starts_with('0') && cleaned.len() > 1) {
            // Java's nondecimal tokens encode the source integer's bit pattern.
            // Decode that pattern before handing off to exact business arithmetic.
            let (digits, radix) = if radix == 10 { (cleaned.as_str(), 8) } else { (&cleaned[2..], radix) };
            let value = u64::from_str_radix(digits, radix)
                .map_err(|_| self.err("invalid nondecimal integer literal"))?;
            if is_long {
                Ok(JavaLit::Long(value as i64))
            } else {
                let word = u32::try_from(value)
                    .map_err(|_| self.err("nondecimal int literal exceeds 32 bits"))?;
                Ok(JavaLit::Int(word as i32))
            }
        } else {
            let v: i64 = cleaned
                .parse()
                .map_err(|_| self.err("invalid int literal"))?;
            if is_long {
                Ok(JavaLit::Long(v))
            } else {
                let v = i32::try_from(v)
                    .map_err(|_| self.err("decimal int literal exceeds its source range"))?;
                Ok(JavaLit::Int(v))
            }
        }
    }
}
