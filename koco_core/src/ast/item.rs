use crate::*;

impl<'t, 's> Parser<'t, 's> {
    pub(crate) fn parse_descriptors(&mut self) -> Result<Descriptors, ParseError> {
        self.expect(TokenKind::Layout, "Expected #layout")?;
        self.expect(TokenKind::LParentheses, "Expected (")?;

        if self.eat(TokenKind::PushConstant) {
            self.expect(TokenKind::RParentheses, "Expected )")?;
            let name = self.expect(TokenKind::Identifier, "Expected variable name")?;
            self.expect(TokenKind::Colon, "Expected :")?;
            let pc_type = self.parse_type()?;
            self.expect(TokenKind::SemiColon, "Expected ;")?;

            return Ok(Descriptors {
                name: self.get_text(&name).to_string(),
                ty: pc_type,
                storage: StorageClass::PushConstant,
                binding: None,
            });
        }

        self.expect(TokenKind::Set, "Expected set")?;
        self.expect(TokenKind::Equal, "Expected =")?;
        let set_token = self.expect(TokenKind::IntLiteral, "Expected set number")?;
        let set = self
            .get_text(&set_token)
            .trim()
            .parse::<u32>()
            .map_err(|_| ParseError {
                got: set_token.clone(),
                expected: "Expected an int literal",
            })?;

        self.expect(TokenKind::Comma, "Expected ,")?;
        self.expect(TokenKind::Binding, "Expected binding")?;
        self.expect(TokenKind::Equal, "Expected =")?;
        let binding_token = self.expect(TokenKind::IntLiteral, "Expected set number")?;
        let binding = self
            .get_text(&binding_token)
            .trim()
            .parse::<u32>()
            .map_err(|_| ParseError {
                got: binding_token.clone(),
                expected: "Expected an int literal",
            })?;

        self.expect(TokenKind::RParentheses, "Expected )")?;

        let name = self.expect(TokenKind::Identifier, "Expected variable name")?;
        self.expect(TokenKind::Colon, "Expected :")?;
        let desc_type = self.parse_type()?;
        self.expect(TokenKind::SemiColon, "Expected ;")?;

        Ok(Descriptors {
            name: self.get_text(&name).to_string(),
            ty: desc_type,
            storage: StorageClass::Descriptor,
            binding: Some(Binding {
                group: set,
                binding,
            }),
        })
    }

    pub(crate) fn parse_struct(&mut self) -> Result<StructDef, ParseError> {
        self.expect(TokenKind::Struct, "Extepected struct ")?;

        let name_token = self.expect(TokenKind::Identifier, "Expected struct name")?;
        let name = self.get_text(&name_token).to_string();

        self.expect(TokenKind::LCurlyBracket, "Expected {")?;

        let mut fields = vec![];

        while self.peek().kind != TokenKind::RCurlyBracket && self.peek().kind != TokenKind::EndOfFile {
            let mut attrs = FieldAttrs::default();

            // Parse optional #[location(N)] or #[builtin(name)]
            if self.peek().kind == TokenKind::Hashtag {
                self.advance();
                self.expect(TokenKind::LSquareParentheses, "Expected [ after #")?;

                let attr_token = self.expect(TokenKind::Identifier, "Expected attribute name")?;
                let attr_name = self.get_text(&attr_token).to_string();

                self.expect(TokenKind::LParentheses, "Expected (")?;

                match attr_name.as_str() {
                    "location" => {
                        let val_token = self.expect(TokenKind::IntLiteral, "Expected location index")?;
                        let val: u32 = self.get_text(&val_token).trim().parse().map_err(|_| ParseError {
                            got: val_token.clone(),
                            expected: "Expected an integer literal",
                        })?;
                        attrs.location = Some(val);
                    }
                    "builtin" => {
                        let name_token = self.expect(TokenKind::Identifier, "Expected builtin name")?;
                        attrs.builtin = Some(self.get_text(&name_token).to_string());
                    }
                    _ => {
                        return Err(ParseError {
                            got: attr_token,
                            expected: "Expected 'location' or 'builtin'",
                        });
                    }
                }

                self.expect(TokenKind::RParentheses, "Expected )")?;
                self.expect(TokenKind::RSquareParentheses, "Expected ]")?;
            }

            let name_token = self.expect(TokenKind::Identifier, "Expected field name")?;
            let name = self.get_text(&name_token).to_string();

            self.expect(TokenKind::Colon, "Expected :")?;

            let ty = self.parse_type()?;

            fields.push((name, ty, attrs));

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RCurlyBracket, "Expected }")?;

        Ok(StructDef {
            name,
            fields,
        })
    }

    pub(crate) fn parse_enum(&mut self) -> Result<EnumDef, ParseError> {
        self.expect(TokenKind::Enum, "Expected enum")?;

        let name_token = self.expect(TokenKind::Identifier, "Expected enum name")?;
        let name = self.get_text(&name_token).to_string();

        self.expect(TokenKind::LCurlyBracket, "Expected {")?;

        let mut kinds = vec![];

        while self.peek().kind != TokenKind::RCurlyBracket && self.peek().kind != TokenKind::EndOfFile {
            let name_token = self.expect(TokenKind::Identifier, "Expected enum kind name")?;
            let name = self.get_text(&name_token).to_string();

            let ty = if self.eat(TokenKind::LParentheses) {
                let ty = self.parse_type()?;
                self.expect(TokenKind::RParentheses, "Extecped )")?;
                ty
            } else {
                ParserType::Void
            };

            kinds.push((name, ty));

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::RCurlyBracket, "Expected }")?;

        Ok(EnumDef {
            name,
            kinds,
        })
    }
}
