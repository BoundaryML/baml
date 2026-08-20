//! Target-neutral profile metadata registry.
//!
//! Engine metadata supplies function/source joins to the direct consumer.
//! Storage is process-shared and bounded by active engine lifetimes.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use crate::prof::EngineProfileMetadata;

fn registry() -> &'static Mutex<HashMap<u64, EngineProfileMetadata>> {
    static META: OnceLock<Mutex<HashMap<u64, EngineProfileMetadata>>> = OnceLock::new();
    META.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers metadata before the engine starts producing profile records.
pub fn register_engine_metadata(engine_id: u64, meta: EngineProfileMetadata) {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(engine_id, meta);
}

pub fn get_engine_metadata(engine_id: u64) -> Option<EngineProfileMetadata> {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&engine_id)
        .cloned()
}

pub(crate) fn remove_engine_metadata(engine_id: u64) -> Option<EngineProfileMetadata> {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&engine_id)
}

#[cfg(test)]
mod tests {
    use super::{get_engine_metadata, register_engine_metadata, remove_engine_metadata};
    use crate::prof::{EngineProfileMetadata, FunctionMetaEntry};

    fn meta(fqn: &str) -> EngineProfileMetadata {
        EngineProfileMetadata {
            program_id: "program".to_string(),
            source_snapshot_id: Some("snapshot".to_string()),
            revision_id: Some("revision".to_string()),
            functions: vec![FunctionMetaEntry {
                function_id: 1,
                fqn: fqn.to_string(),
                source_file: "main.baml".to_string(),
                span_start: 1,
                span_end: 2,
                kind: "bytecode".to_string(),
                definition_key: None,
                owner_type: None,
                parent_function: None,
                lambda_path: None,
                package_name: None,
                namespace: vec!["ns".to_string()],
            }],
        }
    }

    #[test]
    fn metadata_registry_registers_replaces_and_removes() {
        let engine_id = 9_000_001;
        let _ = remove_engine_metadata(engine_id);

        register_engine_metadata(engine_id, meta("first"));
        assert_eq!(
            get_engine_metadata(engine_id).unwrap().functions[0].fqn,
            "first"
        );

        register_engine_metadata(engine_id, meta("second"));
        assert_eq!(
            get_engine_metadata(engine_id).unwrap().functions[0].fqn,
            "second"
        );

        assert!(remove_engine_metadata(engine_id).is_some());
        assert!(get_engine_metadata(engine_id).is_none());
    }
}
