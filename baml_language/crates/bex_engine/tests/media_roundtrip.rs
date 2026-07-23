//! Inbound + outbound round-trip for `BexExternalAdt::Media` values
//! through the engine.
//!
//! Pre-15d, `convert_external_to_vm_value` panicked on
//! `Adt(Media(_))`, so any inbound media argument bottomed out at
//! `bex_engine/src/conversion.rs:317-319`. This test synthesizes the
//! payload shell the Python bridge encoder produces —
//! `Instance{fields:{_data: Adt(Media(arc))}}` under a sparse exact `pdf`
//! annotation — sends it as a function arg, and asserts the engine returns the
//! canonical media value.

mod common;

use std::sync::Arc;

use baml_builtins2::{MediaContent, MediaValue};
use baml_type::MediaKind;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_external_types::BexExternalAdt;
use common::compile_for_engine;
use indexmap::IndexMap;
use sys_native::SysOpsExt;

fn pdf_media() -> Arc<MediaValue> {
    Arc::new(MediaValue::new(
        MediaKind::Pdf,
        MediaContent::Url {
            url: "https://example.test/sample.pdf".to_string(),
            base64_data: None,
        },
        Some("application/pdf".to_string()),
    ))
}

fn pdf_instance(arc: Arc<MediaValue>) -> BexExternalValue {
    let mut fields: IndexMap<String, BexExternalValue> = IndexMap::new();
    fields.insert(
        "_data".to_string(),
        BexExternalValue::Adt(BexExternalAdt::Media(arc)),
    );
    BexExternalValue::Instance {
        class_name: "baml.media.Pdf".to_string(),
        type_args: vec![],
        fields,
    }
}

/// Identity round-trip — the simplest assertion that the engine
/// accepts `Adt(Media(_))` on the inbound path and emits an
/// equivalent value on the outbound path.
#[tokio::test]
async fn media_pdf_roundtrips_through_engine() {
    let source = r#"
class Holder {
  inner pdf

  function unwrap(self) -> pdf {
    self.inner
  }
}
"#;
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );

    let original = pdf_media();
    let mut holder_fields: IndexMap<String, BexExternalValue> = IndexMap::new();
    holder_fields.insert("inner".to_string(), pdf_instance(Arc::clone(&original)));
    let holder_arg = BexExternalValue::Instance {
        class_name: "user.Holder".to_string(),
        type_args: vec![],
        fields: holder_fields,
    };

    let result = engine
        .call_function(
            "user.Holder.unwrap",
            vec![holder_arg],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("Holder.unwrap should round-trip the pdf");

    match result {
        BexExternalValue::Adt(BexExternalAdt::Media(arc)) => {
            assert_eq!(arc.kind, MediaKind::Pdf);
            arc.read_content(|content| match content {
                MediaContent::Url { url, .. } => {
                    assert_eq!(url, "https://example.test/sample.pdf");
                }
                other => panic!("expected Url media content, got {other:?}"),
            });
        }
        other => panic!("expected canonical Adt(Media(_)) for unwrapped pdf, got {other:?}"),
    }
}
