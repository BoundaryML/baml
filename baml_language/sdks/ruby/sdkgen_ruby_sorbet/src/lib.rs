//! Ruby/Sorbet declarations and registrations for the whole BAML symbol pool.
mod descriptors;
mod emit;
mod names;
mod package;
mod types;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
};

use baml_sdkgen_types::{Name, NamingConvention, Symbol, SymbolPool};
use emit::Emitter;
use names::NameTable;
use types::ruby_type;

pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
    metadata: Option<&str>,
) -> HashMap<PathBuf, String> {
    to_source_code_with_bytecode_and_skipped(pool, bytecode, naming, metadata).0
}

/// Also returns sorted BAML names omitted from the generated surface.
pub fn to_source_code_with_bytecode_and_skipped(
    pool: &SymbolPool,
    bytecode: &[u8],
    naming: NamingConvention,
    metadata: Option<&str>,
) -> (HashMap<PathBuf, String>, Vec<String>) {
    let names = NameTable::build(pool, naming);
    let symbols: BTreeMap<_, _> = pool.iter().collect();
    let mut supported: BTreeSet<Name> = symbols
        .iter()
        .filter_map(|(n, s)| match s {
            Symbol::Class(class) if class.generic_params.is_empty() => Some((*n).clone()),
            Symbol::Enum(_) => Some((*n).clone()),
            _ => None,
        })
        .collect();
    // Remove declarations referring to omitted types, including transitive edges.
    loop {
        let rejected: Vec<_> = supported
            .iter()
            .filter(|n| match pool.get(*n) {
                Some(Symbol::Class(c)) => c
                    .properties
                    .iter()
                    .any(|p| ruby_type(&p.ty, &supported, &names).is_none()),
                _ => false,
            })
            .cloned()
            .collect();
        if rejected.is_empty() {
            break;
        }
        for n in rejected {
            supported.remove(&n);
        }
    }
    let mut emitter = Emitter {
        supported: &supported,
        naming,
        names,
        registry: String::new(),
        shells: String::new(),
        skipped: BTreeSet::new(),
    };
    let mut namespaces: BTreeMap<String, String> = BTreeMap::new();
    for (name, symbol) in symbols {
        let module = emitter.names.module_name(name);
        let body = namespaces.entry(module.clone()).or_default();
        match symbol {
            Symbol::Class(_) | Symbol::Enum(_) if supported.contains(name) => match symbol {
                Symbol::Class(class) => emitter.class(name, class, body),
                Symbol::Enum(enum_) => emitter.enum_(name, enum_, body),
                _ => unreachable!(),
            },
            Symbol::Function(function) => {
                emitter.function(&module, &name.to_string(), function, None, body);
            }
            _ => {
                emitter.skipped.insert(name.to_string());
                if let Symbol::Class(class) = symbol {
                    emitter.skipped.extend(
                        class
                            .static_methods
                            .iter()
                            .chain(&class.instance_methods)
                            .map(|m| format!("{name}.{}", m.name)),
                    );
                }
            }
        }
    }
    let files = package::emit(namespaces, &emitter, bytecode, metadata);
    (files, emitter.skipped.into_iter().collect())
}

#[cfg(test)]
mod tests;
