//! VM tests for media types.

use baml_types::{BamlMediaContent, BamlMediaType, MediaBase64, MediaUrl};
use baml_vm::{ObjectIndex, Value, VmExecState};

mod common;
use common::{assert_vm_executes, assert_vm_executes_with_inspection, Program};

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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
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
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
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

#[test]
fn image_from_base64() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function image_from_base64() -> image {
                    image.from_base64("image/png", "abc==")
                }
            "#,
            function: "image_from_base64",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(40))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(40)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(40)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Image);
            assert_eq!(
                media.content,
                BamlMediaContent::Base64(MediaBase64 {
                    base64: "abc==".to_string()
                })
            );

            Ok(())
        },
    )
}

#[test]
fn pdf_from_base64() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function pdf_from_base64() -> pdf {
                    pdf.from_base64("abc==")
                }
            "#,
            function: "pdf_from_base64",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(39))),
        },
        |vm| {
            let baml_vm::Object::Media(media) = &vm.objects[ObjectIndex::from_raw(39)] else {
                panic!(
                    "expected Media, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(39)]
                );
            };

            assert_eq!(media.media_type, BamlMediaType::Pdf);
            assert_eq!(
                media.content,
                BamlMediaContent::Base64(MediaBase64 {
                    base64: "abc==".to_string()
                })
            );

            Ok(())
        },
    )
}

#[test]
fn media_is_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function media_is_url() -> bool {
                let v = video.from_url("https://example.com/video.mp4");

                v.is_url()
            }
        "#,
        function: "media_is_url",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn media_is_base64() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function media_is_base64() -> bool {
                let i = image.from_base64("image/png", "abc==");

                i.is_base64()
            }
        "#,
        function: "media_is_base64",
        expected: VmExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn media_as_url() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function media_as_url() -> string {
                    let i = image.from_url("https://example.com/image.png");

                    i.as_url()
                }
            "#,
            function: "media_as_url",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(40))),
        },
        |vm| {
            let baml_vm::Object::String(string) = &vm.objects[ObjectIndex::from_raw(40)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(40)]
                );
            };

            assert_eq!(string, "https://example.com/image.png");

            Ok(())
        },
    )
}

#[test]
fn media_as_base64() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function media_as_base64() -> string {
                    let i = image.from_base64("image/png", "abc==");

                    i.as_base64()
                }
            "#,
            function: "media_as_base64",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(41))),
        },
        |vm| {
            let baml_vm::Object::String(string) = &vm.objects[ObjectIndex::from_raw(41)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(41)]
                );
            };

            assert_eq!(string, "abc==");

            Ok(())
        },
    )
}

#[test]
fn media_as_mime() -> anyhow::Result<()> {
    assert_vm_executes_with_inspection(
        Program {
            source: r#"
                function media_as_mime() -> string {
                    let i = image.from_base64("image/png", "abc==");

                    i.mime()
                }
            "#,
            function: "media_as_mime",
            expected: VmExecState::Complete(Value::Object(ObjectIndex::from_raw(41))),
        },
        |vm| {
            let baml_vm::Object::String(string) = &vm.objects[ObjectIndex::from_raw(41)] else {
                panic!(
                    "expected String, got {:?}",
                    &vm.objects[ObjectIndex::from_raw(41)]
                );
            };

            assert_eq!(string, "image/png");

            Ok(())
        },
    )
}
