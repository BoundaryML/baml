//! VM tests for media types.

use baml_types::{BamlMediaContent, BamlMediaType, MediaUrl};
use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes_with_inspection, Program};

// Array tests
#[test]
fn image_from_url() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function ReturnImageFromUrl() -> image {
                    image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png")
                }
            "#,
            function: "ReturnImageFromUrl",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(8))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(8)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(8)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Image);
            assert_eq!(
                media.content,
                BamlMediaContent::Url(MediaUrl {
                    url: "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png"
                        .to_string()
                })
            );

            Ok(())
        },
    )
}
