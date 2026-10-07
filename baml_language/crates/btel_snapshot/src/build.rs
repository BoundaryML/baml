//! Building a capture. A [`Builder`] fills one storage; finishing it shapes
//! the capture into blobs and yields the immutable [`Snapshot`].
//!
//! An object's identity comes before its content, so that what it holds can
//! name it: [`Leaves::reserve`] gives the identity and [`Leaves::fill`] the
//! content. A container's items are written in one run, by an operation that
//! takes the items' source and converts each: [`Builder::list`],
//! [`Builder::map`], [`Builder::instance`], [`Builder::arguments`]. The
//! conversion is handed [`Leaves`], which holds strings, names, bigints,
//! types and identities but cannot start another container.
//!
//! Limits are applied here, not by the caller, and what they cut shows in the
//! capture: a container records how long its source was, and a value that
//! could not be held is a truncation marker.
//!
//! A recorded class or enum definition is not captured content: it is a blob
//! made once and shared. [`Leaves::define`] returns the reference a type head
//! or declaration holds, which names the definition's group by ID. The first
//! capture of a stream to name a group also carries it to the writers (see
//! [`Carried`]); limits do not apply to it.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use baml_type::{DeclarationName, typetag::TypeTag};
use bex_str::BexStr;
use btel_settings::snapshot::CARRIED_GROUPS;
use btel_types::{Definition, DefinitionBlob};
use num_bigint::BigInt;
use rustc_hash::FxHashSet;

use crate::{
    Shaper, Snapshot, SnapshotPool, TypeIdentity,
    arena::{Arena, Meter},
    definition::DefinitionRef,
    graph::{
        Bigint, Declared, FieldEntry, FunctionArgs, Graph, LabelId, Limit, MapEntry, NameId,
        ObjectId, OwnedType, Range, SnapshotObject, SnapshotRoot, SnapshotValue, StringId, Type,
        TypeId, Uint8ArrayData,
    },
    hash::{self, Absorb as _},
    pool::{Lease, Limits, Storage},
    tags,
    walk::bigint_limb_bytes,
};

/// The most items an arena indexed by `u32` holds.
const ARENA_ITEMS: usize = u32::MAX as usize - 1;

/// A capture being built.
pub struct Builder(Lease);

/// What converting one value needs: the capture's strings, names, bigints,
/// types and object identities.
pub struct Leaves<'b> {
    objects: &'b mut Arena<SnapshotObject>,
    strings: &'b mut Arena<BexStr>,
    labels: &'b mut Arena<BexStr>,
    bigints: &'b mut Arena<Bigint>,
    types: &'b mut Arena<Type>,
    definitions: &'b mut Arena<Arc<DefinitionBlob>>,
    meter: &'b Meter,
    limits: Limits,
}

/// The definition groups one stream of captures has carried to its writers.
///
/// A capture names a group by ID and carries it only the first time its
/// stream names it, so a group reaches the writers ahead of every later
/// capture that names it, as long as the stream's captures are delivered in
/// the order they are made. Keep one per such stream, and never share one
/// between streams that go to different writers. Forgetting a group, as a
/// full set does, only carries it again: the writers store it once.
///
/// A writer that may have lost a group a capture carried (a capture whose
/// blobs include one: [`crate::Blob::is_definition`]) calls
/// [`forget_carried`], and every stream carries its groups again from its
/// next capture ([`Self::begin`]). Losing a capture that carried none needs
/// nothing.
/// Captures that named a lost group in the meantime read it once a later
/// capture carries it again.
pub struct Carried {
    /// This stream's number, unique in the process. A stream that forgets
    /// what it carried takes a new one, so no group's marker names it.
    stream: u64,
    /// [`LOSSES`] when this stream last started carrying from scratch.
    losses: u64,
    groups: FxHashSet<[u8; 16]>,
}

/// Stream numbers: unique in the process, never 0.
static STREAMS: AtomicU64 = AtomicU64::new(1);
/// How many times a writer may have lost a capture.
static LOSSES: AtomicU64 = AtomicU64::new(0);

/// Make every stream carry its groups again, because a writer may have lost
/// a capture that carried one. Each stream notices when its next capture
/// begins.
pub fn forget_carried() {
    LOSSES.fetch_add(1, Ordering::Relaxed);
}

impl Default for Carried {
    fn default() -> Self {
        Self {
            stream: STREAMS.fetch_add(1, Ordering::Relaxed),
            losses: LOSSES.load(Ordering::Relaxed),
            groups: FxHashSet::default(),
        }
    }
}
impl Carried {
    /// Start a capture: after a possible loss, carry from scratch. Once per
    /// capture, never during one, so a capture carries each group once.
    #[inline]
    pub fn begin(&mut self) {
        if LOSSES.load(Ordering::Relaxed) != self.losses {
            self.restart();
        }
    }
    #[cold]
    #[inline(never)]
    fn restart(&mut self) {
        self.stream = STREAMS.fetch_add(1, Ordering::Relaxed);
        self.losses = LOSSES.load(Ordering::Relaxed);
        self.groups.clear();
    }
    /// Whether `group` is carried already; otherwise mark it carried.
    #[inline]
    fn carries(&mut self, group: &DefinitionBlob) -> bool {
        // The group remembers its last carrier: one load when it is this
        // stream, the usual case.
        group.carried_by(self.stream) || self.carries_slow(group)
    }
    #[inline(never)]
    fn carries_slow(&mut self, group: &DefinitionBlob) -> bool {
        // Another stream carried it last; this one may have too. Not marking
        // it back keeps streams that share groups from writing it in turn.
        if self.groups.contains(&group.id()) {
            return true;
        }
        if self.groups.len() >= CARRIED_GROUPS {
            self.groups.clear();
        }
        self.groups.insert(group.id());
        group.mark_carried(self.stream);
        false
    }
}

/// An object's identity, before its content. It is filled once, by
/// [`Leaves::fill`]; one that is dropped instead reads as truncated.
#[derive(Debug)]
pub struct Reserved(ObjectId);
impl Reserved {
    pub fn id(&self) -> ObjectId {
        self.0
    }
}

impl Leaves<'_> {
    pub fn limits(&self) -> Limits {
        self.limits
    }
    /// Hold a name by handle: an enum variant, a function, a MIME type, a URL
    /// or a path. It is written wherever it is used. `None` when it is over
    /// the leaf limit, or no more names can be numbered.
    pub fn label(&mut self, text: &BexStr) -> Option<LabelId> {
        if !self.limits.holds_leaf(text.len()) || self.labels.len() >= ARENA_ITEMS {
            return None;
        }
        let id = LabelId(u32::try_from(self.labels.len()).expect("bounded labels"));
        self.labels.push(text.clone(), self.meter);
        Some(id)
    }
    /// Hold content by handle: a string value, or a media value's text. `None`
    /// when it is over the leaf limit, or no more strings can be numbered.
    pub fn string(&mut self, text: &BexStr) -> Option<StringId> {
        if !self.limits.holds_leaf(text.len()) || self.strings.len() >= ARENA_ITEMS {
            return None;
        }
        // A slice caches its hash in its own handle, not in what it views.
        // Hash the caller's, so that its next capture finds the hash there
        // and this clone starts with it.
        if matches!(text, BexStr::Slice { .. }) {
            text.content_hash();
        }
        let id = StringId(u32::try_from(self.strings.len()).expect("bounded strings"));
        self.strings.push(text.clone(), self.meter);
        Some(id)
    }
    /// A string as a value: truncated when it cannot be held.
    pub fn string_value(&mut self, text: &BexStr) -> SnapshotValue {
        self.string(text).map_or(
            SnapshotValue::Truncated(Limit::Bytes),
            SnapshotValue::String,
        )
    }
    /// A bigint as a value, held by handle: truncated when it is over the
    /// leaf limit, before any of it is hashed, or cannot be numbered.
    pub fn bigint(&mut self, value: &Arc<BigInt>) -> SnapshotValue {
        if !self.limits.holds_leaf(bigint_limb_bytes(value)) || self.bigints.len() >= ARENA_ITEMS {
            return SnapshotValue::Truncated(Limit::Bytes);
        }
        let id = crate::BigintId(u32::try_from(self.bigints.len()).expect("bounded bigints"));
        self.bigints.push(
            Bigint {
                digest: hash::bigint(value),
                value: Arc::clone(value),
            },
            self.meter,
        );
        SnapshotValue::Bigint(id)
    }
    /// A type description. A head that names a definition must name one
    /// [`Self::define`] added.
    pub fn ty(&mut self, ty: OwnedType) -> TypeId {
        let defined = names_definition(&ty);
        self.described(ty, defined)
    }
    /// [`Self::ty`] for a caller that knows whether a head of `ty` names a
    /// definition.
    pub fn described(&mut self, ty: OwnedType, defined: bool) -> TypeId {
        debug_assert_eq!(
            defined,
            names_definition(&ty),
            "the caller knows whether a head is defined"
        );
        let id = TypeId(u32::try_from(self.types.len()).expect("type arena exhausted"));
        self.types.push(
            Type {
                leaf: hash::ty(&ty),
                ty,
                defined,
            },
            self.meter,
        );
        id
    }
    /// How a type head or declaration names `definition`. The capture
    /// carries its group, and every group that names which `carried` lacks,
    /// when `carried` lacks it.
    #[inline]
    pub fn define(&mut self, definition: &Definition, carried: &mut Carried) -> DefinitionRef {
        if !carried.carries(&definition.group) {
            carry(self.definitions, carried, self.meter, &definition.group);
        }
        DefinitionRef::of(definition)
    }
    /// An object's identity, for what will hold it to name before its
    /// content is known. `None` once the object limit is reached.
    pub fn reserve(&mut self) -> Option<Reserved> {
        self.object(SnapshotObject::Truncated(Limit::Objects))
            .map(Reserved)
    }
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking the identity by value is what makes filling it twice a compile error"
    )]
    pub fn fill(&mut self, slot: Reserved, object: SnapshotObject) {
        self.objects[slot.0.0 as usize] = object;
    }
    /// An object whose content is known already. `None` once the object
    /// limit is reached.
    pub fn object(&mut self, object: SnapshotObject) -> Option<ObjectId> {
        if self.objects.len() >= self.limits.max_objects.unwrap_or(ARENA_ITEMS) {
            return None;
        }
        let id = ObjectId(u32::try_from(self.objects.len()).expect("bounded objects"));
        self.objects.push(object, self.meter);
        Some(id)
    }
}

/// Whether a head of `ty` names a recorded definition.
fn names_definition(ty: &OwnedType) -> bool {
    let mut defined = false;
    ty.visit_heads(&mut |head| defined |= matches!(head, TypeIdentity::Defined(_)));
    defined
}

/// Carry `group`, newly marked carried, after the groups it names that
/// `carried` lacks, marking each. Iterative: a chain of groups can be as long
/// as a program's declarations.
#[cold]
fn carry(
    definitions: &mut Arena<Arc<DefinitionBlob>>,
    carried: &mut Carried,
    meter: &Meter,
    group: &Arc<DefinitionBlob>,
) {
    // A group and the next of its children to look at.
    let mut path = vec![(Arc::clone(group), 0)];
    while let Some((top, next)) = path.last_mut() {
        if let Some(child) = top.children().get(*next) {
            *next += 1;
            if !carried.carries(child) {
                let child = Arc::clone(child);
                path.push((child, 0));
            }
            continue;
        }
        let (done, _) = path
            .pop()
            .unwrap_or_else(|| unreachable!("a group on the path"));
        definitions.push(done, meter);
    }
}

/// A storage's arenas, split by who writes them: a container operation
/// writes its run of values or entries while its conversion writes leaves.
struct Parts<'b> {
    leaves: Leaves<'b>,
    values: &'b mut Arena<SnapshotValue>,
    entries: &'b mut Arena<MapEntry>,
    fields: &'b mut Arena<FieldEntry>,
    meter: &'b Meter,
}

impl Builder {
    pub(crate) fn new(lease: Lease) -> Self {
        Self(lease)
    }
    pub fn limits(&self) -> Limits {
        self.0.limits
    }
    fn parts(&mut self) -> Parts<'_> {
        let Storage {
            graph,
            meter,
            limits,
            ..
        } = &mut *self.0;
        let Graph {
            objects,
            values,
            entries,
            fields,
            bytes: _,
            strings,
            labels,
            bigints,
            types,
            names: _,
            definitions,
        } = graph;
        Parts {
            leaves: Leaves {
                objects,
                strings,
                labels,
                bigints,
                types,
                definitions,
                meter,
                limits: *limits,
            },
            values,
            entries,
            fields,
            meter,
        }
    }
    /// The capture's leaves and identities, outside any container.
    pub fn leaves(&mut self) -> Leaves<'_> {
        self.parts().leaves
    }
    pub fn fill(&mut self, slot: Reserved, object: SnapshotObject) {
        self.leaves().fill(slot, object);
    }
    /// One run of values: as many of `source` as the value limit allows.
    fn values<T>(
        &mut self,
        source: impl ExactSizeIterator<Item = T>,
        mut convert: impl FnMut(&mut Leaves<'_>, T) -> SnapshotValue,
    ) -> Range<SnapshotValue> {
        let Parts {
            mut leaves,
            values,
            entries: _,
            fields: _,
            meter,
        } = self.parts();
        let start = values.len();
        let room = leaves.limits.max_values.unwrap_or(ARENA_ITEMS);
        let count = source.len().min(room.saturating_sub(start));
        values.reserve(count, meter);
        for item in source.take(count) {
            let value = convert(&mut leaves, item);
            values.push(value, meter);
        }
        Range::new(start, values.len() - start)
    }
    /// One run of entries: as many of `source` as the value limit allows. A
    /// key is written where its entry is, so an entry whose key is over the
    /// leaf limit is left out, and the count says the container is cut.
    fn fields<T>(
        &mut self,
        source: impl ExactSizeIterator<Item = T>,
        mut convert: impl FnMut(&mut Leaves<'_>, T) -> (BexStr, SnapshotValue),
    ) -> Range<FieldEntry> {
        let Parts {
            mut leaves,
            values: _,
            entries: _,
            fields: entries,
            meter,
        } = self.parts();
        let start = entries.len();
        let room = leaves.limits.max_values.unwrap_or(ARENA_ITEMS);
        let count = source.len().min(room.saturating_sub(start));
        entries.reserve(count, meter);
        for item in source.take(count) {
            let (key, value) = convert(&mut leaves, item);
            if leaves.limits.holds_leaf(key.len()) {
                entries.push(FieldEntry { key, value }, meter);
            }
        }
        Range::new(start, entries.len() - start)
    }
    fn entries<T>(
        &mut self,
        source: impl ExactSizeIterator<Item = T>,
        mut convert: impl FnMut(&mut Leaves<'_>, T) -> (SnapshotValue, SnapshotValue),
    ) -> Range<MapEntry> {
        let Parts {
            mut leaves,
            entries,
            meter,
            ..
        } = self.parts();
        let start = entries.len();
        let room = leaves
            .limits
            .max_values
            .map_or(ARENA_ITEMS, |values| values / 2);
        let count = source.len().min(room.saturating_sub(start));
        entries.reserve(count, meter);
        for item in source.take(count) {
            let (key, value) = convert(&mut leaves, item);
            entries.push(MapEntry { key, value }, meter);
        }
        Range::new(start, entries.len() - start)
    }
    /// A list of what `convert` makes of each item of `source`.
    pub fn list<T>(
        &mut self,
        element_type: TypeId,
        source: impl ExactSizeIterator<Item = T>,
        convert: impl FnMut(&mut Leaves<'_>, T) -> SnapshotValue,
    ) -> SnapshotObject {
        let original_len = source.len();
        SnapshotObject::List {
            element_type,
            items: self.values(source, convert),
            original_len,
        }
    }
    /// A map of the keys and values `convert` makes of each item of `source`.
    pub fn map<T>(
        &mut self,
        key_type: TypeId,
        value_type: TypeId,
        source: impl ExactSizeIterator<Item = T>,
        convert: impl FnMut(&mut Leaves<'_>, T) -> (SnapshotValue, SnapshotValue),
    ) -> SnapshotObject {
        let original_len = source.len();
        SnapshotObject::Map {
            key_type,
            value_type,
            entries: self.entries(source, convert),
            original_len,
        }
    }
    /// An instance of `declaration`, with the fields `convert` makes of each
    /// item of `source`.
    pub fn instance<T>(
        &mut self,
        declaration: ObjectId,
        type_arguments: impl IntoIterator<Item = OwnedType>,
        source: impl ExactSizeIterator<Item = T>,
        convert: impl FnMut(&mut Leaves<'_>, T) -> (BexStr, SnapshotValue),
    ) -> SnapshotObject {
        let type_arguments = type_arguments.into_iter().map(|ty| {
            let defined = names_definition(&ty);
            (ty, defined)
        });
        self.described_instance(declaration, type_arguments, source, convert)
    }
    /// [`Self::instance`] for a caller that knows, of each type argument,
    /// whether a head names a definition (as [`Leaves::described`]).
    pub fn described_instance<T>(
        &mut self,
        declaration: ObjectId,
        type_arguments: impl IntoIterator<Item = (OwnedType, bool)>,
        source: impl ExactSizeIterator<Item = T>,
        convert: impl FnMut(&mut Leaves<'_>, T) -> (BexStr, SnapshotValue),
    ) -> SnapshotObject {
        let mut leaves = self.leaves();
        let start = leaves.types.len();
        for (ty, defined) in type_arguments {
            leaves.described(ty, defined);
        }
        let type_arguments = Range::new(start, leaves.types.len() - start);
        let original_len = source.len();
        SnapshotObject::Instance {
            type_arguments,
            declaration,
            fields: self.fields(source, convert),
            original_len,
        }
    }
    /// A `uint8array`: its bytes, copied and hashed on the way, or only its
    /// length when it is over the leaf limit or the arena cannot index them
    /// all.
    pub fn bytes(&mut self, bytes: &[u8]) -> SnapshotObject {
        let Storage {
            graph,
            meter,
            limits,
            ..
        } = &mut *self.0;
        if !limits.holds_leaf(bytes.len())
            || bytes.len() > ARENA_ITEMS.saturating_sub(graph.bytes.len())
        {
            return SnapshotObject::Uint8ArrayTruncated {
                original_len: bytes.len(),
            };
        }
        let start = graph.bytes.len();
        graph.bytes.reserve(bytes.len(), meter);
        let mut digest = hash::Hasher::new(tags::HashDomain::Uint8Array);
        for chunk in bytes.chunks(btel_settings::snapshot::COPY_HASH_BATCH_BYTES) {
            digest.absorb(chunk);
            graph.bytes.extend_from_slice(chunk, meter);
        }
        SnapshotObject::Uint8Array {
            data: Uint8ArrayData {
                range: Range::new(start, bytes.len()),
                digest: digest.finish(),
            },
        }
    }
    /// [`Leaves::define`], outside any container.
    #[inline]
    pub fn define(&mut self, definition: &Definition, carried: &mut Carried) -> DefinitionRef {
        if !carried.carries(&definition.group) {
            let Storage { graph, meter, .. } = &mut *self.0;
            carry(&mut graph.definitions, carried, meter, &definition.group);
        }
        DefinitionRef::of(definition)
    }
    /// A class or enum declaration. Its name is held beside the objects.
    /// With its recorded definition (from [`Leaves::define`]), the
    /// declaration is identified by it rather than by its runtime tag.
    pub fn declaration(
        &mut self,
        name: &DeclarationName,
        tag: TypeTag,
        is_enum: bool,
        definition: Option<DefinitionRef>,
    ) -> SnapshotObject {
        let Storage { graph, meter, .. } = &mut *self.0;
        let id = NameId(u32::try_from(graph.names.len()).expect("a name per object"));
        graph.names.push(
            Declared {
                name: name.clone(),
                definition,
            },
            meter,
        );
        SnapshotObject::Declaration {
            name: id,
            tag,
            is_enum,
        }
    }
    /// The arguments of a call as a capture's root: what `convert` makes of
    /// each item of `source`, in callee parameter order.
    pub fn arguments<T>(
        &mut self,
        source: impl ExactSizeIterator<Item = T>,
        convert: impl FnMut(&mut Leaves<'_>, T) -> SnapshotValue,
    ) -> SnapshotRoot {
        SnapshotRoot::FunctionArgs(FunctionArgs {
            parameter_count: source.len(),
            slots: self.values(source, convert),
        })
    }
    /// Shape the capture into blobs; the snapshot is immutable afterwards.
    pub fn finish(mut self, root: impl Into<SnapshotRoot>, shaper: &mut Shaper) -> Snapshot {
        shaper.shape(&mut self.0, root.into());
        Snapshot::new(self.0)
    }
}

/// A `map<string, string>` snapshot built outside any VM, such as a
/// project's sources keyed by path. Entries are taken in order while their
/// keys and values total at most `max_bytes`; the rest are dropped, and the
/// map's recorded length says so. `None` when the pool has no slot.
pub fn string_map(
    pool: &SnapshotPool,
    entries: &[(String, String)],
    max_bytes: usize,
) -> Option<Snapshot> {
    let mut b = pool.try_acquire()?;
    let mut shaper = Shaper::default();
    let mut room = max_bytes;
    let fit = entries
        .iter()
        .take_while(|(key, value)| {
            room.checked_sub(key.len().saturating_add(value.len()))
                .map(|left| room = left)
                .is_some()
        })
        .count();
    let key_type = b.leaves().ty(OwnedType::string());
    let value_type = b.leaves().ty(OwnedType::string());
    let mut map = b.map(
        key_type,
        value_type,
        entries[..fit].iter(),
        |leaves, (key, value)| {
            (
                leaves.string_value(&BexStr::from(key.as_str())),
                leaves.string_value(&BexStr::from(value.as_str())),
            )
        },
    );
    if let SnapshotObject::Map { original_len, .. } = &mut map {
        *original_len = entries.len();
    }
    let root = b.leaves().object(map).map_or(
        SnapshotValue::Truncated(Limit::Objects),
        SnapshotValue::Object,
    );
    Some(b.finish(root, &mut shaper))
}
