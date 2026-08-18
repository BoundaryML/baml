//! Rust projection for closed, non-generic BAML interfaces.
//!
//! Rust needs a concrete `BamlValue` in every generated signature. The
//! compiler-facing symbol pool deliberately retains interfaces as interfaces,
//! so this Rust-only pass replaces interfaces whose complete concrete
//! implementor set is known with an anonymous union of those implementors.
//! Open/generic interfaces remain untouched and fail closed in the normal type
//! translator.

use std::collections::HashMap;

use baml_codegen_types::{CallableParam, Class, Function, Name, Symbol, SymbolPool, Ty};

pub(crate) fn lower(pool: &SymbolPool, implementors: &HashMap<Name, Vec<Ty>>) -> SymbolPool {
    pool.iter()
        .map(|(name, symbol)| (name.clone(), lower_symbol(symbol, implementors)))
        .collect()
}

fn lower_symbol(symbol: &Symbol, implementors: &HashMap<Name, Vec<Ty>>) -> Symbol {
    match symbol {
        Symbol::Function(function) => Symbol::Function(lower_function(function, implementors)),
        Symbol::Class(class) => Symbol::Class(lower_class(class, implementors)),
        Symbol::Enum(_) => symbol.clone(),
        Symbol::TypeAlias(alias) => {
            let mut alias = alias.clone();
            alias.resolves_to = lower_ty(&alias.resolves_to, implementors);
            Symbol::TypeAlias(alias)
        }
    }
}

fn lower_class(class: &Class, implementors: &HashMap<Name, Vec<Ty>>) -> Class {
    let mut class = class.clone();
    for property in &mut class.properties {
        property.ty = lower_ty(&property.ty, implementors);
    }
    class.static_methods = class
        .static_methods
        .iter()
        .map(|function| lower_function(function, implementors))
        .collect();
    class.instance_methods = class
        .instance_methods
        .iter()
        .map(|function| lower_function(function, implementors))
        .collect();
    class
}

fn lower_function(function: &Function, implementors: &HashMap<Name, Vec<Ty>>) -> Function {
    let mut function = function.clone();
    for argument in &mut function.arguments {
        argument.ty = lower_ty(&argument.ty, implementors);
    }
    function.return_type = lower_ty(&function.return_type, implementors);
    function.throws = function
        .throws
        .as_ref()
        .map(|ty| lower_ty(ty, implementors));
    for (_, watcher) in &mut function.watchers {
        *watcher = lower_ty(watcher, implementors);
    }
    function
}

fn lower_ty(ty: &Ty, implementors: &HashMap<Name, Vec<Ty>>) -> Ty {
    let lowered = match ty {
        Ty::Interface(name, generics, associated, attr)
            if generics.is_empty() && associated.is_empty() =>
        {
            match implementors.get(name) {
                Some(concrete) if !concrete.is_empty() => Ty::Union(
                    concrete
                        .iter()
                        .map(|ty| lower_ty(ty, implementors))
                        .collect(),
                    attr.clone(),
                ),
                _ => ty.clone(),
            }
        }
        Ty::Class(name, arguments, attr) => Ty::Class(
            name.clone(),
            arguments
                .iter()
                .map(|ty| lower_ty(ty, implementors))
                .collect(),
            attr.clone(),
        ),
        Ty::Interface(name, generics, associated, attr) => Ty::Interface(
            name.clone(),
            generics
                .iter()
                .map(|ty| lower_ty(ty, implementors))
                .collect(),
            associated
                .iter()
                .map(|(name, ty)| (name.clone(), lower_ty(ty, implementors)))
                .collect(),
            attr.clone(),
        ),
        Ty::List(inner, attr) => Ty::List(Box::new(lower_ty(inner, implementors)), attr.clone()),
        Ty::Map { key, value, attr } => Ty::Map {
            key: Box::new(lower_ty(key, implementors)),
            value: Box::new(lower_ty(value, implementors)),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => Ty::Union(
            members
                .iter()
                .map(|ty| lower_ty(ty, implementors))
                .collect(),
            attr.clone(),
        ),
        Ty::Function {
            params,
            ret,
            throws,
            attr,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| CallableParam {
                    name: param.name.clone(),
                    ty: lower_ty(&param.ty, implementors),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(lower_ty(ret, implementors)),
            throws: Box::new(lower_ty(throws, implementors)),
            attr: attr.clone(),
        },
        Ty::Future(value, error, attr) => Ty::Future(
            Box::new(lower_ty(value, implementors)),
            Box::new(lower_ty(error, implementors)),
            attr.clone(),
        ),
        _ => ty.clone(),
    };
    lowered.canonicalize()
}
