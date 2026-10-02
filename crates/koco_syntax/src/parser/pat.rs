use super::{Parser, PathStyle};
use crate::ast::*;
use crate::token::TokenKind;

impl<'a> Parser<'a> {
    pub(crate) fn parse_pattern(&mut self) -> Option<Idx<Pattern>> {
        let start = self.span();
        self.eat(TokenKind::Pipe);
        let first = self.parse_pattern_no_or()?;
        if !self.at(TokenKind::Pipe) {
            return Some(first);
        }
        let mut alts = vec![first];
        while self.eat(TokenKind::Pipe) {
            alts.push(self.parse_pattern_no_or()?);
        }
        let span = start.to(self.prev_span);
        Some(self.mk_pat(PatternKind::Or(alts), span))
    }

    fn parse_pattern_no_or(&mut self) -> Option<Idx<Pattern>> { self.nested(|p| p.parse_pattern_no_or_inner()) }

    fn parse_pattern_no_or_inner(&mut self) -> Option<Idx<Pattern>> {
        let start = self.span();
        match self.peek() {
            TokenKind::Identifier if self.text(start) == "_" => {
                self.bump();
                Some(self.mk_pat(PatternKind::Wildcard, start))
            }
            TokenKind::Mut => {
                self.bump();
                let (name, _) = self.parse_ident("a binding name after `mut`")?;
                let sub = self.parse_binding_subpattern()?;
                let span = start.to(self.prev_span);
                Some(self.mk_pat(
                    PatternKind::Binding {
                        name,
                        mutable: true,
                        sub,
                    },
                    span,
                ))
            }
            TokenKind::IntLiteral | TokenKind::FloatLiteral | TokenKind::True | TokenKind::False => {
                let value = self.parse_literal()?;
                let span = start.to(self.prev_span);
                Some(self.mk_pat(
                    PatternKind::Literal {
                        value,
                        negative: false,
                    },
                    span,
                ))
            }
            TokenKind::Minus => {
                self.bump();
                if !matches!(self.peek(), TokenKind::IntLiteral | TokenKind::FloatLiteral) {
                    self.error_expected("a numeric literal after `-`");
                    return None;
                }
                let value = self.parse_literal()?;
                let span = start.to(self.prev_span);
                Some(self.mk_pat(
                    PatternKind::Literal {
                        value,
                        negative: true,
                    },
                    span,
                ))
            }
            TokenKind::LParen => {
                self.bump();
                let mut elems = Vec::new();
                let mut trailing_comma = false;
                while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                    elems.push(self.parse_pattern()?);
                    trailing_comma = self.eat(TokenKind::Comma);
                    if !trailing_comma {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "`)`")?;
                if elems.len() == 1 && !trailing_comma {
                    return Some(elems[0]); // parenthesised pattern
                }
                let span = start.to(self.prev_span);
                Some(self.mk_pat(PatternKind::Tuple(elems), span))
            }
            _ if self.at_path_start() => {
                // A lone plain identifier that cannot start a longer path is a binding.
                // Parsing it as a `Path` first, only to throw the path away, would leave
                // an orphaned arena node.
                if self.peek() == TokenKind::Identifier && !matches!(self.peek_at(1), TokenKind::ColonColon | TokenKind::LParen | TokenKind::LBrace) {
                    let tok = self.bump();
                    let name = self.intern(tok.span);
                    let sub = self.parse_binding_subpattern()?;
                    let span = start.to(self.prev_span);
                    return Some(self.mk_pat(
                        PatternKind::Binding {
                            name,
                            mutable: false,
                            sub,
                        },
                        span,
                    ));
                }

                let path = self.parse_path(PathStyle::Expr)?;
                match self.peek() {
                    TokenKind::LParen => {
                        self.bump();
                        let mut fields = Vec::new();
                        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                            fields.push(self.parse_pattern()?);
                            if !self.eat(TokenKind::Comma) {
                                break;
                            }
                        }
                        self.expect(TokenKind::RParen, "`)`")?;
                        let span = start.to(self.prev_span);
                        Some(self.mk_pat(
                            PatternKind::TupleStruct {
                                path,
                                fields,
                            },
                            span,
                        ))
                    }
                    TokenKind::LBrace => self.parse_struct_pattern(path, start),
                    // A multi-segment path (`Shape::Empty`) or a lone `self`/`Self`.
                    _ => {
                        let span = self.arenas.paths[path].span;
                        Some(self.mk_pat(PatternKind::Path(path), span))
                    }
                }
            }
            _ => {
                self.error_expected("a pattern");
                None
            }
        }
    }

    /// The optional `@ pat` after a binding name.
    fn parse_binding_subpattern(&mut self) -> Option<Option<Idx<Pattern>>> {
        if self.eat(TokenKind::At) {
            Some(Some(self.parse_pattern_no_or()?))
        } else {
            Some(None)
        }
    }

    fn parse_struct_pattern(&mut self, path: Idx<Path>, start: koco_span::Span) -> Option<Idx<Pattern>> {
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut fields = Vec::new();
        let mut rest = false;
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            if self.eat(TokenKind::DotDot) {
                rest = true;
                break;
            }

            let fstart = self.span();
            let mutable = self.eat(TokenKind::Mut);
            let (name, name_span) = self.parse_ident("a field name")?;

            let pat = if !mutable && self.eat(TokenKind::Colon) {
                self.parse_pattern()?
            } else {
                self.mk_pat(
                    PatternKind::Binding {
                        name,
                        mutable,
                        sub: None,
                    },
                    fstart.to(name_span),
                )
            };
            fields.push(FieldPattern {
                name,
                pat,
                span: fstart.to(self.prev_span),
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RBrace, "`}`")?;
        let span = start.to(self.prev_span);

        Some(self.mk_pat(
            PatternKind::Struct {
                path,
                fields,
                rest,
            },
            span,
        ))
    }
}
