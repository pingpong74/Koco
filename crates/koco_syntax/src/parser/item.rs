use super::Parser;
use crate::ast::*;
use crate::parser::PathStyle;
use crate::token::TokenKind;

impl<'a> Parser<'a> {
    pub(crate) fn parse_items(&mut self, until: TokenKind) -> Vec<Idx<Item>> {
        let mut items = Vec::new();
        while self.peek() != until && self.peek() != TokenKind::Eof {
            let before = self.pos;
            match self.parse_item() {
                Some(id) => items.push(id),
                None => {
                    if self.pos == before {
                        self.bump();
                    }
                    self.sync_item(until);
                }
            }
        }
        items
    }

    pub(crate) fn parse_item(&mut self) -> Option<Idx<Item>> { self.nested(|p| p.parse_item_inner()) }

    fn parse_item_inner(&mut self) -> Option<Idx<Item>> {
        let start = self.span();
        let attrs = self.parse_attrs()?;
        let vis = self.parse_visibility();

        let kind = match self.peek() {
            TokenKind::Fn => ItemKind::Fn(self.parse_fn()?),
            TokenKind::Struct => ItemKind::Struct(self.parse_struct()?),
            TokenKind::Enum => ItemKind::Enum(self.parse_enum()?),
            TokenKind::Trait => ItemKind::Trait(self.parse_trait()?),
            TokenKind::Impl => ItemKind::Impl(self.parse_impl()?),
            TokenKind::Mod => ItemKind::Mod(self.parse_mod()?),
            TokenKind::Use => ItemKind::Use(self.parse_use()?),
            TokenKind::Const => ItemKind::Const(self.parse_const()?),
            TokenKind::Static => ItemKind::Static(self.parse_static()?),
            TokenKind::Type => ItemKind::TypeAlias(self.parse_type_alias()?),
            _ => {
                self.error_expected("an item (`fn`, `struct`, `enum`, `trait`, `impl`, `mod`, `use`, `const`, `static` or `type`)");
                return None;
            }
        };

        let span = start.to(self.prev_span);
        Some(self.arenas.items.alloc(Item {
            kind,
            vis,
            attrs,
            span,
        }))
    }

    fn parse_visibility(&mut self) -> Visibility {
        if !self.eat(TokenKind::Pub) {
            Visibility::Private
        } else if self.at(TokenKind::LParen) && self.peek_at(2) == TokenKind::RParen {
            let visibility = match self.peek_at(1) {
                TokenKind::Crate => Visibility::Crate,
                TokenKind::Super => Visibility::Super,
                TokenKind::SelfValue => Visibility::Private,
                _ => return Visibility::Public, // error out here
            };

            self.bump();
            self.bump();
            self.bump();

            visibility
        } else {
            Visibility::Public
        }
    }

    fn parse_generics(&mut self) -> Option<Generics> {
        if !self.at(TokenKind::Lt) {
            return Some(Generics::default());
        }
        self.bump();
        let mut params = Vec::new();
        while !self.at_close_angle() && !self.at(TokenKind::Eof) {
            let start = self.span();
            let data = if self.eat(TokenKind::Const) {
                let (name, _) = self.parse_ident("a const parameter name")?;
                self.expect(TokenKind::Colon, "`:` after const parameter name")?;
                let ty = self.parse_ty()?;
                GenericParam {
                    name,
                    kind: GenericParamKind::Const { ty },
                    span: start.to(self.prev_span),
                }
            } else {
                let (name, _) = self.parse_ident("a generic parameter name")?;
                let bounds = if self.eat(TokenKind::Colon) {
                    self.parse_bounds()?
                } else {
                    Vec::new()
                };
                GenericParam {
                    name,
                    kind: GenericParamKind::Type { bounds },
                    span: start.to(self.prev_span),
                }
            };
            params.push(self.arenas.generic_params.alloc(data));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect_close_angle()?;
        Some(Generics { params })
    }

    /// `Trait + Trait<Args>`
    fn parse_bounds(&mut self) -> Option<Vec<Idx<Path>>> {
        let mut bounds = vec![self.parse_path(PathStyle::Ty)?];
        while self.eat(TokenKind::Plus) {
            bounds.push(self.parse_path(PathStyle::Ty)?);
        }
        Some(bounds)
    }

    fn parse_fn(&mut self) -> Option<FnDef> {
        self.expect(TokenKind::Fn, "`fn`")?;
        let (name, name_span) = self.parse_ident("a function name")?;
        let generics = self.parse_generics()?;
        self.expect(TokenKind::LParen, "`(`")?;

        let self_param = self.try_parse_self_param();
        if self_param.is_some() && !self.at(TokenKind::RParen) {
            self.expect(TokenKind::Comma, "`,` after `self`")?;
        }
        let mut params = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
            params.push(self.parse_param()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)`")?;

        let ret_ty = if self.eat(TokenKind::Arrow) {
            Some(self.parse_ty()?)
        } else {
            None
        };
        let body = if self.eat(TokenKind::Semi) {
            None
        } else {
            Some(self.parse_block_expr()?)
        };
        Some(FnDef {
            name,
            name_span,
            generics,
            self_param,
            params,
            ret_ty,
            body,
        })
    }

    /// `self`, `mut self`, `&self` or `&mut self` (consumed only if present).
    fn try_parse_self_param(&mut self) -> Option<SelfParam> {
        let start = self.span();
        let (mode, mutable, len) = match (self.peek(), self.peek_at(1), self.peek_at(2)) {
            (TokenKind::SelfValue, _, _) => (ParamMode::Value, false, 1),
            (TokenKind::Mut, TokenKind::SelfValue, _) => (ParamMode::Value, true, 2),
            (TokenKind::Amp, TokenKind::SelfValue, _) => (ParamMode::RefShared, false, 2),
            (TokenKind::Amp, TokenKind::Mut, TokenKind::SelfValue) => (ParamMode::RefMut, false, 3),
            _ => return None,
        };
        for _ in 0..len {
            self.bump();
        }
        let span = start.to(self.prev_span);
        let pat = self.mk_binding_pat(koco_span::SELF_VALUE, mutable, span);
        Some(SelfParam {
            pat,
            mode,
            span,
        })
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.span();
        let mutable = self.eat(TokenKind::Mut);
        let (name, name_span) = self.parse_ident("a parameter name")?;
        let pat = self.mk_binding_pat(name, mutable, start.to(name_span));
        self.expect(TokenKind::Colon, "`:` after parameter name")?;
        let mode = match self.peek() {
            TokenKind::Amp => {
                self.bump();
                if self.eat(TokenKind::Mut) {
                    ParamMode::RefMut
                } else {
                    ParamMode::RefShared
                }
            }
            TokenKind::AndAnd => {
                let span = self.span();
                self.error(span, "references to references are not supported");
                return None;
            }
            _ => ParamMode::Value,
        };
        let ty = self.parse_ty()?;
        Some(Param {
            pat,
            mode,
            ty,
            span: start.to(self.prev_span),
        })
    }

    fn parse_struct(&mut self) -> Option<StructDef> {
        self.expect(TokenKind::Struct, "`struct`")?;
        let (name, _) = self.parse_ident("a struct name")?;
        let generics = self.parse_generics()?;
        let fields = self.parse_fields(true)?;
        Some(StructDef {
            name,
            generics,
            fields,
        })
    }

    fn parse_enum(&mut self) -> Option<EnumDef> {
        self.expect(TokenKind::Enum, "`enum`")?;
        let (name, _) = self.parse_ident("an enum name")?;
        let generics = self.parse_generics()?;
        self.expect(TokenKind::LBrace, "`{`")?;
        let mut variants = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let start = self.span();
            let attrs = self.parse_attrs()?;
            let (name, _) = self.parse_ident("a variant name")?;
            let fields = self.parse_fields(false)?;
            variants.push(self.arenas.variants.alloc(Variant {
                name,
                fields,
                attrs,
                span: start.to(self.prev_span),
            }));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "`}`")?;
        Some(EnumDef {
            name,
            generics,
            variants,
        })
    }

    /// Body of a struct or enum variant. `require_terminator` requires `;` after the
    /// unit and tuple forms (struct declarations do; enum variants do not).
    fn parse_fields(&mut self, require_terminator: bool) -> Option<Fields> {
        match self.peek() {
            TokenKind::LBrace => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                    let start = self.span();
                    let attrs = self.parse_attrs()?;
                    let vis = self.parse_visibility();
                    let (name, _) = self.parse_ident("a field name")?;
                    self.expect(TokenKind::Colon, "`:` after field name")?;
                    let ty = self.parse_ty()?;
                    fields.push(FieldDef {
                        name: Some(name),
                        ty,
                        vis,
                        attrs,
                        span: start.to(self.prev_span),
                    });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBrace, "`}`")?;
                Some(Fields::Named(fields))
            }
            TokenKind::LParen => {
                self.bump();
                let mut fields = Vec::new();
                while !self.at(TokenKind::RParen) && !self.at(TokenKind::Eof) {
                    let start = self.span();
                    let attrs = self.parse_attrs()?;
                    let vis = self.parse_visibility();
                    let ty = self.parse_ty()?;
                    fields.push(FieldDef {
                        name: None,
                        ty,
                        vis,
                        attrs,
                        span: start.to(self.prev_span),
                    });
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "`)`")?;
                if require_terminator {
                    self.expect(TokenKind::Semi, "`;` after tuple struct")?;
                }
                Some(Fields::Tuple(fields))
            }
            _ => {
                if require_terminator {
                    self.expect(TokenKind::Semi, "`{`, `(` or `;` after struct name")?;
                }
                Some(Fields::Unit)
            }
        }
    }

    fn parse_trait(&mut self) -> Option<TraitDef> {
        self.expect(TokenKind::Trait, "`trait`")?;
        let (name, _) = self.parse_ident("a trait name")?;
        let generics = self.parse_generics()?;
        let supertraits = if self.eat(TokenKind::Colon) {
            self.parse_bounds()?
        } else {
            Vec::new()
        };
        let items = self.parse_assoc_items("traits")?;
        Some(TraitDef {
            name,
            generics,
            supertraits,
            items,
        })
    }

    fn parse_impl(&mut self) -> Option<ImplDef> {
        self.expect(TokenKind::Impl, "`impl`")?;
        let generics = self.parse_generics()?;
        // A *path*, not a full type: the part before `for` is a trait path, and an
        // inherent `impl Ty` target is a path too (struct, enum or scalar name).
        let first = self.parse_path(PathStyle::Ty)?;
        let (trait_ref, self_ty) = if self.eat(TokenKind::For) {
            (Some(first), self.parse_ty()?)
        } else {
            let span = self.arenas.paths[first].span;
            (None, self.mk_type_ref(ParserTypeKind::Path(first), span))
        };
        let items = self.parse_assoc_items("impl blocks")?;
        Some(ImplDef {
            generics,
            trait_ref,
            self_ty,
            items,
        })
    }

    /// `{ ... }` body of a trait or impl. Every item kind is kept in the tree unfiltered:
    /// rejecting `struct`/`mod`/... here is a semantic rule for `koco_hir`, and filtering
    /// would orphan the already-allocated item id.
    fn parse_assoc_items(&mut self, _owner: &str) -> Option<Vec<Idx<Item>>> {
        self.expect(TokenKind::LBrace, "`{`")?;
        let items = self.parse_items(TokenKind::RBrace);
        self.expect(TokenKind::RBrace, "`}`")?;
        Some(items)
    }

    fn parse_mod(&mut self) -> Option<ModDef> {
        self.expect(TokenKind::Mod, "`mod`")?;
        let (name, _) = self.parse_ident("a module name")?;
        if self.eat(TokenKind::Semi) {
            return Some(ModDef {
                name,
                kind: ModKind::Outline,
            });
        }
        self.expect(TokenKind::LBrace, "`{` or `;` after module name")?;
        let items = self.parse_items(TokenKind::RBrace);
        self.expect(TokenKind::RBrace, "`}`")?;
        Some(ModDef {
            name,
            kind: ModKind::Inline(items),
        })
    }

    fn parse_use(&mut self) -> Option<UseTree> {
        self.expect(TokenKind::Use, "`use`")?;
        let tree = self.parse_use_tree()?;
        self.expect(TokenKind::Semi, "`;` after `use`")?;
        Some(tree)
    }

    fn parse_use_tree(&mut self) -> Option<UseTree> { self.nested(|p| p.parse_use_tree_inner()) }

    fn parse_use_tree_inner(&mut self) -> Option<UseTree> {
        let start = self.span();
        let mut prefix = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Star => {
                    self.bump();
                    return Some(UseTree {
                        prefix,
                        kind: UseTreeKind::Glob,
                        span: start.to(self.prev_span),
                    });
                }
                TokenKind::LBrace => {
                    self.bump();
                    let mut trees = Vec::new();
                    while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                        trees.push(self.parse_use_tree()?);
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace, "`}`")?;
                    return Some(UseTree {
                        prefix,
                        kind: UseTreeKind::Nested(trees),
                        span: start.to(self.prev_span),
                    });
                }
                TokenKind::Identifier | TokenKind::SelfValue | TokenKind::SelfType | TokenKind::Crate | TokenKind::Super => {
                    let tok = self.bump();
                    let ident = self.intern(tok.span);
                    prefix.push(UseSegment {
                        ident,
                        span: tok.span,
                    });
                    if self.eat(TokenKind::ColonColon) {
                        continue;
                    }
                    let rename = if self.eat(TokenKind::As) {
                        Some(self.parse_ident("a name after `as`")?.0)
                    } else {
                        None
                    };
                    return Some(UseTree {
                        prefix,
                        kind: UseTreeKind::Simple { rename },
                        span: start.to(self.prev_span),
                    });
                }
                _ => {
                    self.error_expected("a path, `*` or `{` in `use`");
                    return None;
                }
            }
        }
    }

    fn parse_const(&mut self) -> Option<ConstDef> {
        self.expect(TokenKind::Const, "`const`")?;
        let (name, _) = self.parse_ident("a constant name")?;
        self.expect(TokenKind::Colon, "`:` after constant name")?;
        let ty = self.parse_ty()?;
        let value = if self.eat(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi, "`;`")?;
        Some(ConstDef {
            name,
            ty,
            value,
        })
    }

    fn parse_static(&mut self) -> Option<StaticDef> {
        self.expect(TokenKind::Static, "`static`")?;
        let mutable = self.eat(TokenKind::Mut);
        let (name, _) = self.parse_ident("a static name")?;
        self.expect(TokenKind::Colon, "`:` after static name")?;
        let ty = self.parse_ty()?;
        let init = if self.eat(TokenKind::Eq) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi, "`;`")?;
        Some(StaticDef {
            name,
            mutable,
            ty,
            init,
        })
    }

    fn parse_type_alias(&mut self) -> Option<TypeAliasDef> {
        self.expect(TokenKind::Type, "`type`")?;
        let (name, _) = self.parse_ident("a type alias name")?;
        let generics = self.parse_generics()?;
        self.expect(TokenKind::Eq, "`=` in type alias")?;
        let ty = self.parse_ty()?;
        self.expect(TokenKind::Semi, "`;`")?;
        Some(TypeAliasDef {
            name,
            generics,
            ty,
        })
    }
}
