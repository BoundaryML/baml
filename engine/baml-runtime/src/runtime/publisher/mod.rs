use super::InternalBamlRuntime;
use crate::{internal::ir_features::WithInternal, tracingv2::publisher::TypeLookup};
use baml_rpc::ast::{ast_node_id::AstNodeId, tops::BamlFunctionId};
use baml_rpc::BamlTypeId;
use baml_types::FieldType;
use cowstr::CowStr;
use internal_baml_core::ir::ir_hasher;
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

/// Type alias for a value with its dependencies
pub type WithDependency<T> = (Arc<T>, Arc<Vec<Arc<BamlTypeId>>>);

use super::super::tracingv2::publisher::rpc_converters::IntoRpcEvent;

#[derive(Serialize)]
pub struct FunctionSignatureWithDependencies {
    pub function_id: WithDependency<BamlFunctionId>,
    pub inputs: Vec<(String, FieldType)>,
    pub output: FieldType,
}

#[derive(Default, Serialize)]
pub struct AstSignatureWrapper {
    /// Path to source code
    pub source_code: HashMap<PathBuf, CowStr>,
    pub functions: HashMap<String, FunctionSignatureWithDependencies>,
    pub types: HashMap<String, WithDependency<BamlTypeId>>,
    pub env_vars: HashMap<String, String>,
}

impl AstSignatureWrapper {
    pub fn env_var(&self, key: &str) -> Option<&String> {
        self.env_vars.get(key)
    }
}

impl TypeLookup for AstSignatureWrapper {
    fn type_lookup(&self, name: &str) -> Option<Arc<BamlTypeId>> {
        self.types.get(name).map(|(id, _)| id.clone())
    }

    fn function_lookup(&self, name: &str) -> Option<Arc<BamlFunctionId>> {
        self.functions.get(name).map(|f| f.function_id.0.clone())
    }
}

/// Helper to resolve dependencies by name, skipping missing ones
fn resolve_dependencies<'a>(
    dep_names: impl IntoIterator<Item = &'a str>,
    ir_types: &'a HashMap<String, (Arc<BamlTypeId>, Arc<Vec<Arc<BamlTypeId>>>)>,
) -> Arc<Vec<Arc<BamlTypeId>>> {
    Arc::new(
        dep_names
            .into_iter()
            .filter_map(|name| ir_types.get(name).map(|(id, _)| id.clone()))
            .collect(),
    )
}

impl TryFrom<(Arc<InternalBamlRuntime>, HashMap<String, String>)> for AstSignatureWrapper {
    type Error = anyhow::Error;

    fn try_from(
        (ir, env_vars): (Arc<InternalBamlRuntime>, HashMap<String, String>),
    ) -> Result<Self, Self::Error> {
        let ir_signature = ir_hasher::IRSignature::new_from_ir(&ir.ir)?;

        // Collect dependency names for each type before moving out of ir_signature
        let mut type_deps: HashMap<String, Vec<String>> = HashMap::new();
        for (name, signature) in ir_signature.classes.iter() {
            type_deps.insert(name.clone(), signature.dependency_names().clone());
        }
        for (name, signature) in ir_signature.enums.iter() {
            type_deps.insert(name.clone(), signature.dependency_names().clone());
        }
        for (name, signature) in ir_signature.type_aliases.iter() {
            type_deps.insert(name.clone(), signature.dependency_names().clone());
        }

        // Build types map (classes, enums, type_aliases)
        let mut ir_types: HashMap<String, (Arc<BamlTypeId>, Arc<Vec<Arc<BamlTypeId>>>)> =
            HashMap::new();
        for (name, signature) in ir_signature.classes.into_iter() {
            let id = Arc::new(BamlTypeId(AstNodeId::new_class(
                name.clone(),
                signature.interface_hash(),
                signature.implementation_hash(),
            )));
            ir_types.insert(name, (id, Arc::new(vec![]))); // deps filled later
        }
        for (name, signature) in ir_signature.enums.into_iter() {
            let id = Arc::new(BamlTypeId(AstNodeId::new_enum(
                name.clone(),
                signature.interface_hash(),
                signature.implementation_hash(),
            )));
            ir_types.insert(name, (id, Arc::new(vec![]))); // deps filled later
        }
        for (name, signature) in ir_signature.type_aliases.into_iter() {
            let id = Arc::new(BamlTypeId(AstNodeId::new_type_alias(
                name.clone(),
                signature.interface_hash(),
                signature.implementation_hash(),
            )));
            ir_types.insert(name, (id, Arc::new(vec![]))); // deps filled later
        }
        // Now fill in dependencies for each type using the type_deps map
        let ir_types_keys: Vec<String> = ir_types.keys().cloned().collect();
        let mut deps_map: HashMap<String, Arc<Vec<Arc<BamlTypeId>>>> = HashMap::new();
        for name in &ir_types_keys {
            let deps: Vec<Arc<BamlTypeId>> = type_deps
                .get(name)
                .into_iter()
                .flat_map(|deps| deps.iter())
                .filter_map(|dep_name| ir_types.get(dep_name).map(|(id, _)| id.clone()))
                .collect();
            deps_map.insert(name.clone(), Arc::new(deps));
        }
        for name in ir_types_keys {
            if let Some((_id, deps_arc)) = ir_types.get_mut(&name) {
                if let Some(new_deps) = deps_map.get(&name) {
                    *deps_arc = Arc::clone(new_deps);
                }
            }
        }

        // Build functions map
        let functions: HashMap<String, FunctionSignatureWithDependencies> = ir_signature
            .functions
            .into_iter()
            .map(|(name, signature)| {
                let id = Arc::new(BamlFunctionId(AstNodeId::new_function(
                    name.clone(),
                    signature.signature.interface_hash(),
                    signature.signature.implementation_hash(),
                )));
                let deps = resolve_dependencies(
                    signature
                        .signature
                        .dependency_names()
                        .iter()
                        .map(|s| s.as_str()),
                    &ir_types,
                );
                (
                    name,
                    FunctionSignatureWithDependencies {
                        function_id: (id, deps),
                        inputs: signature.inputs.clone(),
                        output: signature.output.clone(),
                    },
                )
            })
            .collect();

        // Build types map for wrapper
        let types: HashMap<String, WithDependency<BamlTypeId>> = ir_types
            .into_iter()
            .map(|(name, (id, deps))| (name, (id, deps)))
            .collect();

        // Build source code map
        let source_code = ir
            .source_files
            .iter()
            .map(|file| (file.path_buf().clone(), CowStr::from(file.as_str())))
            .collect();

        Ok(Self {
            env_vars,
            functions,
            types,
            source_code,
        })
    }
}
