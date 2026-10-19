//! The names no user declaration may take.
//!
//! Keywords come from the lexer and built-in type names from the compiler
//! alias table, so neither list is written down twice. Callers exempt the
//! language packages: the stdlib declares `t.implements(u)`, the
//! `baml.spawn` namespace, and the `json` type alias itself.

/// Why a name is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservedName {
    /// A lexer keyword (`if`, `self`, `root`, ...).
    Keyword,
    /// A name the type lowering reads as a built-in type (`string`, `map`,
    /// ...), so a declaration of it could never be named.
    BuiltinType,
}

impl ReservedName {
    /// Why `name` cannot name a declaration in the type or item namespace.
    pub fn of(name: &str) -> Option<ReservedName> {
        if baml_compiler_lexer::is_keyword(name) {
            Some(ReservedName::Keyword)
        } else if crate::compiler_aliases::by_spelling(name).is_some() {
            Some(ReservedName::BuiltinType)
        } else {
            None
        }
    }
}
