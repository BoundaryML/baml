//! Roundtrip coverage for `baml_sdk::media`.
//!
//! Values exercise both outbound media returned by BAML and Rust-side opaque
//! handles encoded through the bridge's native handle ABI.
use baml_bridge::media::{Audio, Image};
use baml_sdk::media::{
    ImageOrAudio, Media, return_audio, return_image, return_pdf, return_video, round_trip_audio,
    round_trip_image, round_trip_image_or_audio, round_trip_media, round_trip_pdf,
    round_trip_video,
};

const URL: &str = "https://example.com/asset";

// --- decode path (return_*) works -----------------------------------------

#[test]
fn test_media_return_image() {
    // DIVERGENCE(rust): python asserts `is not None`; the successful unwrap
    // of the non-optional media result is that assertion here (and in every
    // test below).
    let image = return_image(URL.to_string(), Some("image/png".to_string())).unwrap();
    assert_eq!(image.url().unwrap().as_deref(), Some(URL));
    assert_eq!(image.file().unwrap(), None);
    assert_eq!(image.base64().unwrap(), "");
    assert_eq!(image.mime_type().unwrap().as_deref(), Some("image/png"));
}

#[test]
fn test_media_return_audio() {
    return_audio(URL.to_string(), None).unwrap();
}

#[test]
fn test_media_return_video() {
    return_video(URL.to_string(), None).unwrap();
}

#[test]
fn test_media_return_pdf() {
    return_pdf(URL.to_string(), None).unwrap();
}

// --- encode path (round_trip_*) ------------------------------------------

#[test]
fn test_media_round_trip_image() {
    let path = std::env::temp_dir().join("baml-rust-sdk-media-example.png");
    let path = path.to_string_lossy().into_owned();
    let img = Image::from_file(path.clone(), Some("image/png".to_string())).unwrap();
    let returned = round_trip_image(img.clone()).unwrap();
    assert_eq!(returned.file().unwrap().as_deref(), Some(path.as_str()));
    assert_eq!(returned.mime_type().unwrap().as_deref(), Some("image/png"));

    // Encoding clones the engine handle for wire ownership. The original
    // opaque value remains live and may be inspected or sent again.
    assert_eq!(img.file().unwrap().as_deref(), Some(path.as_str()));
    round_trip_image(img).unwrap();
}

// SDK_PARITY_LINT(skip): validates the Python-compatible Rust media accessor surface
#[test]
fn test_media_base64_introspection() {
    let image = Image::from_base64("aGk=", Some("image/png".to_string())).unwrap();
    assert_eq!(image.url().unwrap(), None);
    assert_eq!(image.file().unwrap(), None);
    assert_eq!(image.base64().unwrap(), "aGk=");
    assert_eq!(image.mime_type().unwrap().as_deref(), Some("image/png"));
}

#[test]
fn test_media_round_trip_audio() {
    let aud = return_audio(URL.to_string(), None).unwrap();
    round_trip_audio(aud).unwrap();
}

#[test]
fn test_media_round_trip_video() {
    let vid = return_video(URL.to_string(), None).unwrap();
    round_trip_video(vid).unwrap();
}

#[test]
fn test_media_round_trip_pdf() {
    let pdf = return_pdf(URL.to_string(), None).unwrap();
    round_trip_pdf(pdf).unwrap();
}

#[test]
fn test_media_round_trip_media() {
    let m = Media {
        image_field: return_image(URL.to_string(), None).unwrap(),
        audio_field: return_audio(URL.to_string(), None).unwrap(),
        video_field: return_video(URL.to_string(), None).unwrap(),
        pdf_field: return_pdf(URL.to_string(), None).unwrap(),
    };
    round_trip_media(m).unwrap();
}

// SDK_PARITY_LINT(skip): validates Rust-specific generated media-union encode/decode
#[test]
fn test_media_round_trip_union() {
    let img = Image::from_url(URL, Some("image/png".to_string())).unwrap();
    let returned = round_trip_image_or_audio(ImageOrAudio::Image(img)).unwrap();
    assert!(matches!(returned, ImageOrAudio::Image(_)));

    let audio = Audio::from_url(URL, Some("audio/mpeg".to_string())).unwrap();
    let returned = round_trip_image_or_audio(ImageOrAudio::Audio(audio)).unwrap();
    assert!(matches!(returned, ImageOrAudio::Audio(_)));
}

// SDK_PARITY_LINT(skip): validates that opaque returned handles do not re-encode descriptor strings
#[test]
fn test_media_return_preserves_interior_nul() {
    let image = return_image("bad\0url".to_string(), None).unwrap();
    assert_eq!(image.url().unwrap().as_deref(), Some("bad\0url"));
}
