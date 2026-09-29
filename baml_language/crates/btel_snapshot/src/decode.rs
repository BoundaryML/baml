//! Bounded reader for CAS blob format 3 with identity verification.
//!
//! Decoding produces an owned graph: object references stay numbered, so
//! shared objects and cycles are preserved rather than expanded. Every length
//! is checked against the remaining input and the decode budget before any
//! allocation. The blob ID is recomputed with hash format 3 while reading and
//! must equal the header's ID; the header alone proves nothing. Objects must be
//! numbered in first-reference order, and every object and child blob must be
//! referenced, so each blob has exactly one accepted encoding.
//!
//! Only the current format is read. Earlier formats are rejected by version:
//! format 1 types carry attributes that no longer exist, and format 2 numbers
//! objects in capture discovery order and hashes a whole capture as one graph.
//! Their blobs live under separate `cas/v1` and `cas/v2` directories that the
//! reader never looks in.
//!
//! Type metadata uses the derived Borsh representation, which recurses once
//! per nesting level (measured with attribute-free types: at most 4 KiB of
//! stack per level in debug builds, ~1 KiB in release). Every level consumes
//! at least one byte, so a byte budget bounds depth. Descriptions of at most `SHALLOW_TYPE_BYTES` decode in
//! place and stay available; larger ones (up to `max_type_bytes`) are only
//! measured, on a helper thread with a large stack, and kept as verified bytes.
//! A blob starts that thread at its first large description and reuses it.
use std::{
    io,
    sync::{Arc, mpsc},
    thread::{Scope, ScopedJoinHandle},
};

use baml_type::{DeclarationName, TaggedTypeName, typetag::TypeTag};
use borsh::BorshDeserialize;
use num_bigint::{BigInt, BigUint, Sign};

use crate::{
    CasId, Description, Limit, OwnedType, TypeIdentity,
    hash::{Absorb, Digest, Hasher},
    tags::HashDomain,
};

/// Type descriptions this size or smaller decode on the caller's stack.
pub const SHALLOW_TYPE_BYTES: usize = 128;
/// Stack for measuring larger descriptions: `max_type_bytes` levels at the
/// debug-build cost, with headroom.
const TYPE_HELPER_STACK: usize = 64 << 20;

/// A recorded type description: always its verified encoding, plus the
/// decoded type when it is shallow enough to use safely.
#[derive(Clone, Debug, PartialEq)]
pub struct TypeDescription {
    pub encoded: Box<[u8]>,
    pub decoded: Option<Box<OwnedType>>,
}

/// Upper bounds for one blob. Exceeding one yields `BlobError::Limit`, never
/// a partial graph.
#[derive(Clone, Copy, Debug)]
pub struct DecodeLimits {
    /// Values plus map/instance entries across the whole blob.
    pub max_values: usize,
    pub max_objects: usize,
    /// Approximate owned bytes: arena slots plus string/byte/bigint contents.
    pub max_decoded_bytes: usize,
    /// Encoded size of one type description (also its nesting bound).
    pub max_type_bytes: usize,
}
impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_values: 4 << 20,
            max_objects: 1 << 20,
            max_decoded_bytes: 256 << 20,
            max_type_bytes: 4096,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlobError {
    /// Not a BTEL CAS blob.
    Magic,
    /// A blob format this decoder does not implement.
    Version(u32),
    /// Input ended inside a field.
    Truncated,
    /// Structurally invalid content (bad tag, reference, UTF-8, ...).
    Invalid(String),
    /// A decode limit was reached.
    Limit(&'static str),
    /// Content does not hash to the declared blob ID.
    IdMismatch { declared: CasId, computed: CasId },
}
impl std::fmt::Display for BlobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => f.write_str("not a CAS blob"),
            Self::Version(v) => write!(f, "unsupported CAS blob version {v}"),
            Self::Truncated => f.write_str("truncated CAS blob"),
            Self::Invalid(reason) => write!(f, "invalid CAS blob: {reason}"),
            Self::Limit(what) => write!(f, "CAS blob exceeds decoder limit: {what}"),
            Self::IdMismatch { .. } => f.write_str("CAS blob content does not match its ID"),
        }
    }
}
impl std::error::Error for BlobError {}

/// A reference to an object within one blob: its position in that blob's
/// first-reference order. Meaningless outside the blob that contains it; other
/// blobs are named by [`CasId`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedValue {
    Null,
    OmittedArg,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Box<str>),
    Bigint(Box<BigInt>),
    Object(NodeId),
    Type(Box<TypeDescription>),
    Enum {
        declaration: NodeId,
        variant: u32,
        name: Box<str>,
    },
    Truncated(Limit),
}

/// Map entries or instance fields, in captured order.
pub type Entries = Vec<(Box<str>, DecodedValue)>;

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedObject {
    Uint8Array {
        data: Vec<u8>,
        original_len: u64,
    },
    List {
        element_type: TypeDescription,
        items: Vec<DecodedValue>,
        original_len: u64,
    },
    Map {
        key_type: TypeDescription,
        value_type: TypeDescription,
        entries: Entries,
        original_len: u64,
    },
    Instance {
        type_arguments: Vec<TypeDescription>,
        declaration: NodeId,
        fields: Entries,
        original_len: u64,
    },
    Declaration {
        name: DecodedName,
        tag: TypeTag,
        is_enum: bool,
    },
    Cell(DecodedValue),
    NonSnapshotable,
    Descriptive {
        kind: Description,
        name: Option<Box<str>>,
    },
    Truncated(Limit),
}

/// A declaration's recorded name. Equality compares spellings; identity is
/// the declaration's type tag.
#[derive(Clone, Debug)]
pub struct DecodedName(pub DeclarationName);
impl PartialEq for DecodedName {
    fn eq(&self, other: &Self) -> bool {
        match (self.0.declared(), other.0.declared()) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.0.item_name() == other.0.item_name(),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedRoot {
    Value(DecodedValue),
    FunctionArgs {
        parameter_count: u64,
        slots: Vec<DecodedValue>,
    },
}

/// One owned, verified blob. `children` lists the blobs its values
/// reference, in first-use order.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedSnapshot {
    pub id: CasId,
    pub children: Vec<CasId>,
    pub root: DecodedRoot,
    pub objects: Vec<DecodedObject>,
}
impl DecodedSnapshot {
    pub fn object(&self, id: NodeId) -> &DecodedObject {
        &self.objects[id.0 as usize]
    }
}

/// Decode and verify one blob. `bytes` is the complete file contents.
pub fn decode_blob(bytes: &[u8], limits: &DecodeLimits) -> Result<DecodedSnapshot, BlobError> {
    std::thread::scope(|scope| {
        let mut r = Reader {
            input: bytes,
            limits,
            values: 0,
            decoded: 0,
            objects: 0,
            referenced: 0,
            children: 0,
            children_referenced: 0,
            deep: DeepTypes {
                scope,
                helper: None,
            },
        };
        let decoded = decode(&mut r);
        r.deep.finish();
        decoded
    })
}

fn decode(r: &mut Reader<'_, '_>) -> Result<DecodedSnapshot, BlobError> {
    if r.take(8).map_err(|_| BlobError::Magic)? != crate::BLOB_MAGIC {
        return Err(BlobError::Magic);
    }
    let version = r.u32()?;
    if version != crate::BLOB_VERSION {
        return Err(BlobError::Version(version));
    }
    let declared = CasId::from_bytes(r.take(16)?.try_into().expect("16 bytes"));
    let mut blob_hash = Hasher::new(HashDomain::Blob);
    let child_count = r.u32()?;
    if child_count as usize > r.input.len() / 16 {
        return Err(BlobError::Truncated);
    }
    r.charge(child_count as usize * std::mem::size_of::<CasId>())?;
    blob_hash.number(u64::from(child_count));
    let mut children = Vec::with_capacity(child_count as usize);
    for _ in 0..child_count {
        let child = CasId::from_bytes(r.take(16)?.try_into().expect("16 bytes"));
        blob_hash.absorb(child.as_bytes());
        children.push(child);
    }
    // Sorted, so that a long table cannot make the check quadratic.
    let mut sorted: Vec<_> = children.iter().map(CasId::as_bytes).collect();
    sorted.sort_unstable();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(invalid("duplicate child blob".into()));
    }
    r.children = child_count;
    let object_count = r.u32()?;
    // Each object definition occupies at least one byte.
    if object_count as usize > r.input.len() {
        return Err(BlobError::Truncated);
    }
    if object_count as usize > r.limits.max_objects {
        return Err(BlobError::Limit("objects"));
    }
    r.objects = object_count;
    r.charge(object_count as usize * std::mem::size_of::<DecodedObject>())?;
    blob_hash.number(u64::from(object_count));

    let root = match r.u8()? {
        0 => {
            blob_hash.byte(0);
            let value = r.value(&mut blob_hash)?;
            DecodedRoot::Value(value)
        }
        1 => {
            let parameter_count = r.u64()?;
            let (slots, digest) = r.values()?;
            blob_hash.byte(1);
            blob_hash.number(parameter_count);
            range(&mut blob_hash, slots.len(), digest);
            DecodedRoot::FunctionArgs {
                parameter_count,
                slots,
            }
        }
        tag => return Err(invalid(format!("root tag {tag}"))),
    };
    let mut objects = Vec::with_capacity(object_count as usize);
    for number in 0..object_count {
        // First-reference order: every object is referenced before (in byte
        // order) its definition, so the definitions cover exactly the
        // referenced objects.
        if number >= r.referenced {
            return Err(invalid(format!(
                "object {number} is not referenced before its definition"
            )));
        }
        let (object, digest) = r.object()?;
        blob_hash.digest(digest);
        objects.push(object);
    }
    if !r.input.is_empty() {
        return Err(invalid("trailing bytes".into()));
    }
    if r.children_referenced != r.children {
        return Err(invalid("unreferenced child blob".into()));
    }
    let computed = CasId::from_bytes(blob_hash.finish().0);
    if computed != declared {
        return Err(BlobError::IdMismatch { declared, computed });
    }
    let snapshot = DecodedSnapshot {
        id: declared,
        children,
        root,
        objects,
    };
    validate_references(&snapshot)?;
    Ok(snapshot)
}

fn invalid(reason: String) -> BlobError {
    BlobError::Invalid(reason)
}

fn range(h: &mut Hasher, len: usize, digest: Digest) {
    h.number(len as u64);
    h.digest(digest);
}

struct Reader<'a, 'scope> {
    input: &'a [u8],
    limits: &'a DecodeLimits,
    values: usize,
    decoded: usize,
    objects: u32,
    /// Objects referenced so far; the next new reference must be this number.
    referenced: u32,
    children: u32,
    /// Child blobs referenced so far, in the same first-use discipline.
    children_referenced: u32,
    deep: DeepTypes<'scope, 'a>,
}

/// Measures descriptions larger than `SHALLOW_TYPE_BYTES` on one thread
/// whose stack covers `max_type_bytes` nesting levels, started on first use.
struct DeepTypes<'scope, 'a> {
    scope: &'scope Scope<'scope, 'a>,
    helper: Option<TypeHelper<'scope, 'a>>,
}

struct TypeHelper<'scope, 'a> {
    jobs: mpsc::Sender<(&'a [u8], bool)>,
    results: mpsc::Receiver<Result<usize, BlobError>>,
    thread: ScopedJoinHandle<'scope, ()>,
}

impl<'a> DeepTypes<'_, 'a> {
    fn measure(&mut self, bytes: &'a [u8], limited: bool) -> Result<usize, BlobError> {
        let helper = match &mut self.helper {
            Some(helper) => helper,
            None => {
                let (jobs, job_queue) = mpsc::channel::<(&'a [u8], bool)>();
                let (answers, results) = mpsc::channel();
                let thread = std::thread::Builder::new()
                    .name("btel-type-decode".into())
                    .stack_size(TYPE_HELPER_STACK)
                    .spawn_scoped(self.scope, move || {
                        for (bytes, limited) in job_queue {
                            if answers.send(measure_type(bytes, limited)).is_err() {
                                break;
                            }
                        }
                    })
                    .map_err(|_| BlobError::Limit("type decoder thread"))?;
                #[cfg(test)]
                TYPE_HELPERS_STARTED.set(TYPE_HELPERS_STARTED.get() + 1);
                self.helper.insert(TypeHelper {
                    jobs,
                    results,
                    thread,
                })
            }
        };
        let panicked = || invalid("type description decoder panicked".into());
        helper.jobs.send((bytes, limited)).map_err(|_| panicked())?;
        helper.results.recv().map_err(|_| panicked())?
    }

    /// Stop the helper. A panic was already reported by `Self::measure`.
    fn finish(self) {
        if let Some(TypeHelper { jobs, thread, .. }) = self.helper {
            drop(jobs);
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Helper threads started by decodes on this thread.
    static TYPE_HELPERS_STARTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Encoded length of one description, found by decoding it. Runs on the
/// helper's large stack; the deep value is dropped there too.
fn measure_type(bytes: &[u8], limited: bool) -> Result<usize, BlobError> {
    let mut window = Window::new(bytes);
    match OwnedType::deserialize_reader(&mut window) {
        Ok(_) => Ok(window.used()),
        Err(_) if window.exhausted && limited => Err(BlobError::Limit("type description bytes")),
        Err(error) => Err(type_error(&error, window.exhausted)),
    }
}

impl<'a> Reader<'a, '_> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], BlobError> {
        if n > self.input.len() {
            return Err(BlobError::Truncated);
        }
        let (head, tail) = self.input.split_at(n);
        self.input = tail;
        Ok(head)
    }
    fn u8(&mut self) -> Result<u8, BlobError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, BlobError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(invalid(format!("bool byte {other}"))),
        }
    }
    fn u32(&mut self) -> Result<u32, BlobError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }
    fn u64(&mut self) -> Result<u64, BlobError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
    }
    fn charge(&mut self, bytes: usize) -> Result<(), BlobError> {
        self.decoded = self.decoded.saturating_add(bytes);
        if self.decoded > self.limits.max_decoded_bytes {
            return Err(BlobError::Limit("decoded bytes"));
        }
        Ok(())
    }
    /// A sequence length, checked against remaining input (`min_item` bytes
    /// each) and the value budget before allocating.
    fn count(&mut self, min_item: usize) -> Result<usize, BlobError> {
        let n = self.u32()? as usize;
        if n.saturating_mul(min_item) > self.input.len() {
            return Err(BlobError::Truncated);
        }
        self.values = self.values.saturating_add(n);
        if self.values > self.limits.max_values {
            return Err(BlobError::Limit("values"));
        }
        Ok(n)
    }
    /// UTF-8 text and its content hash (`BexStr::content_hash`).
    fn text(&mut self) -> Result<(Box<str>, u128), BlobError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        self.charge(len)?;
        let text = std::str::from_utf8(bytes).map_err(|_| invalid("string is not UTF-8".into()))?;
        Ok((text.into(), xxhash_rust::xxh3::xxh3_128(bytes)))
    }
    /// UTF-8 text and its string-leaf digest.
    fn string(&mut self) -> Result<(Box<str>, Digest), BlobError> {
        let (text, content) = self.text()?;
        let mut h = Hasher::new(HashDomain::String);
        h.string_parts(text.len(), content);
        Ok((text, h.finish()))
    }
    /// One Borsh type description and its digest (hash of the raw bytes).
    fn ty(&mut self) -> Result<(TypeDescription, Digest), BlobError> {
        let mut shallow = Window::new(&self.input[..self.input.len().min(SHALLOW_TYPE_BYTES)]);
        let (used, decoded) = match OwnedType::deserialize_reader(&mut shallow) {
            Ok(ty) => (shallow.used(), Some(Box::new(ty))),
            Err(_) if shallow.exhausted && SHALLOW_TYPE_BYTES < self.input.len() => {
                (self.measure_type()?, None)
            }
            Err(error) => return Err(type_error(&error, shallow.exhausted)),
        };
        let raw = self.take(used)?;
        self.charge(used.saturating_mul(4))?;
        let mut h = Hasher::new(HashDomain::Type);
        h.absorb(raw);
        Ok((
            TypeDescription {
                encoded: raw.into(),
                decoded,
            },
            h.finish(),
        ))
    }
    /// Encoded length of a description larger than the shallow bound.
    fn measure_type(&mut self) -> Result<usize, BlobError> {
        let bytes = &self.input[..self.input.len().min(self.limits.max_type_bytes)];
        let limited = bytes.len() == self.limits.max_type_bytes;
        self.deep.measure(bytes, limited)
    }
    fn limit(&mut self) -> Result<Limit, BlobError> {
        Ok(match self.u8()? {
            0 => Limit::Values,
            1 => Limit::Objects,
            2 => Limit::Bytes,
            3 => Limit::Depth,
            other => return Err(invalid(format!("limit tag {other}"))),
        })
    }
    fn reference(&mut self) -> Result<NodeId, BlobError> {
        let id = self.u32()?;
        if id >= self.objects {
            return Err(invalid(format!("object reference {id} out of range")));
        }
        match id.cmp(&self.referenced) {
            std::cmp::Ordering::Less => Ok(NodeId(id)),
            std::cmp::Ordering::Equal => {
                self.referenced += 1;
                Ok(NodeId(id))
            }
            std::cmp::Ordering::Greater => Err(invalid(format!(
                "object reference {id} precedes a first reference to {}",
                self.referenced
            ))),
        }
    }
    /// One inline value, hashed into `h` exactly as hash format 3 does.
    fn value(&mut self, h: &mut Hasher) -> Result<DecodedValue, BlobError> {
        let tag = self.u8()?;
        h.byte(tag);
        Ok(match tag {
            0 => DecodedValue::Null,
            1 => DecodedValue::OmittedArg,
            2 => {
                let v = self.bool()?;
                h.byte(u8::from(v));
                DecodedValue::Bool(v)
            }
            3 => {
                let raw = self.take(8)?;
                h.absorb(raw);
                DecodedValue::Int(i64::from_le_bytes(raw.try_into().expect("8")))
            }
            4 => {
                let bits = self.u64()?;
                h.number(bits);
                DecodedValue::Float(f64::from_bits(bits))
            }
            5 => {
                let (text, digest) = self.string()?;
                h.digest(digest);
                DecodedValue::String(text)
            }
            6 => {
                let (n, digest) = self.bigint()?;
                h.digest(digest);
                DecodedValue::Bigint(Box::new(n))
            }
            7 => {
                let id = self.reference()?;
                h.number(u64::from(id.0));
                DecodedValue::Object(id)
            }
            8 => {
                let (ty, digest) = self.ty()?;
                h.digest(digest);
                DecodedValue::Type(Box::new(ty))
            }
            9 => {
                let declaration = self.reference()?;
                let variant = self.u32()?;
                let (name, digest) = self.string()?;
                h.number(u64::from(declaration.0));
                h.number(u64::from(variant));
                h.digest(digest);
                DecodedValue::Enum {
                    declaration,
                    variant,
                    name,
                }
            }
            10 => {
                let limit = self.limit()?;
                h.byte(limit_tag(limit));
                DecodedValue::Truncated(limit)
            }
            other => return Err(invalid(format!("value tag {other}"))),
        })
    }
    /// A value sequence and its range digest.
    fn values(&mut self) -> Result<(Vec<DecodedValue>, Digest), BlobError> {
        let n = self.count(1)?;
        self.charge(n * std::mem::size_of::<DecodedValue>())?;
        let mut h = Hasher::new(HashDomain::Range);
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(self.value(&mut h)?);
        }
        Ok((values, h.finish()))
    }
    /// Keyed entries and their range digest.
    fn entries(&mut self) -> Result<(Entries, Digest), BlobError> {
        // Key length (4) plus a value tag (1).
        let n = self.count(5)?;
        self.charge(n * std::mem::size_of::<(Box<str>, DecodedValue)>())?;
        let mut h = Hasher::new(HashDomain::Range);
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let (key, content) = self.text()?;
            h.string_parts(key.len(), content);
            let value = self.value(&mut h)?;
            entries.push((key, value));
        }
        Ok((entries, h.finish()))
    }
    fn bigint(&mut self) -> Result<(BigInt, Digest), BlobError> {
        let sign = self.u8()?;
        let bits = self.u64()?;
        let limbs =
            usize::try_from(bits.div_ceil(64)).map_err(|_| BlobError::Limit("bigint bits"))?;
        if limbs.saturating_mul(8) > self.input.len() {
            return Err(BlobError::Truncated);
        }
        self.charge(limbs * 8)?;
        let mut h = Hasher::new(HashDomain::Bigint);
        h.byte(sign);
        h.number(bits);
        let mut bytes = Vec::with_capacity(limbs * 8);
        for _ in 0..limbs {
            let limb = self.u64()?;
            h.number(limb);
            bytes.extend_from_slice(&limb.to_le_bytes());
        }
        let magnitude = BigUint::from_bytes_le(&bytes);
        if magnitude.bits() != bits {
            return Err(invalid("bigint bit length".into()));
        }
        let sign = match (sign, bits) {
            (1, 0) => Sign::NoSign,
            (0, 1..) => Sign::Minus,
            (2, 1..) => Sign::Plus,
            _ => return Err(invalid("bigint sign".into())),
        };
        Ok((BigInt::from_biguint(sign, magnitude), h.finish()))
    }
    fn types(&mut self) -> Result<(Vec<TypeDescription>, Digest), BlobError> {
        let n = self.count(1)?;
        self.charge(n * std::mem::size_of::<TypeDescription>())?;
        let mut h = Hasher::new(HashDomain::Range);
        let mut types = Vec::with_capacity(n);
        for _ in 0..n {
            let (ty, digest) = self.ty()?;
            h.digest(digest);
            types.push(ty);
        }
        Ok((types, h.finish()))
    }
    fn object(&mut self) -> Result<(DecodedObject, Digest), BlobError> {
        let tag = self.u8()?;
        let mut h = Hasher::new(HashDomain::Object);
        h.byte(tag);
        let object = match tag {
            0 => {
                let original_len = self.u64()?;
                let len = self.u32()? as usize;
                let data = self.take(len)?;
                self.charge(len)?;
                let mut digest = Hasher::new(HashDomain::Uint8Array);
                digest.absorb(data);
                h.number(original_len);
                range(&mut h, len, digest.finish());
                DecodedObject::Uint8Array {
                    data: data.to_vec(),
                    original_len,
                }
            }
            1 => {
                let (element_type, type_digest) = self.ty()?;
                let original_len = self.u64()?;
                let (items, digest) = self.values()?;
                h.digest(type_digest);
                h.number(original_len);
                range(&mut h, items.len(), digest);
                DecodedObject::List {
                    element_type,
                    items,
                    original_len,
                }
            }
            2 => {
                let (key_type, key_digest) = self.ty()?;
                let (value_type, value_digest) = self.ty()?;
                let original_len = self.u64()?;
                let (entries, digest) = self.entries()?;
                h.digest(key_digest);
                h.digest(value_digest);
                h.number(original_len);
                range(&mut h, entries.len(), digest);
                DecodedObject::Map {
                    key_type,
                    value_type,
                    entries,
                    original_len,
                }
            }
            3 => {
                let (type_arguments, types_digest) = self.types()?;
                let declaration = self.reference()?;
                let original_len = self.u64()?;
                let (fields, digest) = self.entries()?;
                range(&mut h, type_arguments.len(), types_digest);
                h.number(u64::from(declaration.0));
                h.number(original_len);
                range(&mut h, fields.len(), digest);
                DecodedObject::Instance {
                    type_arguments,
                    declaration,
                    fields,
                    original_len,
                }
            }
            4 => {
                let mut window = Window::new(self.input);
                let decoded = TypeTag::deserialize_reader(&mut window)
                    .and_then(|tag| Ok((tag, DeclarationName::deserialize_reader(&mut window)?)));
                let (tag, name) = decoded.map_err(|error| {
                    if window.exhausted {
                        BlobError::Truncated
                    } else {
                        invalid(format!("declaration: {error}"))
                    }
                })?;
                let used = window.used();
                h.absorb(self.take(used)?);
                let is_enum = self.bool()?;
                h.byte(u8::from(is_enum));
                self.charge(used * 4)?;
                DecodedObject::Declaration {
                    name: DecodedName(name),
                    tag,
                    is_enum,
                }
            }
            5 => DecodedObject::Cell(self.value(&mut h)?),
            6 => DecodedObject::NonSnapshotable,
            7 => {
                let kind = description(self.u8()?)?;
                h.byte(description_tag(kind));
                let has_name = self.bool()?;
                h.byte(u8::from(has_name));
                let name = if has_name {
                    let (name, digest) = self.string()?;
                    h.digest(digest);
                    Some(name)
                } else {
                    None
                };
                DecodedObject::Descriptive { kind, name }
            }
            8 => {
                let limit = self.limit()?;
                h.byte(limit_tag(limit));
                DecodedObject::Truncated(limit)
            }
            other => return Err(invalid(format!("object tag {other}"))),
        };
        Ok((object, h.finish()))
    }
}

fn type_error(error: &io::Error, exhausted: bool) -> BlobError {
    if exhausted {
        BlobError::Truncated
    } else {
        invalid(format!("type description: {error}"))
    }
}

/// A byte slice reader that records running out of input. Borsh reports
/// that as `InvalidData`, indistinguishable by kind from malformed content.
struct Window<'a> {
    data: &'a [u8],
    len: usize,
    exhausted: bool,
}
impl<'a> Window<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            len: data.len(),
            exhausted: false,
        }
    }
    fn used(&self) -> usize {
        self.len - self.data.len()
    }
}
impl io::Read for Window<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.data.is_empty() && !buf.is_empty() {
            self.exhausted = true;
            return Ok(0);
        }
        let n = buf.len().min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

fn limit_tag(l: Limit) -> u8 {
    match l {
        Limit::Values => 0,
        Limit::Objects => 1,
        Limit::Bytes => 2,
        Limit::Depth => 3,
    }
}

fn description(tag: u8) -> Result<Description, BlobError> {
    Ok(match tag {
        0 => Description::Function,
        1 => Description::Closure,
        2 => Description::BoundMethod,
        3 => Description::GenericFunction,
        4 => Description::HostFunction,
        5 => Description::Future,
        6 => Description::UnscheduledFuture,
        7 => Description::Package,
        8 => Description::Interface,
        9 => Description::Implementation,
        10 => Description::TypeAlias,
        11 => Description::Sentinel,
        other => return Err(invalid(format!("description tag {other}"))),
    })
}

fn description_tag(d: Description) -> u8 {
    match d {
        Description::Function => 0,
        Description::Closure => 1,
        Description::BoundMethod => 2,
        Description::GenericFunction => 3,
        Description::HostFunction => 4,
        Description::Future => 5,
        Description::UnscheduledFuture => 6,
        Description::Package => 7,
        Description::Interface => 8,
        Description::Implementation => 9,
        Description::TypeAlias => 10,
        Description::Sentinel => 11,
    }
}

/// Declarations referenced by instances and enum values must be declaration
/// objects (or the producer's truncation marker).
fn validate_references(snapshot: &DecodedSnapshot) -> Result<(), BlobError> {
    let declaration = |id: NodeId| {
        matches!(
            snapshot.object(id),
            DecodedObject::Declaration { .. } | DecodedObject::Truncated(_)
        )
    };
    let check_value = |value: &DecodedValue| match value {
        DecodedValue::Enum { declaration: d, .. } if !declaration(*d) => Err(invalid(
            "enum value does not reference a declaration".into(),
        )),
        DecodedValue::Enum { .. }
        | DecodedValue::Null
        | DecodedValue::OmittedArg
        | DecodedValue::Bool(_)
        | DecodedValue::Int(_)
        | DecodedValue::Float(_)
        | DecodedValue::String(_)
        | DecodedValue::Bigint(_)
        | DecodedValue::Object(_)
        | DecodedValue::Type(_)
        | DecodedValue::Truncated(_) => Ok(()),
    };
    match &snapshot.root {
        DecodedRoot::Value(value) => check_value(value)?,
        DecodedRoot::FunctionArgs { slots, .. } => slots.iter().try_for_each(check_value)?,
    }
    for object in &snapshot.objects {
        match object {
            DecodedObject::List { items, .. } => items.iter().try_for_each(check_value)?,
            DecodedObject::Map { entries, .. } => {
                entries.iter().try_for_each(|(_, v)| check_value(v))?;
            }
            DecodedObject::Instance {
                declaration: d,
                fields,
                ..
            } => {
                if !declaration(*d) {
                    return Err(invalid("instance does not reference a declaration".into()));
                }
                fields.iter().try_for_each(|(_, v)| check_value(v))?;
            }
            DecodedObject::Cell(value) => check_value(value)?,
            DecodedObject::Uint8Array { .. }
            | DecodedObject::Declaration { .. }
            | DecodedObject::NonSnapshotable
            | DecodedObject::Descriptive { .. }
            | DecodedObject::Truncated(_) => {}
        }
    }
    Ok(())
}

impl BorshDeserialize for TypeIdentity {
    fn deserialize_reader<R: io::Read>(r: &mut R) -> io::Result<Self> {
        match u8::deserialize_reader(r)? {
            1 => {
                let tag = TypeTag::deserialize_reader(r)?;
                let name = DeclarationName::deserialize_reader(r)?;
                Ok(Self::Resolved(TaggedTypeName::new(tag, name)))
            }
            0 => Ok(Self::Unresolved(TypeTag::deserialize_reader(r)?)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("type identity tag {other}"),
            )),
        }
    }
}

/// Display a recorded type head by its recorded name. An unresolved head
/// has only a tag.
impl baml_type::HeadDisplay for TypeIdentity {
    fn head_display_name(&self) -> String {
        match self {
            Self::Resolved(head) => head.head_display_name(),
            Self::Unresolved(tag) => format!("<unresolved {tag:?}>"),
        }
    }
}

/// Shared decoded graphs are immutable; callers typically keep them in an
/// `Arc` for per-query memoization.
pub type SharedSnapshot = Arc<DecodedSnapshot>;

#[cfg(test)]
mod tests;
