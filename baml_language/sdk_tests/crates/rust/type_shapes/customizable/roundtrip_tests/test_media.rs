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
    Media, return_audio_sync, return_image_sync, return_pdf_sync, return_video_sync,
    round_trip_audio_sync, round_trip_image_sync, round_trip_media_sync, round_trip_pdf_sync,
    round_trip_video_sync,
};

const URL: &str = "https://example.com/asset";

// --- decode path (return_*) works -----------------------------------------

#[test]
fn test_media_return_image() {
    // DIVERGENCE(rust): python asserts `is not None`; the successful unwrap
    // of the non-optional media result is that assertion here (and in every
    // test below).
    return_image_sync(URL.to_string(), None).unwrap();
}

#[test]
fn test_media_return_audio() {
    return_audio_sync(URL.to_string(), None).unwrap();
}

#[test]
fn test_media_return_video() {
    return_video_sync(URL.to_string(), None).unwrap();
}

#[test]
fn test_media_return_pdf() {
    return_pdf_sync(URL.to_string(), None).unwrap();
}

// --- encode path (round_trip_*) ------------------------------------------

#[test]
fn test_media_round_trip_image() {
    let img = return_image_sync(URL.to_string(), None).unwrap();
    round_trip_image_sync(img).unwrap();
}

#[test]
fn test_media_round_trip_audio() {
    let aud = return_audio_sync(URL.to_string(), None).unwrap();
    round_trip_audio_sync(aud).unwrap();
}

#[test]
fn test_media_round_trip_video() {
    let vid = return_video_sync(URL.to_string(), None).unwrap();
    round_trip_video_sync(vid).unwrap();
}

#[test]
fn test_media_round_trip_pdf() {
    let pdf = return_pdf_sync(URL.to_string(), None).unwrap();
    round_trip_pdf_sync(pdf).unwrap();
}

#[test]
fn test_media_round_trip_media() {
    let m = Media {
        image_field: return_image_sync(URL.to_string(), None).unwrap(),
        audio_field: return_audio_sync(URL.to_string(), None).unwrap(),
        video_field: return_video_sync(URL.to_string(), None).unwrap(),
        pdf_field: return_pdf_sync(URL.to_string(), None).unwrap(),
    };
    round_trip_media_sync(m).unwrap();
}
