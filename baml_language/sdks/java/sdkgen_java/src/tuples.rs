use std::collections::{BTreeMap, BTreeSet};

use baml_sdkgen_types::{CallableParam, Function, Symbol, SymbolPool, Ty};

use crate::translate_ty::{AliasTable, collect_type_vars, union_arm_token};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TupleDecl {
    pub(crate) ident: String,
    pub(crate) items: Vec<Ty>,
}

#[derive(Debug, Default)]
pub(crate) struct TupleTable {
    by_identity: BTreeMap<Ty, TupleDecl>,
}

impl TupleTable {
    pub(crate) fn collect(
        pool: &SymbolPool,
        aliases: &AliasTable,
        reserved: &BTreeSet<String>,
    ) -> Self {
        let mut identities: BTreeSet<Ty> = BTreeSet::new();
        let mut visit = |ty: &Ty| {
            baml_sdkgen_types::visit_type(ty, &mut |node| {
                if let Ty::Tuple(items) = node {
                    let identity = tuple_identity(items, aliases);
                    let mut type_vars = Vec::new();
                    collect_type_vars(&identity, &mut type_vars);
                    if type_vars.is_empty() {
                        identities.insert(identity);
                    }
                }
            });
        };
        for symbol in pool.values() {
            match symbol {
                Symbol::Class(class) => {
                    for property in &class.properties {
                        visit(&property.ty);
                    }
                    for method in class.static_methods.iter().chain(&class.instance_methods) {
                        visit_function(method, &mut visit);
                    }
                }
                Symbol::Function(function) => visit_function(function, &mut visit),
                Symbol::TypeAlias(alias) => visit(&alias.resolves_to),
                Symbol::Enum(_) => {}
            }
        }

        let mut taken: BTreeSet<String> = reserved.clone();
        let mut by_identity = BTreeMap::new();
        for identity in identities {
            let Ty::Tuple(items) = &identity else {
                unreachable!("a tuple identity is a tuple type");
            };
            let base = tuple_base_name(items);
            let mut ident = base.clone();
            let mut n = 2;
            while taken.contains(&ident) {
                ident = format!("{base}{n}");
                n += 1;
            }
            taken.insert(ident.clone());
            let decl = TupleDecl {
                ident,
                items: items.to_vec(),
            };
            by_identity.insert(identity, decl);
        }
        Self { by_identity }
    }

    pub(crate) fn decl(&self, items: &[Ty], aliases: &AliasTable) -> Option<&TupleDecl> {
        self.by_identity.get(&tuple_identity(items, aliases))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&Ty, &TupleDecl)> {
        self.by_identity.iter()
    }
}

fn visit_function(function: &Function, visit: &mut dyn FnMut(&Ty)) {
    for argument in &function.arguments {
        visit(&argument.ty);
    }
    visit(&function.return_type);
    if let Some(throws) = &function.throws {
        visit(throws);
    }
}

pub(crate) fn tuple_base_name(items: &[Ty]) -> String {
    let mut name = format!("Tuple{}_", items.len());
    for item in items {
        name.push_str(&tuple_slot_token(item));
    }
    name
}

fn tuple_slot_token(ty: &Ty) -> String {
    let Ty::Union(members) = ty else {
        return union_arm_token(ty);
    };
    let tokens: Vec<String> = members
        .iter()
        .filter(|member| !matches!(member, Ty::Null))
        .map(union_arm_token)
        .collect();
    let nullable = tokens.len() != members.len();
    match (nullable, tokens.is_empty()) {
        (_, true) => "Null".to_string(),
        (true, false) => format!("Optional{}", tokens.join("Or")),
        (false, false) => tokens.join("Or"),
    }
}

pub(crate) fn tuple_identity(items: &[Ty], aliases: &AliasTable) -> Ty {
    Ty::Tuple(
        items
            .iter()
            .map(|item| erase_aliases(item, aliases))
            .collect(),
    )
}

fn erase_aliases(ty: &Ty, aliases: &AliasTable) -> Ty {
    let erase = |t: &Ty| erase_aliases(t, aliases);
    let erase_all = |ts: &[Ty]| -> Box<[Ty]> { ts.iter().map(erase).collect() };
    match ty {
        Ty::TypeAlias(name) => match aliases.get(name) {
            Some((resolved, false)) => erase(resolved),
            _ => ty.clone(),
        },
        Ty::Literal(literal, _) => {
            Ty::Literal(literal.clone(), baml_sdkgen_types::Freshness::Regular)
        }
        Ty::Class(name, args) => Ty::Class(name.clone(), erase_all(args)),
        Ty::Interface(name, args, bindings) => Ty::Interface(
            name.clone(),
            erase_all(args),
            bindings
                .iter()
                .map(|(binding, t)| (binding.clone(), erase(t)))
                .collect(),
        ),
        Ty::List(inner) => Ty::List(Box::new(erase(inner))),
        Ty::Map { key, value } => Ty::Map {
            key: Box::new(erase(key)),
            value: Box::new(erase(value)),
        },
        Ty::Union(members) => Ty::Union(erase_all(members)),
        Ty::Tuple(items) => Ty::Tuple(erase_all(items)),
        Ty::Function {
            params,
            ret,
            throws,
        } => Ty::Function {
            params: params
                .iter()
                .map(|param| CallableParam {
                    name: param.name.clone(),
                    ty: erase(&param.ty),
                    mode: param.mode,
                })
                .collect(),
            ret: Box::new(erase(ret)),
            throws: Box::new(erase(throws)),
        },
        Ty::Future(value, error) => Ty::Future(Box::new(erase(value)), Box::new(erase(error))),
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::Enum(_)
        | Ty::EnumVariant(..)
        | Ty::TypeVar(_)
        | Ty::RustType
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Unknown
        | Ty::Never => ty.clone(),
    }
}
