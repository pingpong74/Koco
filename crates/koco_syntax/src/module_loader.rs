use crate::ast::*;
use crate::lexer::lex;
use crate::parser::Parser;
use crate::token::TokenKind;
use koco_span::{DiagnosticsCtx, FileId, Interner, SourceMap};
use std::path::Path;

pub(crate) struct LoadCtx<'a> {
    pub source_map: &'a mut SourceMap,
    pub interner: &'a mut Interner,
    pub diags: &'a DiagnosticsCtx,
    pub arenas: &'a mut Arenas,
}

pub(crate) fn parse_file(ctx: &mut LoadCtx<'_>, file: FileId, dir: &Path) -> Vec<Idx<Item>> {
    let src = ctx.source_map.source(file).to_string();
    let tokens = lex(&src, file, ctx.diags);
    let items = {
        let mut parser = Parser::new(tokens, &src, &mut *ctx.interner, ctx.diags, &mut *ctx.arenas);
        parser.parse_items(TokenKind::Eof)
    };
    expand_outline_mods(ctx, &items, dir);
    items
}

fn expand_outline_mods(ctx: &mut LoadCtx<'_>, items: &[Idx<Item>], dir: &Path) {
    for &id in items {
        let (name, span, outline, children) = match &ctx.arenas.items[id].kind {
            ItemKind::Mod(m) => {
                let children = match &m.kind {
                    ModKind::Inline(v) => Some(v.clone()),
                    ModKind::Outline => None,
                };
                (m.name, ctx.arenas.items[id].span, children.is_none(), children)
            }
            _ => continue,
        };
        let name_str = ctx.interner.resolve(name).to_string();
        let child_dir = dir.join(&name_str);

        if outline {
            let path = dir.join(format!("{name_str}.rs"));
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let child_file = ctx.source_map.add_file(path.display().to_string(), text);
                    let child_items = parse_file(ctx, child_file, &child_dir);
                    if let ItemKind::Mod(m) = &mut ctx.arenas.items[id].kind {
                        m.kind = ModKind::Inline(child_items);
                    }
                }
                Err(err) => {
                    ctx.diags.error(span, format!("cannot load module `{name_str}`: {} ({err})", path.display()));
                }
            }
        } else if let Some(children) = children {
            expand_outline_mods(ctx, &children, &child_dir);
        }
    }
}
