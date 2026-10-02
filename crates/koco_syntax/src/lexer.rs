use crate::token::{Token, TokenKind};
use koco_span::{DiagnosticsCtx, FileId, Span};
use logos::Logos;

pub fn lex(src: &str, file: FileId, diags: &DiagnosticsCtx) -> Vec<Token> {
    let mut lexer = TokenKind::lexer(src);
    let mut tokens = Vec::new();
    let mut bad: Option<(usize, usize)> = None;

    let flush = |bad: &mut Option<(usize, usize)>| {
        if let Some((lo, hi)) = bad.take() {
            let text = &src[lo..hi];
            let msg = if text.starts_with("/*") {
                "unterminated block comment".to_string()
            } else {
                format!("unrecognized character(s) `{text}`")
            };
            diags.error(Span::new(file, lo, hi), msg);
        }
    };

    while let Some(result) = lexer.next() {
        let range = lexer.span();
        match result {
            Ok(kind) => {
                flush(&mut bad);
                tokens.push(Token {
                    kind,
                    span: Span::new(file, range.start, range.end),
                });
            }
            Err(_) => match &mut bad {
                Some((_, hi)) if *hi == range.start => *hi = range.end,
                _ => {
                    flush(&mut bad);
                    bad = Some((range.start, range.end));
                }
            },
        }
    }
    flush(&mut bad);

    tokens.push(Token {
        kind: TokenKind::Eof,
        span: Span::new(file, src.len(), src.len()),
    });
    tokens
}
