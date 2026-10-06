//! A capture's data: values, objects and leaves in flat arenas, addressed by
//! index. Nothing here is derived. A capture's size, its blobs and their IDs
//! come from shaping this.
use std::{marker::PhantomData, sync::Arc};

use baml_type::{DeclarationName, MediaKind, typetag::TypeTag};
use bex_str::BexStr;
use btel_types::DefinitionBlob;
use num_bigint::BigInt;
use rustc_hash::FxHashSet;

use crate::{
    arena::Arena,
    definition::{DefinedHead, DefinitionRef},
    hash,
};

macro_rules! index {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        pub struct $name(pub u32);
    };
}
index!(ObjectId);
index!(StringId);
index!(LabelId);
index!(BigintId);
index!(TypeId);
index!(NameId);

/// An index range in one typed arena. Relocation cannot invalidate it.
#[repr(C)]
pub struct Range<T> {
    start: u32,
    len: u32,
    kind: PhantomData<fn() -> T>,
}
impl<T> Copy for Range<T> {}
impl<T> Clone for Range<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> std::fmt::Debug for Range<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Range")
            .field("start", &self.start)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}
impl<T> Range<T> {
    pub fn empty() -> Self {
        Self {
            start: 0,
            len: 0,
            kind: PhantomData,
        }
    }
    pub fn len(self) -> usize {
        self.len as usize
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    pub(crate) fn new(start: usize, len: usize) -> Self {
        Self {
            start: u32::try_from(start).expect("arena exhausted"),
            len: u32::try_from(len).expect("arena exhausted"),
            kind: PhantomData,
        }
    }
    pub(crate) fn indexes(self) -> std::ops::Range<usize> {
        self.start as usize..self.start as usize + self.len as usize
    }
}
/// Copied bytes and the content digest computed while copying them.
#[derive(Clone, Copy, Debug)]
pub struct Uint8ArrayData {
    pub(crate) range: Range<u8>,
    pub(crate) digest: hash::Digest,
}
impl Uint8ArrayData {
    pub fn len(self) -> usize {
        self.range.len()
    }
    pub fn is_empty(self) -> bool {
        self.range.is_empty()
    }
}
/// Owned nominal identity: a tag, or a recorded definition; names alone are
/// not identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeIdentity {
    Resolved(baml_type::TaggedTypeName),
    Unresolved(TypeTag),
    /// A class or enum named by its recorded definition, whose blob is a
    /// child of every blob that holds this head (format 5).
    Defined(DefinedHead),
}
pub type OwnedType = baml_type::RealizedTy<TypeIdentity>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Values,
    Objects,
    Bytes,
    Depth,
}

/// Copyable values carry only snapshot-local indexes, never owning Rust handles
/// or VM pointers. String/bigint indexes are storage references, not object IDs.
///
/// Text is held two ways. A [`StringId`] is content: a string value, or a
/// media value's base64 text, which a large one leaves to a blob of its own.
/// A [`LabelId`] is a name: an enum variant, a function, a MIME type, a URL
/// or a path, always written where it is used.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotValue {
    Null,
    OmittedArg,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(StringId),
    Bigint(BigintId),
    Object(ObjectId),
    Type(TypeId),
    Enum {
        declaration: ObjectId,
        variant: u32,
        name: LabelId,
    },
    Truncated(Limit),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Description {
    Function,
    Closure,
    BoundMethod,
    GenericFunction,
    HostFunction,
    Future,
    UnscheduledFuture,
    Package,
    Interface,
    Implementation,
    TypeAlias,
    Sentinel,
}
#[derive(Debug)]
pub struct MapEntry {
    pub key: SnapshotValue,
    pub value: SnapshotValue,
}
#[derive(Debug)]
pub struct FieldEntry {
    pub key: BexStr,
    pub value: SnapshotValue,
}
#[derive(Debug)]
pub enum SnapshotObject {
    Uint8Array {
        data: Uint8ArrayData,
    },
    /// A `uint8array` whose bytes were not captured: its length, and nothing
    /// of its content.
    Uint8ArrayTruncated {
        original_len: usize,
    },
    List {
        element_type: TypeId,
        items: Range<SnapshotValue>,
        original_len: usize,
    },
    Map {
        key_type: TypeId,
        value_type: TypeId,
        entries: Range<MapEntry>,
        original_len: usize,
    },
    Instance {
        type_arguments: Range<OwnedType>,
        declaration: ObjectId,
        fields: Range<FieldEntry>,
        original_len: usize,
    },
    Declaration {
        name: NameId,
        tag: TypeTag,
        is_enum: bool,
    },
    Cell(SnapshotValue),
    /// Identity within this snapshot, with neither content nor a source handle.
    NonSnapshotableValue {},
    Descriptive {
        kind: Description,
        name: Option<LabelId>,
    },
    /// A media value: its kind and where its content comes from. Its bytes
    /// are captured, as base64 text, only when the value holds them.
    Media {
        kind: MediaKind,
        mime_type: Option<LabelId>,
        source: MediaSource,
    },
    Truncated(Limit),
}
impl SnapshotObject {
    /// Whether a limit kept part of the object itself out of the capture.
    /// The values it holds may be cut without the object being.
    pub fn is_cut(&self) -> bool {
        match self {
            Self::Uint8ArrayTruncated { .. } | Self::Truncated(_) => true,
            Self::List {
                items,
                original_len,
                ..
            } => items.len() != *original_len,
            Self::Map {
                entries,
                original_len,
                ..
            } => entries.len() != *original_len,
            Self::Instance {
                fields,
                original_len,
                ..
            } => fields.len() != *original_len,
            Self::Uint8Array { .. }
            | Self::Declaration { .. }
            | Self::Cell(_)
            | Self::NonSnapshotableValue {}
            | Self::Descriptive { .. }
            | Self::Media { .. } => false,
        }
    }
}
/// Where a media value's content comes from, as the runtime holds it. `data`
/// is the base64 text of content already loaded from the URL or file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaSource {
    Url {
        url: LabelId,
        data: Option<StringId>,
    },
    File {
        path: LabelId,
        data: Option<StringId>,
    },
    Base64 {
        data: StringId,
    },
}
const _: () = assert!(std::mem::size_of::<SnapshotValue>() == 16);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<MapEntry>() == 32);
// Every object of every capture is one of these. What is large or rare is
// held out of line: a declaration's name, and a byte array's length when its
// bytes are not there to give it.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(std::mem::size_of::<SnapshotObject>() == 32);

/// Callee parameter-slot order, with omission distinct from null and truncation.
#[derive(Clone, Copy, Debug)]
pub struct FunctionArgs {
    pub parameter_count: usize,
    pub(crate) slots: Range<SnapshotValue>,
}
#[derive(Clone, Copy, Debug)]
pub enum SnapshotRoot {
    Value(SnapshotValue),
    FunctionArgs(FunctionArgs),
}
impl From<SnapshotValue> for SnapshotRoot {
    fn from(value: SnapshotValue) -> Self {
        Self::Value(value)
    }
}

/// A captured bigint and the digest of its content, found once.
#[derive(Debug)]
pub(crate) struct Bigint {
    pub(crate) value: Arc<BigInt>,
    pub(crate) digest: hash::Digest,
}
/// A captured type description, with its digest and encoded length.
#[derive(Debug)]
pub(crate) struct Type {
    pub(crate) ty: OwnedType,
    pub(crate) leaf: hash::TypeLeaf,
    /// Whether a head names a recorded definition.
    pub(crate) defined: bool,
}

/// A declaration's name, and the definition it is recorded by, if any.
#[derive(Debug)]
pub(crate) struct Declared {
    pub(crate) name: DeclarationName,
    pub(crate) definition: Option<DefinitionRef>,
}

/// Everything a capture holds. A string's digest is not kept beside it: the
/// handle caches its own content hash.
#[derive(Default)]
pub(crate) struct Graph {
    pub(crate) objects: Arena<SnapshotObject>,
    /// List items and argument slots, each list's contiguous.
    pub(crate) values: Arena<SnapshotValue>,
    /// Map entries, each object's contiguous.
    pub(crate) entries: Arena<MapEntry>,
    /// String-named instance fields, each object's contiguous.
    pub(crate) fields: Arena<FieldEntry>,
    /// `uint8array` content.
    pub(crate) bytes: Arena<u8>,
    /// Content: string values and media text.
    pub(crate) strings: Arena<BexStr>,
    /// Names, written where they are used.
    pub(crate) labels: Arena<BexStr>,
    pub(crate) bigints: Arena<Bigint>,
    pub(crate) types: Arena<Type>,
    /// The names of the declarations among the objects.
    pub(crate) names: Arena<Declared>,
    /// The definition groups the capture names, each after the groups it
    /// names.
    pub(crate) definitions: Arena<Arc<DefinitionBlob>>,
    /// The IDs of `definitions`.
    pub(crate) defined: FxHashSet<[u8; 16]>,
}
impl Graph {
    /// Drop the capture and keep the capacity. Every arena is named, so none
    /// can keep a previous capture's content.
    pub(crate) fn clear(&mut self) {
        let Self {
            objects,
            values,
            entries,
            fields,
            bytes,
            strings,
            labels,
            bigints,
            types,
            names,
            definitions,
            defined,
        } = self;
        objects.clear();
        values.clear();
        entries.clear();
        fields.clear();
        bytes.clear();
        strings.clear();
        labels.clear();
        bigints.clear();
        types.clear();
        names.clear();
        definitions.clear();
        defined.clear();
    }
    pub(crate) fn capacity_bytes(&self) -> usize {
        let Self {
            objects,
            values,
            entries,
            fields,
            bytes,
            strings,
            labels,
            bigints,
            types,
            names,
            definitions,
            defined,
        } = self;
        [
            objects.capacity_bytes(),
            values.capacity_bytes(),
            entries.capacity_bytes(),
            fields.capacity_bytes(),
            bytes.capacity_bytes(),
            strings.capacity_bytes(),
            labels.capacity_bytes(),
            bigints.capacity_bytes(),
            types.capacity_bytes(),
            names.capacity_bytes(),
            definitions.capacity_bytes(),
            defined.capacity().saturating_mul(size_of::<[u8; 16]>()),
        ]
        .into_iter()
        .fold(0, usize::saturating_add)
    }
}
