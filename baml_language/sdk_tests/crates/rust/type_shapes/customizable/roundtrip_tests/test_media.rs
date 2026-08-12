//! Roundtrip coverage for `baml_sdk::media`.
//!
//! Media values can't be hand-built as plain structs, so each value is
//! sourced from the matching `return_*` function (which builds it
//! engine-side via `image.from_url(...)` etc.). The *decode* path yields a
//! handle-backed media value; the *encode* path passes that value back into
//! a `round_trip_*` function.

// PROVISIONAL: media types have no Rust SDK design yet. This port assumes
// opaque handle-backed values that decode from `return_*` and encode back
// through `round_trip_*` with no host-side construction.
use baml_sdk::media::{
    Media, return_audio, return_image, return_pdf, return_video, round_trip_audio,
    round_trip_image, round_trip_media, round_trip_pdf, round_trip_video,
};

const URL: &str = "https://example.com/asset";

// --- decode path (return_*) works -----------------------------------------

#[test]
fn test_media_return_image() {
    // DIVERGENCE(rust): python asserts `is not None`; the successful unwrap
    // of the non-optional media result is that assertion here (and in every
    // test below).
    return_image(URL.to_string(), None).unwrap();
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
    let img = return_image(URL.to_string(), None).unwrap();
    round_trip_image(img).unwrap();
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
