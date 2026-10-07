//! A finished capture taken apart for delivery. The blobs that are one string
//! leave it, each holding its own content, so that one can be let go as soon
//! as it has been sent or is known to be stored. What stays writes the other
//! blobs, which name those strings by ID and need none of their content.
use std::io::{self, Write};

use bex_str::BexStr;

use crate::{
    Blob, BlobIndex, BlobScratch, CasId, Snapshot,
    encoding::{STRING_BLOB_HEADER_BYTES, string_blob_header},
    graph::{SnapshotRoot, SnapshotValue, StringId},
    pool::Storage,
    shape::{BlobEntry, LogicalBytesV1},
};

/// A blob that is one string, on its own: what the blob holds before the
/// string, and the string by handle. Nothing here is a copy of its content.
#[derive(Debug)]
pub struct Leaf {
    id: CasId,
    header: [u8; STRING_BLOB_HEADER_BYTES],
    content: BexStr,
    logical_bytes_v1: Option<u64>,
}
impl Leaf {
    pub fn id(&self) -> CasId {
        self.id
    }
    /// Customer content size, measured before the capture gives up strings.
    pub fn logical_bytes_v1(&self) -> Option<u64> {
        self.logical_bytes_v1
    }
    /// Exact length of what [`Self::write`] writes.
    pub fn encoded_len(&self) -> u64 {
        (STRING_BLOB_HEADER_BYTES + self.content.len()) as u64
    }
    /// The blob's bytes before its string.
    pub fn header(&self) -> &[u8] {
        &self.header
    }
    /// The string, which ends the blob.
    pub fn content(&self) -> &BexStr {
        &self.content
    }
    /// The blob's canonical bytes: its header, then its string.
    pub fn write(&self, w: &mut impl Write) -> io::Result<()> {
        w.write_all(&self.header)?;
        w.write_all(self.content.as_bytes())
    }
    /// Allocation bytes this retains: itself and its string's backing, which
    /// is charged in full even when shared. Saturates on overflow.
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.content.retained_heap_bytes().unwrap_or(usize::MAX))
    }
}

/// A capture that gave up the blobs that are one string. It writes the
/// others, and holds none of the content it gave up.
pub struct Structure(Snapshot);
impl Structure {
    /// The blob a recording references for the capture.
    pub fn root_id(&self) -> CasId {
        self.0.root_id()
    }
    /// The blobs this still writes, each after the ones it references; the
    /// root is last.
    pub fn blobs(&self) -> impl Iterator<Item = Kept<'_>> + '_ {
        self.0
            .blobs()
            .filter(|blob| !is_string(blob.entry()))
            .map(Kept)
    }
    /// The blob at `index`, which must come from [`Self::blobs`].
    pub fn blob(&self, index: BlobIndex) -> Kept<'_> {
        let blob = self.0.blob(index);
        assert!(!is_string(blob.entry()), "the capture gave this blob up");
        Kept(blob)
    }
    /// As [`Snapshot::retained_bytes`], for what is left.
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes()
    }
}

/// One blob a [`Structure`] still writes.
#[derive(Clone, Copy, Debug)]
pub struct Kept<'s>(Blob<'s>);
impl Kept<'_> {
    pub fn id(&self) -> CasId {
        self.0.id()
    }
    pub fn index(&self) -> BlobIndex {
        self.0.index()
    }
    /// Exact length of what [`Self::write`] writes.
    pub fn encoded_len(&self) -> u64 {
        self.0.encoded_len()
    }
    /// Customer content size, retained before the capture gives up strings.
    pub fn logical_bytes_v1(&self) -> Option<u64> {
        self.0.logical_bytes_v1()
    }
    /// As [`Blob::write`].
    pub fn write(&self, scratch: &mut BlobScratch, w: &mut impl Write) -> io::Result<()> {
        self.0.write(scratch, w)
    }
}

/// What [`Snapshot::split`] makes of a capture.
pub struct Split {
    /// The blobs that are one string, in the capture's blob order. They come
    /// before every blob that references them.
    pub leaves: Vec<Leaf>,
    /// The rest. `None` when the capture was one string: its storage is free
    /// already.
    pub structure: Option<Structure>,
}

/// Whether the blob is one string.
fn is_string(entry: &BlobEntry) -> bool {
    match entry.root {
        SnapshotRoot::Value(SnapshotValue::String(_)) => true,
        SnapshotRoot::Value(
            SnapshotValue::Null
            | SnapshotValue::OmittedArg
            | SnapshotValue::Bool(_)
            | SnapshotValue::Int(_)
            | SnapshotValue::Float(_)
            | SnapshotValue::Bigint(_)
            | SnapshotValue::Object(_)
            | SnapshotValue::Type(_)
            | SnapshotValue::Enum { .. }
            | SnapshotValue::Truncated(_),
        )
        | SnapshotRoot::FunctionArgs(_) => false,
    }
}

impl Snapshot {
    /// Take the capture apart: every blob that is one string becomes a
    /// [`Leaf`] holding that string, and the capture lets go of it. A string
    /// is content, never a name written in place, so nothing the capture
    /// still writes reads one it gave up.
    pub fn split(mut self) -> Split {
        // Measure while all child content is still held; delivery can release
        // string leaves before it serializes the blobs that reference them.
        for index in 0..self.0.shape.blobs.len() {
            let index = BlobIndex(u32::try_from(index).expect("bounded blob count"));
            let size = self.blob(index).logical_bytes_v1();
            self.0.shape.blobs[index.0 as usize].logical_bytes_v1 = LogicalBytesV1::Measured(size);
        }
        let Storage { graph, shape, .. } = &mut *self.0;
        let mut leaves = Vec::new();
        let mut kept = false;
        for entry in shape.blobs.iter() {
            let SnapshotRoot::Value(SnapshotValue::String(id)) = entry.root else {
                debug_assert!(!is_string(entry));
                kept = true;
                continue;
            };
            let content = std::mem::replace(&mut graph.strings[id.0 as usize], BexStr::empty());
            let len = u32::try_from(content.len())
                .unwrap_or_else(|_| unreachable!("a captured string is within the leaf limit"));
            let leaf = Leaf {
                id: entry.id,
                header: string_blob_header(entry.id, len),
                content,
                logical_bytes_v1: match entry.logical_bytes_v1 {
                    LogicalBytesV1::Measured(size) => size,
                    LogicalBytesV1::Unmeasured => unreachable!("measured before split"),
                },
            };
            debug_assert_eq!(leaf.encoded_len(), entry.encoded_len);
            leaves.push(leaf);
        }
        // Strings with equal content share a blob: let go of the others too.
        for index in 0..graph.strings.len() {
            let id = StringId(u32::try_from(index).expect("bounded strings"));
            if shape.string_home(id).is_some() {
                graph.strings[index] = BexStr::empty();
            }
        }
        Split {
            leaves,
            structure: kept.then(|| Structure(self)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        Builder, Limits, MediaSource, ShapePolicy, Shaper, SnapshotObject, SnapshotPool,
        SnapshotValue,
    };

    fn bytes_of(blob: Blob<'_>, scratch: &mut BlobScratch) -> Vec<u8> {
        let mut bytes = Vec::new();
        blob.write(scratch, &mut bytes).unwrap();
        bytes
    }

    fn object(b: &mut Builder, object: SnapshotObject) -> SnapshotValue {
        SnapshotValue::Object(b.leaves().object(object).unwrap())
    }

    /// Arguments holding every kind of blob: strings stored alone (one of
    /// them twice by separate handles, one as media content), a bigint and
    /// bytes stored alone, and the root that names them.
    fn capture(pool: &SnapshotPool, texts: &[BexStr; 3]) -> Snapshot {
        let [first, second, content] = texts;
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(baml_type::RealizedTy::Unknown);
        let again = BexStr::from(first.as_str().to_owned());
        let big = b
            .leaves()
            .bigint(&Arc::new(num_bigint::BigInt::from(1) << 4096_u32));
        let array = b.bytes(&[5; 100]);
        let array = object(&mut b, array);
        // Strings are numbered, and so stored, in this order.
        let strings = [first, second, &again].map(|text| b.leaves().string_value(text));
        let mime_type = b.leaves().label(&"image/png".into());
        let data = b.leaves().string(content).unwrap();
        let image = object(
            &mut b,
            SnapshotObject::Media {
                kind: baml_type::MediaKind::Image,
                mime_type,
                source: MediaSource::Base64 { data },
            },
        );
        let list = b.list(ty, strings.into_iter(), |_, value| value);
        let list = object(&mut b, list);
        let slots = [list, big, array, image, strings[1]];
        let root = b.arguments(slots.into_iter(), |_, slot| slot);
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        b.finish(root, &mut shaper)
    }

    fn texts() -> [BexStr; 3] {
        ["a", "b", "c"].map(|letter| BexStr::from(letter.repeat(200)))
    }

    #[test]
    fn a_split_capture_writes_every_blob_as_the_whole_one_did() {
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = capture(&pool, &texts());
        let mut scratch = BlobScratch::default();
        let whole: Vec<_> = snapshot
            .blobs()
            .map(|blob| (blob.id(), bytes_of(blob, &mut scratch)))
            .collect();
        // Three strings, a bigint, the bytes, and the root.
        assert_eq!(whole.len(), 6);
        let Split { leaves, structure } = snapshot.split();
        let structure = structure.unwrap();
        assert_eq!(leaves.len(), 3);
        let mut parts = Vec::new();
        for leaf in &leaves {
            let mut bytes = Vec::new();
            leaf.write(&mut bytes).unwrap();
            assert_eq!(leaf.encoded_len(), bytes.len() as u64);
            assert_eq!(bytes, [leaf.header(), leaf.content().as_bytes()].concat());
            parts.push((leaf.id(), bytes));
        }
        for blob in structure.blobs() {
            let mut bytes = Vec::new();
            blob.write(&mut scratch, &mut bytes).unwrap();
            assert_eq!(blob.encoded_len(), bytes.len() as u64);
            assert_eq!(structure.blob(blob.index()).id(), blob.id());
            parts.push((blob.id(), bytes));
        }
        // The same blobs in the same order: strings first, then the rest,
        // which includes a media object that writes its content's length.
        assert_eq!(parts, whole);
        assert_eq!(structure.root_id(), whole.last().unwrap().0);
        for (_, bytes) in &parts {
            crate::decode_blob(bytes, &crate::DecodeLimits::default()).unwrap();
        }
    }

    #[test]
    fn a_leaf_holds_its_string_and_the_rest_of_the_capture_none_of_it() {
        let pool = SnapshotPool::new(1, Limits::default());
        let texts = texts();
        let backing = texts.each_ref().map(|text| match text {
            BexStr::Flat(flat) => Arc::downgrade(flat),
            other => panic!("expected a heap-backed string, got {other:?}"),
        });
        let Split { leaves, structure } = capture(&pool, &texts).split();
        drop(texts);
        let structure = structure.unwrap();
        // Each leaf is the handle that was captured, not a copy.
        for (leaf, backing) in leaves.iter().zip(&backing) {
            let BexStr::Flat(held) = leaf.content() else {
                panic!("expected the captured handle")
            };
            assert!(Arc::ptr_eq(held, &backing.upgrade().unwrap()));
            assert!(leaf.retained_bytes() > 200);
        }
        // Letting a leaf go frees its string while the capture is held,
        // including the equal string captured through a second handle.
        for (leaf, backing) in leaves.into_iter().zip(&backing) {
            drop(leaf);
            assert!(backing.upgrade().is_none());
        }
        assert_eq!(pool.stats().in_use, 1);
        assert!(structure.retained_bytes() > 0);
        drop(structure);
        assert_eq!(pool.stats().in_use, 0);
    }

    #[test]
    fn a_capture_that_is_one_string_leaves_nothing_behind() {
        let pool = SnapshotPool::new(1, Limits::default());
        for text in ["short", &"long".repeat(1000)] {
            let text = BexStr::from(text);
            let mut b = pool.try_acquire().unwrap();
            let value = b.leaves().string_value(&text);
            let snapshot = b.finish(value, &mut Shaper::default());
            let id = snapshot.root_id();
            let whole = bytes_of(snapshot.root_blob(), &mut BlobScratch::default());
            let Split { leaves, structure } = snapshot.split();
            assert!(structure.is_none());
            assert_eq!(pool.stats().in_use, 0, "the storage is free at once");
            let [leaf] = leaves.as_slice() else {
                panic!("one string is one leaf")
            };
            assert_eq!(leaf.id(), id);
            let mut bytes = Vec::new();
            leaf.write(&mut bytes).unwrap();
            assert_eq!(bytes, whole);
            assert_eq!(leaf.content().as_str(), text.as_str());
        }
    }

    #[test]
    #[should_panic(expected = "the capture gave this blob up")]
    fn a_structure_does_not_write_a_blob_it_gave_up() {
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = capture(&pool, &texts());
        let first = snapshot.blobs().next().unwrap().index();
        let structure = snapshot.split().structure.unwrap();
        structure.blob(first);
    }
}
