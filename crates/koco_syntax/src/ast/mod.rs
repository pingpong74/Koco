mod expression;
mod item;
mod path;
mod pattern;
mod ty;
pub mod visit;

pub use expression::*;
pub use item::*;
pub use path::*;
pub use pattern::*;
pub use ty::*;

use la_arena::Arena;
pub use la_arena::Idx;

#[derive(Default)]
pub struct Arenas {
    pub exprs: Arena<Expression>,
    pub stmts: Arena<Statement>,
    pub items: Arena<Item>,
    pub type_refs: Arena<ParserType>,
    pub pats: Arena<Pattern>,
    pub paths: Arena<Path>,
    pub variants: Arena<Variant>,
    pub generic_params: Arena<GenericParam>,
}

impl Arenas {
    pub fn mod_items(&self, module: Idx<Item>) -> &[Idx<Item>] {
        match &self.items[module].kind {
            ItemKind::Mod(ModDef {
                kind: ModKind::Inline(items),
                ..
            }) => items,
            ItemKind::Mod(ModDef {
                kind: ModKind::Outline,
                ..
            }) => &[],
            _ => panic!("`mod_items` called on an item that is not a module"),
        }
    }
}
