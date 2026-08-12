//! Roundtrip coverage for the `lorem` stream-type / stdlib-routing suite.
//!
//! These are the riskiest shapes in the fixture: `$stream` companion types
//! (`Resume$stream`, `Foo$stream`) are normally engine-internal *partial*
//! values. Whether a host-constructed `stream_types::*` value can be
//! encoded and round-tripped through a `$stream`-typed parameter is what
//! these tests probe.
//!
//! `baml.http.Response`-backed parameters (bare, in a list, or as a
//! `$stream`) can't be driven from pure Rust — the response body handle is
//! engine-minted — so they're omitted here; the handle round-trip is
//! covered by `test_handles.rs` instead.
//!
//! If a `$stream` probe fails with an encode/decode/type-mismatch error
//! from the bridge, that's a bridge-surface limitation rather than a
//! test-authoring bug.

// PROVISIONAL: `$stream` companion types have no Rust SDK design yet. This
// port assumes `stream_types` modules mirroring the source namespaces, with
// partial structs whose fields are all `Option`-wrapped, and one generated
// enum per union named by joining the arm type names with `Or`
// (`ResumeOrResponse`, `ResumeOrResumeStream`).
use baml_sdk::lorem::{
    Box, Resume, ResumeOrResponse, ResumeOrResumeStream, round_trip_box_of_resume_stream,
    round_trip_resume_or_http_response, round_trip_resume_or_resume_stream,
    round_trip_resume_stream, round_trip_root_foo_stream,
};
use baml_sdk::stream_types::Foo as StreamFoo;
use baml_sdk::stream_types::lorem::Resume as StreamResume;

#[test]
fn test_streams_round_trip_resume_stream() {
    let r = StreamResume {
        name: Some("ada".to_string()),
        email: None,
    };
    assert_eq!(round_trip_resume_stream(r.clone()).unwrap(), r);
}

#[test]
fn test_streams_round_trip_root_foo_stream() {
    let f = StreamFoo { v: Some(3) };
    assert_eq!(round_trip_root_foo_stream(f.clone()).unwrap(), f);
}

#[test]
fn test_streams_round_trip_box_of_resume_stream() {
    let b = Box {
        v: StreamResume {
            name: Some("grace".to_string()),
            email: None,
        },
    };
    assert_eq!(round_trip_box_of_resume_stream(b.clone()).unwrap(), b);
}

#[test]
fn test_streams_round_trip_resume_or_resume_stream() {
    // Union arm `Resume` (the non-stream side) is host-constructible.
    let r = Resume {
        name: "hopper".to_string(),
        email: None,
    };
    assert_eq!(
        round_trip_resume_or_resume_stream(ResumeOrResumeStream::Resume(r.clone())).unwrap(),
        ResumeOrResumeStream::Resume(r)
    );
}

#[test]
fn test_streams_round_trip_resume_or_http_response() {
    // Pass the `Resume` arm; the `baml.http.Response` arm isn't
    // host-constructible.
    let r = Resume {
        name: "lovelace".to_string(),
        email: Some("a@x.com".to_string()),
    };
    assert_eq!(
        round_trip_resume_or_http_response(ResumeOrResponse::Resume(r.clone())).unwrap(),
        ResumeOrResponse::Resume(r)
    );
}
