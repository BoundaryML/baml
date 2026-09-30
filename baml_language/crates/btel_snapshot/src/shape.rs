//! Shaping a finished capture into content-addressed blobs.
//!
//! A blob's objects are numbered in first-reference order: walking the blob in
//! encoding order from its root, each object takes the next number the first
//! time a reference to it is met. The same walk hashes the root and every
//! object definition with those numbers and adds up the encoded length, so
//! numbering, hashing and size cannot disagree, and a blob's bytes and
//! identity never depend on the order in which capture discovered its
//! objects. Objects the root cannot reach are not part of the blob.
//!
//! # Where a capture is cut
//!
//! The blob for a value is what capturing that value alone would produce, so
//! whether a value gets a blob of its own depends only on what it reaches:
//!
//! - Objects that reach each other (a cycle) are one unit and never separate.
//! - A unit's weight is the encoded size of its objects plus the weight of
//!   every unit they reference, counted once per reference, or
//!   [`REFERENCE_WEIGHT`] for a referenced unit that is cut. Counting per
//!   reference keeps the weight a function of the value alone.
//! - A unit is cut when its weight reaches the policy's unit size. A string,
//!   bigint or `uint8array` is cut when its content reaches the leaf size.
//! - Declarations are never cut: every blob carries the ones it names.
//!
//! A blob holds its unit and everything that unit reaches without crossing a
//! cut. Objects below the threshold that several blobs reach are copied into
//! each. A cut cycle's blob starts at the member with the smallest
//! [`HashDomain::CycleRoot`] digest, so every capture that reaches the cycle
//! from outside produces the same blob; other blobs name its other members by
//! blob and local number. A capture's own root blob always starts at the
//! captured value.
use rustc_hash::FxHashMap;

use super::{
    BexStr, BigintId, BlobEntry, BlobIndex, CasId, Home, ObjectId, OwnedType, Range,
    SnapshotObject, SnapshotRoot, SnapshotValue, Storage, StringId, grow,
    hash::{Absorb, Digest, Hasher, TypeLeaf},
    tags::HashDomain,
    walk::{self, Both, Length, Reference, Resolver, Visitor, bigint_limb_bytes, infallible},
};

/// Where a capture is cut into blobs. The policy decides blob IDs, never
/// whether a blob can be read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShapePolicy {
    /// One blob per capture.
    #[default]
    Whole,
    /// Cut values that reach these encoded sizes into blobs of their own.
    Split {
        /// Weight at which an object and what it reaches is cut.
        unit_bytes: u64,
        /// Content length at which a string, bigint or `uint8array` is cut.
        leaf_bytes: u64,
    },
}

/// Weight of a reference to a cut value: its entry in the child table.
pub const REFERENCE_WEIGHT: u64 = 16;

/// Bytes of a blob before its child table: magic, version and ID.
pub(crate) const HEADER_BYTES: u64 = 8 + 4 + 16;

/// Reusable shaping scratch. Holds no capture data between captures; keep one
/// per capturing thread.
#[derive(Default)]
pub struct Shaper {
    policy: ShapePolicy,
    /// Capture `ObjectId` to local number in the blob being shaped,
    /// [`UNNUMBERED`] for every other object and between blobs.
    local: Vec<u32>,
    /// Members of the blob being shaped, in local order.
    order: Vec<ObjectId>,
    /// Object digests in local order.
    digests: Vec<Digest>,
    /// The root's hash input, replayed after the counts it follows.
    root: Vec<u8>,
    /// Child table of the blob being shaped, in first-use order.
    children: Vec<BlobIndex>,
    /// Blob index to its slot in `children`, [`UNNUMBERED`] for every other
    /// blob and between blobs.
    slots: Vec<u32>,
    /// This capture's blobs by ID: equal content is stored once.
    ids: FxHashMap<CasId, BlobIndex>,
    cuts: Cuts,
}

/// Local number of an object outside the blob being shaped or written.
pub(crate) const UNNUMBERED: u32 = u32::MAX;

impl Shaper {
    pub fn new(policy: ShapePolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    pub fn policy(&self) -> ShapePolicy {
        self.policy
    }

    pub(crate) fn shape(&mut self, s: &mut Storage, root: SnapshotRoot) {
        debug_assert!(s.blobs.is_empty(), "a capture is shaped once");
        self.local.clear();
        self.local.resize(s.objects.len(), UNNUMBERED);
        self.slots.clear();
        self.ids.clear();
        match self.policy {
            ShapePolicy::Whole => {}
            ShapePolicy::Split {
                unit_bytes,
                leaf_bytes,
            } => self.cut(s, root, unit_bytes, leaf_bytes),
        }
        let index = self.blob(s, root, 0..0);
        debug_assert_eq!(
            index.0 as usize + 1,
            s.blobs.len(),
            "a capture's root blob holds its children, so it equals none of them"
        );
    }

    /// Shape every blob below the capture's root blob, children first.
    fn cut(&mut self, s: &mut Storage, root: SnapshotRoot, unit_bytes: u64, leaf_bytes: u64) {
        self.cuts.find(s, root, unit_bytes, leaf_bytes);
        // The root's own unit is the root blob, wherever its weight falls.
        let root_unit = match root {
            SnapshotRoot::Value(SnapshotValue::Object(id)) => Some(self.cuts.unit[id.0 as usize]),
            SnapshotRoot::Value(
                SnapshotValue::Null
                | SnapshotValue::OmittedArg
                | SnapshotValue::Bool(_)
                | SnapshotValue::Int(_)
                | SnapshotValue::Float(_)
                | SnapshotValue::String(_)
                | SnapshotValue::Bigint(_)
                | SnapshotValue::Type(_)
                | SnapshotValue::Enum { .. }
                | SnapshotValue::Truncated(_),
            )
            | SnapshotRoot::FunctionArgs(_) => None,
        };
        if let Some(unit) = root_unit {
            self.cuts.units[unit as usize].cut = false;
        }
        let cut_units = self.cuts.units.iter().any(|unit| unit.cut);
        let cut_strings = self.cuts.strings.contains(&true);
        let cut_bigints = self.cuts.bigints.contains(&true);
        if !(cut_units || cut_strings || cut_bigints) {
            return;
        }
        let pool = s
            .owner
            .as_ref()
            .unwrap_or_else(|| unreachable!("builder storage has an owner"));
        grow(&mut s.object_homes, s.objects.len(), pool, &mut s.charged);
        s.object_homes.resize(s.objects.len(), None);
        grow(&mut s.string_homes, s.strings.len(), pool, &mut s.charged);
        s.string_homes.resize(s.strings.len(), None);
        grow(&mut s.bigint_homes, s.bigints.len(), pool, &mut s.charged);
        s.bigint_homes.resize(s.bigints.len(), None);

        // Leaves reference nothing, so their blobs can come first.
        for index in 0..s.strings.len() {
            if self.cuts.strings[index] {
                let id = StringId(u32::try_from(index).expect("bounded strings"));
                let leaf = SnapshotRoot::Value(SnapshotValue::String(id));
                s.string_homes[index] = Some(self.blob(s, leaf, 0..0));
            }
        }
        for index in 0..s.bigints.len() {
            if self.cuts.bigints[index] {
                let id = BigintId(u32::try_from(index).expect("bounded bigints"));
                let leaf = SnapshotRoot::Value(SnapshotValue::Bigint(id));
                s.bigint_homes[index] = Some(self.blob(s, leaf, 0..0));
            }
        }
        // Units complete after every unit they reference.
        for unit in 0..self.cuts.units.len() {
            let Unit { members, cut, .. } = self.cuts.units[unit].clone();
            if cut {
                let start = self.cuts.start(s, members.clone());
                let root = SnapshotRoot::Value(SnapshotValue::Object(start));
                self.blob(s, root, members);
            }
        }
    }

    /// Shape the blob rooted at `root` and return its place in the blob
    /// table. `unit` is the range of [`Cuts::members`] that other blobs reach
    /// through this one.
    fn blob(
        &mut self,
        s: &mut Storage,
        root: SnapshotRoot,
        unit: std::ops::Range<usize>,
    ) -> BlobIndex {
        let Self {
            local,
            order,
            digests,
            root: root_input,
            children,
            slots,
            ids,
            cuts,
            policy: _,
        } = self;
        slots.resize(s.blobs.len(), UNNUMBERED);
        order.clear();
        digests.clear();
        root_input.clear();
        children.clear();
        let content_bytes = {
            let s: &Storage = s;
            let mut numbering = Numbering {
                s,
                local: &mut *local,
                order: &mut *order,
                children: &mut *children,
                slots: &mut *slots,
            };
            let mut visitor = Both(
                Hash::new(&mut *root_input, &mut *digests, HashDomain::Object),
                Length::default(),
            );
            infallible(walk::root(&mut visitor, &mut numbering, s, root));
            // Hashing a definition numbers its new references, which extends
            // `order` with the objects to hash next.
            let mut next = 0;
            while let Some(&id) = numbering.order.get(next) {
                let object = &s.objects[id.0 as usize];
                infallible(walk::object(&mut visitor, &mut numbering, s, object));
                next += 1;
            }
            let Both(_, Length(content_bytes)) = visitor;
            content_bytes
        };

        let mut h = Hasher::new(HashDomain::Blob);
        h.size(children.len());
        for child in children.iter() {
            h.absorb(s.blobs[child.0 as usize].id.as_bytes());
        }
        h.size(order.len());
        h.absorb(root_input);
        for digest in digests.iter() {
            h.digest(*digest);
        }
        let id = CasId::from_bytes(h.finish().0);
        // Header, child count and IDs, object count, then the content.
        let encoded_len = HEADER_BYTES
            .saturating_add(4)
            .saturating_add((children.len() as u64).saturating_mul(16))
            .saturating_add(4)
            .saturating_add(content_bytes);

        let index = match ids.get(&id) {
            Some(index) => *index,
            None => {
                let pool = s
                    .owner
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("builder storage has an owner"));
                let index = BlobIndex(u32::try_from(s.blobs.len()).expect("bounded blob count"));
                let members = Range::new(s.members.len(), order.len());
                grow(&mut s.members, order.len(), pool, &mut s.charged);
                s.members.extend_from_slice(order);
                let child_range = Range::new(s.blob_children.len(), children.len());
                grow(&mut s.blob_children, children.len(), pool, &mut s.charged);
                s.blob_children.extend_from_slice(children);
                grow(&mut s.blobs, 1, pool, &mut s.charged);
                s.blobs.push(BlobEntry {
                    id,
                    root,
                    members,
                    children: child_range,
                    encoded_len,
                });
                ids.insert(id, index);
                index
            }
        };
        for member in &cuts.members[unit] {
            let node = local[member.0 as usize];
            debug_assert_ne!(node, UNNUMBERED, "a cycle's root reaches every member");
            s.object_homes[member.0 as usize] = Some(Home { blob: index, node });
        }
        for member in order.iter() {
            local[member.0 as usize] = UNNUMBERED;
        }
        for child in children.iter() {
            slots[child.0 as usize] = UNNUMBERED;
        }
        index
    }
}

/// Numbers the members and children of the blob being shaped.
struct Numbering<'a> {
    s: &'a Storage,
    local: &'a mut [u32],
    order: &'a mut Vec<ObjectId>,
    children: &'a mut Vec<BlobIndex>,
    slots: &'a mut [u32],
}
impl Numbering<'_> {
    fn slot(&mut self, blob: BlobIndex) -> u32 {
        let slot = &mut self.slots[blob.0 as usize];
        if *slot == UNNUMBERED {
            *slot = u32::try_from(self.children.len()).expect("bounded blob count");
            self.children.push(blob);
        }
        *slot
    }
}
impl Resolver for Numbering<'_> {
    fn member(&mut self, id: ObjectId) -> u32 {
        let number = &mut self.local[id.0 as usize];
        if *number == UNNUMBERED {
            *number = u32::try_from(self.order.len()).expect("bounded objects");
            self.order.push(id);
        }
        *number
    }
    fn object(&mut self, id: ObjectId) -> Reference {
        let number = self.local[id.0 as usize];
        if number != UNNUMBERED {
            return Reference::Local(number);
        }
        match self.s.object_home(id) {
            Some(Home { blob, node: 0 }) => Reference::Child(self.slot(blob)),
            Some(Home { blob, node }) => Reference::ChildNode {
                slot: self.slot(blob),
                node,
            },
            None => Reference::Local(self.member(id)),
        }
    }
    fn string(&mut self, id: StringId) -> Option<u32> {
        self.s.string_home(id).map(|blob| self.slot(blob))
    }
    fn bigint(&mut self, id: BigintId) -> Option<u32> {
        self.s.bigint_home(id).map(|blob| self.slot(blob))
    }
}

/// Hash format 3 input. The root goes to a buffer; each object and each
/// sequence has a hasher of its own.
struct Hash<'a> {
    root: &'a mut Vec<u8>,
    digests: &'a mut Vec<Digest>,
    /// The open object's hasher; meaningful while `at` is not the root.
    object: Hasher,
    /// The open sequence's hasher, its length and the scope it opened in;
    /// meaningful while `at` is a range.
    range: Hasher,
    range_len: usize,
    range_in: At,
    at: At,
    /// Domain of each object's digest.
    domain: HashDomain,
}
/// The innermost open scope: a sequence, else an object, else the root.
#[derive(Clone, Copy)]
enum At {
    Root,
    Object,
    Range,
}
impl<'a> Hash<'a> {
    fn new(root: &'a mut Vec<u8>, digests: &'a mut Vec<Digest>, domain: HashDomain) -> Self {
        Self {
            root,
            digests,
            object: Hasher::new(domain),
            range: Hasher::new(HashDomain::Range),
            range_len: 0,
            range_in: At::Root,
            at: At::Root,
            domain,
        }
    }
}
impl Absorb for Hash<'_> {
    #[inline]
    fn absorb(&mut self, bytes: &[u8]) {
        match self.at {
            At::Range => self.range.absorb(bytes),
            At::Object => self.object.absorb(bytes),
            At::Root => self.root.extend_from_slice(bytes),
        }
    }
}
impl Visitor for Hash<'_> {
    type Error = std::convert::Infallible;
    fn byte(&mut self, byte: u8) -> Result<(), Self::Error> {
        Absorb::byte(self, byte);
        Ok(())
    }
    fn int(&mut self, value: i64) -> Result<(), Self::Error> {
        self.absorb(&value.to_le_bytes());
        Ok(())
    }
    fn bits(&mut self, bits: u64) -> Result<(), Self::Error> {
        self.number(bits);
        Ok(())
    }
    fn index(&mut self, index: u32) -> Result<(), Self::Error> {
        self.number(u64::from(index));
        Ok(())
    }
    fn length(&mut self, length: usize) -> Result<(), Self::Error> {
        self.size(length);
        Ok(())
    }
    fn string(&mut self, _: &BexStr, digest: Digest) -> Result<(), Self::Error> {
        self.digest(digest);
        Ok(())
    }
    fn key(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        self.string_parts(text.len(), text.content_hash());
        Ok(())
    }
    fn bigint(&mut self, _: &num_bigint::BigInt, digest: Digest) -> Result<(), Self::Error> {
        self.digest(digest);
        Ok(())
    }
    fn ty(&mut self, _: &OwnedType, leaf: TypeLeaf) -> Result<(), Self::Error> {
        self.digest(leaf.digest);
        Ok(())
    }
    fn bytes(&mut self, bytes: &[u8], digest: Digest) -> Result<(), Self::Error> {
        self.size(bytes.len());
        self.digest(digest);
        Ok(())
    }
    fn declaration(
        &mut self,
        tag: baml_type::typetag::TypeTag,
        name: &baml_type::DeclarationName,
    ) -> Result<(), Self::Error> {
        debug_assert!(matches!(self.at, At::Object), "a declaration is an object");
        self.object.borsh(&tag);
        self.object.borsh(name);
        Ok(())
    }
    fn begin_range(&mut self, len: usize) -> Result<(), Self::Error> {
        debug_assert!(!matches!(self.at, At::Range), "sequences never nest");
        self.range = Hasher::new(HashDomain::Range);
        self.range_len = len;
        self.range_in = self.at;
        self.at = At::Range;
        Ok(())
    }
    fn end_range(&mut self) -> Result<(), Self::Error> {
        debug_assert!(
            matches!(self.at, At::Range),
            "a sequence ends after it begins"
        );
        let digest = self.range.finish();
        let len = self.range_len;
        self.at = self.range_in;
        self.size(len);
        self.digest(digest);
        Ok(())
    }
    fn begin_object(&mut self) {
        debug_assert!(matches!(self.at, At::Root), "objects never nest");
        self.object = Hasher::new(self.domain);
        self.at = At::Object;
    }
    fn end_object(&mut self) {
        debug_assert!(
            matches!(self.at, At::Object),
            "an object ends after it begins"
        );
        self.digests.push(self.object.finish());
        self.at = At::Root;
    }
}

/// Every reference the same: what an object holds without what it points at.
struct Unresolved;
impl Resolver for Unresolved {
    fn member(&mut self, _: ObjectId) -> u32 {
        0
    }
    fn object(&mut self, _: ObjectId) -> Reference {
        Reference::Local(0)
    }
    fn string(&mut self, _: StringId) -> Option<u32> {
        None
    }
    fn bigint(&mut self, _: BigintId) -> Option<u32> {
        None
    }
}

/// Objects that reach each other, found together.
#[derive(Clone, Debug)]
struct Unit {
    /// Range of [`Cuts::members`].
    members: std::ops::Range<usize>,
    weight: u64,
    cut: bool,
}

const UNVISITED: u32 = u32::MAX;
const NO_UNIT: u32 = u32::MAX;

/// Which values of one capture get blobs of their own.
///
/// One depth-first pass (Tarjan's algorithm, without recursion) finds the
/// units in an order where each follows every unit it references, and adds
/// up weights as units complete.
#[derive(Default)]
struct Cuts {
    /// Discovery number per object, [`UNVISITED`] before its visit.
    index: Vec<u32>,
    lowlink: Vec<u32>,
    /// Unit per object, [`NO_UNIT`] until its unit completes.
    unit: Vec<u32>,
    /// Per object: its encoded size plus what its completed references weigh.
    weight: Vec<u64>,
    /// Per object: its range of `edges`.
    edge_range: Vec<(u32, u32)>,
    /// Every reference to an object, in encoding order.
    edges: Vec<ObjectId>,
    /// Visited objects whose unit is incomplete.
    stack: Vec<ObjectId>,
    /// The path being explored: an object and its next edge.
    path: Vec<(ObjectId, u32)>,
    units: Vec<Unit>,
    /// Each unit's objects, concatenated.
    members: Vec<ObjectId>,
    /// Per string and bigint: whether a value that holds it is cut there.
    strings: Vec<bool>,
    bigints: Vec<bool>,
    /// Scratch for choosing a cycle's root.
    digests: Vec<Digest>,
    root: Vec<u8>,
}

impl Cuts {
    fn find(&mut self, s: &Storage, root: SnapshotRoot, unit_bytes: u64, leaf_bytes: u64) {
        let objects = s.objects.len();
        self.index.clear();
        self.index.resize(objects, UNVISITED);
        self.lowlink.clear();
        self.lowlink.resize(objects, 0);
        self.unit.clear();
        self.unit.resize(objects, NO_UNIT);
        self.weight.clear();
        self.weight.resize(objects, 0);
        self.edge_range.clear();
        self.edge_range.resize(objects, (0, 0));
        self.edges.clear();
        self.stack.clear();
        self.path.clear();
        self.units.clear();
        self.members.clear();
        self.strings.clear();
        self.strings.resize(s.strings.len(), false);
        self.bigints.clear();
        self.bigints.resize(s.bigints.len(), false);

        // The root's references start every search.
        let mut edges = Edges {
            s,
            leaf_bytes,
            found: &mut self.edges,
            strings: &mut self.strings,
            bigints: &mut self.bigints,
        };
        infallible(walk::root(&mut Length::default(), &mut edges, s, root));
        let roots = self.edges.len();
        let mut visited = 0;
        for at in 0..roots {
            let start = self.edges[at];
            if self.index[start.0 as usize] == UNVISITED {
                self.search(s, start, &mut visited, unit_bytes, leaf_bytes);
            }
        }
    }

    /// First visit: number the object, measure it and list its references.
    fn enter(&mut self, s: &Storage, id: ObjectId, visited: &mut u32, leaf_bytes: u64) {
        let at = id.0 as usize;
        self.index[at] = *visited;
        self.lowlink[at] = *visited;
        *visited += 1;
        self.stack.push(id);
        let start = self.edges.len();
        let mut length = Length::default();
        let mut edges = Edges {
            s,
            leaf_bytes,
            found: &mut self.edges,
            strings: &mut self.strings,
            bigints: &mut self.bigints,
        };
        infallible(walk::object(&mut length, &mut edges, s, &s.objects[at]));
        self.weight[at] = length.0;
        self.edge_range[at] = (
            u32::try_from(start).expect("bounded references"),
            u32::try_from(self.edges.len() - start).expect("bounded references"),
        );
        self.path.push((id, 0));
    }

    /// What a reference to a completed unit adds to the referrer's weight.
    fn reference_weight(&self, unit: u32) -> u64 {
        let unit = &self.units[unit as usize];
        if unit.cut {
            REFERENCE_WEIGHT
        } else {
            unit.weight
        }
    }

    fn search(
        &mut self,
        s: &Storage,
        start: ObjectId,
        visited: &mut u32,
        unit_bytes: u64,
        leaf_bytes: u64,
    ) {
        self.enter(s, start, visited, leaf_bytes);
        while let Some(top) = self.path.last_mut() {
            let (id, next) = *top;
            let at = id.0 as usize;
            let (first, count) = self.edge_range[at];
            if next < count {
                top.1 = next + 1;
                let target = self.edges[(first + next) as usize];
                let to = target.0 as usize;
                if self.index[to] == UNVISITED {
                    self.enter(s, target, visited, leaf_bytes);
                } else if self.unit[to] == NO_UNIT {
                    // Still on the stack: part of this object's unit.
                    self.lowlink[at] = self.lowlink[at].min(self.index[to]);
                } else {
                    let weight = self.reference_weight(self.unit[to]);
                    self.weight[at] = self.weight[at].saturating_add(weight);
                }
                continue;
            }
            self.path.pop();
            if self.lowlink[at] == self.index[at] {
                self.complete(s, id, unit_bytes, leaf_bytes);
            }
            if let Some((parent, _)) = self.path.last() {
                let parent = parent.0 as usize;
                self.lowlink[parent] = self.lowlink[parent].min(self.lowlink[at]);
                if self.unit[at] != NO_UNIT {
                    let weight = self.reference_weight(self.unit[at]);
                    self.weight[parent] = self.weight[parent].saturating_add(weight);
                }
            }
        }
    }

    /// `first` was the first of its unit to be visited, and the unit's other
    /// objects are above it on the stack.
    fn complete(&mut self, s: &Storage, first: ObjectId, unit_bytes: u64, leaf_bytes: u64) {
        let unit = u32::try_from(self.units.len()).expect("bounded objects");
        let start = self.members.len();
        let mut weight = 0_u64;
        loop {
            let member = self
                .stack
                .pop()
                .unwrap_or_else(|| unreachable!("a unit's first object is on the stack"));
            self.unit[member.0 as usize] = unit;
            self.members.push(member);
            weight = weight.saturating_add(self.weight[member.0 as usize]);
            if member == first {
                break;
            }
        }
        let cut = match &s.objects[first.0 as usize] {
            SnapshotObject::Declaration { .. } => false,
            SnapshotObject::Uint8Array { data, .. } => data.len() as u64 >= leaf_bytes,
            SnapshotObject::List { .. }
            | SnapshotObject::Map { .. }
            | SnapshotObject::Instance { .. }
            | SnapshotObject::Cell(_)
            | SnapshotObject::NonSnapshotableValue {}
            | SnapshotObject::Descriptive { .. }
            | SnapshotObject::Truncated(_) => weight >= unit_bytes,
        };
        self.units.push(Unit {
            members: start..self.members.len(),
            weight,
            cut,
        });
    }

    /// The object a cut unit's blob starts at: the one with the smallest
    /// digest of its own content, the first visited among equals.
    fn start(&mut self, s: &Storage, members: std::ops::Range<usize>) -> ObjectId {
        if let [only] = &self.members[members.clone()] {
            return *only;
        }
        let mut best: Option<(Digest, u32, ObjectId)> = None;
        for member in &self.members[members] {
            self.digests.clear();
            self.root.clear();
            let mut visitor = Hash::new(&mut self.root, &mut self.digests, HashDomain::CycleRoot);
            let object = &s.objects[member.0 as usize];
            infallible(walk::object(&mut visitor, &mut Unresolved, s, object));
            let digest = self.digests[0];
            let candidate = (digest, self.index[member.0 as usize], *member);
            if best.is_none_or(|(least, visited, _)| (digest.0, candidate.1) < (least.0, visited)) {
                best = Some(candidate);
            }
        }
        best.unwrap_or_else(|| unreachable!("a unit has a member"))
            .2
    }
}

/// Lists an object's references and marks the leaves cut where it holds them.
struct Edges<'a> {
    s: &'a Storage,
    leaf_bytes: u64,
    found: &'a mut Vec<ObjectId>,
    strings: &'a mut [bool],
    bigints: &'a mut [bool],
}
impl Resolver for Edges<'_> {
    fn member(&mut self, id: ObjectId) -> u32 {
        self.found.push(id);
        0
    }
    fn object(&mut self, id: ObjectId) -> Reference {
        self.found.push(id);
        Reference::Local(0)
    }
    fn string(&mut self, id: StringId) -> Option<u32> {
        let cut = self.s.strings[id.0 as usize].len() as u64 >= self.leaf_bytes;
        self.strings[id.0 as usize] |= cut;
        cut.then_some(0)
    }
    fn bigint(&mut self, id: BigintId) -> Option<u32> {
        let cut = bigint_limb_bytes(&self.s.bigints[id.0 as usize]) as u64 >= self.leaf_bytes;
        self.bigints[id.0 as usize] |= cut;
        cut.then_some(0)
    }
}

#[cfg(test)]
mod tests;
