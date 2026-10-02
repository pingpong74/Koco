use super::{Parser, PathStyle};
use crate::ast::*;
use crate::token::TokenKind;

enum StmtOrTail {
    Stmt(Idx<Statement>),
    Tail(Idx<Expression>),
}

fn binary_op_for(kind: TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        TokenKind::Plus => BinaryOp::Add,
        TokenKind::Minus => BinaryOp::Sub,
        TokenKind::Star => BinaryOp::Mul,
        TokenKind::Slash => BinaryOp::Div,
        TokenKind::Percent => BinaryOp::Rem,
        TokenKind::EqEq => BinaryOp::Eq,
        TokenKind::NotEq => BinaryOp::Ne,
        TokenKind::Lt => BinaryOp::Lt,
        TokenKind::Gt => BinaryOp::Gt,
        TokenKind::Le => BinaryOp::Le,
        TokenKind::Ge => BinaryOp::Ge,
        TokenKind::AndAnd => BinaryOp::And,
        TokenKind::OrOr => BinaryOp::Or,
        TokenKind::Amp => BinaryOp::BitAnd,
        TokenKind::Pipe => BinaryOp::BitOr,
        TokenKind::Caret => BinaryOp::BitXor,
        TokenKind::Shl => BinaryOp::Shl,
        TokenKind::Shr => BinaryOp::Shr,
        _ => return None,
    })
}

fn assign_op_for(kind: TokenKind) -> Option<Option<BinaryOp>> {
    Some(match kind {
        TokenKind::Eq => None,
        TokenKind::PlusEq => Some(BinaryOp::Add),
        TokenKind::MinusEq => Some(BinaryOp::Sub),
        TokenKind::StarEq => Some(BinaryOp::Mul),
        TokenKind::SlashEq => Some(BinaryOp::Div),
        TokenKind::PercentEq => Some(BinaryOp::Rem),
        TokenKind::AmpEq => Some(BinaryOp::BitAnd),
        TokenKind::PipeEq => Some(BinaryOp::BitOr),
        TokenKind::CaretEq => Some(BinaryOp::BitXor),
        TokenKind::ShlEq => Some(BinaryOp::Shl),
        TokenKind::ShrEq => Some(BinaryOp::Shr),
        _ => return None,
    })
}

/// binding power of as
const CAST_BP: u8 = 25;

impl<'a> Parser<'a> {
    pub(crate) fn parse_block_expr(&mut self) -> Option<Idx<Expression>> {
        self.nested(|parser| {
            let start = parser.expect(TokenKind::LBrace, "`{`")?.span;

            let mut statements = Vec::new();
            let mut tail = None;

            loop {
                match parser.peek() {
                    TokenKind::RBrace | TokenKind::Eof => break,
                    TokenKind::Semi => {
                        parser.bump();
                        continue;
                    }
                    _ => {}
                }

                let before = parser.pos;

                match parser.parse_stmt_or_tail() {
                    Some(StmtOrTail::Stmt(id)) => statements.push(id),
                    Some(StmtOrTail::Tail(e)) => {
                        tail = Some(e);
                        break;
                    }
                    None => {
                        if parser.pos == before {
                            parser.bump();
                        }
                        parser.sync_stmt();
                    }
                }
            }

            let end = parser.expect(TokenKind::RBrace, "`}`")?.span;
            let span = start.to(end);
            Some(parser.mk_expr(
                ExpressionKind::Block(Block {
                    stmts: statements,
                    tail,
                    span,
                }),
                span,
            ))
        })
    }

    fn parse_stmt_or_tail(&mut self) -> Option<StmtOrTail> {
        let start = self.span();
        match self.peek() {
            TokenKind::Let => {
                let kind = {
                    self.expect(TokenKind::Let, "`let`")?;

                    let pattern = self.parse_pattern()?;
                    let ty = if self.eat(TokenKind::Colon) {
                        Some(self.parse_ty()?)
                    } else {
                        None
                    };
                    let init = if self.eat(TokenKind::Eq) {
                        Some(self.parse_expr()?)
                    } else {
                        None
                    };

                    self.expect(TokenKind::Semi, "`;` after `let` statement")?;

                    Some(StatementKind::Let {
                        pat: pattern,
                        ty,
                        init,
                    })
                };

                let span = start.to(self.prev_span);
                Some(StmtOrTail::Stmt(self.arenas.stmts.alloc(Statement {
                    kind: kind?,
                    span,
                })))
            }
            TokenKind::Fn | TokenKind::Struct | TokenKind::Enum | TokenKind::Trait | TokenKind::Impl | TokenKind::Mod | TokenKind::Use | TokenKind::Pub | TokenKind::Type | TokenKind::Const | TokenKind::Static | TokenKind::Hash => {
                let item = self.parse_item()?;
                let span = start.to(self.prev_span);

                Some(StmtOrTail::Stmt(self.arenas.stmts.alloc(Statement {
                    kind: StatementKind::Item(item),
                    span,
                })))
            }
            _ => {
                let block_like = matches!(self.peek(), TokenKind::If | TokenKind::Match | TokenKind::Loop | TokenKind::While | TokenKind::LBrace);

                let expr = if block_like {
                    self.parse_block_like(true)?
                } else {
                    self.parse_expr()?
                };

                if self.eat(TokenKind::Semi) {
                    let span = start.to(self.prev_span);

                    Some(StmtOrTail::Stmt(self.arenas.stmts.alloc(Statement {
                        kind: StatementKind::Expr {
                            expr,
                            semi: true,
                        },
                        span,
                    })))
                } else if self.at(TokenKind::RBrace) {
                    Some(StmtOrTail::Tail(expr))
                } else if block_like {
                    let span = start.to(self.prev_span);

                    Some(StmtOrTail::Stmt(self.arenas.stmts.alloc(Statement {
                        kind: StatementKind::Expr {
                            expr,
                            semi: false,
                        },
                        span,
                    })))
                } else {
                    self.error_expected("`;` or `}`");
                    None
                }
            }
        }
    }

    pub(crate) fn parse_expr(&mut self) -> Option<Idx<Expression>> { self.parse_expr_ctx(true) }

    pub(crate) fn parse_expr_no_struct(&mut self) -> Option<Idx<Expression>> { self.parse_expr_ctx(false) }

    fn parse_expr_ctx(&mut self, allow_struct: bool) -> Option<Idx<Expression>> {
        let lhs = self.parse_binary(0, allow_struct)?;
        let Some(op) = assign_op_for(self.peek()) else {
            return Some(lhs);
        };
        self.bump();
        let value = self.nested(|p| p.parse_expr_ctx(allow_struct))?;
        let lhs_span = self.arenas.exprs[lhs].span;
        let span = lhs_span.to(self.arenas.exprs[value].span);
        let kind = match op {
            None => ExpressionKind::Assign {
                target: lhs,
                value,
            },
            Some(op) => ExpressionKind::AssignOp {
                op,
                target: lhs,
                value,
            },
        };
        Some(self.mk_expr(kind, span))
    }

    fn parse_binary(&mut self, min_bp: u8, allow_struct: bool) -> Option<Idx<Expression>> {
        let mut lhs = self.parse_prefix(allow_struct)?;
        loop {
            if self.at(TokenKind::As) {
                if CAST_BP < min_bp {
                    break;
                }
                self.bump();
                let ty = self.parse_ty()?;
                let span = self.arenas.exprs[lhs].span.to(self.prev_span);
                lhs = self.mk_expr(
                    ExpressionKind::Cast {
                        expr: lhs,
                        ty,
                    },
                    span,
                );
                continue;
            }
            let Some(op) = binary_op_for(self.peek()) else {
                break;
            };
            let (l_bp, r_bp) = op.binding_power();
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.nested(|p| p.parse_binary(r_bp, allow_struct))?;
            let span = self.arenas.exprs[lhs].span.to(self.arenas.exprs[rhs].span);
            lhs = self.mk_expr(
                ExpressionKind::Binary {
                    op,
                    lhs,
                    rhs,
                },
                span,
            );
        }
        Some(lhs)
    }

    fn parse_prefix(&mut self, allow_struct: bool) -> Option<Idx<Expression>> { self.nested(|p| p.parse_prefix_inner(allow_struct)) }

    fn parse_prefix_inner(&mut self, allow_struct: bool) -> Option<Idx<Expression>> {
        let start = self.span();
        match self.peek() {
            TokenKind::Minus | TokenKind::Bang => {
                let op = if self.at(TokenKind::Minus) {
                    UnaryOp::Neg
                } else {
                    UnaryOp::Not
                };
                self.bump();
                let expr = self.parse_prefix(allow_struct)?;
                let span = start.to(self.arenas.exprs[expr].span);
                Some(self.mk_expr(ExpressionKind::Unary { op, expr }, span))
            }
            TokenKind::Amp => {
                self.bump();
                let mutable = self.eat(TokenKind::Mut);
                let expr = self.parse_prefix(allow_struct)?;
                let span = start.to(self.arenas.exprs[expr].span);
                Some(self.mk_expr(
                    ExpressionKind::Ref {
                        mutable,
                        expr,
                    },
                    span,
                ))
            }
            TokenKind::AndAnd => {
                self.error(start, "references to references are not supported");
                None
            }
            _ => self.parse_postfix(allow_struct),
        }
    }

    fn parse_postfix(&mut self, allow_struct: bool) -> Option<Idx<Expression>> {
        let mut e = self.parse_primary(allow_struct)?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.bump();
                    e = self.parse_after_dot(e)?;
                }
                TokenKind::LBracket => {
                    self.bump();
                    let index = self.parse_expr()?;
                    self.expect(TokenKind::RBracket, "`]`")?;
                    let span = self.arenas.exprs[e].span.to(self.prev_span);
                    e = self.mk_expr(
                        ExpressionKind::Index {
                            base: e,
                            index,
                        },
                        span,
                    );
                }
                TokenKind::LParen => {
                    let args = self.parse_call_args()?;
                    let span = self.arenas.exprs[e].span.to(self.prev_span);
                    e = self.mk_expr(
                        ExpressionKind::Call {
                            callee: e,
                            args,
                        },
                        span,
                    );
                }
                _ => break,
            }
        }
        Some(e)
    }

    /// Everything that can follow `base.`: a field, a tuple index, or a method call.
    fn parse_after_dot(&mut self, base: Idx<Expression>) -> Option<Idx<Expression>> {
        let base_span = self.arenas.exprs[base].span;
        match self.peek() {
            TokenKind::Identifier => {
                let tok = self.bump();
                let name = self.intern(tok.span);
                let generic_args = if self.at(TokenKind::ColonColon) && self.peek_at(1) == TokenKind::Lt {
                    self.bump();
                    self.parse_generic_args()?
                } else {
                    Vec::new()
                };
                if self.at(TokenKind::LParen) {
                    let args = self.parse_call_args()?;
                    let span = base_span.to(self.prev_span);
                    Some(self.mk_expr(
                        ExpressionKind::MethodCall {
                            receiver: base,
                            method: name,
                            generic_args,
                            args,
                        },
                        span,
                    ))
                } else {
                    if !generic_args.is_empty() {
                        self.error(tok.span, "a field access cannot have generic arguments");
                        return None;
                    }
                    let span = base_span.to(tok.span);
                    Some(self.mk_expr(
                        ExpressionKind::Field {
                            base,
                            field: FieldName::Named(name),
                        },
                        span,
                    ))
                }
            }
            TokenKind::IntLiteral => {
                // tuple field: `t.0`
                let tok = self.bump();
                let Some(index) = self.int_value(tok.span) else {
                    return None;
                };
                let Ok(index) = u32::try_from(index) else {
                    self.error(tok.span, "tuple field index is too large");
                    return None;
                };
                let span = base_span.to(tok.span);
                Some(self.mk_expr(
                    ExpressionKind::Field {
                        base,
                        field: FieldName::Index(index),
                    },
                    span,
                ))
            }
            TokenKind::FloatLiteral => {
                // `t.0.1` lexes as `t` `.` `0.1`; split it back into two field accesses.
                let tok = self.bump();
                let text = self.text(tok.span);
                let parts: Vec<&str> = text.split('.').collect();
                if parts.len() != 2 || !parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())) {
                    self.error(tok.span, "unexpected float literal after `.`");
                    return None;
                }
                let (Ok(first), Ok(second)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) else {
                    self.error(tok.span, "tuple field index is too large");
                    return None;
                };
                let mid = tok.span.lo + parts[0].len() as u32;
                let first_span = koco_span::Span {
                    file: tok.span.file,
                    lo: tok.span.lo,
                    hi: mid,
                };
                let inner = self.mk_expr(
                    ExpressionKind::Field {
                        base,
                        field: FieldName::Index(first),
                    },
                    base_span.to(first_span),
                );
                let span = base_span.to(tok.span);
                Some(self.mk_expr(
                    ExpressionKind::Field {
                        base: inner,
                        field: FieldName::Index(second),
                    },
                    span,
                ))
            }
            _ => {
                self.error_expected("a field or method name after `.`");
                None
            }
        }
    }

    fn parse_call_args(&mut self) -> Option<Vec<Idx<Expression>>> {
        self.expect(TokenKind::LParen, "`(`")?;
        let mut args = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
            args.push(self.parse_expr()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;
        Some(args)
    }

    fn can_start_expr(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::IntLiteral
                | TokenKind::FloatLiteral
                | TokenKind::True
                | TokenKind::False
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::If
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Match
                | TokenKind::Return
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Identifier
                | TokenKind::SelfValue
                | TokenKind::SelfType
                | TokenKind::Crate
                | TokenKind::Super
                | TokenKind::Minus
                | TokenKind::Bang
                | TokenKind::Amp
        )
    }

    fn parse_primary(&mut self, allow_struct: bool) -> Option<Idx<Expression>> {
        let start = self.span();
        match self.peek() {
            TokenKind::IntLiteral | TokenKind::FloatLiteral | TokenKind::True | TokenKind::False => {
                let lit = self.parse_literal()?;
                Some(self.mk_expr(ExpressionKind::Literal(lit), start))
            }
            TokenKind::LParen => self.parse_paren_or_tuple(),
            TokenKind::LBracket => self.parse_array(),
            TokenKind::LBrace | TokenKind::If | TokenKind::While | TokenKind::Loop | TokenKind::Match => self.parse_block_like(false),
            TokenKind::Return => {
                self.bump();
                let value = if self.can_start_expr() {
                    Some(self.parse_expr_ctx(allow_struct)?)
                } else {
                    None
                };
                let span = start.to(self.prev_span);
                Some(self.mk_expr(ExpressionKind::Return(value), span))
            }
            TokenKind::Break => {
                self.bump();
                let value = if self.can_start_expr() {
                    Some(self.parse_expr_ctx(allow_struct)?)
                } else {
                    None
                };
                let span = start.to(self.prev_span);
                Some(self.mk_expr(ExpressionKind::Break(value), span))
            }
            TokenKind::Continue => {
                self.bump();
                Some(self.mk_expr(ExpressionKind::Continue, start))
            }
            _ if self.at_path_start() => {
                let path = self.parse_path(PathStyle::Expr)?;
                if allow_struct && self.at(TokenKind::LBrace) && self.struct_lit_follows() {
                    return self.parse_struct_lit(path, start);
                }
                let span = self.arenas.paths[path].span;
                Some(self.mk_expr(ExpressionKind::Path(path), span))
            }
            _ => {
                self.error_expected("an expression");
                None
            }
        }
    }

    /// Cheap lookahead deciding whether `Path {` begins a struct literal: `{ }`,
    /// `{ name :`, `{ name ,` or `{ name }`.
    fn struct_lit_follows(&self) -> bool {
        match self.peek_at(1) {
            TokenKind::RBrace => true,
            TokenKind::Identifier => matches!(self.peek_at(2), TokenKind::Colon | TokenKind::Comma | TokenKind::RBrace),
            _ => false,
        }
    }

    fn parse_struct_lit(&mut self, path: Idx<Path>, start: koco_span::Span) -> Option<Idx<Expression>> {
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let field_start = self.span();
            let (name, name_span) = self.parse_ident("a field name")?;

            let value = if self.eat(TokenKind::Colon) {
                self.parse_expr()?
            } else {
                let shorthand_path = self.mk_path(Path {
                    segments: vec![PathSegment {
                        ident: name,
                        args: Vec::new(),
                        span: name_span,
                    }],
                    span: name_span,
                });
                self.mk_expr(ExpressionKind::Path(shorthand_path), name_span)
            };

            fields.push(FieldInit {
                name,
                value,
                span: field_start.to(self.prev_span),
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "`}`")?;
        let span = start.to(self.prev_span);
        Some(self.mk_expr(
            ExpressionKind::StructDeclration {
                path,
                fields,
            },
            span,
        ))
    }

    fn parse_paren_or_tuple(&mut self) -> Option<Idx<Expression>> {
        let start = self.span();
        self.expect(TokenKind::LParen, "`(`")?;

        if self.eat(TokenKind::RParen) {
            let span = start.to(self.prev_span);
            Some(self.mk_expr(ExpressionKind::Tuple(Vec::new()), span))
        } else {
            let first = self.parse_expr()?;
            if self.at(TokenKind::Comma) {
                let mut elems = vec![first];
                while self.eat(TokenKind::Comma) {
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_expr()?);
                }
                self.expect(TokenKind::RParen, "`)`")?;
                let span = start.to(self.prev_span);
                return Some(self.mk_expr(ExpressionKind::Tuple(elems), span));
            }
            self.expect(TokenKind::RParen, "`)`")?;
            Some(first)
        }
    }

    fn parse_array(&mut self) -> Option<Idx<Expression>> {
        let start = self.span();
        self.expect(TokenKind::LBracket, "`[`")?;

        if self.eat(TokenKind::RBracket) {
            let span = start.to(self.prev_span);
            return Some(self.mk_expr(ExpressionKind::Array(Vec::new()), span));
        }

        let first = self.parse_expr()?;

        if self.eat(TokenKind::Semi) {
            let len = self.parse_expr()?;
            self.expect(TokenKind::RBracket, "`]`")?;

            let span = start.to(self.prev_span);
            Some(self.mk_expr(
                ExpressionKind::ArrayRepeat {
                    value: first,
                    len,
                },
                span,
            ))
        } else {
            let mut elems = vec![first];
            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::RBracket) {
                    break;
                }
                elems.push(self.parse_expr()?);
            }

            self.expect(TokenKind::RBracket, "`]`")?;
            let span = start.to(self.prev_span);

            Some(self.mk_expr(ExpressionKind::Array(elems), span))
        }
    }

    /// `{ }`, `if`, `while`, `loop` or `match`.
    pub(crate) fn parse_block_like(&mut self, _stmt_position: bool) -> Option<Idx<Expression>> {
        match self.peek() {
            TokenKind::LBrace => self.parse_block_expr(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::Loop => self.parse_loop(),
            TokenKind::Match => self.parse_match(),
            _ => {
                self.error_expected("a block, `if`, `while`, `loop` or `match`");
                None
            }
        }
    }

    // While let is not yet supported
    fn parse_while(&mut self) -> Option<Idx<Expression>> {
        let start = self.expect(TokenKind::While, "`while`")?.span;
        let cond = self.parse_expr_no_struct()?;
        let body = self.parse_block_expr()?;
        let span = start.to(self.arenas.exprs[body].span);
        Some(self.mk_expr(ExpressionKind::While { cond, body }, span))
    }

    fn parse_loop(&mut self) -> Option<Idx<Expression>> {
        let start = self.expect(TokenKind::Loop, "`loop`")?.span;
        let body = self.parse_block_expr()?;
        let span = start.to(self.arenas.exprs[body].span);
        Some(self.mk_expr(ExpressionKind::Loop { body }, span))
    }

    fn parse_if(&mut self) -> Option<Idx<Expression>> {
        self.nested(|parser| {
            let start = parser.expect(TokenKind::If, "`if`")?.span;

            if parser.at(TokenKind::Let) {
                let span = parser.span();
                parser.error(span, "`if let` is not supported yet; use `match`");
                return None;
            }

            let cond = parser.parse_expr_no_struct()?;
            let then_branch = parser.parse_block_expr()?;

            let else_branch = if parser.eat(TokenKind::Else) {
                if parser.at(TokenKind::If) {
                    Some(parser.parse_if()?)
                } else {
                    Some(parser.parse_block_expr()?)
                }
            } else {
                None
            };

            let span = start.to(parser.prev_span);
            Some(parser.mk_expr(
                ExpressionKind::If {
                    cond,
                    then_branch,
                    else_branch,
                },
                span,
            ))
        })
    }

    fn parse_match(&mut self) -> Option<Idx<Expression>> {
        let start = self.expect(TokenKind::Match, "`match`")?.span;
        let scrutinee = self.parse_expr_no_struct()?;

        self.expect(TokenKind::LBrace, "`{`")?;

        let mut arms = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let arm_start = self.span();
            let pattern = self.parse_pattern()?;

            let guard = if self.eat(TokenKind::If) {
                Some(self.parse_expr()?)
            } else {
                None
            };

            self.expect(TokenKind::FatArrow, "`=>`")?;

            let body = self.parse_expr()?;

            // if the body is like a block (meaning ends with }) it need not have a command. Otherwise a Comma is needed
            if !self.eat(TokenKind::Comma) && !self.at(TokenKind::RBrace) && !matches!(self.arenas.exprs[body].kind, ExpressionKind::Block(_) | ExpressionKind::If { .. } | ExpressionKind::While { .. } | ExpressionKind::Loop { .. } | ExpressionKind::Match { .. }) {
                self.error_expected("`,` or `}` after match arm");
                return None;
            }

            arms.push(MatchArm {
                pat: pattern,
                guard,
                body,
                span: arm_start.to(self.prev_span),
            });
        }

        self.expect(TokenKind::RBrace, "`}`")?;
        let span = start.to(self.prev_span);
        Some(self.mk_expr(
            ExpressionKind::Match {
                scrutinee,
                arms,
            },
            span,
        ))
    }
}
