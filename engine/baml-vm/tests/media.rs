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
                function image_from_url() -> image {
                    image.from_url("https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png")
                }
            "#,
            function: "image_from_url",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(11))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(11)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(11)]
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

#[test]
fn audio_from_url() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function audio_from_url() -> audio {
                    audio.from_url("https://example.com/audio.mp3")
                }
            "#,
            function: "audio_from_url",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(11))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(11)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(11)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Audio);
            assert_eq!(
                media.content,
                BamlMediaContent::Url(MediaUrl {
                    url: "https://example.com/audio.mp3".to_string()
                })
            );

            Ok(())
        },
    )
}

#[test]
fn video_from_url() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function video_from_url() -> video {
                    video.from_url("https://example.com/video.mp4")
                }
            "#,
            function: "video_from_url",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(11))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(11)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(11)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Video);
            assert_eq!(
                media.content,
                BamlMediaContent::Url(MediaUrl {
                    url: "https://example.com/video.mp4".to_string()
                })
            );

            Ok(())
        },
    )
}

#[test]
fn pdf_from_url() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function pdf_from_url() -> pdf {
                    pdf.from_url("https://example.com/pdf.pdf")
                }
            "#,
            function: "pdf_from_url",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(11))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(11)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(11)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Pdf);
            assert_eq!(
                media.content,
                BamlMediaContent::Url(MediaUrl {
                    url: "https://example.com/pdf.pdf".to_string()
                })
            );

            Ok(())
        },
    )
}
