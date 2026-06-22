use crate::*;

impl<'t, 's> Parser<'t, 's> {
    pub(crate) fn parse_shader_stage(&mut self) -> Result<ShaderStage, ParseError> {
        if self.eat(TokenKind::Hashtag) {
            self.expect(TokenKind::LSquareParentheses, "Expected [ after #")?;

            let token = self.expect(TokenKind::Identifier, "Expected shader stage")?;
            let attrib = self.get_text(&token);

            let stg = match attrib {
                "vertex" => ShaderStage::Vertex,
                "fragment" => ShaderStage::Fragment,
                "compute" => {
                    self.expect(
                        TokenKind::LParentheses,
                        "Specify work groups for compute shader like: compute(8, 8, 8)",
                    )?;

                    let mut grps = [0; 3];

                    for i in 0..3 {
                        let token = self.expect(
                            TokenKind::IntLiteral,
                            "Specify work groups for compute shader like: compute(8, 8, 8)",
                        )?;
                        let text = self.get_text(&token);

                        grps[i] = text.trim().parse::<u32>().map_err(|_| ParseError {
                            got: token.clone(),
                            expected: "Expected an int literal",
                        })?;

                        if i == 2 {
                            self.expect(TokenKind::RParentheses, "Expected )")?;
                        } else {
                            self.expect(TokenKind::Comma, "Expected ,")?;
                        }
                    }

                    ShaderStage::Compute {
                        workgroup_size: grps,
                    }
                }
                _ => {
                    return Err(ParseError {
                        got: token.clone(),
                        expected: "Expected function attribute",
                    });
                }
            };

            self.expect(TokenKind::RSquareParentheses, "Expected ]")?;

            Ok(stg)
        } else {
            Ok(ShaderStage::None)
        }
    }

    pub(crate) fn parse_parameters(&mut self) -> Result<Vec<Parameter>, ParseError> {
        self.expect(TokenKind::LParentheses, "Expected ( ")?;

        let mut parameters = vec![];

        while self.peek().kind != TokenKind::RParentheses && self.peek().kind != TokenKind::EndOfFile {
            let name_token = self.expect(TokenKind::Identifier, "Expected variable name")?;
            let name = self.get_text(&name_token).to_string();

            self.expect(TokenKind::Colon, "Expected : after parameter")?;

            let var_type = self.parse_type()?;

            parameters.push(Parameter {
                name,
                var_type,
            });

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RParentheses, "Expected )")?;

        Ok(parameters)
    }

    pub(crate) fn parse_function(&mut self) -> Result<Function, ParseError> {
        let shader_stage = self.parse_shader_stage();
        let shader_stage = self.recover(shader_stage, ShaderStage::None);

        self.expect(
            TokenKind::Function,
            "Function declaration expected after shader stages have been specified",
        )?;
        let name_token = self.expect(TokenKind::Identifier, "Expected function name")?;
        let name = self.get_text(&name_token).to_string();

        let param_res = self.parse_parameters();
        let parameters = self.recover(param_res, vec![]);

        let return_type = if self.eat(TokenKind::Arrow) {
            let ty_res = self.parse_type();
            self.recover(ty_res, ParserType::Void)
        } else {
            ParserType::Void
        };

        let scope = self.parse_scope()?;

        Ok(Function {
            name,
            params: parameters,
            return_type,
            body: scope,
            stage: shader_stage,
        })
    }
}
