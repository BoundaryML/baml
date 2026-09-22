//! Native cloud publisher and bounded delivery for Btel recordings.
//!
//! The JSON prepare protocol and protobuf upload envelope are a proposed BCS
//! contract. Server interoperability requires a matching BCS implementation.

#![cfg(not(target_arch = "wasm32"))]

pub mod delivery;
pub mod liveness;
mod plan;
pub mod publisher;
pub mod wire;

pub use publisher::{CloudPublisher, PublisherConfig};

pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/btel.cloud.v1.rs"));
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::proto::CloudUploadEnvelope;

    #[test]
    fn envelope_wire_tags_preserve_opaque_recording_bytes() {
        let envelope = CloudUploadEnvelope {
            format_version: 1,
            plan_id: "p".into(),
            upload_id: "u".into(),
            recording_file: Some(vec![0xff, 0]),
            cas_objects: vec![],
        };
        let golden = b"\x08\x01\x12\x01p\x1a\x01u\x22\x02\xff\x00";
        assert_eq!(envelope.encode_to_vec(), golden);
        assert_eq!(CloudUploadEnvelope::decode(&golden[..]).unwrap(), envelope);
    }
}
