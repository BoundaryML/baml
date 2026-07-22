//! Language-neutral boundary tests for preserving dynamic value occurrences.

mod common;

use std::sync::Arc;

use baml_type::{Freshness, Literal, RuntimeTy, TyAttr};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, UnionMetadata};
use common::compile_for_engine;
use sys_native::SysOpsExt;

fn call_context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test]
async fn unknown_identity_preserves_collection_and_union_occurrences() {
    let snapshot = compile_for_engine(
        r#"
            function echo_unknown(value: unknown) -> unknown {
                value
            }
        "#,
    );
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );

    let list = BexExternalValue::Array {
        element_type: RuntimeTy::unknown(),
        items: vec![
            BexExternalValue::Int(1),
            BexExternalValue::String("two".into()),
        ],
    };
    let list_result = engine
        .call_function("echo_unknown", vec![list.clone()], call_context(), true)
        .await
        .unwrap();
    assert_eq!(list_result, list);

    let literal = RuntimeTy::Literal(
        Literal::String("fixed".to_string()),
        Freshness::Regular,
        TyAttr::default(),
    );
    let union_type = RuntimeTy::Union(
        vec![RuntimeTy::string(), literal.clone()],
        TyAttr::default(),
    );
    let union = BexExternalValue::Union {
        value: Box::new(BexExternalValue::String("fixed".into())),
        metadata: UnionMetadata::new(union_type, literal),
    };
    let union_result = engine
        .call_function("echo_unknown", vec![union.clone()], call_context(), true)
        .await
        .unwrap();
    assert_eq!(union_result, union);
}
