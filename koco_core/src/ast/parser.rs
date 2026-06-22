use crate::*;

pub struct Parser<'t, 's> {
    tokens: &'t [Token],
    src: &'s str,
    current: usize,
    errors: Vec<ParseError>,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub got: Token,
    pub expected: &'static str,
}

impl<'t, 's> Parser<'t, 's> {
    pub fn new(tokens: &'t [Token], src: &'s str) -> Self {
        Parser {
            tokens,
            src,
            current: 0,
            errors: vec![],
        }
    }

    pub fn parse(&mut self) -> (SyntaxTree, Vec<ParseError>) {
        let mut funcs = vec![];
        let mut desc = vec![];
        let mut structs = vec![];
        let mut const_globals = vec![];
        let mut enums = vec![];

        while self.peek().kind != TokenKind::EndOfFile {
            match self.peek().kind {
                TokenKind::Layout => match self.parse_descriptors() {
                    Ok(d) => desc.push(d),
                    Err(e) => {
                        self.push_error(e);
                        self.sync_top_level();
                    }
                },
                TokenKind::Hashtag | TokenKind::Function => match self.parse_function() {
                    Ok(f) => funcs.push(f),
                    Err(e) => {
                        self.push_error(e);
                        self.sync_top_level();
                    }
                },
                TokenKind::Struct => match self.parse_struct() {
                    Ok(s) => structs.push(s),
                    Err(e) => {
                        self.push_error(e);
                        self.sync_top_level();
                    }
                },
                TokenKind::Const => match self.parse_statement() {
                    Ok(s) => const_globals.push(s),
                    Err(e) => {
                        self.push_error(e);
                        self.sync_top_level();
                    }
                },
                TokenKind::Enum => match self.parse_enum() {
                    Ok(e) => enums.push(e),
                    Err(e) => {
                        self.push_error(e);
                        self.sync_top_level();
                    }
                },
                _ => {
                    self.push_error(ParseError {
                        got: self.peek().clone(),
                        expected: "Expected fn, #, struct, const, enum or #layout",
                    });
                    self.sync_top_level();
                }
            }
        }

        let tree = SyntaxTree {
            functions: funcs,
            descriptors: desc,
            global_variables: const_globals,
            structs,
            enums,
        };
        let errors = std::mem::take(&mut self.errors);
        (tree, errors)
    }

    pub(crate) fn peek(&self) -> &Token {
        self.tokens.get(self.current).unwrap_or(&Token::END_OF_FILE)
    }

    pub(crate) fn advance(&mut self) -> Token {
        let token = self.tokens.get(self.current).cloned().unwrap_or(Token::END_OF_FILE);
        self.current += 1;
        token
    }

    pub(crate) fn expect(&mut self, expected: TokenKind, msg: &'static str) -> Result<Token, ParseError> {
        if self.peek().kind == expected {
            Ok(self.advance())
        } else {
            Err(ParseError {
                got: self.peek().clone(),
                expected: msg,
            })
        }
    }

    pub(crate) fn eat(&mut self, tok: TokenKind) -> bool {
        if self.peek().kind == tok {
            self.advance();
            true
        } else {
            false
        }
    }

    pub(crate) fn recover<T>(&mut self, result: Result<T, ParseError>, fallback: T) -> T {
        match result {
            Ok(v) => v,
            Err(e) => {
                self.push_error(e);
                fallback
            }
        }
    }

    pub(crate) fn push_error(&mut self, err: ParseError) {
        self.errors.push(err);
    }

    fn sync_top_level(&mut self) {
        loop {
            match self.peek().kind {
                TokenKind::Function
                | TokenKind::Struct
                | TokenKind::Const
                | TokenKind::Enum
                | TokenKind::Hashtag
                | TokenKind::Layout
                | TokenKind::EndOfFile => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    pub(crate) fn sync_statement(&mut self) {
        loop {
            match self.peek().kind {
                TokenKind::SemiColon => {
                    self.advance();
                    break;
                }
                TokenKind::RCurlyBracket => break,
                TokenKind::If => break,
                TokenKind::Let => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    pub(crate) fn get_text(&self, token: &Token) -> &'s str {
        &self.src[token.span.clone()]
    }

    pub(crate) fn parse_type(&mut self) -> Result<ParserType, ParseError> {
        if self.eat(TokenKind::LSquareParentheses) {
            let ty = self.parse_type()?;

            self.expect(TokenKind::SemiColon, "Expected ;")?;

            let len_token = self.expect(TokenKind::IntLiteral, "Expected int")?;
            let len = self
                .get_text(&len_token)
                .trim()
                .parse::<usize>()
                .map_err(|_| ParseError {
                    got: len_token.clone(),
                    expected: "Expected an int literal",
                })?;

            self.expect(TokenKind::RSquareParentheses, "Expected ]")?;

            return Ok(ParserType::Array(Box::new(ty), len as u32));
        }

        let token = self.expect(TokenKind::Identifier, "Expected a type")?;
        Ok(ParserType::Single(self.get_text(&token).to_string()))
    }
}
