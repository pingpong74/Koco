use logos::Logos;
use std::ops::Range;

#[derive(Logos, Debug, PartialEq, Eq, Hash, Clone, Copy)]
#[logos(error = String)]
pub enum TokenKind {
    // key words
    #[token("fn")]
    Function,
    #[token("struct")]
    Struct,
    #[token("enum")]
    Enum,
    #[token("let")]
    Let,
    #[token("mut")]
    Mut,
    #[token("const")]
    Const,
    #[token("return")]
    Return,
    #[token("if")]
    If,
    #[token("else if")]
    ElseIf,
    #[token("else")]
    Else,
    #[token("loop")]
    Loop,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("while")]
    While,
    #[token("as")]
    As,

    // global variable declarations
    #[token("#layout")]
    Layout,
    #[token("set")]
    Set,
    #[token("binding")]
    Binding,
    #[token("_push_constant")]
    PushConstant,

    // symbols
    #[token("{")]
    LCurlyBracket,
    #[token("}")]
    RCurlyBracket,
    #[token("(")]
    LParentheses,
    #[token(")")]
    RParentheses,
    #[token("[")]
    LSquareParentheses,
    #[token("]")]
    RSquareParentheses,
    #[token(",")]
    Comma,
    #[token(";")]
    SemiColon,
    #[token(":")]
    Colon,
    #[token("->")]
    Arrow,
    #[token("<")]
    LAngle,
    #[token(">")]
    RAngle,
    #[token("#")]
    Hashtag,
    #[token(".")]
    Dot,
    #[token("::")]
    ColonColon,

    // operators
    // arithematic
    #[token("=")]
    Equal,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Product,
    #[token("/")]
    Divide,
    #[token("%")]
    Percent,
    #[token("!")]
    Not,

    // boolean
    #[token("==")]
    IsEqualTo,
    #[token("!=")]
    IsNotEqualTo,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,

    // logical operators
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,

    // bitwise operators
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("<<")]
    ShiftLeft,
    #[token(">>")]
    ShiftRight,

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*")]
    Identifier,

    #[regex(r"[0-9]+\.[0-9]+")]
    FloatLiteral,

    #[regex(r"[0-9]+")]
    IntLiteral,

    // Skip whitespace
    #[regex(r"[ \t\n\f]+", logos::skip)]
    WhiteSpace,

    EndOfFile,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

#[derive(Debug)]
pub struct LexError<'a> {
    pub slice: &'a str,
    pub span: Range<usize>,
}

impl Token {
    pub const END_OF_FILE: Token = Token {
        kind: TokenKind::EndOfFile,
        span: Range {
            start: 0,
            end: 0,
        },
    };

    #[inline]
    pub const fn non_file_token(token_kind: TokenKind) -> Token {
        return Token {
            kind: token_kind,
            span: Range {
                start: 0,
                end: 0,
            },
        };
    }

    pub fn lex<'a>(source: &'a str) -> Result<Vec<Token>, Vec<LexError<'a>>> {
        let mut lexer = TokenKind::lexer(source);
        let mut tokens = Vec::new();
        let mut errors = Vec::new();

        while let Some(result) = lexer.next() {
            let span = lexer.span();

            match result {
                Ok(kind) => {
                    tokens.push(Token {
                        kind,
                        span: span,
                    });
                }
                Err(_) => {
                    errors.push(LexError {
                        slice: &source[span.clone()],
                        span: span,
                    });
                }
            }
        }

        if errors.is_empty() { Ok(tokens) } else { Err(errors) }
    }
}
