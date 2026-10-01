//! A slim AWS event stream codec: [`frame`] reads and writes
//! `application/vnd.amazon.eventstream` frames, one message each, validated as
//! upstream `aws-smithy-eventstream` does (see `tests/upstream_vectors.rs`),
//! plus an incremental [`frame::MessageFrameDecoder`] for response bodies that
//! arrive in chunks.
//!
//! This is everything BAML needs from `aws-smithy-eventstream` +
//! `aws-smithy-types` to read Bedrock `ConverseStream` responses; the events
//! themselves are decoded in BAML (`baml_std/aws/ns_internal/bedrock.baml`).
//! The only dependency is `crc32fast`.

pub mod frame;
