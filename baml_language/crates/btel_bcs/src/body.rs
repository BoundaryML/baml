//! Upload bodies. A body is a `btel.cloud.v1.CloudUploadEnvelope`, written
//! here field by field so that a blob's bytes go into it once, and a string's
//! content not at all: it is sent from the memory that already holds it. The
//! bytes are exactly those of encoding the whole message.
use std::sync::Arc;

use btel_snapshot::{BlobScratch, Leaf, Structure};
use bytes::Bytes;
use prost::{
    Message,
    encoding::{WireType, encode_key, encode_varint, encoded_len_varint, key_len, uint32},
};
use sha2::{Digest, Sha256};

use crate::{
    delivery::{Candidate, DeliveryError, Source},
    proto::CloudUploadEnvelope,
};

/// Zero would be left out of the message, as the protobuf encoding of a
/// default value: the version is written unconditionally here.
const BLOB_VERSION: u32 = btel_settings::snapshot::BLOB_VERSION;
const _: () = assert!(BLOB_VERSION != 0);

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
    pub(super) const LOGICAL_BYTES_APPROX_V1: u32 = 5;
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

/// The lengths of one `CasObject` field of a body, in the order written:
/// buffered before the blob's content, the content when buffered or when
/// sent from where it is held, and the digest and logical size that follow.
struct Piece {
    object_len: usize,
    blob_len: usize,
    before: usize,
    buffered: usize,
    shared: usize,
    after: usize,
    logical_bytes_approx_v1: Option<u64>,
}
impl Piece {
    fn of(candidate: &Candidate, owners: &[Arc<Structure>]) -> Result<Self, DeliveryError> {
        let logical_bytes_approx_v1 = match &candidate.source {
            Source::Leaf(leaf) => leaf.logical_bytes_approx_v1(),
            Source::Kept { owner, blob } => owners[*owner as usize]
                .blob(*blob)
                .logical_bytes_approx_v1(),
        };
        let blob_len =
            usize::try_from(candidate.encoded_len).map_err(|_| DeliveryError::Capacity)?;
        let header = match &candidate.source {
            Source::Leaf(leaf) if leaf.encoded_len() != candidate.encoded_len => {
                return Err(DeliveryError::Encoding);
            }
            Source::Leaf(leaf) => leaf.header().len(),
            Source::Kept { .. } => 0,
        };
        let shared = blob_len
            - usize::try_from(candidate.buffered_len())
                .unwrap_or_else(|_| unreachable!("part of `blob_len`"));
        let after = delimited_len(field::BLOB_SHA256, <Sha256 as Digest>::output_size())
            + logical_bytes_approx_v1.map_or(0, |size| {
                key_len(field::LOGICAL_BYTES_APPROX_V1) + encoded_len_varint(size)
            });
        let fields = delimited_len(field::SNAPSHOT_ID, candidate.id.as_bytes().len())
            + uint32::encoded_len(field::SNAPSHOT_FORMAT_VERSION, &BLOB_VERSION)
            + key_len(field::BLOB)
            + encoded_len_varint(candidate.encoded_len);
        let object_len = fields
            .checked_add(blob_len)
            .and_then(|len| len.checked_add(after))
            .ok_or(DeliveryError::Capacity)?;
        Ok(Self {
            object_len,
            blob_len,
            before: key_len(field::CAS_OBJECTS)
                + encoded_len_varint(object_len as u64)
                + fields
                + header,
            buffered: blob_len - header - shared,
            shared,
            after,
            logical_bytes_approx_v1,
        })
    }

    /// The length of the whole field.
    fn total(&self) -> usize {
        self.before + self.buffered + self.shared + self.after
    }
}

/// What is buffered in one piece of memory from the start of `pieces`: up to
/// the first content sent from where it is held, or to the end.
fn run(pieces: &[Piece]) -> usize {
    let mut len = 0;
    for piece in pieces {
        len += piece.before + piece.buffered;
        if piece.shared > 0 {
            return len;
        }
        len += piece.after;
    }
    len
}

/// A buffer for exactly `len` bytes. A length that cannot be had is an
/// error, not an abort.
fn buffer(len: usize) -> Result<Vec<u8>, DeliveryError> {
    let mut buffer = Vec::new();
    buffer
        .try_reserve_exact(len)
        .map_err(|_| DeliveryError::Capacity)?;
    Ok(buffer)
}

/// The key and length of a length-delimited field.
fn delimited(buffer: &mut Vec<u8>, tag: u32, len: usize) {
    encode_key(tag, WireType::LengthDelimited, buffer);
    encode_varint(len as u64, buffer);
}

/// Write one body: `head`, then each of `blobs` as a `CasObject`. Every
/// length is known before anything is written, so a body over its limit is
/// refused first, and each stretch of buffered bytes is allocated once.
pub(crate) fn build(
    head: &Head<'_>,
    blobs: Vec<Candidate>,
    owners: &[Arc<Structure>],
    scratch: &mut BlobScratch,
) -> Result<Body, DeliveryError> {
    // Every field before the blobs. The recording goes in by hand, so that
    // it is copied once.
    let start = CloudUploadEnvelope {
        format_version: 1,
        plan_id: head.plan_id.to_owned(),
        upload_id: head.upload_id.to_owned(),
        recording_file: None,
        cas_objects: Vec::new(),
    };
    let head_len = start.encoded_len()
        + head
            .recording_file
            .map_or(0, |file| delimited_len(field::RECORDING_FILE, file.len()));
    let pieces = blobs
        .iter()
        .map(|candidate| Piece::of(candidate, owners))
        .collect::<Result<Vec<_>, _>>()?;
    let len = pieces
        .iter()
        .try_fold(head_len, |len, piece| len.checked_add(piece.total()))
        .filter(|len| *len <= head.limit)
        .ok_or(DeliveryError::Capacity)?;
    let mut chunks = Vec::new();
    let mut buffered = buffer(head_len + run(&pieces))?;
    start
        .encode(&mut buffered)
        .map_err(|_| DeliveryError::Encoding)?;
    if let Some(file) = head.recording_file {
        delimited(&mut buffered, field::RECORDING_FILE, file.len());
        buffered.extend_from_slice(file);
    }
    for (index, (candidate, piece)) in blobs.into_iter().zip(&pieces).enumerate() {
        delimited(&mut buffered, field::CAS_OBJECTS, piece.object_len);
        delimited(
            &mut buffered,
            field::SNAPSHOT_ID,
            candidate.id.as_bytes().len(),
        );
        buffered.extend_from_slice(candidate.id.as_bytes());
        uint32::encode(field::SNAPSHOT_FORMAT_VERSION, &BLOB_VERSION, &mut buffered);
        delimited(&mut buffered, field::BLOB, piece.blob_len);
        let digest = match candidate.source {
            Source::Leaf(leaf) => {
                let digest = Sha256::new()
                    .chain_update(leaf.header())
                    .chain_update(leaf.content().as_bytes())
                    .finalize();
                buffered.extend_from_slice(leaf.header());
                if piece.shared == 0 {
                    buffered.extend_from_slice(leaf.content().as_bytes());
                } else {
                    // The string goes out from the memory that holds it,
                    // and is let go when the upload is over.
                    debug_assert_eq!(buffered.len(), buffered.capacity());
                    chunks.push(Bytes::from(buffered));
                    chunks.push(Bytes::from_owner(Content(leaf)));
                    buffered = buffer(piece.after + run(&pieces[index + 1..]))?;
                }
                digest
            }
            Source::Kept { owner, blob } => {
                let start = buffered.len();
                owners[owner as usize]
                    .blob(blob)
                    .write(scratch, &mut buffered)
                    .map_err(|_| DeliveryError::Encoding)?;
                if buffered.len() - start != piece.blob_len {
                    return Err(DeliveryError::Encoding);
                }
                Sha256::digest(&buffered[start..])
            }
        };
        // The digest follows the blob, as it does in the message.
        delimited(&mut buffered, field::BLOB_SHA256, digest.len());
        buffered.extend_from_slice(&digest);
        // Optional presence matters: a measured empty value emits zero too.
        if let Some(size) = piece.logical_bytes_approx_v1 {
            encode_key(
                field::LOGICAL_BYTES_APPROX_V1,
                WireType::Varint,
                &mut buffered,
            );
            encode_varint(size, &mut buffered);
        }
    }
    debug_assert_eq!(buffered.len(), buffered.capacity());
    if !buffered.is_empty() {
        chunks.push(Bytes::from(buffered));
    }
    debug_assert_eq!(chunks.iter().map(Bytes::len).sum::<usize>(), len);
    Ok(Body { chunks, len })
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
                    logical_bytes_approx_v1: blob.logical_bytes_approx_v1(),
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

    fn body_of(
        snapshot: Snapshot,
        recording_file: Option<&[u8]>,
        limit: usize,
    ) -> Result<Body, DeliveryError> {
        let Split { leaves, structure } = snapshot.split();
        let owners: Vec<_> = structure.into_iter().map(Arc::new).collect();
        let leaves = leaves.into_iter().map(|leaf| Candidate {
            id: leaf.id(),
            encoded_len: leaf.encoded_len(),
            source: Source::Leaf(leaf),
        });
        let kept = owners
            .iter()
            .flat_map(|owner| owner.blobs())
            .map(|blob| Candidate {
                id: blob.id(),
                encoded_len: blob.encoded_len(),
                source: Source::Kept {
                    owner: 0,
                    blob: blob.index(),
                },
            });
        let head = Head {
            plan_id: "plan",
            upload_id: "upload",
            recording_file,
            limit,
        };
        super::build(
            &head,
            leaves.chain(kept).collect(),
            &owners,
            &mut BlobScratch::default(),
        )
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
            let body = body_of(capture(&pool, &texts), recording_file, usize::MAX).unwrap();
            assert_eq!(body.len(), expected.len());
            assert_eq!(body.chunks().concat(), expected);
            let decoded = CloudUploadEnvelope::decode(body.chunks().concat().as_slice()).unwrap();
            assert_eq!(
                decoded
                    .cas_objects
                    .iter()
                    .map(|value| value.logical_bytes_approx_v1)
                    .collect::<Vec<_>>(),
                [
                    Some(2056),
                    Some(2055),
                    Some(10_008),
                    Some(308),
                    Some(14_443)
                ]
            );
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
    fn an_empty_cas_value_has_present_zero_logical_bytes() {
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish(SnapshotValue::OmittedArg, &mut Shaper::default());
        let expected = encoded_whole(&snapshot, None);
        let body = body_of(snapshot, None, usize::MAX).unwrap();
        assert_eq!(body.chunks().concat(), expected);
        let decoded = CloudUploadEnvelope::decode(expected.as_slice()).unwrap();
        assert_eq!(decoded.cas_objects.len(), 1);
        assert_eq!(decoded.cas_objects[0].logical_bytes_approx_v1, Some(0));
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
        let body = body_of(capture(&pool, &texts), None, usize::MAX).unwrap();
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
        assert!(body_of(capture(&pool, &texts), None, whole).is_ok());
        assert!(matches!(
            body_of(capture(&pool, &texts), None, whole - 1),
            Err(DeliveryError::Capacity)
        ));
        // The recording alone can be too long.
        let head = Head {
            plan_id: "plan",
            upload_id: "upload",
            recording_file: Some(&[0; 64]),
            limit: 64,
        };
        assert!(matches!(
            super::build(&head, Vec::new(), &[], &mut BlobScratch::default()),
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
