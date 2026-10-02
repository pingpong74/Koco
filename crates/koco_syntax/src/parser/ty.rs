use super::Parser;
use crate::ast::*;
use crate::token::TokenKind;

/// Where a path appears. In expressions generic arguments need the turbofish (`foo::<T>()`)
/// because `foo<T>` would read as a comparison; in types a bare `<` opens them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathStyle {
    Expr,
    Ty,
}

fn is_path_segment_start(kind: TokenKind) -> bool { matches!(kind, TokenKind::Identifier | TokenKind::SelfValue | TokenKind::SelfType | TokenKind::Crate | TokenKind::Super) }

impl<'a> Parser<'a> {
    pub(super) fn at_path_start(&self) -> bool { is_path_segment_start(self.peek()) }

    pub(crate) fn parse_path(&mut self, style: PathStyle) -> Option<Idx<Path>> { self.nested(|p| p.parse_path_inner(style)) }

    fn parse_path_inner(&mut self, style: PathStyle) -> Option<Idx<Path>> {
        let start = self.span();
        let mut segments = Vec::new();
        loop {
            let seg_start = self.span();
            if !is_path_segment_start(self.peek()) {
                self.error_expected("an identifier");
                return None;
            }
            let tok = self.bump();
            let ident = self.intern(tok.span);

            let mut args = Vec::new();
            let turbofish = self.at(TokenKind::ColonColon) && self.peek_at(1) == TokenKind::Lt;
            if turbofish {
                self.bump(); // `::`
                args = self.parse_generic_args()?;
            } else if style == PathStyle::Ty && self.at(TokenKind::Lt) {
                args = self.parse_generic_args()?;
            }

            segments.push(PathSegment {
                ident,
                args,
                span: seg_start.to(self.prev_span),
            });

            if self.at(TokenKind::ColonColon) && is_path_segment_start(self.peek_at(1)) {
                self.bump(); // `::`
                continue;
            }
            break;
        }
        let span = start.to(self.prev_span);
        Some(self.mk_path(Path {
            segments,
            span,
        }))
    }

    /// `<A, B, Name = C, 3, { N + 1 }>`
    pub(crate) fn parse_generic_args(&mut self) -> Option<Vec<GenericArg>> {
        self.expect(TokenKind::Lt, "`<`")?;
        let mut args = Vec::new();
        while !self.at_close_angle() && !self.at(TokenKind::Eof) {
            args.push(self.parse_generic_arg()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect_close_angle()?;
        Some(args)
    }

    fn parse_generic_arg(&mut self) -> Option<GenericArg> {
        match self.peek() {
            TokenKind::IntLiteral | TokenKind::FloatLiteral | TokenKind::True | TokenKind::False => {
                let start = self.span();
                let lit = self.parse_literal()?;
                Some(GenericArg::Const(ConstArg::Literal(lit, start.to(self.prev_span))))
            }
            TokenKind::LBrace => {
                let block = self.parse_block_expr()?;
                Some(GenericArg::Const(ConstArg::Block(block)))
            }
            TokenKind::Identifier if self.peek_at(1) == TokenKind::Eq => {
                let start = self.span();
                let (name, _) = self.parse_ident("an associated type name")?;
                self.bump(); // `=`
                let ty = self.parse_ty()?;
                Some(GenericArg::AssocBinding {
                    name,
                    ty,
                    span: start.to(self.prev_span),
                })
            }
            _ => Some(GenericArg::Type(self.parse_ty()?)),
        }
    }

    /// An array length: literal, named constant, or `{ expr }`.
    fn parse_const_arg(&mut self) -> Option<ConstArg> {
        match self.peek() {
            TokenKind::IntLiteral => {
                let start = self.span();
                let lit = self.parse_literal()?;
                Some(ConstArg::Literal(lit, start.to(self.prev_span)))
            }
            TokenKind::LBrace => Some(ConstArg::Block(self.parse_block_expr()?)),
            k if is_path_segment_start(k) => Some(ConstArg::Path(self.parse_path(PathStyle::Expr)?)),
            _ => {
                self.error_expected("an array length");
                None
            }
        }
    }

    pub(crate) fn parse_ty(&mut self) -> Option<Idx<ParserType>> { self.nested(|p| p.parse_ty_inner()) }

    fn parse_ty_inner(&mut self) -> Option<Idx<ParserType>> {
        let start = self.span();
        match self.peek() {
            TokenKind::LBracket => {
                self.bump();
                let elem = self.parse_ty()?;
                self.expect(TokenKind::Semi, "`;` in array type")?;
                let len = self.parse_const_arg()?;
                self.expect(TokenKind::RBracket, "`]`")?;
                let span = start.to(self.prev_span);
                Some(self.mk_type_ref(ParserTypeKind::Array(elem, len), span))
            }
            TokenKind::LParen => {
                self.bump();
                if self.eat(TokenKind::RParen) {
                    let span = start.to(self.prev_span);
                    return Some(self.mk_type_ref(ParserTypeKind::Tuple(Vec::new()), span));
                }
                let mut elems = vec![self.parse_ty()?];
                let mut trailing_comma = false;
                while self.eat(TokenKind::Comma) {
                    trailing_comma = true;
                    if self.at(TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_ty()?);
                    trailing_comma = false;
                }
                self.expect(TokenKind::RParen, "`)`")?;
                if elems.len() == 1 && !trailing_comma {
                    return Some(elems[0]); // just a parenthesised type
                }
                let span = start.to(self.prev_span);
                Some(self.mk_type_ref(ParserTypeKind::Tuple(elems), span))
            }
            TokenKind::Amp | TokenKind::AndAnd => {
                // References are call-boundary parameter modes, not types.
                let span = self.span();
                self.error(span, "reference types (`&T`, `&mut T`) are only allowed as function parameter types");
                let was_double = self.at(TokenKind::AndAnd);
                self.bump();
                if !was_double {
                    self.eat(TokenKind::Mut);
                }
                // Recover by parsing the type behind the reference.
                self.parse_ty()
            }
            k if is_path_segment_start(k) => {
                let path = self.parse_path(PathStyle::Ty)?;
                let span = self.arenas.paths[path].span;
                Some(self.mk_type_ref(ParserTypeKind::Path(path), span))
            }
            _ => {
                self.error_expected("a type");
                None
            }
        }
    }
}
