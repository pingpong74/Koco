use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Symbol(u32);

impl Symbol {
    pub fn as_u32(self) -> u32 { self.0 }
}

// some symbols which are known before hand
pub const SELF_VALUE: Symbol = Symbol(0);
pub const SELF_TYPE: Symbol = Symbol(1);
pub const CRATE: Symbol = Symbol(2);
pub const SUPER: Symbol = Symbol(3);
pub const UNDERSCORE: Symbol = Symbol(4);

const KNOWN_STR: &[&str] = &[
    "self", "Self", "crate", "super", "_",
];

pub struct Interner {
    map: HashMap<Rc<str>, Symbol>,
    strings: Vec<Rc<str>>,
}

impl Default for Interner {
    fn default() -> Interner { Interner::new() }
}

impl Interner {
    pub fn new() -> Interner {
        let mut interner = Interner {
            map: HashMap::new(),
            strings: Vec::new(),
        };

        for (i, text) in KNOWN_STR.iter().enumerate() {
            let sym = interner.intern(text);
            assert_eq!(sym.0 as usize, i, "Already known symbols must be unique");
        }

        interner
    }

    pub fn intern(&mut self, s: &str) -> Symbol {
        if let Some(&sym) = self.map.get(s) {
            return sym;
        }

        let index = u32::try_from(self.strings.len()).expect("too many interned strings");
        let sym = Symbol(index);
        let text: Rc<str> = Rc::from(s);

        self.strings.push(Rc::clone(&text));
        self.map.insert(text, sym);

        sym
    }

    pub fn get(&self, s: &str) -> Option<Symbol> { self.map.get(s).copied() }

    pub fn resolve(&self, sym: Symbol) -> &str { &self.strings[sym.0 as usize] }

    pub fn len(&self) -> usize { self.strings.len() }

    pub fn is_empty(&self) -> bool { self.strings.is_empty() }
}
