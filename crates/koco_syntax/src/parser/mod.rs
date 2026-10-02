mod expr;
mod item;
mod pat;
mod ty;

use crate::ast::*;
use crate::token::{Token, TokenKind};
use koco_span::{DiagnosticsCtx, Interner, Span, Symbol};

pub(crate) use ty::PathStyle;

// also i suppose the experission and statement parsing returns too early (upon error) in alot of places.
// need to think that through

pub(crate) struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    prev_span: Span,
    src: &'a str,
    interner: &'a mut Interner,
    diags: &'a DiagnosticsCtx,
    arenas: &'a mut Arenas,
    last_error_pos: Option<usize>,
    depth: u32,
}

const MAX_NESTING: u32 = 128;

impl<'a> Parser<'a> {
    pub(crate) fn new(tokens: Vec<Token>, src: &'a str, interner: &'a mut Interner, diags: &'a DiagnosticsCtx, arenas: &'a mut Arenas) -> Parser<'a> {
        let first = tokens.first().expect("token stream must end with Eof").span;
        Parser {
            tokens,
            pos: 0,
            prev_span: Span {
                file: first.file,
                lo: first.lo,
                hi: first.lo,
            },
            src,
            interner,
            diags,
            arenas,
            last_error_pos: None,
            depth: 0,
        }
    }

    // Use this when parsing iside a nested block
    fn nested<T, F: FnOnce(&mut Self) -> Option<T>>(&mut self, f: F) -> Option<T> {
        if self.depth >= MAX_NESTING {
            let span = self.span();
            self.error(span, "nesting is too deep");
            return None;
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    /// Check the current token kind
    fn peek(&self) -> TokenKind { self.peek_at(0) }

    /// Check the token at current + n
    fn peek_at(&self, n: usize) -> TokenKind {
        let i = (self.pos + n).min(self.tokens.len() - 1);
        self.tokens[i].kind
    }

    /// The span of the current token
    fn span(&self) -> Span { self.tokens[self.pos.min(self.tokens.len() - 1)].span }

    /// Whether the current token is equal to kind
    fn at(&self, kind: TokenKind) -> bool { self.peek() == kind }

    /// Move the current token forward
    fn bump(&mut self) -> Token {
        let tok = self.tokens[self.pos.min(self.tokens.len() - 1)];
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        self.prev_span = tok.span;
        tok
    }

    /// If current token is equal to kind,
    /// bump and return true
    /// else return false
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Whether the current token is equal to kind,
    /// If yes, bump and return it
    /// Else, log an error
    fn expect(&mut self, kind: TokenKind, what: &str) -> Option<Token> {
        if self.at(kind) {
            Some(self.bump())
        } else {
            self.error_expected(what);
            None
        }
    }

    fn text(&self, span: Span) -> &'a str { &self.src[span.range()] }

    fn intern(&mut self, span: Span) -> Symbol {
        let text = &self.src[span.range()];
        self.interner.intern(text)
    }

    fn error(&mut self, span: Span, msg: impl Into<String>) {
        if self.last_error_pos.is_some_and(|p| self.pos <= p) {
            return;
        }
        self.last_error_pos = Some(self.pos);
        self.diags.error(span, msg);
    }

    fn error_expected(&mut self, what: &str) {
        let found = self.peek().describe();
        let span = self.span();
        self.error(span, format!("expected {what}, found {found}"));
    }

    fn parse_ident(&mut self, what: &str) -> Option<(Symbol, Span)> {
        if self.at(TokenKind::Identifier) {
            let token = self.bump();
            Some((self.intern(token.span), token.span))
        } else {
            self.error_expected(what);
            None
        }
    }

    /// Consume a single `>` where a generic argument list closes, splitting compound
    /// tokens that start with `>` (`>>`, `>=`, `>>=`) so `Vec<Vec<T>>` parses.
    fn eat_close_angle(&mut self) -> bool {
        let tok = self.tokens[self.pos];
        let rest = match tok.kind {
            TokenKind::Gt => {
                self.bump();
                return true;
            }
            TokenKind::Shr => TokenKind::Gt,
            TokenKind::Ge => TokenKind::Eq,
            TokenKind::ShrEq => TokenKind::Ge,
            _ => return false,
        };
        self.prev_span = Span {
            file: tok.span.file,
            lo: tok.span.lo,
            hi: tok.span.lo + 1,
        };
        self.tokens[self.pos] = Token {
            kind: rest,
            span: Span {
                file: tok.span.file,
                lo: tok.span.lo + 1,
                hi: tok.span.hi,
            },
        };
        true
    }

    fn at_close_angle(&self) -> bool { matches!(self.peek(), TokenKind::Gt | TokenKind::Shr | TokenKind::Ge | TokenKind::ShrEq) }

    fn expect_close_angle(&mut self) -> Option<()> {
        if self.eat_close_angle() {
            Some(())
        } else {
            self.error_expected("`>`");
            None
        }
    }

    /// Value of an integer literal token (decimal or `0x` hex, `_` separators allowed).
    fn int_value(&mut self, span: Span) -> Option<u64> {
        let text: String = self.text(span).chars().filter(|c| *c != '_').collect();
        let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(hex, 16),
            None => text.parse::<u64>(),
        };
        match parsed {
            Ok(v) => Some(v),
            Err(_) => {
                self.error(span, "invalid integer literal (malformed or too large)");
                None
            }
        }
    }

    fn float_value(&mut self, span: Span) -> Option<f64> {
        let text: String = self.text(span).chars().filter(|c| *c != '_').collect();
        match text.parse::<f64>() {
            Ok(v) => Some(v),
            Err(_) => {
                self.error(span, "invalid float literal");
                None
            }
        }
    }

    /// Consume an int / float / bool literal token.
    fn parse_literal(&mut self) -> Option<Literal> {
        match self.peek() {
            TokenKind::IntLiteral => {
                let tok = self.bump();
                Some(Literal::Int(self.int_value(tok.span)?))
            }
            TokenKind::FloatLiteral => {
                let tok = self.bump();
                Some(Literal::Float(self.float_value(tok.span)?))
            }
            TokenKind::True => {
                self.bump();
                Some(Literal::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Some(Literal::Bool(false))
            }
            _ => {
                self.error_expected("a literal");
                None
            }
        }
    }

    fn mk_expr(&mut self, kind: ExpressionKind, span: Span) -> Idx<Expression> { self.arenas.exprs.alloc(Expression { kind, span }) }

    fn mk_type_ref(&mut self, kind: ParserTypeKind, span: Span) -> Idx<ParserType> { self.arenas.type_refs.alloc(ParserType { kind, span }) }

    fn mk_path(&mut self, data: Path) -> Idx<Path> { self.arenas.paths.alloc(data) }

    /// A `Binding` pattern for a plain name (function parameters and `self`, which do not
    /// go through `parse_pattern`).
    fn mk_binding_pat(&mut self, name: Symbol, mutable: bool, span: Span) -> Idx<Pattern> {
        self.mk_pat(
            PatternKind::Binding {
                name,
                mutable,
                sub: None,
            },
            span,
        )
    }

    fn mk_pat(&mut self, kind: PatternKind, span: Span) -> Idx<Pattern> { self.arenas.pats.alloc(Pattern { kind, span }) }

    /// Zero or more `#[...]` attributes.
    fn parse_attrs(&mut self) -> Option<Vec<Attribute>> {
        let mut attrs = Vec::new();
        while self.at(TokenKind::Hash) {
            attrs.push(self.parse_attr()?);
        }
        Some(attrs)
    }

    fn parse_attr(&mut self) -> Option<Attribute> {
        let start = self.span();
        self.expect(TokenKind::Hash, "`#`")?;
        self.expect(TokenKind::LBracket, "`[` after `#`")?;
        let (name, _) = self.parse_ident("an attribute name")?;
        let mut args = Vec::new();
        if self.eat(TokenKind::LParen) {
            while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                args.push(self.parse_attr_arg()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "`)`")?;
        }
        let end = self.expect(TokenKind::RBracket, "`]`")?.span;
        Some(Attribute {
            name,
            args,
            span: start.to(end),
        })
    }

    fn parse_attr_arg(&mut self) -> Option<AttributeArg> {
        let start = self.span();
        let key = if self.at(TokenKind::Identifier) && self.peek_at(1) == TokenKind::Eq {
            let (key, _) = self.parse_ident("an attribute key")?;
            self.bump(); // `=`
            Some(key)
        } else {
            None
        };
        let value = match self.peek() {
            TokenKind::Identifier => {
                let tok = self.bump();
                AttrValue::Ident(self.intern(tok.span))
            }
            TokenKind::IntLiteral => {
                let tok = self.bump();
                AttrValue::Int(self.int_value(tok.span)?)
            }
            _ => {
                self.error_expected("an identifier or integer attribute value");
                return None;
            }
        };
        Some(AttributeArg {
            key,
            value,
            span: start.to(self.prev_span),
        })
    }

    /// Skip to a pausible start of next item
    fn sync_item(&mut self, until: TokenKind) {
        let mut depth = 0i32;
        loop {
            let k = self.peek();
            if k == TokenKind::Eof {
                break;
            }
            if depth == 0 && (k == until || is_item_start(k)) {
                break;
            }
            match k {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace if depth > 0 => depth -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    /// Skip to a pausible start of next statement inside a block
    fn sync_stmt(&mut self) {
        let mut depth = 0i32;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::Semi if depth == 0 => {
                    self.bump();
                    break;
                }
                TokenKind::RBrace => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.bump();
                }
                TokenKind::LBrace => {
                    depth += 1;
                    self.bump();
                }
                TokenKind::Let | TokenKind::Return | TokenKind::If | TokenKind::While | TokenKind::Loop | TokenKind::Match if depth == 0 => break,
                _ => {
                    self.bump();
                }
            }
        }
    }
}

fn is_item_start(kind: TokenKind) -> bool { matches!(kind, TokenKind::Fn | TokenKind::Struct | TokenKind::Enum | TokenKind::Trait | TokenKind::Impl | TokenKind::Mod | TokenKind::Use | TokenKind::Pub | TokenKind::Type | TokenKind::Const | TokenKind::Static | TokenKind::Hash) }
