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
    // Rendered as the loader will render them: under the label of the
    // package whose cells hold each function.
    let expected_functions: Vec<_> = (0..snapshot.objects.len())
        .filter_map(|index| snapshot.function_name(bex_vm_types::ObjectIndex::from_raw(index)))
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

/// Every function kind's metadata is read off its identity: a class method
/// owns up to its class, an interface default body to its interface, an
/// impl-provided body to nothing (its for-target is a pattern), and a lambda
/// names its parent by the parent's definition key.
#[tokio::test]
async fn function_metadata_reads_owner_and_parent_off_the_identity() {
    let source = r#"
        interface Speaker {
            function speak(self) -> string throws never
            function twice(self) -> string throws never { self.speak() + self.speak() }
        }

        class Dog {
            name string

            function bark(self) -> string {
                let shout = (s: string) -> string { s + "!" };
                shout(self.name)
            }

            implements Speaker {
                function speak(self) -> string throws never { self.name }
            }
        }
    "#;
    let snapshot = compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap();
    let metadata = engine.program_metadata().await;
    let by_fqn = |fqn: &str| {
        metadata
            .function_table
            .functions
            .iter()
            .find(|metadata| metadata.fqn == fqn)
            .unwrap_or_else(|| panic!("expected metadata for `{fqn}`"))
    };

    assert_eq!(
        by_fqn("user.Dog.bark").owner_type,
        Some(bex_events::DefinitionKey("class:user.Dog".to_string()))
    );
    assert_eq!(
        by_fqn("user.Speaker.twice").owner_type,
        Some(bex_events::DefinitionKey(
            "interface:user.Speaker".to_string()
        ))
    );
    let provided = by_fqn("user.<(user.Dog as user.Speaker)>.speak");
    assert_eq!(provided.owner_type, None);
    assert_eq!(provided.namespace, Vec::<String>::new());

    let lambda = by_fqn(".<lambda(Dog.bark, 0)>");
    assert_eq!(
        lambda.parent_function,
        Some(bex_events::DefinitionKey(
            "function:user.Dog.bark".to_string()
        ))
    );
    assert_eq!(lambda.lambda_path.as_deref(), Some("<lambda(Dog.bark, 0)>"));
    assert_eq!(lambda.package_name.as_deref(), Some("user"));
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
