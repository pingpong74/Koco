use koco_span::Span;
use logos::Logos;

/// Consumes a `// ...` comment up to the end of the line. A callback rather than a regex
/// because logos 0.16 rejects unbounded dot-like repetitions in `skip`/`regex` patterns.
fn line_comment(lex: &mut logos::Lexer<TokenKind>) -> logos::FilterResult<(), ()> {
    let rest = lex.remainder();
    lex.bump(rest.find('\n').unwrap_or(rest.len()));
    logos::FilterResult::Skip
}

/// Consumes a `/* ... */` comment. logos cannot express this as a regex (it would need
/// backtracking), so the callback scans for the terminator; an unterminated comment
/// becomes a lexer error covering the rest of the file.
fn block_comment(lex: &mut logos::Lexer<TokenKind>) -> logos::FilterResult<(), ()> {
    let rest = lex.remainder();
    match rest.find("*/") {
        Some(end) => {
            lex.bump(end + 2);
            logos::FilterResult::Skip
        }
        None => {
            lex.bump(rest.len());
            logos::FilterResult::Error(())
        }
    }
}

/// Every kind of token the lexer produces. Attributes are *not* special-cased:
/// `layout`, `set`, `vertex`, ... are plain identifiers that the parser reads generically,
/// so new attributes need no lexer change.
#[derive(Logos, Debug, PartialEq, Eq, Hash, Clone, Copy)]
#[logos(skip r"[ \t\r\n\f]+")]
pub enum TokenKind {
    /// Never emitted; the `line_comment` callback skips it.
    #[token("//", line_comment)]
    LineComment,

    /// Never emitted; the `block_comment` callback skips it.
    #[token("/*", block_comment)]
    BlockComment,

    // ---- keywords ----
    #[token("fn")]
    Fn,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("trait")]
    Trait,
    #[token("impl")]
    Impl,
    #[token("mod")]
    Mod,
    #[token("use")]
    Use,
    #[token("pub")]
    Pub,
    #[token("type")]
    Type,
    #[token("let")]
    Let,
    #[token("mut")]
    Mut,
    #[token("const")]
    Const,
    #[token("static")]
    Static,
    #[token("return")]
    Return,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("loop")]
    Loop,
    #[token("while")]
    While,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("as")]
    As,
    #[token("match")]
    Match,
    #[token("for")]
    For,
    /// `self`
    #[token("self")]
    SelfValue,
    /// `Self`
    #[token("Self")]
    SelfType,
    #[token("crate")]
    Crate,
    #[token("super")]
    Super,

    // ---- delimiters & punctuation ----
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
    #[token(":")]
    Colon,
    #[token("::")]
    ColonColon,
    #[token("->")]
    Arrow,
    #[token("=>")]
    FatArrow,
    #[token("#")]
    Hash,
    #[token(".")]
    Dot,
    #[token("..")]
    DotDot,
    #[token("@")]
    At,

    // ---- operators ----
    #[token("=")]
    Eq,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("!")]
    Bang,

    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,

    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,

    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,

    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("<<=")]
    ShlEq,
    #[token(">>=")]
    ShrEq,

    // ---- literals & identifiers ----
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Identifier,
    #[regex(r"[0-9][0-9_]*")]
    #[regex(r"0[xX][0-9a-fA-F_]+")]
    IntLiteral,
    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*([eE][+-]?[0-9]+)?")]
    #[regex(r"[0-9][0-9_]*[eE][+-]?[0-9]+")]
    FloatLiteral,

    /// Synthetic end-of-input marker appended by [`crate::lex`]; never produced by logos.
    Eof,
}

impl TokenKind {
    /// Human readable name used in "expected X, found Y" messages.
    pub fn describe(self) -> &'static str {
        use TokenKind::*;
        match self {
            LineComment | BlockComment => "a comment",
            Fn => "`fn`",
            Struct => "`struct`",
            Enum => "`enum`",
            Trait => "`trait`",
            Impl => "`impl`",
            Mod => "`mod`",
            Use => "`use`",
            Pub => "`pub`",
            Type => "`type`",
            Let => "`let`",
            Mut => "`mut`",
            Const => "`const`",
            Static => "`static`",
            Return => "`return`",
            If => "`if`",
            Else => "`else`",
            Loop => "`loop`",
            While => "`while`",
            Break => "`break`",
            Continue => "`continue`",
            True => "`true`",
            False => "`false`",
            As => "`as`",
            Match => "`match`",
            For => "`for`",
            SelfValue => "`self`",
            SelfType => "`Self`",
            Crate => "`crate`",
            Super => "`super`",
            LBrace => "`{`",
            RBrace => "`}`",
            LParen => "`(`",
            RParen => "`)`",
            LBracket => "`[`",
            RBracket => "`]`",
            Comma => "`,`",
            Semi => "`;`",
            Colon => "`:`",
            ColonColon => "`::`",
            Arrow => "`->`",
            FatArrow => "`=>`",
            Hash => "`#`",
            Dot => "`.`",
            DotDot => "`..`",
            At => "`@`",
            Eq => "`=`",
            Plus => "`+`",
            Minus => "`-`",
            Star => "`*`",
            Slash => "`/`",
            Percent => "`%`",
            Bang => "`!`",
            EqEq => "`==`",
            NotEq => "`!=`",
            Lt => "`<`",
            Gt => "`>`",
            Le => "`<=`",
            Ge => "`>=`",
            AndAnd => "`&&`",
            OrOr => "`||`",
            Amp => "`&`",
            Pipe => "`|`",
            Caret => "`^`",
            Shl => "`<<`",
            Shr => "`>>`",
            PlusEq => "`+=`",
            MinusEq => "`-=`",
            StarEq => "`*=`",
            SlashEq => "`/=`",
            PercentEq => "`%=`",
            AmpEq => "`&=`",
            PipeEq => "`|=`",
            CaretEq => "`^=`",
            ShlEq => "`<<=`",
            ShrEq => "`>>=`",
            Identifier => "an identifier",
            IntLiteral => "an integer literal",
            FloatLiteral => "a float literal",
            Eof => "end of file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}
