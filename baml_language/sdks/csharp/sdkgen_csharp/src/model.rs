//! C#-owned callable projection over Canary's generator-facing symbol pool.

use std::collections::{BTreeSet, HashMap, HashSet};

use baml_base::Name as BaseName;
use baml_codegen_types::{Class, Name, Symbol, SymbolPool};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableVariant {
    Execute,
    Spec,
    Stream,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableKey {
    Free(Name),
    StaticMethod { owner: Name, wire_name: BaseName },
    InstanceMethod { owner: Name, wire_name: BaseName },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallableReceiver {
    pub(crate) wire_name: BaseName,
    pub(crate) source_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallableIdentity {
    pub(crate) family_name: BaseName,
    pub(crate) wire_name: BaseName,
    pub(crate) variant: CallableVariant,
    pub(crate) receiver: Option<CallableReceiver>,
}

pub(crate) struct CodegenModel {
    pub(crate) symbols: SymbolPool,
    pub(crate) callables: HashMap<CallableKey, CallableIdentity>,
}

/// Runtime names from the compiled Canary program. Generator-facing class
/// methods retain their source identity (`CsvRows.next`), while an interface
/// implementation is emitted under its dispatch identity
/// (`CsvRows.root.iter.Iterator.next`). C# calls the engine by name, so it must
/// use the latter without changing the shared symbol-pool contract.
pub(crate) struct RuntimeCallableIdentities {
    function_names: BTreeSet<String>,
}

impl RuntimeCallableIdentities {
    pub(crate) fn from_program_bytes(bytes: &[u8]) -> Result<Self, String> {
        let program: bex_vm_types::Program =
            baml_artifact::decode(baml_artifact::ArtifactKind::Program, bytes)
                .map_err(|error| format!("failed to decode compiled BAML program: {error}"))?;
        Ok(Self {
            function_names: program.function_indices.into_keys().collect(),
        })
    }

    pub(crate) fn method_identity(
        &self,
        owner: &Name,
        wire_name: &BaseName,
    ) -> Result<String, String> {
        let source_identity = format!("{owner}.{wire_name}");
        if self.function_names.contains(&source_identity) {
            return Ok(source_identity);
        }

        let owner_prefix = format!("{owner}.");
        let method_suffix = format!(".{wire_name}");
        let candidates = self
            .function_names
            .iter()
            .filter(|name| name.starts_with(&owner_prefix) && name.ends_with(&method_suffix))
            .cloned()
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [identity] => Ok(identity.clone()),
            [] => Err(format!(
                "compiled BAML program has no runtime callable for `{source_identity}`"
            )),
            _ => Err(format!(
                "compiled BAML program has ambiguous runtime callables for `{source_identity}`: {}",
                candidates.join(", ")
            )),
        }
    }
}

impl CodegenModel {
    pub(crate) fn from_symbol_pool(pool: &SymbolPool) -> Self {
        let symbols = pool
            .iter()
            .map(|(name, symbol)| {
                let symbol = match symbol {
                    Symbol::Class(class) => Symbol::Class(reclassify_companion_methods(class)),
                    other => other.clone(),
                };
                (name.clone(), symbol)
            })
            .collect::<SymbolPool>();
        let mut callables = HashMap::new();
        for (name, symbol) in &symbols {
            match symbol {
                Symbol::Function(function) => {
                    callables.insert(
                        CallableKey::Free(name.clone()),
                        identity(&function.name, None),
                    );
                }
                Symbol::Class(class) => {
                    for method in &class.static_methods {
                        callables.insert(
                            CallableKey::StaticMethod {
                                owner: name.clone(),
                                wire_name: method.name.clone(),
                            },
                            identity(&method.name, None),
                        );
                    }
                    for method in &class.instance_methods {
                        callables.insert(
                            CallableKey::InstanceMethod {
                                owner: name.clone(),
                                wire_name: method.name.clone(),
                            },
                            identity(
                                &method.name,
                                Some(CallableReceiver {
                                    wire_name: BaseName::new("self"),
                                    source_index: 0,
                                }),
                            ),
                        );
                    }
                }
                Symbol::Enum(_) | Symbol::TypeAlias(_) => {}
            }
        }
        Self { symbols, callables }
    }

    pub(crate) fn callable(&self, key: &CallableKey) -> Option<&CallableIdentity> {
        self.callables.get(key)
    }
}

fn reclassify_companion_methods(class: &Class) -> Class {
    let instance_families = class
        .instance_methods
        .iter()
        .filter_map(|method| {
            let (family, variant) = callable_parts(&method.name);
            (variant == CallableVariant::Execute).then_some(family)
        })
        .collect::<HashSet<_>>();
    let static_families = class
        .static_methods
        .iter()
        .filter_map(|method| {
            let (family, variant) = callable_parts(&method.name);
            (variant == CallableVariant::Execute).then_some(family)
        })
        .collect::<HashSet<_>>();
    let mut static_methods = Vec::new();
    let mut instance_methods = Vec::new();
    for (was_static, method) in class
        .static_methods
        .iter()
        .map(|method| (true, method))
        .chain(class.instance_methods.iter().map(|method| (false, method)))
    {
        let (family, _) = callable_parts(&method.name);
        let is_instance = instance_families.contains(&family)
            || (!static_families.contains(&family) && !was_static);
        if is_instance {
            instance_methods.push(method.clone());
        } else {
            static_methods.push(method.clone());
        }
    }
    Class {
        name: class.name.clone(),
        generic_params: class.generic_params.clone(),
        docstring: class.docstring.clone(),
        properties: class.properties.clone(),
        static_methods,
        instance_methods,
        origin: class.origin.clone(),
    }
}

fn identity(name: &BaseName, receiver: Option<CallableReceiver>) -> CallableIdentity {
    let (family_name, variant) = callable_parts(name);
    CallableIdentity {
        family_name,
        wire_name: name.clone(),
        variant,
        receiver,
    }
}

fn callable_parts(name: &BaseName) -> (BaseName, CallableVariant) {
    const SUFFIXES: [(&str, CallableVariant); 2] = [
        ("@stream", CallableVariant::Stream),
        ("@spec", CallableVariant::Spec),
    ];
    for (suffix, variant) in SUFFIXES {
        if let Some(family) = name.as_str().strip_suffix(suffix) {
            return (BaseName::new(family), variant);
        }
    }
    (name.clone(), CallableVariant::Execute)
}

#[cfg(test)]
mod tests {
    use baml_base::TyAttr;
    use baml_codegen_types::{Function, Origin, Ty};

    use super::*;

    fn method(name: &str) -> Function {
        Function {
            name: BaseName::new(name),
            generic_params: Vec::new(),
            docstring: None,
            arguments: Vec::new(),
            return_type: Ty::String {
                attr: TyAttr::default(),
            },
            throws: None,
            watchers: Vec::new(),
            origin: Origin {
                source_file_path: "fixture.baml".to_string(),
                span_start: 0,
            },
        }
    }

    fn class(static_methods: &[&str], instance_methods: &[&str]) -> Class {
        Class {
            name: Name::new(BaseName::new("test"), Vec::new(), BaseName::new("Fixture")),
            generic_params: Vec::new(),
            docstring: None,
            properties: Vec::new(),
            static_methods: static_methods.iter().map(|name| method(name)).collect(),
            instance_methods: instance_methods.iter().map(|name| method(name)).collect(),
            origin: Origin {
                source_file_path: "fixture.baml".to_string(),
                span_start: 0,
            },
        }
    }

    fn method_names(methods: &[Function]) -> Vec<&str> {
        methods.iter().map(|method| method.name.as_str()).collect()
    }

    #[test]
    fn companion_suffixes_are_interpreted_inside_the_csharp_generator() {
        assert_eq!(
            callable_parts(&BaseName::new("Extract@spec")),
            (BaseName::new("Extract"), CallableVariant::Spec)
        );
        assert_eq!(
            callable_parts(&BaseName::new("Extract@stream")),
            (BaseName::new("Extract"), CallableVariant::Stream)
        );
        assert_eq!(
            callable_parts(&BaseName::new("Extract")),
            (BaseName::new("Extract"), CallableVariant::Execute)
        );
    }

    #[test]
    fn static_execute_reclassifies_instance_only_companions_as_static() {
        let class =
            reclassify_companion_methods(&class(&["Extract"], &["Extract@spec", "Extract@stream"]));

        assert_eq!(
            method_names(&class.static_methods),
            ["Extract", "Extract@spec", "Extract@stream"]
        );
        assert!(class.instance_methods.is_empty());
    }

    #[test]
    fn instance_execute_reclassifies_static_only_companions_as_instance() {
        let class =
            reclassify_companion_methods(&class(&["Extract@spec", "Extract@stream"], &["Extract"]));

        assert!(class.static_methods.is_empty());
        assert_eq!(
            method_names(&class.instance_methods),
            ["Extract@spec", "Extract@stream", "Extract"]
        );
    }

    #[test]
    fn orphan_companions_preserve_their_original_method_kind() {
        let class =
            reclassify_companion_methods(&class(&["StaticOnly@spec"], &["InstanceOnly@stream"]));

        assert_eq!(method_names(&class.static_methods), ["StaticOnly@spec"]);
        assert_eq!(
            method_names(&class.instance_methods),
            ["InstanceOnly@stream"]
        );
    }

    #[test]
    fn interface_method_uses_the_compiled_runtime_identity() {
        let identities = RuntimeCallableIdentities {
            function_names: BTreeSet::from([
                "baml.csv.CsvRows.root.iter.Iterator.next".to_string(),
                "baml.csv.CsvRows.root.iter.Iterable.iter".to_string(),
            ]),
        };
        let owner = Name::new(
            BaseName::new("baml"),
            vec![BaseName::new("csv")],
            BaseName::new("CsvRows"),
        );
        assert_eq!(
            identities.method_identity(&owner, &BaseName::new("next")),
            Ok("baml.csv.CsvRows.root.iter.Iterator.next".to_string())
        );
    }
}
