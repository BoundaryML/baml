//! VM tests for media types.

use baml_types::{BamlMedia, BamlMediaType};

mod common;
use common::{assert_vm_executes, ExecState, Program, Value};

use crate::common::Object;

#[test]
fn image_from_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function image_from_url() -> image {
                image.from_url("https://example.com/image.png")
            }
        "#,
        function: "image_from_url",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::url(
            BamlMediaType::Image,
            "https://example.com/image.png".to_string(),
            None,
        )))),
    })
}

#[test]
fn audio_from_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function audio_from_url() -> audio {
                audio.from_url("https://example.com/audio.mp3")
            }
        "#,
        function: "audio_from_url",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::url(
            BamlMediaType::Audio,
            "https://example.com/audio.mp3".to_string(),
            None,
        )))),
    })
}

#[test]
fn video_from_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function video_from_url() -> video {
                video.from_url("https://example.com/video.mp4")
            }
        "#,
        function: "video_from_url",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::url(
            BamlMediaType::Video,
            "https://example.com/video.mp4".to_string(),
            None,
        )))),
    })
}

#[test]
fn pdf_from_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function pdf_from_url() -> pdf {
                pdf.from_url("https://example.com/pdf.pdf")
            }
        "#,
        function: "pdf_from_url",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::url(
            BamlMediaType::Pdf,
            "https://example.com/pdf.pdf".to_string(),
            None,
        )))),
    })
}

#[test]
fn image_from_base64() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function image_from_base64() -> image {
                image.from_base64("image/png", "abc==")
            }
        "#,
        function: "image_from_base64",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::base64(
            BamlMediaType::Image,
            "abc==".to_string(),
            Some("image/png".to_string()),
        )))),
    })
}

#[test]
fn pdf_from_base64() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function pdf_from_base64() -> pdf {
                pdf.from_base64("abc==")
            }
        "#,
        function: "pdf_from_base64",
        expected: ExecState::Complete(Value::Object(Object::Media(BamlMedia::base64(
            BamlMediaType::Pdf,
            "abc==".to_string(),
            Some("application/pdf".to_string()),
        )))),
    })
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
        expected: ExecState::Complete(Value::Bool(true)),
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
        expected: ExecState::Complete(Value::Bool(true)),
    })
}

#[test]
fn media_as_url() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
                function media_as_url() -> string {
                    let i = image.from_url("https://example.com/image.png");

                    i.as_url()
                }
            "#,
        function: "media_as_url",
        expected: ExecState::Complete(Value::string("https://example.com/image.png")),
    })
}

#[test]
fn media_as_base64() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function media_as_base64() -> string {
                let i = image.from_base64("image/png", "abc==");

                i.as_base64()
            }
        "#,
        function: "media_as_base64",
        expected: ExecState::Complete(Value::string("abc==")),
    })
}

#[test]
fn media_as_mime() -> anyhow::Result<()> {
    assert_vm_executes(Program {
        source: r#"
            function media_as_mime() -> string {
                let i = image.from_base64("image/png", "abc==");

                i.mime()
            }
        "#,
        function: "media_as_mime",
        expected: ExecState::Complete(Value::string("image/png")),
    })
}
