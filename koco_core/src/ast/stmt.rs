use crate::*;

impl<'t, 's> Parser<'t, 's> {
    pub(crate) fn parse_scope(&mut self) -> Result<Scope, ParseError> {
        self.expect(TokenKind::LCurlyBracket, "{")?;

        let mut statements = vec![];

        while self.peek().kind != TokenKind::RCurlyBracket && self.peek().kind != TokenKind::EndOfFile {
            let stmt = self.parse_statement();
            match stmt {
                Ok(stmt) => statements.push(stmt),
                Err(err) => {
                    self.push_error(err);
                    self.sync_statement();
                }
            }
        }

        self.expect(TokenKind::RCurlyBracket, "}")?;

        return Ok(Scope { statements });
    }

    pub(crate) fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        let statement = match self.peek().kind {
            TokenKind::Let => {
                self.advance();

                let mutatble = if self.peek().kind == TokenKind::Mut {
                    self.advance();
                    true
                } else {
                    false
                };

                let name = if self.peek().kind == TokenKind::Identifier {
                    self.get_text(self.peek()).to_string()
                } else {
                    return Err(ParseError {
                        got: self.peek().clone(),
                        expected: "Expected variable name",
                    });
                };

                self.advance();

                let var_type = if self.peek().kind == TokenKind::Colon {
                    self.advance();
                    let ty = Some(self.parse_type()?);
                    ty
                } else {
                    None
                };

                let init_expr = if self.peek().kind == TokenKind::Equal {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };

                Statement::Declaration(LocalVariable {
                    mutable: mutatble,
                    name,
                    var_type,
                    init: init_expr,
                })
            }
            TokenKind::Const => {
                self.advance();

                let name = if self.peek().kind == TokenKind::Identifier {
                    self.get_text(self.peek()).to_string()
                } else {
                    return Err(ParseError {
                        got: self.peek().clone(),
                        expected: "Expected Identifier",
                    });
                };

                self.advance();

                let var_type = if self.peek().kind == TokenKind::Colon {
                    self.advance();
                    let ty = Some(self.parse_type()?);
                    ty
                } else {
                    return Err(ParseError {
                        got: self.peek().clone(),
                        expected: "Expected : followed by variable type ",
                    });
                };

                let init_expr = if self.peek().kind == TokenKind::Equal {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    return Err(ParseError {
                        got: self.peek().clone(),
                        expected: "Const variables must initialized ",
                    });
                };

                Statement::Declaration(LocalVariable {
                    mutable: false,
                    name,
                    var_type,
                    init: init_expr,
                })
            }
            TokenKind::Return => {
                self.advance();
                let expr = if self.peek().kind == TokenKind::SemiColon {
                    None
                } else {
                    Some(self.parse_expression()?)
                };
                Statement::Return(expr)
            }
            TokenKind::Identifier => {
                let target = self.parse_expression()?;

                if self.eat(TokenKind::Equal) {
                    let value = self.parse_expression()?;
                    Statement::Assign {
                        target,
                        value,
                    }
                } else {
                    match target {
                        Expression::Call { .. } => Statement::FunctionCall(target),
                        _ => {
                            return Err(ParseError {
                                got: self.peek().clone(),
                                expected: "= or function call",
                            });
                        }
                    }
                }
            }
            TokenKind::If => {
                self.advance();

                let mut conditions = vec![self.parse_expression_no_struct()?];
                let mut scopes = vec![self.parse_scope()?];

                while self.peek().kind == TokenKind::Else || self.peek().kind == TokenKind::ElseIf {
                    let token = self.advance();

                    if token.kind == TokenKind::ElseIf {
                        conditions.push(self.parse_expression_no_struct()?);
                        scopes.push(self.parse_scope()?);
                    } else {
                        scopes.push(self.parse_scope()?);
                        break;
                    }
                }

                Statement::If {
                    scopes,
                    conditions,
                }
            }
            TokenKind::Loop => {
                self.advance();

                let scope = self.parse_scope()?;

                Statement::Loop { scope }
            }
            TokenKind::While => {
                self.advance();

                let condition = self.parse_expression_no_struct()?;
                let scope = self.parse_scope()?;

                Statement::While {
                    condition,
                    scope,
                }
            }
            TokenKind::Break => {
                self.advance();
                Statement::Break
            }
            TokenKind::Continue => {
                self.advance();
                Statement::Continue
            }
            _ => {
                return Err(ParseError {
                    got: self.peek().clone(),
                    expected: "Got unexpected token",
                });
            }
        };

        self.expect(TokenKind::SemiColon, "Expected ; at end of statement")?;

        Ok(statement)
    }
}
