use std::collections::{BTreeMap, BTreeSet};

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::routing::{LeafPath, route, route_class_ref};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InterfaceNames {
    pub reference: String,
    pub input: String,
    pub proof: String,
    pub associated: Vec<baml_base::Name>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LeafHelpers {
    pub interface_base: String,
    pub interface_type: String,
    pub typed_type: String,
    pub no_infer: String,
    pub declare_type: String,
    pub concrete_base: String,
    pub witnesses: String,
    pub runtime_classes: BTreeMap<Name, String>,
    pub reserved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TypeScriptInterfaces {
    pub declarations: BTreeMap<Name, InterfaceNames>,
    pub concrete_inputs: BTreeSet<Name>,
    pub input_aliases: BTreeMap<Name, String>,
    pub type_factories: BTreeMap<Name, String>,
    pub helpers: BTreeMap<LeafPath, LeafHelpers>,
}

pub(crate) fn allocate(mut candidate: String, used: &mut BTreeSet<String>) -> String {
    while !used.insert(candidate.clone()) {
        candidate.push('_');
    }
    candidate
}

impl TypeScriptInterfaces {
    pub(crate) fn new(pool: &SymbolPool) -> Self {
        let mut used: BTreeMap<LeafPath, BTreeSet<String>> = BTreeMap::new();
        for (name, symbol) in pool.iter() {
            let set = used.entry(route(name)).or_default();
            set.insert(name.name().to_string());
            if matches!(symbol, Symbol::Function(_)) {
                set.insert(format!("{}_async", name.name()));
            }
        }
        for name in pool.interfaces.declarations.keys() {
            used.entry(route_class_ref(name))
                .or_default()
                .insert(name.name().to_string());
        }
        // Namespace bindings also occupy identifiers, including cross-leaf imports.
        let top_names: BTreeSet<_> = used
            .keys()
            .filter_map(|leaf| leaf.segments.first().cloned())
            .collect();
        for set in used.values_mut() {
            set.extend(top_names.iter().cloned());
            set.extend(
                [
                    "_bamlRoot",
                    "BamlType",
                    "BamlTypeToken",
                    "BamlTypeValue",
                    "BamlCallContext",
                    "defineFunction",
                    "defineInstanceFunction",
                ]
                .map(str::to_owned),
            );
        }
        let mut declarations = BTreeMap::new();
        for (index, (name, declaration)) in pool.interfaces.declarations.iter().enumerate() {
            let set = used.entry(route_class_ref(name)).or_default();
            declarations.insert(
                name.clone(),
                InterfaceNames {
                    reference: allocate(format!("{}Ref", name.name()), set),
                    input: allocate(format!("{}Input", name.name()), set),
                    proof: format!("view_{index}"),
                    associated: declaration
                        .associated_types
                        .iter()
                        .map(|a| a.name.clone())
                        .collect(),
                },
            );
        }
        let input_aliases = pool
            .iter()
            .filter_map(|(name, symbol)| {
                if !matches!(symbol, Symbol::TypeAlias(_)) {
                    return None;
                }
                let set = used.entry(route(name)).or_default();
                Some((
                    name.clone(),
                    allocate(format!("_{}Input", name.name()), set),
                ))
            })
            .collect();
        // Carrier declarations denote structural builtins, not nominal classes.
        // Their canonical type factories belong to the builtin surface.
        let type_factories = pool
            .iter()
            .filter_map(|(name, symbol)| {
                if !matches!(symbol, Symbol::Class(_) | Symbol::Enum(_))
                    || baml_type::compiler_aliases::by_definition(name).is_some()
                {
                    return None;
                }
                let set = used.entry(route_class_ref(name)).or_default();
                Some((name.clone(), allocate(format!("{}Type", name.name()), set)))
            })
            .collect();
        let concrete_inputs: BTreeSet<_> = pool
            .interfaces
            .concrete_classes
            .iter()
            .filter(|(_, d)| !d.implemented_interfaces.is_empty())
            .map(|(name, _)| name.clone())
            .collect();
        let helpers = used
            .into_iter()
            .map(|(leaf, mut set)| {
                let interface_base = allocate("_BamlInterfaceRef".to_owned(), &mut set);
                let interface_type = allocate("_BamlInterfaceType".to_owned(), &mut set);
                let typed_type = allocate("_BamlType".to_owned(), &mut set);
                let no_infer = allocate("_BamlNoInfer".to_owned(), &mut set);
                let declare_type = allocate("_declareType".to_owned(), &mut set);
                let concrete_base = allocate("_BamlConcreteRef".to_owned(), &mut set);
                let witnesses = allocate("_interfaceTypes".to_owned(), &mut set);
                let runtime_classes = concrete_inputs
                    .iter()
                    .filter(|name| route_class_ref(name) == leaf)
                    .map(|name| {
                        (
                            name.clone(),
                            allocate(format!("_Runtime{}", name.name()), &mut set),
                        )
                    })
                    .collect();
                (
                    leaf,
                    LeafHelpers {
                        interface_base,
                        interface_type,
                        typed_type,
                        no_infer,
                        declare_type,
                        concrete_base,
                        witnesses,
                        runtime_classes,
                        reserved: set,
                    },
                )
            })
            .collect();
        Self {
            declarations,
            concrete_inputs,
            input_aliases,
            type_factories,
            helpers,
        }
    }

    pub(crate) fn name(&self, name: &Name, input: bool) -> Name {
        let names = &self.declarations[name];
        Name::new(
            name.package().clone(),
            name.namespace().to_vec(),
            baml_base::Name::new(if input {
                &names.input
            } else {
                &names.reference
            }),
        )
    }
}
