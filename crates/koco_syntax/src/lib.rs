//! Lexer, syntax tree and parser for Koco (Rust-flavoured GPU language).
//!
//! Entry point: [`parse_crate`], which returns the whole multi-file program as one
//! shared set of [`ast::Arenas`] plus the id of the root module item.

pub mod ast;
mod lexer;
mod module_loader;
mod parser;
mod token;

pub use lexer::lex;
pub use token::{Token, TokenKind};

use ast::{Arenas, Idx, Item, ItemKind, ModDef, ModKind, Visibility};
use koco_span::{DiagnosticsCtx, FileId, Interner, SourceMap, Span};
use std::path::Path;

pub struct ParsedCrate {
    pub arenas: Arenas,
    pub root: Idx<Item>,
}

pub fn parse_crate(source_map: &mut SourceMap, interner: &mut Interner, diags: &DiagnosticsCtx, entry: FileId, root_dir: &Path) -> ParsedCrate {
    let mut arenas = Arenas::default();
    let file_len = source_map.source(entry).len();
    let items = {
        let mut ctx = module_loader::LoadCtx {
            source_map,
            interner: &mut *interner,
            diags,
            arenas: &mut arenas,
        };
        module_loader::parse_file(&mut ctx, entry, root_dir)
    };
    let name = interner.intern("crate");
    let root = arenas.items.alloc(Item {
        kind: ItemKind::Mod(ModDef {
            name,
            kind: ModKind::Inline(items),
        }),
        vis: Visibility::Public,
        attrs: Vec::new(),
        span: Span::new(entry, 0, file_len),
    });
    ParsedCrate {
        arenas,
        root,
    }
}
