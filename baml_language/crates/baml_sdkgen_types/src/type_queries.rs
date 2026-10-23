use std::collections::BTreeSet;

use crate::{Name, Symbol, SymbolPool, Ty};

/// Whether `null` inhabits `ty`, following type aliases (recursive ones such
/// as `json` included). The runtime reads a class field of such a type that a
/// host leaves out as `null`, so generators may let callers omit it.
pub fn is_nullable(ty: &Ty, pool: &SymbolPool) -> bool {
    fn go<'a>(ty: &'a Ty, pool: &'a SymbolPool, visiting: &mut BTreeSet<&'a Name>) -> bool {
        match ty {
            Ty::Null => true,
            Ty::Union(items) => items.iter().any(|item| go(item, pool, visiting)),
            Ty::TypeAlias(name) => match pool.get(name) {
                Some(Symbol::TypeAlias(alias)) if visiting.insert(name) => {
                    go(&alias.resolves_to, pool, visiting)
                }
                _ => false,
            },
            _ => false,
        }
    }
    go(ty, pool, &mut BTreeSet::new())
}

fn collect_interface_tys(ty: &Ty, out: &mut BTreeSet<Name>) {
    crate::visit_type(ty, &mut |ty| {
        if let Ty::Interface(name, ..) = ty {
            out.insert(name.clone());
        }
    });
}

/// Interface tokens referenced by public symbol signatures, fields and watchers.
pub fn public_interface_tokens(pool: &SymbolPool) -> BTreeSet<Name> {
    fn function(value: &crate::Function, out: &mut BTreeSet<Name>) {
        for arg in &value.arguments {
            collect_interface_tys(&arg.ty, out);
        }
        collect_interface_tys(&value.return_type, out);
        if let Some(throws) = &value.throws {
            collect_interface_tys(throws, out);
        }
        for (_, watcher) in &value.watchers {
            collect_interface_tys(watcher, out);
        }
    }
    let mut out = BTreeSet::new();
    for symbol in pool.values() {
        match symbol {
            Symbol::Function(value) => function(value, &mut out),
            Symbol::Class(value) => {
                for property in &value.properties {
                    collect_interface_tys(&property.ty, &mut out);
                }
                for method in value.static_methods.iter().chain(&value.instance_methods) {
                    function(method, &mut out);
                }
            }
            Symbol::TypeAlias(value) => collect_interface_tys(&value.resolves_to, &mut out),
            Symbol::Enum(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;

    use super::*;
    use crate::{Origin, TypeAlias};

    fn name(n: &str) -> Name {
        Name::new(BaseName::new("user"), Vec::new(), BaseName::new(n))
    }

    fn insert_alias(pool: &mut SymbolPool, name: &Name, resolves_to: Ty) {
        pool.insert(
            name.clone(),
            Symbol::TypeAlias(TypeAlias {
                name: name.clone(),
                resolves_to,
                recursive: false,
                origin: Origin {
                    source_file_path: "main.baml".to_string(),
                    span_start: 0,
                },
            }),
        );
    }

    #[test]
    fn is_nullable_follows_aliases() {
        let nullable_string = Ty::Union(Box::new([Ty::String, Ty::Null]));
        let (text, chain, cycle) = (name("Text"), name("Chain"), name("Cycle"));
        let mut pool = SymbolPool::new();
        insert_alias(&mut pool, &text, nullable_string.clone());
        // `Chain = Chain[] | null`, like `json`.
        let chain_items = Ty::List(Box::new(Ty::TypeAlias(chain.clone())));
        insert_alias(
            &mut pool,
            &chain,
            Ty::Union(Box::new([chain_items, Ty::Null])),
        );
        // A cycle with no `null` anywhere must still terminate.
        let cycle_body = Ty::Union(Box::new([Ty::TypeAlias(cycle.clone()), Ty::Int]));
        insert_alias(&mut pool, &cycle, cycle_body);

        for (ty, expected) in [
            (Ty::Null, true),
            (nullable_string.clone(), true),
            (Ty::TypeAlias(text), true),
            (Ty::TypeAlias(chain), true),
            (Ty::TypeAlias(cycle), false),
            (Ty::TypeAlias(name("Undeclared")), false),
            (Ty::String, false),
            (Ty::List(Box::new(nullable_string)), false),
        ] {
            assert_eq!(is_nullable(&ty, &pool), expected, "{ty:?}");
        }
    }
}
