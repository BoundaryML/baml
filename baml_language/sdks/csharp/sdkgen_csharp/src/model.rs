//! C#-owned callable projection over Canary's generator-facing symbol pool.

use std::collections::{BTreeSet, HashMap};

use baml_base::Name as BaseName;
use baml_codegen_types::{Name, Symbol, SymbolPool};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableVariant {
    Direct,
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
        let symbols = pool.clone();
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

fn identity(name: &BaseName, receiver: Option<CallableReceiver>) -> CallableIdentity {
    CallableIdentity {
        family_name: name.clone(),
        wire_name: name.clone(),
        variant: CallableVariant::Direct,
        receiver,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_names_are_not_interpreted_as_companions() {
        let name = BaseName::new("Extract$stream");
        let identity = identity(&name, None);
        assert_eq!(identity.family_name, name);
        assert_eq!(identity.wire_name, name);
        assert_eq!(identity.variant, CallableVariant::Direct);
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
