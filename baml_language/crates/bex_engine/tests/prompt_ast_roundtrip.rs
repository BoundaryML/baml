//! Inbound round-trip coverage for `BexExternalAdt::PromptAst` values.

mod common;

use std::sync::Arc;

use baml_builtins2::{PromptAst, PromptAstSimple};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_external_types::BexExternalAdt;
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

fn prompt_instance(ast: Arc<PromptAst>) -> BexExternalValue {
    let mut fields = IndexMap::new();
    fields.insert(
        "_data".to_string(),
        BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)),
    );
    BexExternalValue::Instance {
        class_name: "baml.llm.PromptAst".to_string(),
        type_args: vec![],
        fields,
    }
}

#[tokio::test]
async fn prompt_ast_handle_can_reenter_its_stdlib_accessor() {
    let source = r#"
function prompt_text(value: baml.llm.PromptAst) -> string {
  value.text()
}
"#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let ast = Arc::new(PromptAst::Message {
        role: "user".to_string(),
        content: Arc::new(PromptAstSimple::String("hello".to_string())),
        metadata: Default::default(),
    });

    let result = engine
        .call_function(
            "user.prompt_text",
            vec![prompt_instance(ast)],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("PromptAst.text should accept a round-tripped prompt handle");

    assert_eq!(result, BexExternalValue::String("[user]\nhello".into()));
}
