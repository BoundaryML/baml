use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use baml_rpc::{NamedType, TypeDefinition, TypeReference};
use baml_types::HasFieldType;

use super::{repr::Node, FieldType, IntermediateRepr, Walker};

mod class;
mod client;
mod r#enum;
mod field_type;
mod function;
mod retry_policy;
mod type_alias;

trait ShallowSignature {
    fn shallow_hash_prefix(&self) -> &'static str;
    fn shallow_interface_hash(&self) -> impl std::hash::Hash;
    fn unsorted_interface_dependencies(&self) -> HashSet<String>;
    fn interface_dependencies(&self) -> Vec<String> {
        let mut deps: Vec<String> = self.unsorted_interface_dependencies().into_iter().collect();
        deps.sort();
        deps
    }
    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash>;
    fn unsorted_implementation_dependencies(&self) -> HashSet<String> {
        self.unsorted_interface_dependencies()
    }
    fn implementation_dependencies(&self) -> Vec<String> {
        let mut deps: Vec<String> = self
            .unsorted_implementation_dependencies()
            .into_iter()
            .collect();
        deps.sort();
        deps
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureType {
    AST,
    Function,
    Class,
    Enum,
    TypeAlias,
    Client,
    RetryPolicy,
}

#[derive(Clone)]
pub struct Signature {
    pub r#type: SignatureType,
    display_name: Arc<String>,
    interface_hash: u64,
    implementation_hash: Option<u64>,
    dependencies: Arc<Vec<String>>,
}

#[derive(Clone)]
pub struct TypeNodeSignature {
    pub signature: Signature,
    pub field_type: Arc<baml_types::FieldType>,
}

#[derive(Clone)]
pub struct FunctionSignature {
    pub signature: Signature,
    pub inputs: Arc<Vec<(String, FieldType)>>,
    pub output: Arc<FieldType>,
}

#[derive(Clone)]
pub struct ClassSignatureDetails {
    pub fields: Arc<Vec<(String, Arc<FieldType>)>>,
}

#[derive(Clone)]
pub struct EnumSignatureDetails {
    pub values: Arc<Vec<String>>,
}

fn recursively_collect_dependencies<'a>(
    name: &str,
    shallow_hash: &'a HashMap<&str, ShallowHash>,
    find_dependencies: fn(&str, &'a HashMap<&str, ShallowHash>) -> Option<&'a Vec<String>>,
) -> Result<Vec<&'a String>> {
    // Recursively collect all dependencies
    let mut seen = HashSet::new();
    let mut queue = vec![name];
    while let Some(name) = queue.pop() {
        let dep_hash = find_dependencies(name, shallow_hash)
            .ok_or(anyhow::anyhow!("Dependency: {} not found", name))?;
        for dep in dep_hash {
            // For recursive dependencies, we want to insert self back into the queue
            // This is why seen starts empty, so we actually insert the dependency
            // back into the queue, when/if we see it again
            if seen.insert(dep) {
                queue.push(dep);
            }
        }
    }

    // Sort dependencies so hasher is deterministic
    let mut dependencies = seen.into_iter().collect::<Vec<_>>();
    dependencies.sort();
    Ok(dependencies)
}

impl Signature {
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn dependency_names(&self) -> &Vec<String> {
        &self.dependencies
    }

    pub fn interface_hash(&self) -> u64 {
        self.interface_hash
    }

    pub fn implementation_hash(&self) -> Option<u64> {
        self.implementation_hash
    }

    fn new_function(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::Function, name, shallow_hash)
    }

    fn new_class(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::Class, name, shallow_hash)
    }

    fn new_enum(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::Enum, name, shallow_hash)
    }

    fn new_type_alias(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::TypeAlias, name, shallow_hash)
    }

    fn new_client(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::Client, name, shallow_hash)
    }

    fn new_retry_policy(name: &str, shallow_hash: &HashMap<&str, ShallowHash>) -> Result<Self> {
        Self::new(SignatureType::RetryPolicy, name, shallow_hash)
    }

    fn new(
        r#type: SignatureType,
        name: &str,
        shallow_hash: &HashMap<&str, ShallowHash>,
    ) -> Result<Self> {
        let item = shallow_hash
            .get(name)
            .ok_or(anyhow::anyhow!("Item: {} not found", name))?;

        let mut all_dependencies = HashSet::new();

        let interface_hash = {
            let dependencies =
                recursively_collect_dependencies(name, shallow_hash, |name, shallow_hash| {
                    shallow_hash.get(name).map(|h| &h.interface_dependencies)
                })?;

            all_dependencies.extend(dependencies.iter().cloned());

            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            item.interface_hash.hash(&mut hasher);
            for dep in dependencies {
                let dep_hash = shallow_hash
                    .get(dep.as_str())
                    .ok_or(anyhow::anyhow!("Dependency: {} not found", dep))?;
                dep.as_str().hash(&mut hasher);
                dep_hash.interface_hash.hash(&mut hasher);
            }
            hasher.finish()
        };

        let implementation_hash = {
            let dependencies =
                recursively_collect_dependencies(name, shallow_hash, |name, shallow_hash| {
                    shallow_hash
                        .get(name)
                        .map(|h| &h.implementation_dependencies)
                })?;
            all_dependencies.extend(dependencies.iter().cloned());
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            let mut has_implementation_hash = item.implementation_hash.is_some();
            if let Some(h) = item.implementation_hash {
                h.hash(&mut hasher);
            }
            for dep in dependencies {
                let dep_hash = shallow_hash
                    .get(dep.as_str())
                    .ok_or(anyhow::anyhow!("Dependency: {} not found", dep))?;
                if let Some(h) = dep_hash.implementation_hash {
                    has_implementation_hash = true;
                    dep.as_str().hash(&mut hasher);
                    h.hash(&mut hasher);
                };
            }
            has_implementation_hash.then_some(hasher.finish())
        };

        Ok(Self {
            r#type,
            display_name: Arc::new(name.to_string()),
            interface_hash,
            implementation_hash,
            dependencies: Arc::new(all_dependencies.into_iter().cloned().collect()),
        })
    }
}

impl<'a, T: ShallowSignature> ShallowSignature for Walker<'a, &'a T> {
    fn shallow_hash_prefix(&self) -> &'static str {
        self.item.shallow_hash_prefix()
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        self.item.shallow_interface_hash()
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        self.item.shallow_implementation_hash()
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        self.item.unsorted_interface_dependencies()
    }

    fn unsorted_implementation_dependencies(&self) -> HashSet<String> {
        self.item.unsorted_implementation_dependencies()
    }

    fn interface_dependencies(&self) -> Vec<String> {
        self.item.interface_dependencies()
    }

    fn implementation_dependencies(&self) -> Vec<String> {
        self.item.implementation_dependencies()
    }
}

struct ShallowHash {
    interface_hash: u64,
    implementation_hash: Option<u64>,
    interface_dependencies: Vec<String>,
    implementation_dependencies: Vec<String>,
}

impl ShallowHash {
    fn from_signature(signature: impl ShallowSignature) -> Self {
        Self {
            interface_hash: {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                signature.shallow_interface_hash().hash(&mut hasher);
                hasher.finish()
            },
            implementation_hash: signature.shallow_implementation_hash().map(|h| {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                h.hash(&mut hasher);
                hasher.finish()
            }),
            interface_dependencies: signature.interface_dependencies(),
            implementation_dependencies: signature.implementation_dependencies(),
        }
    }
}

pub struct IRSignature {
    pub functions: HashMap<String, FunctionSignature>,
    pub classes: HashMap<String, (TypeNodeSignature, ClassSignatureDetails)>,
    pub enums: HashMap<String, (TypeNodeSignature, EnumSignatureDetails)>,
    pub type_aliases: HashMap<String, TypeNodeSignature>,
    pub clients: HashMap<String, Signature>,
    pub retry_policies: HashMap<String, Signature>,

    // Aggregate signature for the AST
    pub ast_signature: Signature,
}

impl IRSignature {
    pub fn new_from_ir(ir: &IntermediateRepr) -> Result<Self> {
        let mut shallow_hash = HashMap::new();

        for class in ir.walk_classes() {
            shallow_hash.insert(class.name(), ShallowHash::from_signature(class));
        }
        for r#enum in ir.walk_enums() {
            shallow_hash.insert(r#enum.name(), ShallowHash::from_signature(r#enum));
        }
        for type_alias in ir.walk_type_aliases() {
            shallow_hash.insert(type_alias.name(), ShallowHash::from_signature(type_alias));
        }
        for func in ir.walk_functions() {
            shallow_hash.insert(func.name(), ShallowHash::from_signature(func));
        }
        for client in ir.walk_clients() {
            shallow_hash.insert(client.name(), ShallowHash::from_signature(client));
        }
        for retry_policy in ir.walk_retry_policies() {
            shallow_hash.insert(
                retry_policy.name(),
                ShallowHash::from_signature(retry_policy),
            );
        }

        let functions_map: HashMap<String, FunctionSignature> = ir
            .walk_functions()
            .map(|func| {
                Ok((
                    func.name().to_string(),
                    FunctionSignature {
                        signature: Signature::new_function(func.name(), &shallow_hash)?,
                        inputs: Arc::new(
                            func.inputs()
                                .iter()
                                .map(|(name, r#type)| (name.clone(), r#type.clone()))
                                .collect(),
                        ),
                        output: Arc::new(func.output().clone()),
                    },
                ))
            })
            .collect::<Result<_>>()?;

        let classes_map: HashMap<String, (TypeNodeSignature, ClassSignatureDetails)> = ir
            .walk_classes()
            .map(|class_walker| {
                Ok((
                    class_walker.name().to_string(),
                    (
                        TypeNodeSignature {
                            signature: Signature::new_class(class_walker.name(), &shallow_hash)?,
                            field_type: Arc::new(baml_types::FieldType::class(class_walker.name())),
                        },
                        ClassSignatureDetails {
                            fields: Arc::new(
                                class_walker
                                    .elem()
                                    .static_fields
                                    .iter()
                                    .map(|field_node| {
                                        (
                                            field_node.elem.name.clone(),
                                            Arc::new(field_node.elem.r#type.elem.clone()),
                                        )
                                    })
                                    .collect(),
                            ),
                        },
                    ),
                ))
            })
            .collect::<Result<_>>()?;

        let enums_map: HashMap<String, (TypeNodeSignature, EnumSignatureDetails)> = ir
            .walk_enums()
            .map(|enum_walker| {
                Ok((
                    enum_walker.name().to_string(),
                    (
                        TypeNodeSignature {
                            signature: Signature::new_enum(enum_walker.name(), &shallow_hash)?,
                            field_type: Arc::new(baml_types::FieldType::r#enum(enum_walker.name())),
                        },
                        EnumSignatureDetails {
                            values: Arc::new(
                                enum_walker
                                    .elem()
                                    .values
                                    .iter()
                                    .map(|(enum_value_node, _doc_string)| {
                                        enum_value_node.elem.0.clone()
                                    })
                                    .collect(),
                            ),
                        },
                    ),
                ))
            })
            .collect::<Result<_>>()?;

        let type_aliases_map: HashMap<String, TypeNodeSignature> = ir
            .walk_type_aliases()
            .map(|type_alias_walker| {
                let resolved_field_type = type_alias_walker.elem().r#type.elem.clone();
                Ok((
                    type_alias_walker.name().to_string(),
                    TypeNodeSignature {
                        signature: Signature::new_type_alias(
                            type_alias_walker.name(),
                            &shallow_hash,
                        )?,
                        field_type: Arc::new(resolved_field_type),
                    },
                ))
            })
            .collect::<Result<_>>()?;

        let clients_map: HashMap<String, Signature> = ir
            .walk_clients()
            .map(|client| {
                Ok((
                    client.name().to_string(),
                    Signature::new_client(client.name(), &shallow_hash)?,
                ))
            })
            .collect::<Result<_>>()?;

        let retry_policies_map: HashMap<String, Signature> = ir
            .walk_retry_policies()
            .map(|retry_policy| {
                Ok((
                    retry_policy.name().to_string(),
                    Signature::new_retry_policy(retry_policy.name(), &shallow_hash)?,
                ))
            })
            .collect::<Result<_>>()?;

        // Calculate AST signature
        let ast_signature = {
            let mut has_implementation_hash = false;
            let mut implementation_hasher = std::collections::hash_map::DefaultHasher::new();
            let mut interface_hasher = std::collections::hash_map::DefaultHasher::new();

            // Hash functions (FunctionSignature)
            for (name, func_sig) in &functions_map {
                name.hash(&mut interface_hasher);
                func_sig
                    .signature
                    .interface_hash
                    .hash(&mut interface_hasher);

                if let Some(h) = func_sig.signature.implementation_hash {
                    name.hash(&mut implementation_hasher);
                    h.hash(&mut implementation_hasher);
                    has_implementation_hash = true;
                }
            }

            // Hash classes_map
            for (name, (type_node_sig, _details)) in &classes_map {
                name.hash(&mut interface_hasher);
                type_node_sig
                    .signature
                    .interface_hash
                    .hash(&mut interface_hasher);
                if let Some(h) = type_node_sig.signature.implementation_hash {
                    name.hash(&mut implementation_hasher);
                    h.hash(&mut implementation_hasher);
                    has_implementation_hash = true;
                }
            }

            // Hash enums_map
            for (name, (type_node_sig, _details)) in &enums_map {
                name.hash(&mut interface_hasher);
                type_node_sig
                    .signature
                    .interface_hash
                    .hash(&mut interface_hasher);
                if let Some(h) = type_node_sig.signature.implementation_hash {
                    name.hash(&mut implementation_hasher);
                    h.hash(&mut implementation_hasher);
                    has_implementation_hash = true;
                }
            }

            // Hash TypeNodeSignature for type_aliases_map (already correctly separate)
            for (name, type_node_sig) in &type_aliases_map {
                name.hash(&mut interface_hasher);
                type_node_sig
                    .signature
                    .interface_hash
                    .hash(&mut interface_hasher);
                // Assuming type aliases can have impl hash, as per previous logic.
                // If not, the `if let Some(h)` block handles it.
                if let Some(h) = type_node_sig.signature.implementation_hash {
                    name.hash(&mut implementation_hasher);
                    h.hash(&mut implementation_hasher);
                    has_implementation_hash = true;
                }
            }

            // Hash collections of Signature (clients_map, retry_policies_map - already correct)
            let signature_collections_with_impl_flag =
                [(&clients_map, true), (&retry_policies_map, false)];

            for (collection, has_impl) in signature_collections_with_impl_flag.iter() {
                for (name, sig) in collection.iter() {
                    name.hash(&mut interface_hasher);
                    sig.interface_hash.hash(&mut interface_hasher);
                    if *has_impl {
                        if let Some(h) = sig.implementation_hash {
                            name.hash(&mut implementation_hasher);
                            h.hash(&mut implementation_hasher);
                            has_implementation_hash = true;
                        }
                    }
                }
            }

            Signature {
                r#type: SignatureType::AST,
                display_name: Arc::new("baml_src".to_string()),
                interface_hash: interface_hasher.finish(),
                implementation_hash: has_implementation_hash
                    .then_some(implementation_hasher.finish()),
                dependencies: Arc::new(vec![]),
            }
        };

        Ok(Self {
            functions: functions_map,
            classes: classes_map,
            enums: enums_map,
            type_aliases: type_aliases_map,
            clients: clients_map,
            retry_policies: retry_policies_map,
            ast_signature,
        })
    }
}
