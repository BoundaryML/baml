//! Parity with upstream `aws-smithy-eventstream` 0.60.11 on its own test
//! vectors (`test_data/`, see `test_data/NOTICE.md`): the assertions mirror
//! the `read_all_headers_and_payload`, `write_all_headers_and_payload` and
//! `invalid_messages` tests in upstream `src/frame.rs`.

use aws_eventstream::frame::{
    ErrorKind, Header, HeaderValue, Message, MessageFrameDecoder, read_message_from, write_message,
};

macro_rules! vector {
    ($name:literal) => {
        include_bytes!(concat!("../test_data/", $name)).as_slice()
    };
}

fn all_headers_message() -> Message {
    Message {
        headers: vec![
            Header::new("true", HeaderValue::Bool(true)),
            Header::new("false", HeaderValue::Bool(false)),
            Header::new("byte", HeaderValue::Byte(50)),
            Header::new("short", HeaderValue::Int16(20_000)),
            Header::new("int", HeaderValue::Int32(500_000)),
            Header::new("long", HeaderValue::Int64(50_000_000_000)),
            Header::new("bytes", HeaderValue::ByteArray(b"some bytes".to_vec())),
            Header::new("str", HeaderValue::String("some str".into())),
            // Upstream: `DateTime::from_secs(5_000_000)`.
            Header::new("time", HeaderValue::Timestamp(5_000_000_000)),
            Header::new(
                "uuid",
                HeaderValue::Uuid(0xb79b_c914_de21_4e13_b8b2_bc47_e85b_7f0b),
            ),
        ],
        payload: b"some payload".to_vec(),
    }
}

#[test]
fn reads_all_headers_and_payload() {
    let (message, len) = read_message_from(vector!("valid_with_all_headers_and_payload")).unwrap();
    assert_eq!(message, all_headers_message());
    assert_eq!(len, vector!("valid_with_all_headers_and_payload").len());
}

#[test]
fn writes_byte_identical_frames() {
    assert_eq!(
        write_message(&all_headers_message()).unwrap(),
        vector!("valid_with_all_headers_and_payload")
    );
}

#[test]
fn reads_empty_payload_and_no_headers() {
    let (message, _) = read_message_from(vector!("valid_empty_payload")).unwrap();
    assert!(message.payload.is_empty());
    let (message, _) = read_message_from(vector!("valid_no_headers")).unwrap();
    assert!(message.headers.is_empty());
    assert!(!message.payload.is_empty());
}

#[test]
fn streams_multiple_messages_at_every_chunk_size() {
    let mut stream = vector!("valid_with_all_headers_and_payload").to_vec();
    stream.extend_from_slice(vector!("valid_empty_payload"));
    for chunk_size in 1..=stream.len() {
        let mut decoder = MessageFrameDecoder::new();
        let mut messages = Vec::new();
        for chunk in stream.chunks(chunk_size) {
            messages.extend(decoder.feed(chunk).unwrap());
        }
        decoder.finish().unwrap();
        assert_eq!(messages.len(), 2, "chunk size {chunk_size}");
        assert_eq!(
            messages[0],
            all_headers_message(),
            "chunk size {chunk_size}"
        );
    }
}

#[test]
fn rejects_invalid_messages_like_upstream() {
    let cases: [(&[u8], ErrorKind); 7] = [
        (
            vector!("invalid_header_string_value_length"),
            ErrorKind::InvalidHeaderValue,
        ),
        (
            vector!("invalid_header_string_length_cut_off"),
            ErrorKind::InvalidHeaderValue,
        ),
        (
            vector!("invalid_header_value_type"),
            ErrorKind::InvalidHeaderValueType(0x60),
        ),
        (
            vector!("invalid_header_name_length"),
            ErrorKind::InvalidHeaderNameLength,
        ),
        (
            vector!("invalid_headers_length"),
            ErrorKind::InvalidHeadersLength,
        ),
        (
            vector!("invalid_prelude_checksum"),
            ErrorKind::PreludeChecksumMismatch(0x8BB4_95FB, 0xDEAD_BEEF),
        ),
        (
            vector!("invalid_message_checksum"),
            ErrorKind::MessageChecksumMismatch(0x01A0_5860, 0xDEAD_BEEF),
        ),
    ];
    for (bytes, expected) in cases {
        let err = read_message_from(bytes).unwrap_err();
        assert_eq!(err.kind(), &expected, "{err}");
    }
}

/// The one deliberate divergence. This 123-byte vector declares a 93-byte
/// frame whose first header name claims 102 bytes. Upstream's one-shot
/// `read_message_from` reads headers from ALL remaining input, so the name runs
/// past the frame into the trailing bytes and fails as invalid UTF-8. The fork
/// bounds every read to the declared frame, as upstream's own streaming
/// `MessageFrameDecoder` does (it hands `read_message_from` a
/// `take(remaining_len)` buffer), so the name is simply too long for the frame.
#[test]
fn bounds_header_reads_to_the_frame() {
    let err = read_message_from(vector!("invalid_header_name_length_too_long")).unwrap_err();
    assert_eq!(err.kind(), &ErrorKind::InvalidHeaderNameLength, "{err}");
}
