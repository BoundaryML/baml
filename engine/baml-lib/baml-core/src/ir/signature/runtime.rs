#[cfg(test)]
mod test;

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use baml_types::rpc;
use indexmap::IndexMap;
use itertools::Itertools;
use serde::de;

use crate::ir::{IRHelper, IntermediateRepr};

use super::{BamlHash, ProvideBamlHash};

#[derive(Debug)]
struct MerkleTree {
    name: String,
    hash: BamlHash,
    flat_dependencies: Vec<String>,
}

impl MerkleTree {
    pub fn print_tree(&self, all_nodes: &HashMap<String, MerkleTree>) -> String {
        self.print_tree_internal(0, true, all_nodes, vec![])
    }

    fn print_tree_internal(
        &self,
        indent: usize,
        last_dependency: bool,
        all_nodes: &HashMap<String, MerkleTree>,
        stack: Vec<String>,
    ) -> String {
        // Print the current node
        let mut result = format!(
            "{}{}─ {}\n",
            " ".repeat(indent),
            if last_dependency { "└" } else { "├" },
            self.name
        );

        // Iterate through each dependency and print its tree
        for (index, dependency) in self.flat_dependencies.iter().enumerate() {
            let is_last_dependency = index == self.flat_dependencies.len() - 1;
            let prefix = " ".repeat(indent) + if last_dependency { "  " } else { "│ " };

            if stack.contains(dependency) {
                result.push_str(&format!(
                    "{}{}─ {} (circular dependency)\n",
                    prefix,
                    if last_dependency { "└" } else { "├" },
                    dependency
                ));
                continue;
            }
            let dependency = all_nodes.get(dependency).unwrap();

            // Create prefix with vertical bars for non-last items
            let mut new_stack = stack.clone();
            new_stack.push(dependency.name.clone());
            result.push_str(
                &dependency
                    .print_tree_internal(indent + 2, is_last_dependency, all_nodes, new_stack)
                    .replace(&" ".repeat(indent + 2), &prefix),
            );
        }
        result
    }

    fn new(name: String, hash: BamlHash, flat_dependencies: Vec<String>) -> Self {
        Self {
            name,
            hash,
            flat_dependencies,
        }
    }

    fn unique_hashed_id(
        &self,
        all_nodes: &HashMap<String, MerkleTree>,
    ) -> rpc::upload_baml_src::UniqueId {
        let interface_hash = {
            // hash the interface
            let mut hasher = DefaultHasher::new();
            if let Some(interface_hash) = self.hash.interface_hash {
                interface_hash.hash(&mut hasher);
            }
            for dependency in self.flat_dependencies.iter() {
                let dependency = all_nodes.get(dependency).unwrap();
                if let Some(interface_hash) = dependency.hash.interface_hash {
                    interface_hash.hash(&mut hasher);
                }
            }
            hasher.finish()
        };

        let impl_hash = {
            let mut hasher = DefaultHasher::new();
            if let Some(impl_hash) = self.hash.impl_hash {
                impl_hash.hash(&mut hasher);
            }
            for dependency in self.flat_dependencies.iter() {
                let dependency = all_nodes.get(dependency).unwrap();
                if let Some(impl_hash) = dependency.hash.impl_hash {
                    impl_hash.hash(&mut hasher);
                }
            }
            hasher.finish()
        };
        rpc::upload_baml_src::UniqueId {
            type_name: self.hash.type_name.to_string(),
            name: self.name.clone(),
            interface_hash: Some(interface_hash),
            impl_hash: Some(impl_hash),
        }
    }

    fn diff(&self, other: &MerkleTree, all_nodes: &HashMap<String, MerkleTree>) -> Vec<String> {
        assert_eq!(self.hash.type_name, other.hash.type_name);
        let mut diff = Vec::new();
        if self.unique_hashed_id(all_nodes) != other.unique_hashed_id(all_nodes) {
            // zip the dependencies together by matching name
            let dependencies = self
                .flat_dependencies
                .iter()
                .map(|d| (d, other.flat_dependencies.iter().find(|d2| *d2 == d)))
                .collect::<Vec<_>>();
            let other_only = other
                .flat_dependencies
                .iter()
                .filter(|d| !self.flat_dependencies.iter().any(|d2| d2 == *d))
                .collect::<Vec<_>>();
            for (self_dependency, other_dependency) in dependencies {
                if let Some(other_dependency) = other_dependency {
                    diff.extend(
                        all_nodes
                            .get(self_dependency)
                            .unwrap()
                            .diff(all_nodes.get(other_dependency).unwrap(), all_nodes)
                            .into_iter()
                            .map(|d| format!("{}->{}", self_dependency, d)),
                    );
                } else {
                    diff.push(format!("missing dependency: {}", self_dependency));
                }
            }
            for other_dependency in other_only {
                diff.push(format!("extra dependency: {}", other_dependency));
            }
        }
        diff
    }
}

impl IntermediateRepr {
    fn to_baml_type_reference(
        &self,
        field_type: &crate::ir::FieldType,
        all_nodes: &HashMap<String, MerkleTree>,
    ) -> rpc::upload_baml_src::BamlTypeReference {
        use rpc::upload_baml_src::BamlTypeReference;
        match field_type {
            baml_types::FieldType::Primitive(type_value) => {
                match type_value {
                    baml_types::TypeValue::String => BamlTypeReference::String,
                    baml_types::TypeValue::Int => BamlTypeReference::Int,
                    baml_types::TypeValue::Float => BamlTypeReference::Float,
                    baml_types::TypeValue::Bool => BamlTypeReference::Bool,
                    baml_types::TypeValue::Null => BamlTypeReference::Null,
                    baml_types::TypeValue::Media(baml_media_type) => {
                        match baml_media_type {
                            baml_types::BamlMediaType::Image => BamlTypeReference::Media(rpc::upload_baml_src::BamlMediaType::Image),
                            baml_types::BamlMediaType::Audio => BamlTypeReference::Media(rpc::upload_baml_src::BamlMediaType::Audio),
                        }
                    },
                }
            },
            baml_types::FieldType::Enum(e) => BamlTypeReference::Enum {
                type_id: all_nodes
                    .get(e)
                    .expect(format!("Enum {} not found", e).as_str())
                    .unique_hashed_id(&all_nodes)
                    .into(),
            },
            baml_types::FieldType::Literal(literal_value) => {
                BamlTypeReference::Literal(literal_value.into())
            },
            baml_types::FieldType::Class(c) => BamlTypeReference::Class {
                type_id: all_nodes
                    .get(c)
                    .expect(format!("Class {} not found", c).as_str())
                    .unique_hashed_id(&all_nodes)
                    .into(),
            },
            baml_types::FieldType::List(field_type) =>  {
                BamlTypeReference::Array {
                    items: Box::new(self.to_baml_type_reference(field_type, &all_nodes)),
                }
            },
            baml_types::FieldType::Map(field_type, field_type1) => {
                BamlTypeReference::Map {
                    key: Box::new(self.to_baml_type_reference(field_type, &all_nodes)),
                    value: Box::new(self.to_baml_type_reference(field_type1, &all_nodes)),
                }
            },
            baml_types::FieldType::Union(field_types) => {
                BamlTypeReference::Union {
                    any_of: field_types.iter().map(|f| self.to_baml_type_reference(f, &all_nodes)).collect(),
                }
            },
            baml_types::FieldType::Tuple(field_types) => {
                BamlTypeReference::Tuple {
                    items: field_types.iter().map(|f| self.to_baml_type_reference(f, &all_nodes)).collect(),
                }
            },
            baml_types::FieldType::Optional(field_type) => {
                BamlTypeReference::Union {
                    any_of: vec![
                        self.to_baml_type_reference(field_type, &all_nodes),
                        BamlTypeReference::Null,
                    ],
                }
            },
            baml_types::FieldType::WithMetadata {
                base,
                constraints,
                streaming_behavior,
            } => {
                self.to_baml_type_reference(base, &all_nodes)
            }
            baml_types::FieldType::RecursiveTypeAlias(recursive_type_alias) => {
                BamlTypeReference::TypeAlias {
                    type_id: all_nodes.get(recursive_type_alias.as_str()).unwrap().unique_hashed_id(&all_nodes).into(),
                }
            }
        }
    }
    pub fn to_boundary_upload_request(
        &self,
        project_id: String,
    ) -> rpc::upload_baml_src::UploadBamlSrcRequest {
        let (baml_src_node, all_nodes) = self.create_merkle_tree();

        let function_definitions = self
            .functions
            .iter()
            .filter_map(|f| {
                all_nodes.get(f.elem.name.as_str()).map(|n| {
                    rpc::upload_baml_src::BamlFunctionDefinition {
                        function_id: n.unique_hashed_id(&all_nodes).into(),
                        inputs: f
                            .elem
                            .inputs
                            .iter()
                            .map(
                                |(name, field_type)| rpc::upload_baml_src::BamlFunctionInput {
                                    name: name.clone(),
                                    value: self.to_baml_type_reference(field_type, &all_nodes),
                                },
                            )
                            .collect(),
                        output: rpc::upload_baml_src::BamlTypeReference::Null,
                    }
                })
            })
            .collect();

        let class_definitions = self.classes.iter().filter_map(|c| {
            all_nodes.get(c.elem.name.as_str()).map(|n| {
                rpc::upload_baml_src::BamlTypeDefinition::Class(
                    rpc::upload_baml_src::BamlClassDefinition {
                        type_id: n.unique_hashed_id(&all_nodes).into(),
                        fields: c
                            .elem
                            .static_fields
                            .iter()
                            .map(|f| rpc::upload_baml_src::BamlClassField {
                                name: f.elem.name.clone(),
                                r#type: self
                                    .to_baml_type_reference(&f.elem.r#type.elem, &all_nodes),
                            })
                            .collect(),
                    },
                )
            })
        });

        let enum_definitions = self.enums.iter().filter_map(|e| {
            all_nodes.get(e.elem.name.as_str()).map(|n| {
                rpc::upload_baml_src::BamlTypeDefinition::Enum(
                    rpc::upload_baml_src::BamlEnumDefinition {
                        type_id: n.unique_hashed_id(&all_nodes).into(),
                        values: e.elem.values.iter().map(|v| v.0.elem.0.clone()).collect(),
                    },
                )
            })
        });

        let type_alias_definitions = self.type_aliases.iter().filter_map(|a| {
            all_nodes.get(a.elem.name.as_str()).map(|n| {
                rpc::upload_baml_src::BamlTypeDefinition::TypeAlias(
                    rpc::upload_baml_src::BamlTypeAliasDefinition {
                        type_id: n.unique_hashed_id(&all_nodes).into(),
                        type_reference: self
                            .to_baml_type_reference(&a.elem.r#type.elem, &all_nodes),
                    },
                )
            })
        });

        rpc::upload_baml_src::UploadBamlSrcRequest {
            project_id,
            baml_src_id: baml_src_node.unique_hashed_id(&all_nodes),
            function_definitions,
            type_definitions: class_definitions
                .chain(enum_definitions)
                .chain(type_alias_definitions)
                .collect(),
        }
    }

    fn create_merkle_tree<'a>(&'a self) -> (MerkleTree, HashMap<String, MerkleTree>) {
        // hash everything we can based on the IR
        let enums = self.enums.iter().map(|e| {
            let hash = e.to_baml_hash();

            (e.elem.name.as_str(), hash)
        });

        let classes = self.classes.iter().map(|c| {
            let hash = c.to_baml_hash();

            (c.elem.name.as_str(), hash)
        });

        let type_aliases = self.type_aliases.iter().map(|a| {
            let hash = a.to_baml_hash();

            (a.elem.name.as_str(), hash)
        });

        let clients = self.clients.iter().map(|c| {
            let hash = c.to_baml_hash();

            (c.elem.name.as_str(), hash)
        });

        let retry_policies = self.retry_policies.iter().map(|r| {
            let hash = r.to_baml_hash();

            (r.elem.name.0.as_str(), hash)
        });

        let template_strings = self.template_strings.iter().map(|t| {
            let hash = t.to_baml_hash();

            (t.elem.name.as_str(), hash)
        });

        let functions = self.functions.iter().map(|f| {
            let hash = f.to_baml_hash();

            (f.elem.name.as_str(), hash)
        });

        // Sort by type then by name
        let all_trees = functions
            .chain(retry_policies)
            .chain(template_strings)
            .chain(clients)
            .chain(type_aliases)
            .chain(classes)
            .chain(enums)
            .map(|(name, hash)| (name, MerkleTree::new(name.to_string(), hash, vec![])))
            .collect::<HashMap<_, _>>();

        let mut dependency_graph = std::collections::HashMap::new();
        self.functions.iter().for_each(|f| {
            let mut dependencies = std::collections::HashSet::new();

            for (_, field_type) in f.elem.inputs.iter() {
                // Get all names
                dependencies.extend(field_type.names());
            }

            dependencies.extend(f.elem.output().names());

            // TODO: Also get any template strings used in the body
            dependency_graph.insert(f.elem.name.clone(), dependencies);
        });

        self.classes.iter().for_each(|c| {
            let mut dependencies = std::collections::HashSet::new();
            dependencies.extend(
                c.elem
                    .static_fields
                    .iter()
                    .flat_map(|f| f.elem.r#type.elem.names()),
            );
            dependency_graph.insert(c.elem.name.clone(), dependencies);
        });

        self.clients.iter().for_each(|c| {
            dependency_graph.insert(c.elem.name.clone(), c.elem.names());
        });

        self.enums.iter().for_each(|e| {
            dependency_graph.insert(e.elem.name.clone(), Default::default());
        });

        self.retry_policies.iter().for_each(|r| {
            dependency_graph.insert(r.elem.name.0.clone(), Default::default());
        });

        self.template_strings.iter().for_each(|t| {
            let mut dependencies = std::collections::HashSet::new();
            dependencies.extend(t.elem.params.iter().flat_map(|f| f.r#type.elem.names()));
            // TODO: Also get any template strings used in the body
            dependency_graph.insert(t.elem.name.clone(), dependencies);
        });

        self.type_aliases.iter().for_each(|a| {
            dependency_graph.insert(a.elem.name.clone(), a.elem.r#type.elem.names());
        });

        let all_nodes = {
            let all_dependencies = find_all_dependencies(&dependency_graph, &all_trees);
            // for each tree, add its dependencies
            let mut deps = all_dependencies
                .into_iter()
                .map(|(name, dependencies)| {
                    let dependencies = dependencies.into_iter().sorted().collect::<Vec<_>>();
                    (name, dependencies)
                })
                .collect::<HashMap<_, _>>();
            all_trees
                .into_iter()
                .map(|(name, mut tree)| {
                    if let Some(dependencies) = deps.remove(name) {
                        tree.flat_dependencies = dependencies;
                    }
                    (name, tree)
                })
                .collect::<HashMap<_, _>>()
        };
        let all_names = all_nodes
            .values()
            .map(|t| (t.hash.type_name, t.name.clone()))
            .sorted()
            .map(|(_, name)| name)
            .collect::<Vec<_>>();

        let full_hash = {
            let mut hasher = DefaultHasher::new();
            for name in all_names.iter() {
                all_nodes.get(name.as_str()).unwrap().hash.hash(&mut hasher);
            }
            hasher.finish()
        };

        let baml_src_node = MerkleTree::new(
            "baml_src".to_string(),
            BamlHash {
                type_name: "IR",
                interface_hash: Some(full_hash),
                impl_hash: None,
            },
            all_names,
        );

        let all_nodes = all_nodes
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<HashMap<_, _>>();

        (baml_src_node, all_nodes)
    }
}

fn find_all_dependencies(
    one_level_dependency: &std::collections::HashMap<String, std::collections::HashSet<String>>,
    all_names: &HashMap<&str, MerkleTree>,
) -> std::collections::HashMap<String, std::collections::HashSet<String>> {
    // for each name in all_names, find all dependencies
    all_names
        .iter()
        .map(|(name, tree)| {
            let mut dependencies = std::collections::HashSet::new();
            let mut stack = vec![tree];
            while let Some(name) = stack.pop() {
                if let Some(deps) = one_level_dependency.get(tree.name.as_str()) {
                    for dep in deps.iter() {
                        if dependencies.insert(dep.clone()) {
                            if dep != tree.name.as_str() {
                                match all_names.get(dep.as_str()) {
                                    Some(tree) => {
                                        stack.push(tree);
                                    }
                                    None => {
                                        log::warn!("Dependency {} not found", dep);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            (name.to_string(), dependencies)
        })
        .collect::<std::collections::HashMap<_, _>>()
}
