use crate::*;

impl<'t, 's> Parser<'t, 's> {
    pub(crate) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_expr_recursive(0, true)
    }

    pub(crate) fn parse_expression_no_struct(&mut self) -> Result<Expression, ParseError> {
        self.parse_expr_recursive(0, false)
    }

    pub(crate) fn parse_expr_recursive(&mut self, min_bp: u8, allow_struct: bool) -> Result<Expression, ParseError> {
        let mut lhs = match self.peek().kind {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr_recursive(99, allow_struct)?;
                Expression::Unary {
                    op: UnaryOp::Negate,
                    expr: Box::new(expr),
                }
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr_recursive(99, allow_struct)?;
                Expression::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                }
            }
            TokenKind::LParentheses => {
                self.advance();
                let expr = self.parse_expr_recursive(0, true)?;
                self.expect(TokenKind::RParentheses, ")")?;
                expr
            }

            _ => self.parse_expr_main(allow_struct)?,
        };

        loop {
            if self.eat(TokenKind::Dot) {
                let field_token = self.expect(TokenKind::Identifier, "Expected field name")?;
                let field_name = self.get_text(&field_token).to_string();

                lhs = Expression::Member {
                    object: Box::new(lhs),
                    field: field_name,
                };

                continue;
            }

            if self.eat(TokenKind::LSquareParentheses) {
                let index = self.parse_expression()?;
                self.expect(TokenKind::RSquareParentheses, "]")?;

                lhs = Expression::Index {
                    object: Box::new(lhs),
                    index: Box::new(index),
                };

                continue;
            }

            if self.eat(TokenKind::As) {
                let ty = self.parse_type()?;
                lhs = Expression::Cast {
                    ty,
                    expr: Box::new(lhs),
                };
                continue;
            }

            let op = match self.peek().kind {
                // arithemetic
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                TokenKind::Product => BinaryOp::Multiply,
                TokenKind::Divide => BinaryOp::Divide,
                TokenKind::Percent => BinaryOp::Remainder,

                TokenKind::IsEqualTo => BinaryOp::IsEqual,
                TokenKind::IsNotEqualTo => BinaryOp::IsNotEqual,
                TokenKind::LAngle => BinaryOp::LessThan,
                TokenKind::RAngle => BinaryOp::GreaterThan,
                TokenKind::GreaterEqual => BinaryOp::GreaterEqual,
                TokenKind::LessEqual => BinaryOp::LessEqual,

                TokenKind::AndAnd => BinaryOp::And,
                TokenKind::OrOr => BinaryOp::Or,

                TokenKind::Ampersand => BinaryOp::BitAnd,
                TokenKind::Pipe => BinaryOp::BitOr,
                TokenKind::Caret => BinaryOp::BitXor,
                TokenKind::ShiftLeft => BinaryOp::BitShiftL,
                TokenKind::ShiftRight => BinaryOp::BitShiftR,
                _ => break,
            };

            let (l_bp, r_bp) = op.get_binding_power();

            if l_bp < min_bp {
                break;
            }

            self.advance();
            let rhs = self.parse_expr_recursive(r_bp, allow_struct)?;

            lhs = Expression::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_expr_main(&mut self, allow_struct: bool) -> Result<Expression, ParseError> {
        match self.peek().kind {
            TokenKind::IntLiteral => {
                let token = self.advance();
                let text = self.get_text(&token);

                let n = text.trim().parse::<i64>().map_err(|_| ParseError {
                    got: token.clone(),
                    expected: "Expected an int literal",
                })?;

                Ok(Expression::Literal(Literal::Int(n)))
            }
            TokenKind::FloatLiteral => {
                let token = self.advance();
                let text = self.get_text(&token);

                let f = text.trim().parse::<f64>().map_err(|_| ParseError {
                    got: token.clone(),
                    expected: "Expected an float literal",
                })?;

                Ok(Expression::Literal(Literal::Float(f)))
            }
            TokenKind::Identifier => {
                let token = self.advance();
                let name = self.get_text(&token).to_string();

                if self.eat(TokenKind::LParentheses) {
                    let mut args = vec![];

                    while self.peek().kind != TokenKind::RParentheses {
                        args.push(self.parse_expression()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }

                    self.expect(TokenKind::RParentheses, ")")?;

                    Ok(Expression::Call {
                        function: name,
                        args,
                    })
                } else if allow_struct && self.eat(TokenKind::LCurlyBracket) {
                    let mut fields = vec![];

                    while self.peek().kind != TokenKind::RCurlyBracket && self.peek().kind != TokenKind::EndOfFile {
                        let field_token = self.expect(TokenKind::Identifier, "Expected field name")?;
                        let field_name = self.get_text(&field_token).to_string();

                        self.expect(TokenKind::Colon, ":")?;

                        let value = self.parse_expression()?;

                        fields.push((field_name, value));

                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }

                    self.expect(TokenKind::RCurlyBracket, "}")?;

                    Ok(Expression::StructLiteral {
                        name,
                        fields,
                    })
                } else if self.eat(TokenKind::ColonColon) {
                    let variant_token = self.expect(TokenKind::Identifier, "Expected enum variant name")?;
                    let variant = self.get_text(&variant_token).to_string();
                    let payload = if self.eat(TokenKind::LParentheses) {
                        let expr = self.parse_expression()?;
                        self.expect(TokenKind::RParentheses, "Expected )")?;
                        Some(Box::new(expr))
                    } else {
                        None
                    };
                    Ok(Expression::EnumLiteral {
                        enum_name: name,
                        variant,
                        payload,
                    })
                } else {
                    Ok(Expression::Variable(name))
                }
            }

            TokenKind::LSquareParentheses => {
                self.advance();
                let mut elements = vec![];

                while self.peek().kind != TokenKind::RSquareParentheses && self.peek().kind != TokenKind::EndOfFile {
                    let expr = self.parse_expression()?;

                    if self.eat(TokenKind::SemiColon) {
                        let count = self.parse_expression()?;
                        self.expect(TokenKind::RSquareParentheses, "]")?;
                        return Ok(Expression::ArrayDeclaration(ArrayDeclaration::Repeat {
                            value: Box::new(expr),
                            len: Box::new(count),
                        }));
                    }

                    elements.push(expr);

                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }

                self.expect(TokenKind::RSquareParentheses, "]")?;
                Ok(Expression::ArrayDeclaration(ArrayDeclaration::Normal {
                    values: elements,
                }))
            }

            TokenKind::True => {
                self.advance();
                Ok(Expression::Literal(Literal::Bool(true)))
            }

            TokenKind::False => {
                self.advance();
                Ok(Expression::Literal(Literal::Bool(false)))
            }

            _ => Err(ParseError {
                got: self.peek().clone(),
                expected: "an expression",
            }),
        }
    }
}
