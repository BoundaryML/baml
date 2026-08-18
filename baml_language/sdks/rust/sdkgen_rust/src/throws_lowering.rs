//! Rust projection for error-contract arms whose nominal type is not emitted.
//!
//! The bridge already preserves an error-arm value that does not decode as the
//! generated throws type as `Error::Runtime`. Remove only references to nominal
//! types that the pool analysis proved cannot be emitted; all structurally
//! unsupported types remain in place and make the callable fail closed.

use baml_codegen_types::{Class, Function, Symbol, SymbolPool, Ty};

use crate::analyze::Analysis;

pub(crate) fn lower(pool: &SymbolPool, analysis: &Analysis) -> SymbolPool {
    pool.iter()
        .map(|(name, symbol)| (name.clone(), lower_symbol(symbol, analysis)))
        .collect()
}

fn lower_symbol(symbol: &Symbol, analysis: &Analysis) -> Symbol {
    match symbol {
        Symbol::Function(function) => Symbol::Function(lower_function(function, analysis)),
        Symbol::Class(class) => Symbol::Class(lower_class(class, analysis)),
        Symbol::Enum(_) | Symbol::TypeAlias(_) => symbol.clone(),
    }
}

fn lower_class(class: &Class, analysis: &Analysis) -> Class {
    let mut class = class.clone();
    class.static_methods = class
        .static_methods
        .iter()
        .map(|function| lower_function(function, analysis))
        .collect();
    class.instance_methods = class
        .instance_methods
        .iter()
        .map(|function| lower_function(function, analysis))
        .collect();
    class
}

fn lower_function(function: &Function, analysis: &Analysis) -> Function {
    let mut function = function.clone();
    function.throws = function
        .throws
        .as_ref()
        .and_then(|throws| lower_contract(throws, analysis));
    function
}

fn lower_contract(ty: &Ty, analysis: &Analysis) -> Option<Ty> {
    match ty {
        Ty::Union(items, attr) => {
            let retained: Vec<_> = items
                .iter()
                .filter(|item| nominal_is_emitted(item, analysis))
                .cloned()
                .collect();
            match retained.as_slice() {
                [] => None,
                [only] => Some(only.clone()),
                _ => Some(Ty::Union(retained, attr.clone()).canonicalize()),
            }
        }
        nominal if nominal_is_emitted(nominal, analysis) => Some(nominal.clone()),
        _ => None,
    }
}

fn nominal_is_emitted(ty: &Ty, analysis: &Analysis) -> bool {
    match ty {
        Ty::Class(name, _, _)
        | Ty::Enum(name, _)
        | Ty::EnumVariant(name, _, _)
        | Ty::TypeAlias(name, _) => analysis.is_emitted(name),
        _ => true,
    }
}
