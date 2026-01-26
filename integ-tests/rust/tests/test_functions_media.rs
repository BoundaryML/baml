//! Media input tests - ported from test_functions_media_test.go
//!
//! Tests for media inputs including:
//! - Image from URL
//! - Image from base64
//! - Image list
//! - Audio from URL
//! - PDF inputs

use rust::baml_client::sync_client::B;
use rust::baml_client::{
    new_audio_from_url, new_image_from_base64, new_image_from_url, new_pdf_from_base64,
};

/// Test image from URL - Go: TestImageInputURL
#[test]
fn test_image_from_url() {
    let image = new_image_from_url(
        "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
        None,
    );

    let result = B.TestImageInput.call(&image);
    assert!(
        result.is_ok(),
        "Expected successful image input, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: Should contain words related to Shrek
    assert!(
        output.contains("green")
            || output.contains("yellow")
            || output.contains("shrek")
            || output.contains("ogre"),
        "Expected result to mention Shrek-related words, got: {}",
        output
    );
}

/// Test image from base64 - Go: TestImageInputBase64
#[test]
fn test_image_from_base64() {
    // Minimal valid PNG (1x1 transparent pixel)
    let base64_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    let image = new_image_from_base64(base64_data, Some("image/png"));

    let result = B.TestImageInput.call(&image);
    assert!(
        result.is_ok(),
        "Expected successful base64 image input, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty result for base64 image")
    assert!(
        !output.is_empty(),
        "Expected non-empty result for base64 image"
    );
}

/// Test image list - Go: TestImageListInput
#[test]
fn test_image_list() {
    let image1 = new_image_from_url(
        "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
        None,
    );
    let image2 = new_image_from_url(
        "https://www.google.com/images/branding/googlelogo/2x/googlelogo_color_92x30dp.png",
        None,
    );

    let result = B.TestImageListInput.call(&[image1, image2]);
    assert!(
        result.is_ok(),
        "Expected successful image list input, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: assert.True(...Contains "green" or "yellow")
    assert!(
        output.contains("green") || output.contains("yellow"),
        "Expected result to mention colors, got: {}",
        output
    );
}

/// Test image with Anthropic (Rust-only test)
#[test]
fn test_image_input_anthropic() {
    let image = new_image_from_url(
        "https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png",
        None,
    );

    let result = B.TestImageInputAnthropic.call(&image);
    assert!(
        result.is_ok(),
        "Expected successful Anthropic image input, got {:?}",
        result
    );
    let output = result.unwrap();
    assert!(
        !output.is_empty(),
        "Expected non-empty Anthropic image result"
    );
}

/// Test audio from URL - Go: TestAudioInputURL
#[test]
fn test_audio_from_url() {
    let audio = new_audio_from_url(
        "https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg",
        None,
    );

    let result = B.AudioInput.call(&audio);
    assert!(
        result.is_ok(),
        "Expected successful audio input, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: assert.Contains(t, resultLower, "no", "Expected different audio result")
    assert!(
        output.contains("no"),
        "Expected audio result to contain 'no', got: {}",
        output
    );
}

/// Test audio with OpenAI URL - Go: TestAudioInputOpenAIURL
#[test]
fn test_audio_openai_url() {
    let audio = new_audio_from_url(
        "https://github.com/sourcesounds/tf/raw/refs/heads/master/sound/vo/engineer_cloakedspyidentify09.mp3",
        None,
    );

    let result = B.AudioInputOpenai.call(&audio, "transcribe this");
    assert!(
        result.is_ok(),
        "Expected successful OpenAI audio input, got {:?}",
        result
    );
    let output = result.unwrap().to_lowercase();
    // Go: assert.Contains(t, resultLower, "spy", "Expected transcription to contain 'spy'")
    assert!(
        output.contains("spy"),
        "Expected transcription to contain 'spy', got: {}",
        output
    );
}

/// Test PDF from base64 - Go: TestPDFInput
#[test]
fn test_pdf_from_base64() {
    // Minimal PDF from Go test
    let base64_pdf = "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y=";

    let pdf = new_pdf_from_base64(base64_pdf, None);

    let result = B.PdfInput.call(&pdf);
    assert!(
        result.is_ok(),
        "Expected successful PDF input, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty PDF processing result")
    assert!(
        !output.is_empty(),
        "Expected non-empty PDF processing result"
    );
}

/// Test PDF with OpenAI - Go: TestPDFInputOpenAI
#[test]
fn test_pdf_openai() {
    let base64_pdf = "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y=";

    let pdf = new_pdf_from_base64(base64_pdf, None);

    let result = B.PdfInputOpenai.call(&pdf, "summarize this document");
    assert!(
        result.is_ok(),
        "Expected successful OpenAI PDF input, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty OpenAI PDF result")
    assert!(!output.is_empty(), "Expected non-empty OpenAI PDF result");
}

/// Test PDF with Vertex - Go: TestPDFInputVertex
#[test]
fn test_pdf_vertex() {
    let base64_pdf = "JVBERi0xLjQKJcOkw7zDtsOfCjIgMCBvYmoKPDwKL1R5cGUgL0NhdGFsb2cKL1BhZ2VzIDEgMCBSCj4+CmVuZG9iagoKMSAwIG9iago8PAovVHlwZSAvUGFnZXMKL0tpZHMgWzMgMCBSXQovQ291bnQgMQo+PgplbmRvYmoKCjMgMCBvYmoKPDwKL1R5cGUgL1BhZ2UKL1BhcmVudCAxIDAgUgovTWVkaWFCb3ggWzAgMCA2MTIgNzkyXQo+PgplbmRvYmoKCnhyZWYKMCA0CjAwMDAwMDAwMDAgNjU1MzUgZiAKMDAwMDAwMDAwOSAwMDAwMCBuIAowMDAwMDAwMDc0IDAwMDAwIG4gCjAwMDAwMDAxMjAgMDAwMDAgbiAKdHJhaWxlcgo8PAovU2l6ZSA0Ci9Sb290IDIgMCBSCj4+CnN0YXJ0eHJlZgoxNzgKJSVFT0Y=";

    let pdf = new_pdf_from_base64(base64_pdf, None);

    let result = B.PdfInputVertex.call(&pdf);
    assert!(
        result.is_ok(),
        "Expected successful Vertex PDF input, got {:?}",
        result
    );
    let output = result.unwrap();
    // Go: assert.NotEmpty(t, result, "Expected non-empty Vertex PDF result")
    assert!(!output.is_empty(), "Expected non-empty Vertex PDF result");
}
