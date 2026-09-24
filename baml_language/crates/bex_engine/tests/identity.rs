mod common;

use std::sync::Arc;

use bex_engine::BexEngine;
use common::compile_for_engine;
use sys_native::SysOpsExt;

#[tokio::test]
async fn function_metadata_derives_owner_type_for_class_methods() {
    let source = r#"
        class Holder {
            value int

            function unwrap(self) -> int {
                self.value
            }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let expected_functions: Vec<_> = snapshot
        .objects
        .iter()
        .filter_map(|object| match object {
            bex_vm_types::Object::Function(function) => Some(function.name.clone()),
            _ => None,
        })
        .collect();
    let engine =
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap();

    assert_eq!(
        engine
            .program_metadata()
            .await
            .function_table
            .functions
            .iter()
            .map(|metadata| metadata.fqn.clone())
            .collect::<Vec<_>>(),
        expected_functions,
        "metadata should contain exactly the compiled functions",
    );

    let metadata = engine.program_metadata().await;
    let method = metadata
        .function_table
        .functions
        .iter()
        .find(|metadata| metadata.fqn == "user.Holder.unwrap")
        .expect("expected method metadata");
    assert_eq!(
        method.owner_type,
        Some(bex_events::DefinitionKey("class:user.Holder".to_string()))
    );
}

#[tokio::test]
async fn program_identity_is_uuid_v7_with_or_without_a_source_hash() {
    fn assert_uuid_v7(program_id: bex_events::ids::ProgramId) {
        assert_eq!(program_id.0[6] >> 4, 7);
        assert_eq!(program_id.0[8] >> 6, 2);
    }

    let with_hash = compile_for_engine("function main() -> null { null }");
    let source_hash = with_hash
        .source_content_hash
        .expect("the compiler should stamp a source-content hash");
    let with_hash_engine = BexEngine::new(
        with_hash,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
    )
    .unwrap();
    assert_uuid_v7(with_hash_engine.program_metadata().await.program_id);
    assert_eq!(
        with_hash_engine.program_metadata().await.source_snapshot_id,
        Some(bex_events::ids::SourceSnapshotId(source_hash))
    );

    let mut without_hash = compile_for_engine("function main() -> null { null }");
    without_hash.source_content_hash = None;
    let without_hash_engine = BexEngine::new(
        without_hash,
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
    )
    .unwrap();
    assert_uuid_v7(without_hash_engine.program_metadata().await.program_id);
    assert_ne!(
        with_hash_engine.program_metadata().await.program_id,
        without_hash_engine.program_metadata().await.program_id
    );
    assert_eq!(
        without_hash_engine
            .program_metadata()
            .await
            .source_snapshot_id,
        None
    );
}
