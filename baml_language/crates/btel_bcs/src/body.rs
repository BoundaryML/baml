//! Upload bodies. A body is a `btel.cloud.v1.CloudUploadEnvelope`, written
//! here field by field so that a blob's bytes go into it once, and a string's
//! content not at all: it is sent from the memory that already holds it. The
//! bytes are exactly those of encoding the whole message.
use std::sync::Arc;

use btel_snapshot::{BlobScratch, CasId, Leaf, Structure};
use bytes::Bytes;
use prost::{
    Message,
    encoding::{WireType, encode_key, encode_varint, encoded_len_varint, key_len, uint32},
};
use sha2::{Digest, Sha256};

use crate::{
    delivery::{DeliveryError, Source},
    proto::CloudUploadEnvelope,
};

/// A string at least this long is sent from where it is held. A shorter one
/// is copied beside its framing, which costs less than a chunk of its own.
pub(crate) const SHARED_CONTENT_BYTES: usize = 2 * 1024;

/// Field numbers of `CloudUploadEnvelope` and `CasObject` in `cloud.proto`.
mod field {
    pub(super) const RECORDING_FILE: u32 = 4;
    pub(super) const CAS_OBJECTS: u32 = 5;
    pub(super) const SNAPSHOT_ID: u32 = 1;
    pub(super) const SNAPSHOT_FORMAT_VERSION: u32 = 2;
    pub(super) const BLOB: u32 = 3;
    pub(super) const BLOB_SHA256: u32 = 4;
}

/// How much of a blob's `encoded_len` bytes a body holds in a buffer of its
/// own: all of them, except the content of a string sent from where it is.
pub(crate) fn buffered_len(leaf: Option<&Leaf>, encoded_len: u64) -> u64 {
    match leaf {
        Some(leaf) if leaf.content().len() >= SHARED_CONTENT_BYTES => {
            encoded_len.saturating_sub(leaf.content().len() as u64)
        }
        Some(_) | None => encoded_len,
    }
}

/// The bytes of one upload, in the order they are sent.
pub(crate) struct Body {
    chunks: Vec<Bytes>,
    len: usize,
}
impl Body {
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    /// The chunks for one attempt. They share what the body holds: nothing
    /// is copied for a retry.
    pub(crate) fn chunks(&self) -> Vec<Bytes> {
        self.chunks.clone()
    }
}

/// A string blob's content, held for as long as the chunk that sends it.
struct Content(Leaf);
impl AsRef<[u8]> for Content {
    fn as_ref(&self) -> &[u8] {
        self.0.content().as_bytes()
    }
}

/// The length of a length-delimited field of `len` bytes.
fn delimited_len(tag: u32, len: usize) -> usize {
    key_len(tag) + encoded_len_varint(len as u64) + len
}

/// What an upload body starts with.
pub(crate) struct Head<'a> {
    pub(crate) plan_id: &'a str,
    pub(crate) upload_id: &'a str,
    pub(crate) recording_file: Option<&'a [u8]>,
    /// The most the whole body may be.
    pub(crate) limit: usize,
}

/// Writes one body. No allocation is made without asking first, so a length
/// that cannot be had is an error, not an abort.
pub(crate) struct Writer {
    chunks: Vec<Bytes>,
    /// Bytes written since the last chunk.
    buffer: Vec<u8>,
    /// The length of `chunks`.
    sent: usize,
    limit: usize,
}
impl Writer {
    pub(crate) fn new(head: &Head<'_>) -> Result<Self, DeliveryError> {
        let mut writer = Self {
            chunks: Vec::new(),
            buffer: Vec::new(),
            sent: 0,
            limit: head.limit,
        };
        // Every field before the blobs. The recording goes in by hand, so
        // that it is copied once.
        let start = CloudUploadEnvelope {
            format_version: 1,
            plan_id: head.plan_id.to_owned(),
            upload_id: head.upload_id.to_owned(),
            recording_file: None,
            cas_objects: Vec::new(),
        };
        let recording = head
            .recording_file
            .map_or(0, |file| delimited_len(field::RECORDING_FILE, file.len()));
        writer.reserve(start.encoded_len().saturating_add(recording))?;
        start
            .encode(&mut writer.buffer)
            .map_err(|_| DeliveryError::Encoding)?;
        if let Some(file) = head.recording_file {
            writer.delimited(field::RECORDING_FILE, file.len());
            writer.buffer.extend_from_slice(file);
        }
        Ok(writer)
    }

    fn len(&self) -> usize {
        self.sent + self.buffer.len()
    }

    /// Room for `additional` more bytes in the buffer, within the limit.
    fn reserve(&mut self, additional: usize) -> Result<(), DeliveryError> {
        if additional > self.limit.saturating_sub(self.len()) {
            return Err(DeliveryError::Capacity);
        }
        self.buffer
            .try_reserve_exact(additional)
            .map_err(|_| DeliveryError::Capacity)
    }

    /// The key and length of a length-delimited field.
    fn delimited(&mut self, tag: u32, len: usize) {
        encode_key(tag, WireType::LengthDelimited, &mut self.buffer);
        encode_varint(len as u64, &mut self.buffer);
    }

    /// End the buffered chunk.
    fn flush(&mut self) {
        if !self.buffer.is_empty() {
            self.sent += self.buffer.len();
            self.chunks
                .push(Bytes::from(std::mem::take(&mut self.buffer)));
        }
    }

    /// One `CasObject`: the blob `id`, which writes `encoded_len` bytes.
    pub(crate) fn blob(
        &mut self,
        id: CasId,
        encoded_len: u64,
        source: Source,
        owners: &[Arc<Structure>],
        scratch: &mut BlobScratch,
    ) -> Result<(), DeliveryError> {
        let version = btel_settings::snapshot::BLOB_VERSION;
        let blob_len = usize::try_from(encoded_len).map_err(|_| DeliveryError::Capacity)?;
        let digest_len = <Sha256 as Digest>::output_size();
        let object_len = delimited_len(field::SNAPSHOT_ID, id.as_bytes().len())
            .saturating_add(uint32::encoded_len(
                field::SNAPSHOT_FORMAT_VERSION,
                &version,
            ))
            .saturating_add(
                key_len(field::BLOB)
                    .saturating_add(encoded_len_varint(encoded_len))
                    .saturating_add(blob_len),
            )
            .saturating_add(delimited_len(field::BLOB_SHA256, digest_len));
        let total = key_len(field::CAS_OBJECTS)
            .saturating_add(encoded_len_varint(object_len as u64))
            .saturating_add(object_len);
        if total > self.limit.saturating_sub(self.len()) {
            return Err(DeliveryError::Capacity);
        }
        let buffered = match &source {
            Source::Leaf(leaf) => buffered_len(Some(leaf), encoded_len),
            Source::Kept { .. } => buffered_len(None, encoded_len),
        };
        let shared = blob_len
            - usize::try_from(buffered).unwrap_or_else(|_| unreachable!("part of `blob_len`"));
        self.reserve(total - shared)?;
        self.delimited(field::CAS_OBJECTS, object_len);
        self.delimited(field::SNAPSHOT_ID, id.as_bytes().len());
        self.buffer.extend_from_slice(id.as_bytes());
        uint32::encode(field::SNAPSHOT_FORMAT_VERSION, &version, &mut self.buffer);
        self.delimited(field::BLOB, blob_len);
        let digest = match source {
            Source::Leaf(leaf) => {
                if leaf.encoded_len() != encoded_len {
                    return Err(DeliveryError::Encoding);
                }
                let digest = Sha256::new()
                    .chain_update(leaf.header())
                    .chain_update(leaf.content().as_bytes())
                    .finalize();
                self.buffer.extend_from_slice(leaf.header());
                if shared == 0 {
                    self.buffer.extend_from_slice(leaf.content().as_bytes());
                } else {
                    // The string goes out from the memory that holds it,
                    // and is let go when the upload is over.
                    self.flush();
                    self.sent += shared;
                    self.chunks.push(Bytes::from_owner(Content(leaf)));
                }
                digest
            }
            Source::Kept { owner, blob } => {
                let start = self.buffer.len();
                owners[owner as usize]
                    .blob(blob)
                    .write(scratch, &mut self.buffer)
                    .map_err(|_| DeliveryError::Encoding)?;
                if self.buffer.len() - start != blob_len {
                    return Err(DeliveryError::Encoding);
                }
                Sha256::digest(&self.buffer[start..])
            }
        };
        // The digest follows the blob, as it does in the message.
        self.reserve(delimited_len(field::BLOB_SHA256, digest_len))?;
        self.delimited(field::BLOB_SHA256, digest_len);
        self.buffer.extend_from_slice(&digest);
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Body {
        self.flush();
        Body {
            chunks: self.chunks,
            len: self.sent,
        }
    }
}

#[cfg(test)]
mod tests {
    use btel_snapshot::{
        BexStr, Limits, ShapePolicy, Shaper, Snapshot, SnapshotPool, SnapshotValue, Split,
    };

    use super::*;
    use crate::proto::CasObject;

    /// A list of these strings, each long enough to be a blob of its own,
    /// with some bytes stored alone beside them.
    fn capture(pool: &SnapshotPool, texts: &[BexStr]) -> Snapshot {
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(btel_snapshot::OwnedType::unknown());
        let bytes = b.bytes(&[9; 300]);
        let bytes = SnapshotValue::Object(b.leaves().object(bytes).unwrap());
        let mut values: Vec<_> = texts
            .iter()
            .map(|text| b.leaves().string_value(text))
            .collect();
        values.push(bytes);
        let list = b.list(ty, values.into_iter(), |_, value| value);
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        b.finish(SnapshotValue::Object(list), &mut shaper)
    }

    /// The same envelope, encoded whole from copies of every blob.
    fn encoded_whole(snapshot: &Snapshot, recording_file: Option<&[u8]>) -> Vec<u8> {
        let mut scratch = BlobScratch::default();
        let cas_objects = snapshot
            .blobs()
            .map(|blob| {
                let mut bytes = Vec::new();
                blob.write(&mut scratch, &mut bytes).unwrap();
                CasObject {
                    snapshot_id: blob.id().as_bytes().to_vec(),
                    snapshot_format_version: btel_settings::snapshot::BLOB_VERSION,
                    blob_sha256: Sha256::digest(&bytes).to_vec(),
                    blob: bytes,
                }
            })
            .collect();
        CloudUploadEnvelope {
            format_version: 1,
            plan_id: "plan".into(),
            upload_id: "upload".into(),
            recording_file: recording_file.map(<[u8]>::to_vec),
            cas_objects,
        }
        .encode_to_vec()
    }

    fn build(
        snapshot: Snapshot,
        recording_file: Option<&[u8]>,
        limit: usize,
    ) -> Result<Body, DeliveryError> {
        let Split { leaves, structure } = snapshot.split();
        let owners: Vec<_> = structure.into_iter().map(Arc::new).collect();
        let mut writer = Writer::new(&Head {
            plan_id: "plan",
            upload_id: "upload",
            recording_file,
            limit,
        })?;
        let mut scratch = BlobScratch::default();
        for leaf in leaves {
            let (id, len) = (leaf.id(), leaf.encoded_len());
            writer.blob(id, len, Source::Leaf(leaf), &owners, &mut scratch)?;
        }
        let kept: Vec<_> = owners
            .iter()
            .flat_map(|owner| owner.blobs())
            .map(|blob| (blob.id(), blob.encoded_len(), blob.index()))
            .collect();
        for (id, len, blob) in kept {
            let source = Source::Kept { owner: 0, blob };
            writer.blob(id, len, source, &owners, &mut scratch)?;
        }
        Ok(writer.finish())
    }

    fn texts() -> Vec<BexStr> {
        vec![
            // Sent from where it is held, copied, and sent again.
            BexStr::from("a".repeat(SHARED_CONTENT_BYTES)),
            BexStr::from("b".repeat(SHARED_CONTENT_BYTES - 1)),
            BexStr::from("c".repeat(10_000)),
        ]
    }

    #[test]
    fn a_body_is_the_bytes_of_the_whole_message_and_holds_no_copy_of_a_long_string() {
        let pool = SnapshotPool::new(1, Limits::default());
        let texts = texts();
        for recording_file in [None, Some(&b"recording bytes"[..]), Some(&[][..])] {
            let expected = encoded_whole(&capture(&pool, &texts), recording_file);
            let body = build(capture(&pool, &texts), recording_file, usize::MAX).unwrap();
            assert_eq!(body.len(), expected.len());
            assert_eq!(body.chunks().concat(), expected);
            // Framing, the first string, framing with the short string
            // copied into it, the third string, and the rest.
            let chunks = body.chunks();
            assert_eq!(chunks.len(), 5);
            for (chunk, text) in [(&chunks[1], &texts[0]), (&chunks[3], &texts[2])] {
                assert_eq!(chunk.as_ptr(), text.as_bytes().as_ptr());
                assert_eq!(chunk.len(), text.len());
            }
            // A second attempt sends the same memory.
            assert_eq!(body.chunks()[3].as_ptr(), texts[2].as_bytes().as_ptr());
        }
        assert_eq!(pool.stats().in_use, 0);
    }

    #[test]
    fn a_string_is_held_until_its_body_is_dropped() {
        let pool = SnapshotPool::new(1, Limits::default());
        let texts = texts();
        let backing: Vec<_> = texts
            .iter()
            .map(|text| match text {
                BexStr::Flat(flat) => Arc::downgrade(flat),
                other => panic!("expected a heap-backed string, got {other:?}"),
            })
            .collect();
        let body = build(capture(&pool, &texts), None, usize::MAX).unwrap();
        drop(texts);
        // The capture is free once the body is written; the long strings
        // are not, and the short one was copied.
        assert_eq!(pool.stats().in_use, 0);
        let alive = |backing: &[std::sync::Weak<_>]| -> Vec<bool> {
            backing.iter().map(|b| b.upgrade().is_some()).collect()
        };
        assert_eq!(alive(&backing), [true, false, true]);
        let attempt = body.chunks();
        drop(body);
        assert_eq!(alive(&backing), [true, false, true]);
        drop(attempt);
        assert_eq!(alive(&backing), [false, false, false]);
    }

    #[test]
    fn a_body_over_its_limit_is_refused_before_anything_is_written_for_it() {
        let pool = SnapshotPool::new(1, Limits::default());
        let texts = texts();
        let whole = encoded_whole(&capture(&pool, &texts), None).len();
        assert!(build(capture(&pool, &texts), None, whole).is_ok());
        assert!(matches!(
            build(capture(&pool, &texts), None, whole - 1),
            Err(DeliveryError::Capacity)
        ));
        // The recording alone can be too long.
        assert!(matches!(
            Writer::new(&Head {
                plan_id: "plan",
                upload_id: "upload",
                recording_file: Some(&[0; 64]),
                limit: 64,
            }),
            Err(DeliveryError::Capacity)
        ));
        assert_eq!(pool.stats().in_use, 0);
    }

    #[test]
    fn only_a_long_string_is_left_out_of_the_buffered_length() {
        let pool = SnapshotPool::new(1, Limits::default());
        let Split { leaves, .. } = capture(&pool, &texts()).split();
        let buffered: Vec<_> = leaves
            .iter()
            .map(|leaf| buffered_len(Some(leaf), leaf.encoded_len()))
            .collect();
        let header = leaves[0].header().len() as u64;
        assert_eq!(
            buffered,
            [header, header + SHARED_CONTENT_BYTES as u64 - 1, header]
        );
        assert_eq!(buffered_len(None, 5000), 5000);
    }
}
