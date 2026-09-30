//! A slim AWS event stream codec and the Bedrock `ConverseStream` event model.
//!
//! * [`frame`] reads and writes `application/vnd.amazon.eventstream` frames,
//!   one message each, validated exactly as upstream `aws-smithy-eventstream`
//!   does (see `tests/upstream_vectors.rs`), plus an incremental
//!   [`frame::MessageFrameDecoder`] for response bodies that arrive in chunks.
//! * [`converse_stream`] models every `ConverseStreamOutput` event and stream
//!   exception from `aws-sdk-bedrockruntime`, and decodes a message into one.
//!
//! Compared to `aws-smithy-eventstream` + `aws-smithy-types` +
//! `aws-sdk-bedrockruntime`, this drops the Smithy runtime, signing, and the
//! rest of the Bedrock API. The only dependencies are `crc32fast`, `serde`,
//! `serde_json` and `base64`.

pub mod converse_stream;
pub mod frame;
