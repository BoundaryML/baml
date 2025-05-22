use anyhow::Result;
use baml_types::HasFieldType;
use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
};

use baml_rpc::{NamedType, TypeDefinition, TypeReference};

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

#[derive(Clone)]
enum SignatureType {
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
    r#type: SignatureType,
    display_name: String,
    interface_hash: u64,
    implementation_hash: Option<u64>,
    dependencies: Vec<String>,
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
            item.implementation_hash.map(|h| h.hash(&mut hasher));
            for dep in dependencies {
                let dep_hash = shallow_hash
                    .get(dep.as_str())
                    .ok_or(anyhow::anyhow!("Dependency: {} not found", dep))?;
                dep_hash.implementation_hash.map(|h| {
                    has_implementation_hash = true;
                    dep.as_str().hash(&mut hasher);
                    h.hash(&mut hasher);
                });
            }
            has_implementation_hash.then_some(hasher.finish())
        };

        Ok(Self {
            r#type,
            display_name: name.to_string(),
            interface_hash,
            implementation_hash,
            dependencies: all_dependencies.into_iter().cloned().collect(),
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
    pub classes: HashMap<String, Signature>,
    pub enums: HashMap<String, Signature>,
    pub type_aliases: HashMap<String, Signature>,
    pub clients: HashMap<String, Signature>,
    pub retry_policies: HashMap<String, Signature>,

    // Aggregate signature for the AST
    pub ast_signature: Signature,
}

pub struct FunctionSignature {
    pub signature: Signature,
    pub inputs: Vec<(String, FieldType)>,
    pub output: FieldType,
}

impl IRSignature {
    pub fn new_from_ir(ir: &IntermediateRepr) -> Result<Self> {
        // Collect all walks and shallow hashes in a single pass for each type

        // Functions
        let mut shallow_hashes = HashMap::new();
        let mut functions_data = Vec::new();
        for function in ir.walk_functions() {
            let name = function.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(function));
            functions_data.push((name.to_string(), function));
        }

        // Classes
        let mut classes_data = Vec::new();
        for class in ir.walk_classes() {
            let name = class.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(class));
            classes_data.push((name.to_string(), class));
        }

        // Enums
        let mut enums_data = Vec::new();
        for enum_node in ir.walk_enums() {
            let name = enum_node.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(enum_node));
            enums_data.push((name.to_string(), enum_node));
        }

        // Type aliases
        let mut type_aliases_data = Vec::new();
        for type_alias in ir.walk_type_aliases() {
            let name = type_alias.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(type_alias));
            type_aliases_data.push((name.to_string(), type_alias));
        }

        // Clients
        let mut clients_data = Vec::new();
        for client in ir.walk_clients() {
            let name = client.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(client));
            clients_data.push((name.to_string(), client));
        }

        // Retry policies
        let mut retry_policies_data = Vec::new();
        for retry_policy in ir.walk_retry_policies() {
            let name = retry_policy.name();
            shallow_hashes.insert(name, ShallowHash::from_signature(retry_policy));
            retry_policies_data.push((name.to_string(), retry_policy));
        }

        // Generate signature objects for all types except functions first
        let classes = {
            classes_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            classes_data
                .into_iter()
                .map(|(name, class)| {
                    Signature::new_class(class.name(), &shallow_hashes).map(|s| (name, s))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        let enums = {
            enums_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            enums_data
                .into_iter()
                .map(|(name, enum_node)| {
                    Signature::new_enum(enum_node.name(), &shallow_hashes).map(|s| (name, s))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        let type_aliases = {
            type_aliases_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            type_aliases_data
                .into_iter()
                .map(|(name, type_alias)| {
                    Signature::new_type_alias(type_alias.name(), &shallow_hashes).map(|s| (name, s))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        let clients = {
            clients_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            clients_data
                .into_iter()
                .map(|(name, client)| {
                    Signature::new_client(client.name(), &shallow_hashes).map(|s| (name, s))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        let retry_policies = {
            retry_policies_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            retry_policies_data
                .into_iter()
                .map(|(name, retry_policy)| {
                    Signature::new_retry_policy(retry_policy.name(), &shallow_hashes)
                        .map(|s| (name, s))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        // Now create function signatures with their respective input and output signatures
        let functions = {
            functions_data.sort_by(|(name_a, _), (name_b, _)| name_a.cmp(name_b));
            functions_data
                .into_iter()
                .map(|(name, function)| {
                    let signature = Signature::new_function(function.name(), &shallow_hashes)?;

                    // Create simplified FunctionSignature
                    let mut inputs = Vec::new();
                    for (input_name, input_type) in function.inputs().iter() {
                        inputs.push((input_name.to_string(), input_type.field_type()));
                    }

                    // Create output signature
                    let output = function.output().field_type().clone();

                    Ok((
                        name,
                        FunctionSignature {
                            signature,
                            inputs: inputs
                                .into_iter()
                                .map(|(name, field_type)| (name, field_type.clone()))
                                .collect(),
                            output,
                        },
                    ))
                })
                .collect::<Result<HashMap<_, _>>>()?
        };

        // Calculate AST signature
        let ast_signature = {
            let mut has_implementation_hash = false;
            let mut implementation_hash = std::collections::hash_map::DefaultHasher::new();
            let mut interface_hash = std::collections::hash_map::DefaultHasher::new();

            // Hash functions
            for (name, function_sig) in &functions {
                name.hash(&mut interface_hash);
                function_sig
                    .signature
                    .interface_hash
                    .hash(&mut interface_hash);

                if let Some(h) = function_sig.signature.implementation_hash {
                    name.hash(&mut implementation_hash);
                    h.hash(&mut implementation_hash);
                    has_implementation_hash = true;
                }
            }

            // Hash other types
            for (collection, has_impl) in [
                (&classes, true),
                (&enums, true),
                (&type_aliases, true),
                (&clients, true),
                (&retry_policies, false),
            ] {
                for (name, signature) in collection.iter() {
                    name.hash(&mut interface_hash);
                    signature.interface_hash.hash(&mut interface_hash);

                    if has_impl {
                        if let Some(h) = signature.implementation_hash {
                            name.hash(&mut implementation_hash);
                            h.hash(&mut implementation_hash);
                            has_implementation_hash = true;
                        }
                    }
                }
            }

            Signature {
                r#type: SignatureType::AST,
                display_name: "baml_src".into(),
                interface_hash: interface_hash.finish(),
                implementation_hash: has_implementation_hash
                    .then_some(implementation_hash.finish()),
                dependencies: vec![],
            }
        };

        Ok(Self {
            functions,
            classes,
            enums,
            type_aliases,
            clients,
            retry_policies,
            ast_signature,
        })
    }
}
