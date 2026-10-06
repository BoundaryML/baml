//! Bounded reader for CAS blob formats 3, 4 and 5 with identity verification.
//!
//! Decoding produces an owned graph: object references stay numbered, so
//! shared objects and cycles are preserved rather than expanded. Every length
//! is checked against the remaining input and the decode budget before any
//! allocation. The blob ID is recomputed with its hash format while reading and
//! must equal the header's ID; the header alone proves nothing. Objects and
//! child blobs must be numbered in first-reference order, and every one of
//! them must be referenced, so a blob's numbering has exactly one accepted
//! order. Well-formed content a producer would never write, such as a byte
//! array longer than its recorded original length, is still accepted.
//!
//! A blob is verified alone: a value stored in another blob stays a reference
//! to that child, and nothing here reads the child. Leaf contents are shared
//! handles, so a decoded value is cheap to copy out of its blob.
//!
//! In format 5 a type head or declaration may name a recorded definition by
//! its group's blob ID. The groups a blob names, in the order they are first
//! named, must be exactly what its child table lists after the children its
//! values use. A definition group's blob (root tag 2) holds no objects, and
//! its child table is exactly the groups its field types name.
//!
//! Formats 3 to 5 are read. Earlier formats are rejected by version:
//! format 1 types carry attributes that no longer exist, and format 2 numbers
//! objects in capture discovery order and hashes a whole capture as one graph.
//! Their blobs live under separate `cas/v1` and `cas/v2` directories that the
//! reader never looks in.
//!
//! Type metadata uses the derived Borsh representation, which recurses once
//! per nesting level (measured with attribute-free types: at most 4 KiB of
//! stack per level in debug builds, ~1 KiB in release). Every level consumes
//! at least one byte, so a byte budget bounds depth. Descriptions of at most `SHALLOW_TYPE_BYTES` decode in
//! place and stay available; larger ones (up to `max_type_bytes`) are decoded
//! on a helper thread with a large stack and kept as verified bytes, and as
//! the decoded type too when it nests at most `SHALLOW_TYPE_BYTES` levels:
//! a long type, such as a union of classes named with their definitions, is
//! usually shallow. A blob starts that thread at its first large description
//! and reuses it.
use std::{
    io,
    sync::{Arc, mpsc},
    thread::{Scope, ScopedJoinHandle},
};

use baml_type::{DeclarationName, MediaKind, TaggedTypeName, TyTemplate, typetag::TypeTag};
use borsh::BorshDeserialize;
use num_bigint::{BigInt, BigUint, Sign};

use crate::{
    CasId, Description, Limit, OwnedType, TypeIdentity,
    definition::{self, DefinedHead, DefinitionHead, DefinitionRef, DefinitionType, Meta, Variant},
    hash::{Absorb, Digest, Hasher},
    tags::{self, HashDomain},
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

/// A blob's position in the child table of the blob that names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChildIndex(pub u32);

#[derive(Clone, Debug, PartialEq)]
pub enum DecodedValue {
    Null,
    OmittedArg,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Bigint(Arc<BigInt>),
    Object(NodeId),
    Type(Arc<TypeDescription>),
    Enum {
        declaration: NodeId,
        variant: u32,
        name: Arc<str>,
    },
    Truncated(Limit),
    /// The root value of a child blob.
    External(ChildIndex),
    /// An object of a child blob other than its root: a member of a cycle
    /// that the child holds whole.
    ExternalNode {
        child: ChildIndex,
        node: NodeId,
    },
}

/// Map keys and values, in captured order.
pub type Entries = Vec<(DecodedValue, DecodedValue)>;
/// String-named instance fields, in captured order.
pub type Fields = Vec<(Box<str>, DecodedValue)>;

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
        fields: Fields,
        original_len: u64,
    },
    /// A class or enum, identified by its recorded definition (format 5) or
    /// by its runtime tag.
    Declaration {
        name: DecodedName,
        tag: Option<TypeTag>,
        is_enum: bool,
        definition: Option<DefinitionRef>,
    },
    Cell(DecodedValue),
    NonSnapshotable,
    Descriptive {
        kind: Description,
        name: Option<Box<str>>,
    },
    Media(DecodedMedia),
    Truncated(Limit),
}

/// A media value: its kind and where its content comes from.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedMedia {
    pub kind: MediaKind,
    pub mime_type: Option<Box<str>>,
    pub source: DecodedMediaSource,
}

/// Where a media value's content comes from. `data` is content already
/// loaded from the URL or file.
#[derive(Clone, Debug, PartialEq)]
pub enum DecodedMediaSource {
    Url {
        url: Box<str>,
        data: Option<MediaPayload>,
    },
    File {
        path: Box<str>,
        data: Option<MediaPayload>,
    },
    Base64 {
        data: MediaPayload,
    },
}

impl DecodedMediaSource {
    /// The captured content, if the value holds any.
    pub fn data(&self) -> Option<&MediaPayload> {
        match self {
            Self::Url { data, .. } | Self::File { data, .. } => data.as_ref(),
            Self::Base64 { data } => Some(data),
        }
    }
}

/// Media content as base64 text.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaPayload {
    /// Stored in this blob.
    Inline(Arc<str>),
    /// The root value of a child blob: a string of `text_len` bytes. Only a
    /// reader of that child can confirm it.
    External { child: ChildIndex, text_len: u64 },
}

impl MediaPayload {
    /// The length of the base64 text, in bytes.
    pub fn text_len(&self) -> u64 {
        match self {
            Self::Inline(text) => text.len() as u64,
            Self::External { text_len, .. } => *text_len,
        }
    }
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
    /// A group of recorded definitions, by position (format 5).
    Definitions(Vec<DecodedDefinition>),
}

/// A recorded field type: always its verified encoding, plus the decoded
/// type when it is shallow enough to use safely.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldType {
    pub encoded: Box<[u8]>,
    pub decoded: Option<Box<DefinitionType>>,
}

/// One recorded class or enum definition.
pub type DecodedDefinition = definition::Declaration<FieldType>;

/// One owned, verified blob. `children` lists the blobs its values
/// reference, in first-use order.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedSnapshot {
    pub id: CasId,
    /// Bytes of the blob this was decoded from.
    pub encoded_len: u64,
    pub children: Vec<CasId>,
    pub root: DecodedRoot,
    pub objects: Vec<DecodedObject>,
}
impl DecodedSnapshot {
    pub fn object(&self, id: NodeId) -> &DecodedObject {
        &self.objects[id.0 as usize]
    }
    pub fn child(&self, index: ChildIndex) -> CasId {
        self.children[index.0 as usize]
    }
}

/// Decode and verify one blob. `bytes` is the complete file contents.
pub fn decode_blob(bytes: &[u8], limits: &DecodeLimits) -> Result<DecodedSnapshot, BlobError> {
    std::thread::scope(|scope| {
        let mut r = Reader {
            input: bytes,
            version: 0,
            limits,
            values: 0,
            decoded: 0,
            objects: 0,
            referenced: 0,
            children: 0,
            children_referenced: 0,
            named: Vec::new(),
            positions: None,
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
    let encoded_len = r.input.len() as u64;
    if r.take(8).map_err(|_| BlobError::Magic)? != crate::BLOB_MAGIC {
        return Err(BlobError::Magic);
    }
    let version = r.u32()?;
    if !btel_settings::snapshot::READABLE_BLOB_VERSIONS.contains(&version) {
        return Err(BlobError::Version(version));
    }
    r.version = version;
    let declared = CasId::from_bytes(r.take(16)?.try_into().expect("16 bytes"));
    let child_count = r.u32()?;
    if child_count as usize > r.input.len() / 16 {
        return Err(BlobError::Truncated);
    }
    r.charge(child_count as usize * std::mem::size_of::<CasId>())?;
    let mut children = Vec::with_capacity(child_count as usize);
    for _ in 0..child_count {
        let child = CasId::from_bytes(r.take(16)?.try_into().expect("16 bytes"));
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

    // One stream, fed in byte order as the hash format defines it: the root,
    // the objects, then the child table and object count.
    let mut blob_hash = Hasher::versioned(HashDomain::Blob, version);
    let root = match r.u8()? {
        0 => {
            blob_hash.byte(0);
            let value = r.value(&mut blob_hash)?;
            if matches!(
                value,
                DecodedValue::External(_) | DecodedValue::ExternalNode { .. }
            ) {
                return Err(invalid("a blob's root value is stored in it".into()));
            }
            DecodedRoot::Value(value)
        }
        1 => {
            blob_hash.byte(1);
            let parameter_count = r.u64()?;
            blob_hash.number(parameter_count);
            let slots = r.values(&mut blob_hash)?;
            DecodedRoot::FunctionArgs {
                parameter_count,
                slots,
            }
        }
        2 if version >= 5 => {
            blob_hash.byte(2);
            if object_count != 0 {
                return Err(invalid("a definition group holds no objects".into()));
            }
            // Its content holds no leaves to replace: it is hashed as written.
            let content = r.input;
            let definitions = r.definitions()?;
            blob_hash.absorb(&content[..content.len() - r.input.len()]);
            DecodedRoot::Definitions(definitions)
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
        objects.push(r.object(&mut blob_hash)?);
    }
    if !r.input.is_empty() {
        return Err(invalid("trailing bytes".into()));
    }
    // The children values use come first; the groups the blob names follow,
    // in the order they were first named.
    let mut seen = rustc_hash::FxHashSet::default();
    r.named.retain(|group| seen.insert(*group));
    if children[r.children_referenced as usize..] != r.named[..] {
        return Err(
            if r.named.len() < children.len() - r.children_referenced as usize {
                invalid("unreferenced child blob".into())
            } else {
                invalid("a named definition group is not a child".into())
            },
        );
    }
    blob_hash.size(children.len());
    for child in &children {
        blob_hash.absorb(child.as_bytes());
    }
    blob_hash.size(objects.len());
    let computed = CasId::from_bytes(blob_hash.finish().0);
    if computed != declared {
        return Err(BlobError::IdMismatch { declared, computed });
    }
    let snapshot = DecodedSnapshot {
        id: declared,
        encoded_len,
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

struct Reader<'a, 'scope> {
    input: &'a [u8],
    version: u32,
    limits: &'a DecodeLimits,
    values: usize,
    decoded: usize,
    objects: u32,
    /// Objects referenced so far; the next new reference must be this number.
    referenced: u32,
    children: u32,
    /// Child blobs referenced so far, in the same first-use discipline.
    children_referenced: u32,
    /// Definition groups named so far, in encoding order, with repeats.
    named: Vec<CasId>,
    /// While a definition group is read: its member count, which positions
    /// must be under.
    positions: Option<u32>,
    deep: DeepTypes<'scope, 'a>,
}

/// Measures descriptions larger than `SHALLOW_TYPE_BYTES` on one thread
/// whose stack covers `max_type_bytes` nesting levels, started on first use.
struct DeepTypes<'scope, 'a> {
    scope: &'scope Scope<'scope, 'a>,
    helper: Option<TypeHelper<'scope, 'a>>,
}

struct TypeHelper<'scope, 'a> {
    jobs: mpsc::Sender<(&'a [u8], bool, Described)>,
    results: mpsc::Receiver<Result<Measured, BlobError>>,
    thread: ScopedJoinHandle<'scope, ()>,
}

/// Which kind of type description is read.
#[derive(Clone, Copy, Debug)]
enum Described {
    /// A captured type: [`OwnedType`].
    Value,
    /// A recorded definition's field type: [`DefinitionType`].
    Field,
}

/// What reading a type description finds besides the type itself.
#[derive(Debug, Default)]
struct Measured {
    len: usize,
    /// Definition groups its heads name, in order.
    groups: Vec<CasId>,
    /// The highest group member position its heads name.
    position: Option<u32>,
    /// A long description's decoded type, when it is shallow enough to use
    /// and drop on any stack.
    shallow: Option<Shallow>,
}

/// A type the helper decoded that nests at most `SHALLOW_TYPE_BYTES` levels.
#[derive(Debug)]
enum Shallow {
    Value(Box<OwnedType>),
    Field(Box<DefinitionType>),
}

/// How deeply a type nests: one for a leaf.
fn depth<N: Clone>(ty: &TyTemplate<N>) -> usize {
    let deepest =
        |types: &mut dyn Iterator<Item = &TyTemplate<N>>| types.map(depth).max().unwrap_or(0);
    1 + match ty {
        TyTemplate::Class(_, args) | TyTemplate::Union(args) => deepest(&mut args.iter()),
        TyTemplate::Interface(_, args, associated) => {
            deepest(&mut args.iter().chain(associated.iter().map(|(_, ty)| ty)))
        }
        TyTemplate::List(item) => depth(item),
        TyTemplate::Map { key, value } | TyTemplate::Future(key, value) => {
            depth(key).max(depth(value))
        }
        TyTemplate::Function {
            params,
            ret,
            throws,
        } => deepest(
            &mut params
                .iter()
                .map(|param| &param.ty)
                .chain([&**ret, &**throws]),
        ),
        TyTemplate::AssociatedTypeProjection {
            base, interface, ..
        } => depth(base).max(deepest(
            &mut interface
                .generics
                .iter()
                .chain(interface.associated_types.iter().map(|(_, ty)| ty)),
        )),
        TyTemplate::Int
        | TyTemplate::Bigint
        | TyTemplate::Float
        | TyTemplate::String
        | TyTemplate::Bool
        | TyTemplate::Null
        | TyTemplate::Uint8Array
        | TyTemplate::Media(_)
        | TyTemplate::Literal(..)
        | TyTemplate::Enum(_)
        | TyTemplate::EnumVariant(..)
        | TyTemplate::RustType
        | TyTemplate::Type
        | TyTemplate::Resource
        | TyTemplate::PromptAst
        | TyTemplate::Void
        | TyTemplate::TypeAlias(_)
        | TyTemplate::Unknown
        | TyTemplate::Never
        | TyTemplate::TypeArgRef(_) => 0,
    }
}

fn value_heads(ty: &OwnedType, found: &mut Measured) {
    ty.visit_heads(&mut |head| {
        if let TypeIdentity::Defined(head) = head {
            found.groups.push(head.definition.group);
        }
    });
}

fn field_heads(ty: &DefinitionType, found: &mut Measured) {
    ty.visit_heads(&mut |head| match head {
        DefinitionHead::Defined(head) => found.groups.push(head.definition.group),
        DefinitionHead::Member(position) => {
            found.position = found.position.max(Some(*position));
        }
        DefinitionHead::Named(_) => {}
    });
}

impl<'a> DeepTypes<'_, 'a> {
    fn measure(
        &mut self,
        bytes: &'a [u8],
        limited: bool,
        described: Described,
    ) -> Result<Measured, BlobError> {
        let helper = match &mut self.helper {
            Some(helper) => helper,
            None => {
                let (jobs, job_queue) = mpsc::channel::<(&'a [u8], bool, Described)>();
                let (answers, results) = mpsc::channel();
                let thread = std::thread::Builder::new()
                    .name("btel-type-decode".into())
                    .stack_size(TYPE_HELPER_STACK)
                    .spawn_scoped(self.scope, move || {
                        for (bytes, limited, described) in job_queue {
                            let measured = measure_type(bytes, limited, described);
                            if answers.send(measured).is_err() {
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
        helper
            .jobs
            .send((bytes, limited, described))
            .map_err(|_| panicked())?;
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

/// Encoded length of one description, and the heads it names, found by
/// decoding it. Runs on the helper's large stack; the deep value is dropped
/// there too.
fn measure_type(bytes: &[u8], limited: bool, described: Described) -> Result<Measured, BlobError> {
    let mut window = Window::new(bytes);
    let mut found = Measured::default();
    let decoded = match described {
        Described::Value => OwnedType::deserialize_reader(&mut window).map(|ty| {
            value_heads(&ty, &mut found);
            if depth(&TyTemplate::from(ty.clone())) <= SHALLOW_TYPE_BYTES {
                found.shallow = Some(Shallow::Value(Box::new(ty)));
            }
        }),
        Described::Field => DefinitionType::deserialize_reader(&mut window).map(|ty| {
            field_heads(&ty, &mut found);
            if depth(&ty) <= SHALLOW_TYPE_BYTES {
                found.shallow = Some(Shallow::Field(Box::new(ty)));
            }
        }),
    };
    match decoded {
        Ok(()) => {
            found.len = window.used();
            Ok(found)
        }
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
    fn text(&mut self) -> Result<&'a str, BlobError> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        self.charge(len)?;
        std::str::from_utf8(bytes).map_err(|_| invalid("string is not UTF-8".into()))
    }
    /// UTF-8 text and its string-leaf digest, its content hash
    /// (`BexStr::content_hash`).
    fn string(&mut self) -> Result<(&'a str, Digest), BlobError> {
        let text = self.text()?;
        let digest = Digest(xxhash_rust::xxh3::xxh3_128(text.as_bytes()).to_le_bytes());
        Ok((text, digest))
    }
    /// One Borsh type description and its digest (hash of the raw bytes).
    fn ty(&mut self) -> Result<(TypeDescription, Digest), BlobError> {
        let (raw, decoded) =
            self.described::<OwnedType>(Described::Value, value_heads, |shallow| match shallow {
                Shallow::Value(ty) => Some(ty),
                Shallow::Field(_) => None,
            })?;
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
    /// One recorded field type, which the group's hash covers as written.
    fn field_type(&mut self) -> Result<FieldType, BlobError> {
        let (raw, decoded) = self.described::<DefinitionType>(
            Described::Field,
            field_heads,
            |shallow| match shallow {
                Shallow::Field(ty) => Some(ty),
                Shallow::Value(_) => None,
            },
        )?;
        Ok(FieldType {
            encoded: raw.into(),
            decoded,
        })
    }
    /// One Borsh description: its bytes, and the decoded value when it is
    /// shallow. The groups its heads name are added to those this blob
    /// names; a member position past the group being read is invalid.
    fn described<T: BorshDeserialize>(
        &mut self,
        described: Described,
        heads: fn(&T, &mut Measured),
        from_helper: fn(Shallow) -> Option<Box<T>>,
    ) -> Result<(&'a [u8], Option<Box<T>>), BlobError> {
        let mut shallow = Window::new(&self.input[..self.input.len().min(SHALLOW_TYPE_BYTES)]);
        let (found, decoded) = match T::deserialize_reader(&mut shallow) {
            Ok(ty) => {
                let mut found = Measured::default();
                heads(&ty, &mut found);
                found.len = shallow.used();
                (found, Some(Box::new(ty)))
            }
            Err(_) if shallow.exhausted && SHALLOW_TYPE_BYTES < self.input.len() => {
                let mut found = self.measure_type(described)?;
                let decoded = found.shallow.take().and_then(from_helper);
                (found, decoded)
            }
            Err(error) => return Err(type_error(&error, shallow.exhausted)),
        };
        if let Some(position) = found.position
            && self.positions.is_none_or(|members| position >= members)
        {
            return Err(invalid(format!(
                "definition member {position} out of range"
            )));
        }
        if self.version < 5 && !found.groups.is_empty() {
            return Err(invalid("a definition named before format 5".into()));
        }
        self.named.extend(found.groups);
        let raw = self.take(found.len)?;
        self.charge(found.len.saturating_mul(4))?;
        Ok((raw, decoded))
    }
    /// Encoded length and heads of a description larger than the shallow
    /// bound.
    fn measure_type(&mut self, described: Described) -> Result<Measured, BlobError> {
        let bytes = &self.input[..self.input.len().min(self.limits.max_type_bytes)];
        let limited = bytes.len() == self.limits.max_type_bytes;
        self.deep.measure(bytes, limited, described)
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
    /// A child blob named by its slot, in the same first-use discipline as
    /// object references.
    fn child(&mut self) -> Result<ChildIndex, BlobError> {
        let slot = self.u32()?;
        if slot >= self.children {
            return Err(invalid(format!("child reference {slot} out of range")));
        }
        match slot.cmp(&self.children_referenced) {
            std::cmp::Ordering::Less => Ok(ChildIndex(slot)),
            std::cmp::Ordering::Equal => {
                self.children_referenced += 1;
                Ok(ChildIndex(slot))
            }
            std::cmp::Ordering::Greater => Err(invalid(format!(
                "child reference {slot} precedes a first reference to {}",
                self.children_referenced
            ))),
        }
    }
    /// One inline value, hashed into `h` in encoding order.
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
                DecodedValue::String(text.into())
            }
            6 => {
                let (n, digest) = self.bigint()?;
                h.digest(digest);
                DecodedValue::Bigint(Arc::new(n))
            }
            7 => {
                let id = self.reference()?;
                h.number(u64::from(id.0));
                DecodedValue::Object(id)
            }
            8 => {
                let (ty, digest) = self.ty()?;
                h.digest(digest);
                DecodedValue::Type(Arc::new(ty))
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
                    name: name.into(),
                }
            }
            10 => {
                let limit = self.limit()?;
                h.byte(limit_tag(limit));
                DecodedValue::Truncated(limit)
            }
            11 => {
                let child = self.child()?;
                h.number(u64::from(child.0));
                DecodedValue::External(child)
            }
            12 => {
                let child = self.child()?;
                let node = self.u32()?;
                // A child's root is named by its blob alone.
                if node == 0 {
                    return Err(invalid("a child's root is referenced as a node".into()));
                }
                h.number(u64::from(child.0));
                h.number(u64::from(node));
                DecodedValue::ExternalNode {
                    child,
                    node: NodeId(node),
                }
            }
            other => return Err(invalid(format!("value tag {other}"))),
        })
    }
    /// A value sequence.
    fn values(&mut self, h: &mut Hasher) -> Result<Vec<DecodedValue>, BlobError> {
        let n = self.count(1)?;
        self.charge(n * std::mem::size_of::<DecodedValue>())?;
        h.size(n);
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(self.value(h)?);
        }
        Ok(values)
    }
    fn entries(&mut self, h: &mut Hasher) -> Result<Entries, BlobError> {
        let n = self.count(if self.version == 3 { 5 } else { 2 })?;
        self.values = self.values.saturating_add(n);
        if self.values > self.limits.max_values {
            return Err(BlobError::Limit("values"));
        }
        self.charge(n * std::mem::size_of::<(DecodedValue, DecodedValue)>())?;
        h.size(n);
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let key = if self.version == 3 {
                let key = self.text()?;
                h.size(key.len());
                h.absorb(key.as_bytes());
                DecodedValue::String(key.into())
            } else {
                self.value(h)?
            };
            entries.push((key, self.value(h)?));
        }
        Ok(entries)
    }
    /// String-named fields; names are hashed in place.
    fn fields(&mut self, h: &mut Hasher) -> Result<Fields, BlobError> {
        // Key length (4) plus a value tag (1).
        let n = self.count(5)?;
        self.charge(n * std::mem::size_of::<(Box<str>, DecodedValue)>())?;
        h.size(n);
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            let key = self.text()?;
            h.size(key.len());
            h.absorb(key.as_bytes());
            let value = self.value(h)?;
            entries.push((key.into(), value));
        }
        Ok(entries)
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
    fn types(&mut self, h: &mut Hasher) -> Result<Vec<TypeDescription>, BlobError> {
        let n = self.count(1)?;
        self.charge(n * std::mem::size_of::<TypeDescription>())?;
        h.size(n);
        let mut types = Vec::with_capacity(n);
        for _ in 0..n {
            let (ty, digest) = self.ty()?;
            h.digest(digest);
            types.push(ty);
        }
        Ok(types)
    }
    /// One object definition, hashed into `h` in encoding order.
    fn object(&mut self, h: &mut Hasher) -> Result<DecodedObject, BlobError> {
        let tag = self.u8()?;
        h.byte(tag);
        Ok(match tag {
            0 => {
                let original_len = self.u64()?;
                h.number(original_len);
                let len = self.u32()? as usize;
                let data = self.take(len)?;
                self.charge(len)?;
                let mut digest = Hasher::new(HashDomain::Uint8Array);
                digest.absorb(data);
                h.size(len);
                h.digest(digest.finish());
                DecodedObject::Uint8Array {
                    data: data.to_vec(),
                    original_len,
                }
            }
            1 => {
                let (element_type, type_digest) = self.ty()?;
                h.digest(type_digest);
                let original_len = self.u64()?;
                h.number(original_len);
                let items = self.values(h)?;
                DecodedObject::List {
                    element_type,
                    items,
                    original_len,
                }
            }
            2 => {
                let (key_type, key_digest) = self.ty()?;
                h.digest(key_digest);
                let (value_type, value_digest) = self.ty()?;
                h.digest(value_digest);
                let original_len = self.u64()?;
                h.number(original_len);
                let entries = self.entries(h)?;
                DecodedObject::Map {
                    key_type,
                    value_type,
                    entries,
                    original_len,
                }
            }
            3 => {
                let type_arguments = self.types(h)?;
                let declaration = self.reference()?;
                h.number(u64::from(declaration.0));
                let original_len = self.u64()?;
                h.number(original_len);
                let fields = self.fields(h)?;
                DecodedObject::Instance {
                    type_arguments,
                    declaration,
                    fields,
                    original_len,
                }
            }
            4 => {
                let mut window = Window::new(self.input);
                let decoded = (|| {
                    let form = if self.version >= 5 {
                        u8::deserialize_reader(&mut window)?
                    } else {
                        tags::DeclarationTag::Tagged as u8
                    };
                    match form {
                        0 => {
                            let tag = TypeTag::deserialize_reader(&mut window)?;
                            let name = DeclarationName::deserialize_reader(&mut window)?;
                            Ok((name, Some(tag), None))
                        }
                        1 => {
                            let name = DeclarationName::deserialize_reader(&mut window)?;
                            let definition = DefinitionRef::deserialize_reader(&mut window)?;
                            Ok((name, None, Some(definition)))
                        }
                        other => Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("declaration form {other}"),
                        )),
                    }
                })();
                let (name, tag, definition) = decoded.map_err(|error| {
                    if window.exhausted {
                        BlobError::Truncated
                    } else {
                        invalid(format!("declaration: {error}"))
                    }
                })?;
                let used = window.used();
                h.absorb(self.take(used)?);
                if let Some(definition) = &definition {
                    self.named.push(definition.group);
                }
                let is_enum = self.bool()?;
                h.byte(u8::from(is_enum));
                self.charge(used * 4)?;
                DecodedObject::Declaration {
                    name: DecodedName(name),
                    tag,
                    is_enum,
                    definition,
                }
            }
            5 => DecodedObject::Cell(self.value(h)?),
            6 => DecodedObject::NonSnapshotable,
            7 => {
                let kind = description(self.u8()?)?;
                h.byte(description_tag(kind));
                let has_name = self.bool()?;
                h.byte(u8::from(has_name));
                let name = if has_name {
                    let (name, digest) = self.string()?;
                    h.digest(digest);
                    Some(name.into())
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
            9 => DecodedObject::Media(self.media(h)?),
            other => return Err(invalid(format!("object tag {other}"))),
        })
    }
    /// A definition group's members, after its root tag.
    fn definitions(&mut self) -> Result<Vec<DecodedDefinition>, BlobError> {
        // A kind, a name and four metadata lengths at least.
        let n = self.count(1 + 1 + 4 + 3 + 4)?;
        self.charge(n.saturating_mul(std::mem::size_of::<DecodedDefinition>()))?;
        self.positions = Some(u32::try_from(n).expect("a u32 count"));
        let mut members = Vec::with_capacity(n);
        for _ in 0..n {
            let kind = self.u8()?;
            let name = self.declaration_name()?;
            let meta = self.meta()?;
            members.push(match kind {
                0 => {
                    let type_params = self.u32()?;
                    let stream_done = self.bool()?;
                    // A name, a type and the metadata and flags at least.
                    let count = self.count(4 + 1 + 3 + 4 + 3)?;
                    self.charge(
                        count.saturating_mul(std::mem::size_of::<definition::Field<FieldType>>()),
                    )?;
                    let mut fields = Vec::with_capacity(count);
                    for _ in 0..count {
                        let name = self.text()?.to_owned();
                        let ty = self.field_type()?;
                        let meta = self.meta()?;
                        fields.push(definition::Field {
                            name,
                            ty,
                            meta,
                            skip: self.bool()?,
                            stream_done: self.bool()?,
                            must_exist: self.bool()?,
                        });
                    }
                    definition::Declaration::Class(definition::Class {
                        name,
                        type_params,
                        meta,
                        stream_done,
                        fields,
                    })
                }
                1 => {
                    let count = self.count(4 + 3 + 4 + 1)?;
                    self.charge(count.saturating_mul(std::mem::size_of::<Variant>()))?;
                    let mut variants = Vec::with_capacity(count);
                    for _ in 0..count {
                        let name = self.text()?.to_owned();
                        let meta = self.meta()?;
                        variants.push(Variant {
                            name,
                            meta,
                            skip: self.bool()?,
                        });
                    }
                    definition::Declaration::Enum(definition::Enum {
                        name,
                        meta,
                        variants,
                    })
                }
                other => return Err(invalid(format!("definition kind {other}"))),
            });
        }
        self.positions = None;
        Ok(members)
    }
    fn declaration_name(&mut self) -> Result<DeclarationName, BlobError> {
        let mut window = Window::new(self.input);
        let name = DeclarationName::deserialize_reader(&mut window).map_err(|error| {
            if window.exhausted {
                BlobError::Truncated
            } else {
                invalid(format!("declaration name: {error}"))
            }
        })?;
        let used = window.used();
        self.take(used)?;
        self.charge(used.saturating_mul(4))?;
        Ok(name)
    }
    fn optional_text(&mut self) -> Result<Option<String>, BlobError> {
        Ok(if self.bool()? {
            Some(self.text()?.to_owned())
        } else {
            None
        })
    }
    fn meta(&mut self) -> Result<Meta, BlobError> {
        let description = self.optional_text()?;
        let alias = self.optional_text()?;
        let docstring = self.optional_text()?;
        let count = self.count(8)?;
        self.charge(count.saturating_mul(std::mem::size_of::<(String, String)>()))?;
        let mut attributes = Vec::with_capacity(count);
        for _ in 0..count {
            let key = self.text()?.to_owned();
            let value = self.text()?.to_owned();
            attributes.push((key, value));
        }
        Ok(Meta {
            description,
            alias,
            docstring,
            attributes,
        })
    }
    /// A media object after its tag.
    fn media(&mut self, h: &mut Hasher) -> Result<DecodedMedia, BlobError> {
        let tag = self.u8()?;
        let kind =
            tags::media_kind_of(tag).ok_or_else(|| invalid(format!("media kind tag {tag}")))?;
        h.byte(tag);
        let has_mime_type = self.bool()?;
        h.byte(u8::from(has_mime_type));
        let mime_type = if has_mime_type {
            Some(self.text_in_place(h)?.into())
        } else {
            None
        };
        let tag = self.u8()?;
        h.byte(tag);
        let source = match tag {
            0 => DecodedMediaSource::Url {
                url: self.text_in_place(h)?.into(),
                data: self.loaded(h)?,
            },
            1 => DecodedMediaSource::File {
                path: self.text_in_place(h)?.into(),
                data: self.loaded(h)?,
            },
            2 => DecodedMediaSource::Base64 {
                data: self.payload(h)?,
            },
            other => return Err(invalid(format!("media source tag {other}"))),
        };
        Ok(DecodedMedia {
            kind,
            mime_type,
            source,
        })
    }
    /// A string written in place, hashed by its digest.
    fn text_in_place(&mut self, h: &mut Hasher) -> Result<&'a str, BlobError> {
        let (text, digest) = self.string()?;
        h.digest(digest);
        Ok(text)
    }
    /// Media content loaded from a URL or file, when the value holds it.
    fn loaded(&mut self, h: &mut Hasher) -> Result<Option<MediaPayload>, BlobError> {
        let present = self.bool()?;
        h.byte(u8::from(present));
        present.then(|| self.payload(h)).transpose()
    }
    /// Media content: its length, then its text as a string value.
    fn payload(&mut self, h: &mut Hasher) -> Result<MediaPayload, BlobError> {
        let text_len = self.u64()?;
        h.number(text_len);
        match self.value(h)? {
            DecodedValue::String(text) if text.len() as u64 == text_len => {
                Ok(MediaPayload::Inline(text))
            }
            DecodedValue::External(child) => Ok(MediaPayload::External { child, text_len }),
            DecodedValue::String(_) => Err(invalid("media content length".into())),
            DecodedValue::Null
            | DecodedValue::OmittedArg
            | DecodedValue::Bool(_)
            | DecodedValue::Int(_)
            | DecodedValue::Float(_)
            | DecodedValue::Bigint(_)
            | DecodedValue::Object(_)
            | DecodedValue::Type(_)
            | DecodedValue::Enum { .. }
            | DecodedValue::Truncated(_)
            | DecodedValue::ExternalNode { .. } => {
                Err(invalid("media content is not a string".into()))
            }
        }
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
        | DecodedValue::Truncated(_)
        | DecodedValue::External(_)
        | DecodedValue::ExternalNode { .. } => Ok(()),
    };
    match &snapshot.root {
        DecodedRoot::Value(value) => check_value(value)?,
        DecodedRoot::FunctionArgs { slots, .. } => slots.iter().try_for_each(check_value)?,
        DecodedRoot::Definitions(_) => {}
    }
    for object in &snapshot.objects {
        match object {
            DecodedObject::List { items, .. } => items.iter().try_for_each(check_value)?,
            DecodedObject::Map { entries, .. } => {
                entries.iter().try_for_each(|(k, v)| {
                    check_value(k)?;
                    check_value(v)
                })?;
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
            | DecodedObject::Media(_)
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
            2 => {
                let name = DeclarationName::deserialize_reader(r)?;
                let definition = DefinitionRef::deserialize_reader(r)?;
                Ok(Self::Defined(DefinedHead { name, definition }))
            }
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
            Self::Defined(head) => head.name.to_string(),
        }
    }
}

/// Shared decoded graphs are immutable; callers typically keep them in an
/// `Arc` for per-query memoization.
pub type SharedSnapshot = Arc<DecodedSnapshot>;

#[cfg(test)]
mod tests;
