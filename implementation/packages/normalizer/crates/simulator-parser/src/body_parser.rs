//! 方法体解析：Java 语句/表达式 → AST。
//!
//! 设计原则：
//! - 手写递归下降 + 表达式优先级爬升（precedence climbing）
//! - 解析失败 → ParseError，不静默跳过
//! - 复用 parser.rs 的 Parser struct 和 helper（通过 pub(crate)）

use crate::ast::*;
use crate::parser::{parse_type, ParseError, Parser};

pub(crate) fn parse_method_body_text(
    body_text: &str,
    file: &str,
    package: &str,
    imports: Vec<String>,
    imports_wildcard: Vec<String>,
) -> Result<Vec<JavaStatement>, ParseError> {
    let mut p = Parser::new(body_text, file);
    p.package = package.to_string();
    p.imports = imports;
    p.imports_wildcard = imports_wildcard;
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
            let stmt = self.parse_statement()?;
            out.push(stmt);
        }
        Ok(out)
    }

    /// 创建继承当前解析上下文的子 parser。
    ///
    fn child_parser<'b>(&self, text: &'b str) -> Parser<'b> {
        let mut sub = Parser::new(text, &self.file);
        sub.package = self.package.clone();
        sub.imports = self.imports.clone();
        sub.imports_wildcard = self.imports_wildcard.clone();
        sub
    }

    /// 解析单个语句。
    fn parse_statement(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();

        if let Some(start) = self.local_ir_start() {
            let end = self.src[start..].windows(2).position(|part| part == b"*/")
                .map(|offset| start + offset).ok_or_else(|| self.err("unterminated @normalizer-ir comment"))?;
            let source = std::str::from_utf8(&self.src[start..end]).map_err(|_| self.err("invalid UTF-8 in @normalizer-ir"))?;
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
            let body_text = self.skip_balanced_braces()?;
            let mut sub = self.child_parser(&body_text);
            let stmts = sub.parse_statements_until_end()?;
            return Ok(JavaStatement::Block(stmts));
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
        for keyword in ["switch", "synchronized", "do", "assert"] {
            if self.eat_keyword(keyword) {
                return Err(self.err(&format!("{keyword} requires explicit preprocessing")));
            }
        }
        if self.eat_keyword("break") {
            self.skip_ws();
            let label = self.read_ident();
            self.skip_ws();
            if !self.eat_char(b';') {
                return Err(self.err("expected ';' after break"));
            }
            return Ok(JavaStatement::Break { label });
        }
        if self.eat_keyword("continue") {
            self.skip_ws();
            let label = self.read_ident();
            self.skip_ws();
            if !self.eat_char(b';') {
                return Err(self.err("expected ';' after continue"));
            }
            return Ok(JavaStatement::Continue { label });
        }
        // 尝试变量声明：Type name ... 或 Type name = expr;
        let save = self.pos;
        if let Some(stmt) = self.try_parse_vardecl()? {
            return Ok(stmt);
        }
        self.pos = save;

        // 否则是表达式语句（含赋值）
        let expr = self.parse_expression()?;
        self.skip_ws();
        // 赋值表达式在 statement 位置仍归一为 JavaStatement::Assign，
        // 让 loop lift / 静态分析等 statement-level 语义继续看到赋值信号。
        if let JavaExpr::Assign { target, value } = expr {
            self.eat_char(b';');
            return Ok(JavaStatement::Assign {
                target: *target,
                value: *value,
            });
        }
        self.eat_char(b';');
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
                let stmt = if let Some(v) = self.try_parse_vardecl()? {
                    v
                } else {
                    let e = self.parse_expression()?;
                    self.skip_ws();
                    self.eat_char(b';');
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
        self.eat_char(b';');
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
        self.eat_char(b';');
        Ok(JavaStatement::Return(Some(expr)))
    }

    fn parse_throw(&mut self) -> Result<JavaStatement, ParseError> {
        let expr = self.parse_expression()?;
        self.skip_ws();
        self.eat_char(b';');
        Ok(JavaStatement::Throw(expr))
    }

    fn parse_try(&mut self) -> Result<JavaStatement, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            return Err(self.err("try-with-resources is not supported in normalized Java"));
        }
        let try_body_text = self.skip_balanced_braces()?;
        let mut sub = self.child_parser(&try_body_text);
        let try_body = sub.parse_statements_until_end()?;

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
            let exception_type =
                parse_type(self).ok_or_else(|| self.err("expected exception type"))?;
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
            let catch_body_text = self.skip_balanced_braces()?;
            let mut csub = self.child_parser(&catch_body_text);
            let body = csub.parse_statements_until_end()?;
            catches.push(CatchClause {
                exception_type,
                var_name,
                body,
            });
        }

        self.skip_ws();
        let finally = if self.eat_keyword("finally") {
            let fin_text = self.skip_balanced_braces()?;
            let mut fsub = self.child_parser(&fin_text);
            Some(fsub.parse_statements_until_end()?)
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
            let body_text = self.skip_balanced_braces()?;
            let mut sub = self.child_parser(&body_text);
            sub.parse_statements_until_end()
        } else {
            let stmt = self.parse_statement()?;
            Ok(vec![stmt])
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

    fn try_parse_vardecl(&mut self) -> Result<Option<JavaStatement>, ParseError> {
        let save = self.pos;
        self.skip_ws();
        // Local variable annotations are legal Java syntax (for example
        // `@SuppressWarnings("unchecked") Set<String> set = ...;`). They do
        // not affect normalized business semantics, but parser must accept
        // them so unrelated source-root files do not block target entries.
        let _annotations = self.read_annotations()?;
        self.skip_ws();
        self.eat_keyword("final");
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
        let ty = match self.parse_c_style_array_suffix(base_ty.clone()) {
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

        // 多声明：`Type n1 = e1, n2, n3 = e3;` —— 第一个作为主语句，后续合成Block
        let mut extras: Vec<JavaStatement> = Vec::new();
        while self.eat_char(b',') {
            self.skip_ws();
            let n2 = self
                .read_ident()
                .ok_or_else(|| self.err("expected var name in multi-decl"))?;
            self.skip_ws();
            let ty2 = self
                .parse_c_style_array_suffix(base_ty.clone())
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
        self.eat_char(b';');

        let main = JavaStatement::VarDecl {
            name,
            java_type: ty,
            initializer: init,
        };
        if extras.is_empty() {
            Ok(Some(main))
        } else {
            let mut stmts = vec![main];
            stmts.extend(extras);
            Ok(Some(JavaStatement::SameScope(stmts)))
        }
    }

    fn parse_c_style_array_suffix(&mut self, mut ty: JavaType) -> Option<JavaType> {
        loop {
            self.skip_ws();
            if self.peek() != Some(b'[') {
                break;
            }
            self.pos += 1;
            self.skip_ws();
            if !self.eat_char(b']') {
                return None;
            }
            ty = JavaType::Array(Box::new(ty));
        }
        Some(ty)
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
            // instanceof 优先级与比较运算符同级（9），不是常规 BinOp
            if self.starts_with("instanceof") {
                let after = self.src.get(self.pos + 10).copied();
                let is_kw = !matches!(after, Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'$');
                if is_kw && 9u8 >= min_prec {
                    self.pos += 10;
                    let ty = parse_type(self)
                        .ok_or_else(|| self.err("expected type after instanceof"))?;
                    left = JavaExpr::InstanceOf {
                        operand: Box::new(left),
                        java_type: ty,
                    };
                    continue;
                }
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
                self.pos += 1;
                let inner = self.parse_unary()?;
                return Ok(JavaExpr::UnaryOp {
                    op: JavaUnaryOp::BitNot,
                    operand: Box::new(inner),
                });
            }
            Some(b'-') => {
                if let Some(lit) = self.try_parse_negative_long_min_literal() {
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

    /// Java 允许十进制 long literal `9223372036854775808L` 只作为一元负号的
    /// 操作数出现，用来表示 `Long.MIN_VALUE`。它不是普通正数字面量，不能
    /// 在 `read_number_literal` 里当作兜底放开；只在真实 `-literal` 位置折叠。
    fn try_parse_negative_long_min_literal(&mut self) -> Option<JavaExpr> {
        let save = self.pos;
        debug_assert_eq!(self.peek(), Some(b'-'));
        self.pos += 1;
        self.skip_ws();
        let start = self.pos;
        // A leading underscore is a Java identifier, not a digit separator.
        if self.peek() == Some(b'9') {
            while self.peek().is_some_and(|c| c.is_ascii_digit() || c == b'_') {
                self.pos += 1;
            }
            let digits = &self.src[start..self.pos];
            if digits.last() == Some(&b'8')
                && matches!(self.peek(), Some(b'L' | b'l'))
                && !self.peek_at(1).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
                && digits.iter().copied().filter(|c| *c != b'_').eq(b"9223372036854775808".iter().copied())
            {
                self.pos += 1;
                return Some(JavaExpr::Lit(JavaLit::Long(i64::MIN)));
            }
        }
        self.pos = save;
        None
    }

    /// 尝试解析 cast `(Type) expr`。失败返回 None 并回退位置。
    fn try_parse_cast(&mut self) -> Result<Option<JavaExpr>, ParseError> {
        let save = self.pos;
        if !self.eat_char(b'(') {
            self.pos = save;
            return Ok(None);
        }
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
            let after = self.src.get(self.pos + kw.len()).copied();
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
        let paren_end = self.pos;
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
        let _ = paren_end;
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

        // this / super: 后跟 `(` 是构造器调用 this(args)/super(args)，否则是 Ident
        if self.eat_keyword("this") {
            self.skip_ws();
            if self.peek() == Some(b'(') {
                let args = self.parse_arg_list()?;
                return Ok(JavaExpr::MethodCall {
                    target: None,
                    method: "this".to_string(),
                    args,
                });
            }
            return Ok(JavaExpr::Ident("this".to_string()));
        }
        if self.eat_keyword("super") {
            self.skip_ws();
            if self.peek() == Some(b'(') {
                let args = self.parse_arg_list()?;
                return Ok(JavaExpr::MethodCall {
                    target: None,
                    method: "super".to_string(),
                    args,
                });
            }
            return Ok(JavaExpr::Ident("super".to_string()));
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
        self.skip_ws();
        let ty = Self::parse_new_element_type(self)
            .ok_or_else(|| self.err("expected type after 'new'"))?;
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

    fn parse_new_element_type(p: &mut Parser) -> Option<JavaType> {
        p.skip_ws();

        // `new T[expr]` 的 T 是元素类型；不能调用 parse_type，
        // 因为 parse_type 会把紧随其后的 `[expr]` 误当作类型数组后缀 `[]` 去消费。
        // 这里只解析 primitive/class/generic 本体，数组维度由 parse_new_expr 统一处理。
        for prim in &[
            "boolean", "byte", "short", "int", "long", "float", "double", "char",
        ] {
            if p.eat_keyword(prim) {
                return Some(JavaType::Primitive(prim.to_string()));
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

        Some(t)
    }

    fn parse_arg_list(&mut self) -> Result<Vec<JavaExpr>, ParseError> {
        self.skip_ws();
        if !self.eat_char(b'(') {
            return Err(self.err("expected '(' for arg list"));
        }
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
            b'u' => Err(self.err("Unicode escapes require lexical preprocessing before parsing")),
            _ => Err(self.err("invalid Java literal escape")),
        }
    }

    fn read_literal_unit(&mut self) -> Result<char, ParseError> {
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
                let value = self.src.get(self.pos..end)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(|text| text.chars().next())
                    .ok_or_else(|| self.err("invalid UTF-8 in Java literal"))?;
                self.pos = end;
                Ok(value)
            }
        }
    }

    fn read_string_literal(&mut self) -> Result<String, ParseError> {
        if !self.eat_char(b'"') {
            return Err(self.err("expected string literal"));
        }
        let mut out = String::new();
        loop {
            if self.peek() == Some(b'"') {
                self.pos += 1;
                return Ok(out);
            }
            out.push(self.read_literal_unit()?);
        }
    }

    fn read_char_literal(&mut self) -> Result<char, ParseError> {
        if !self.eat_char(b'\'') || self.peek() == Some(b'\'') {
            return Err(self.err("expected one Java character"));
        }
        let value = self.read_literal_unit()?;
        if value.len_utf16() != 1 || self.peek() != Some(b'\'') {
            return Err(self.err("Java character literal must contain one UTF-16 code unit"));
        }
        self.pos += 1;
        Ok(value)
    }

    fn read_number_literal(&mut self) -> Result<JavaLit, ParseError> {
        let start = self.pos;
        let mut is_float = false;
        let mut is_long = false;
        let radix = match (self.peek(), self.peek_at(1)) {
            (Some(b'0'), Some(b'x' | b'X')) => 16,
            (Some(b'0'), Some(b'b' | b'B')) => 2,
            _ => 10,
        };

        if radix != 10 {
            self.pos += 2;
            let start_digits = self.pos;
            while self.peek().is_some_and(|c| {
                c == b'_' || if radix == 16 { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }
            }) {
                self.pos += 1;
            }
            if self.pos == start_digits || self.src[start_digits] == b'_' {
                return Err(self.err("expected digit after integer radix prefix"));
            }
        } else {
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == b'_' {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            // 小数
            if self.peek() == Some(b'.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
                is_float = true;
                self.pos += 1;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == b'_' {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
            // 指数
            if matches!(self.peek(), Some(b'e') | Some(b'E')) {
                is_float = true;
                self.pos += 1;
                if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                    self.pos += 1;
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let end = self.pos;
        // A binary or hexadecimal integer takes only the long suffix.
        match self.peek() {
            Some(b'L' | b'l') if !is_float => {
                is_long = true;
                self.pos += 1;
            }
            Some(b'F' | b'f' | b'D' | b'd') if radix == 10 => {
                is_float = true;
                self.pos += 1;
            }
            _ => {}
        }
        if self.src.get(end - 1) == Some(&b'_')
            || self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'$')
        {
            return Err(self.err("invalid numeric literal suffix"));
        }

        let text = std::str::from_utf8(&self.src[start..end]).unwrap_or_else(|e| {
            panic!(
                "[BODY-PARSER-TEXT-UTF8-FAIL] text utf8 decode failed at {}..{}: {:?}",
                start, end, e
            )
        });
        let cleaned: String = text.chars().filter(|&c| c != '_').collect();

        if is_float {
            let v: f64 = cleaned
                .parse()
                .map_err(|_| self.err("invalid float literal"))?;
            Ok(JavaLit::Double(v))
        } else if radix != 10 || (cleaned.starts_with('0') && cleaned.len() > 1) {
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
                Ok(JavaLit::Int(i64::from(word as i32)))
            }
        } else {
            let v: i64 = cleaned
                .parse()
                .map_err(|_| self.err("invalid int literal"))?;
            if is_long {
                Ok(JavaLit::Long(v))
            } else {
                Ok(JavaLit::Int(v))
            }
        }
    }
}
