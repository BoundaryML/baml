use std::collections::BTreeSet;

use crate::{Name, Symbol, SymbolPool, Ty};

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
