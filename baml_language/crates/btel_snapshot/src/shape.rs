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
//! The blob for a value is what capturing that value alone would produce, and
//! no object is stored in two blobs of one capture:
//!
//! - Objects that reach each other (a cycle) are one unit and never separate.
//! - A unit's region is what it reaches without entering a cut unit, and its
//!   weight is the encoded size of that region, each object once, plus
//!   [`REFERENCE_WEIGHT`] for every reference to a cut unit.
//! - A unit is closed when no object of its region, other than its own, is
//!   referenced from outside the region. Only a closed unit is cut, and it is
//!   when its weight reaches the policy's unit size. Its blob is its region,
//!   and no other blob holds any of it.
//! - A string, bigint or `uint8array` is cut when its content reaches the
//!   leaf size; a `uint8array` never by its weight.
//! - Declarations are never cut, and an instance or enum value naming one adds
//!   nothing to the weight: every blob carries the declarations it names.
//!
//! So a value that shares an object with the rest of the capture stays in
//! the blob of whatever holds both, however large the value is: sharing costs
//! a cut, never a copy. Whether a value is cut therefore depends on the
//! capture. What its blob holds does not: every reference to the inside of a
//! cut unit comes from its own region, in any capture that reaches it.
//!
//! Finding the units costs a pass over every object, which most captures do
//! not need: with nothing cut, no unit weighs more than the capture encodes
//! to, so a capture whose size bound is under the unit size and whose largest
//! leaf is under the leaf size is one blob without the pass.
//!
//! A cut cycle's blob starts at the member with the smallest
//! [`HashDomain::CycleRoot`] digest, so captures that reach the cycle from
//! outside produce the same blob, unless several members share that digest:
//! the first visited then starts it, and the blob can differ between
//! captures. Other blobs name its other members by blob and local number. A
//! capture's own root blob always starts at the captured value.
use rustc_hash::FxHashMap;

use super::{
    BigintId, BlobEntry, BlobIndex, CasId, Home, ObjectId, Range, SnapshotObject, SnapshotRoot,
    SnapshotValue, Storage, StringId, grow,
    hash::{Absorb, Digest, Hasher},
    tags::HashDomain,
    walk::{self, Both, Length, Reference, Resolver, bigint_limb_bytes, infallible},
};

/// Where a capture is cut into blobs. The policy decides blob IDs, never
/// whether a blob can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapePolicy {
    /// One blob per capture.
    Whole,
    /// Cut values that reach these encoded sizes into blobs of their own.
    Split {
        /// Weight at which an object and what it reaches is cut.
        unit_bytes: u64,
        /// Content length at which a string, bigint or `uint8array` is cut.
        leaf_bytes: u64,
    },
}

impl Default for ShapePolicy {
    /// The policy captures are shaped with:
    /// [`SPLIT_UNIT_BYTES`](btel_settings::snapshot::SPLIT_UNIT_BYTES) and
    /// [`SPLIT_LEAF_BYTES`](btel_settings::snapshot::SPLIT_LEAF_BYTES).
    fn default() -> Self {
        Self::Split {
            unit_bytes: btel_settings::snapshot::SPLIT_UNIT_BYTES,
            leaf_bytes: btel_settings::snapshot::SPLIT_LEAF_BYTES,
        }
    }
}

/// Weight of a reference to a cut unit: its entry in the child table. A
/// reference to a cut string or bigint weighs its encoded reference.
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
        let bound = s.encoded_bound();
        match self.policy {
            ShapePolicy::Whole => {}
            ShapePolicy::Split {
                unit_bytes,
                leaf_bytes,
            } => {
                if bound >= unit_bytes || s.largest_leaf as u64 >= leaf_bytes {
                    self.cut(s, root, unit_bytes, leaf_bytes);
                } else {
                    #[cfg(debug_assertions)]
                    self.check_nothing_to_cut(s, root, unit_bytes, leaf_bytes);
                }
            }
        }
        let index = self.blob(s, root, 0..0);
        debug_assert_eq!(
            index.0 as usize + 1,
            s.blobs.len(),
            "a capture's root blob holds its children, so it equals none of them"
        );
        debug_assert!(
            s.blobs.len() > 1 || s.blobs[0].encoded_len <= bound,
            "a capture's size bound covers its blob"
        );
        #[cfg(debug_assertions)]
        Self::check_nothing_copied(s);
    }

    /// Only closed units are cut, so no object is in two blobs. Declarations
    /// are, in every blob that names them.
    #[cfg(debug_assertions)]
    fn check_nothing_copied(s: &Storage) {
        let mut held = vec![false; s.objects.len()];
        for member in &s.members {
            let object = &s.objects[member.0 as usize];
            if !matches!(object, SnapshotObject::Declaration { .. }) {
                assert!(
                    !std::mem::replace(&mut held[member.0 as usize], true),
                    "an object is stored in one blob of its capture"
                );
            }
        }
    }

    /// The pass a capture under both thresholds skips would have cut nothing.
    #[cfg(debug_assertions)]
    fn check_nothing_to_cut(
        &mut self,
        s: &Storage,
        root: SnapshotRoot,
        unit_bytes: u64,
        leaf_bytes: u64,
    ) {
        self.cuts.find(s, root, unit_bytes, leaf_bytes);
        assert!(
            self.cuts.units.iter().all(|unit| !unit.cut)
                && !self.cuts.strings.contains(&true)
                && !self.cuts.bigints.contains(&true),
            "a capture under the size bound has nothing to cut"
        );
    }

    /// Shape every blob below the capture's root blob, children first.
    fn cut(&mut self, s: &mut Storage, root: SnapshotRoot, unit_bytes: u64, leaf_bytes: u64) {
        self.cuts.find(s, root, unit_bytes, leaf_bytes);
        // The root's own unit is the root blob, wherever its weight falls.
        let root_unit = match root {
            SnapshotRoot::Value(SnapshotValue::Object(id)) => {
                Some(self.cuts.nodes[id.0 as usize].unit)
            }
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
            children,
            slots,
            ids,
            cuts,
            policy: _,
        } = self;
        slots.resize(s.blobs.len(), UNNUMBERED);
        order.clear();
        children.clear();
        let (mut h, content_bytes) = {
            let s: &Storage = s;
            let mut numbering = Numbering {
                s,
                local: &mut *local,
                order: &mut *order,
                children: &mut *children,
                slots: &mut *slots,
            };
            let mut visitor = Both(Hasher::new(HashDomain::Blob), Length::default());
            infallible(walk::root(&mut visitor, &mut numbering, s, root));
            // Hashing a definition numbers its new references, which extends
            // `order` with the objects to hash next.
            let mut next = 0;
            while let Some(&id) = numbering.order.get(next) {
                let object = &s.objects[id.0 as usize];
                infallible(walk::object(&mut visitor, &mut numbering, s, object));
                next += 1;
            }
            let Both(h, Length(content_bytes)) = visitor;
            (h, content_bytes)
        };
        // The child table is complete only now; the ID covers it last.
        h.size(children.len());
        for child in children.iter() {
            h.absorb(s.blobs[child.0 as usize].id.as_bytes());
        }
        h.size(order.len());
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
    /// Range of [`Cuts::members`]. The first visited is the last listed.
    members: std::ops::Range<usize>,
    /// The encoded size of its region.
    weight: u64,
    cut: bool,
    /// What the uncut units discovered from its objects bring to its region.
    inner: Bounds,
}

const UNVISITED: u32 = u32::MAX;
const NO_UNIT: u32 = u32::MAX;
const NO_PARENT: u32 = u32::MAX;
/// As the last referrer: the capture's root references the object.
const OUTSIDE: u32 = u32::MAX;

/// The discovery numbers that decide whether part of a capture is closed.
#[derive(Clone, Copy, Debug)]
struct Bounds {
    /// The least and the greatest number of an object that references one of
    /// the part's objects, [`OUTSIDE`] as the greatest when the root does.
    first_referrer: u32,
    last_referrer: u32,
    /// The least number of an uncut object the part references.
    first_target: u32,
}
impl Bounds {
    const NONE: Self = Self {
        first_referrer: u32::MAX,
        last_referrer: 0,
        first_target: u32::MAX,
    };

    fn join(&mut self, other: Self) {
        self.first_referrer = self.first_referrer.min(other.first_referrer);
        self.last_referrer = self.last_referrer.max(other.last_referrer);
        self.first_target = self.first_target.min(other.first_target);
    }

    /// Whether every referrer and target was discovered by the visit that
    /// took the numbers `first..=last`.
    fn within(self, first: u32, last: u32) -> bool {
        self.first_referrer >= first && self.last_referrer <= last && self.first_target >= first
    }
}

/// One object's state during the search, kept together for locality.
#[derive(Clone, Copy)]
struct Node {
    /// Discovery number, [`UNVISITED`] before its visit.
    index: u32,
    lowlink: u32,
    /// Its unit, [`NO_UNIT`] until that completes.
    unit: u32,
    /// The last number its visit gave out: it discovered `index..=last`.
    last: u32,
    /// The object whose reference discovered it, [`NO_PARENT`] when the
    /// capture's root did.
    parent: u32,
    /// The least and the greatest number of an object that references it,
    /// [`OUTSIDE`] as the greatest when the root does.
    first_referrer: u32,
    last_referrer: u32,
    /// Its range of [`Cuts::edges`].
    first_edge: u32,
    edge_count: u32,
    /// Its encoded size.
    size: u64,
}
const FRESH: Node = Node {
    index: UNVISITED,
    lowlink: 0,
    unit: NO_UNIT,
    last: 0,
    parent: NO_PARENT,
    first_referrer: u32::MAX,
    last_referrer: 0,
    first_edge: 0,
    edge_count: 0,
    size: 0,
};

/// Which values of one capture get blobs of their own.
///
/// One depth-first pass (Tarjan's algorithm, without recursion) finds the
/// units in an order where each follows every unit it references, and numbers
/// the objects in the order it discovers them. An object's visit discovers a
/// run of numbers, and a closed unit's region is exactly what its first
/// object's visit discovered outside cut units. So a unit is closed when
/// nothing in that run references an uncut object numbered before it, and
/// nothing numbered outside it references an object of the run. Both need
/// every reference, so the cuts are decided once the pass is done.
#[derive(Default)]
struct Cuts {
    nodes: Vec<Node>,
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
}

impl Cuts {
    fn find(&mut self, s: &Storage, root: SnapshotRoot, unit_bytes: u64, leaf_bytes: u64) {
        self.nodes.clear();
        self.nodes.resize(s.objects.len(), FRESH);
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
        if let SnapshotRoot::Value(SnapshotValue::Object(id)) = root {
            self.edges.push(id);
        }
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
            let node = &mut self.nodes[start.0 as usize];
            if node.index == UNVISITED {
                self.search(s, start, &mut visited, leaf_bytes);
            } else {
                node.last_referrer = OUTSIDE;
            }
        }
        self.decide(s, unit_bytes, leaf_bytes);
    }

    /// First visit: number the object, measure it and list its references.
    /// `from` references it, or the capture's root does.
    fn enter(
        &mut self,
        s: &Storage,
        id: ObjectId,
        from: Option<ObjectId>,
        visited: &mut u32,
        leaf_bytes: u64,
    ) {
        let at = id.0 as usize;
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
        let (parent, first_referrer, last_referrer) = match from {
            Some(parent) => {
                let referrer = self.nodes[parent.0 as usize].index;
                (parent.0, referrer, referrer)
            }
            None => (NO_PARENT, u32::MAX, OUTSIDE),
        };
        self.nodes[at] = Node {
            index: *visited,
            lowlink: *visited,
            unit: NO_UNIT,
            last: *visited,
            parent,
            first_referrer,
            last_referrer,
            first_edge: u32::try_from(start).expect("bounded references"),
            edge_count: u32::try_from(self.edges.len() - start).expect("bounded references"),
            size: length.0,
        };
        *visited += 1;
        self.stack.push(id);
        self.path.push((id, 0));
    }

    fn search(&mut self, s: &Storage, start: ObjectId, visited: &mut u32, leaf_bytes: u64) {
        self.enter(s, start, None, visited, leaf_bytes);
        while let Some(top) = self.path.last_mut() {
            let (id, next) = *top;
            let at = id.0 as usize;
            let Node {
                index,
                first_edge,
                edge_count,
                ..
            } = self.nodes[at];
            if next < edge_count {
                top.1 = next + 1;
                let target = self.edges[(first_edge + next) as usize];
                let to = &mut self.nodes[target.0 as usize];
                if to.index == UNVISITED {
                    self.enter(s, target, Some(id), visited, leaf_bytes);
                    continue;
                }
                to.first_referrer = to.first_referrer.min(index);
                to.last_referrer = to.last_referrer.max(index);
                if to.unit == NO_UNIT {
                    // Still on the stack: part of this object's unit.
                    let reached = to.index;
                    let node = &mut self.nodes[at];
                    node.lowlink = node.lowlink.min(reached);
                }
                continue;
            }
            self.path.pop();
            self.nodes[at].last = *visited - 1;
            if self.nodes[at].lowlink == index {
                self.complete(id);
            }
            if let Some((parent, _)) = self.path.last() {
                let lowlink = self.nodes[at].lowlink;
                let node = &mut self.nodes[parent.0 as usize];
                node.lowlink = node.lowlink.min(lowlink);
            }
        }
    }

    /// `first` was the first of its unit to be visited, and the unit's other
    /// objects are above it on the stack.
    fn complete(&mut self, first: ObjectId) {
        let unit = u32::try_from(self.units.len()).expect("bounded objects");
        let start = self.members.len();
        loop {
            let member = self
                .stack
                .pop()
                .unwrap_or_else(|| unreachable!("a unit's first object is on the stack"));
            self.nodes[member.0 as usize].unit = unit;
            self.members.push(member);
            if member == first {
                break;
            }
        }
        self.units.push(Unit {
            members: start..self.members.len(),
            weight: 0,
            cut: false,
            inner: Bounds::NONE,
        });
    }

    /// Weigh every unit and cut the closed ones that reach the unit size. In
    /// the order the units completed, each follows the units it references,
    /// and an uncut one hands its region to the unit that discovered it.
    fn decide(&mut self, s: &Storage, unit_bytes: u64, leaf_bytes: u64) {
        for unit in 0..self.units.len() {
            let Unit {
                members,
                mut weight,
                inner,
                cut: _,
            } = self.units[unit].clone();
            let first = self.members[members.end - 1];
            let Node {
                index,
                last,
                parent,
                ..
            } = self.nodes[first.0 as usize];
            // The unit's own objects may be referenced from anywhere. What
            // references them matters to the region that holds the unit.
            let mut region = inner;
            let mut own = Bounds::NONE;
            for member in &self.members[members] {
                let node = self.nodes[member.0 as usize];
                weight = weight.saturating_add(node.size);
                own.first_referrer = own.first_referrer.min(node.first_referrer);
                own.last_referrer = own.last_referrer.max(node.last_referrer);
                let edges = node.first_edge as usize..(node.first_edge + node.edge_count) as usize;
                for target in &self.edges[edges] {
                    let to = self.nodes[target.0 as usize];
                    if to.unit as usize == unit {
                        continue;
                    }
                    if self.units[to.unit as usize].cut {
                        weight = weight.saturating_add(REFERENCE_WEIGHT);
                    } else {
                        region.first_target = region.first_target.min(to.index);
                    }
                }
            }
            let closed = region.within(index, last);
            let cut = closed
                && match &s.objects[first.0 as usize] {
                    SnapshotObject::Declaration { .. } => false,
                    SnapshotObject::Uint8Array { data, .. } => data.len() as u64 >= leaf_bytes,
                    SnapshotObject::List { .. }
                    | SnapshotObject::Map { .. }
                    | SnapshotObject::Instance { .. }
                    | SnapshotObject::Cell(_)
                    | SnapshotObject::NonSnapshotableValue {}
                    | SnapshotObject::Descriptive { .. }
                    | SnapshotObject::Media { .. }
                    | SnapshotObject::Truncated(_) => weight >= unit_bytes,
                };
            self.units[unit].weight = weight;
            self.units[unit].cut = cut;
            if !cut && parent != NO_PARENT {
                let holder = self.nodes[parent as usize].unit as usize;
                debug_assert!(holder > unit, "a unit completes before what discovered it");
                region.join(own);
                let holder = &mut self.units[holder];
                holder.weight = holder.weight.saturating_add(weight);
                holder.inner.join(region);
            }
        }
    }

    /// The object a cut unit's blob starts at: the one with the smallest
    /// digest of its own content, the first visited among equals.
    fn start(&self, s: &Storage, members: std::ops::Range<usize>) -> ObjectId {
        if let [only] = &self.members[members.clone()] {
            return *only;
        }
        let mut best: Option<(Digest, u32, ObjectId)> = None;
        for member in &self.members[members] {
            let mut h = Hasher::new(HashDomain::CycleRoot);
            let object = &s.objects[member.0 as usize];
            infallible(walk::object(&mut h, &mut Unresolved, s, object));
            let digest = h.finish();
            let candidate = (digest, self.nodes[member.0 as usize].index, *member);
            if best.is_none_or(|(least, visited, _)| (digest.0, candidate.1) < (least.0, visited)) {
                best = Some(candidate);
            }
        }
        best.unwrap_or_else(|| unreachable!("a unit has a member"))
            .2
    }
}

/// Lists an object's references and marks the leaves cut where it holds them.
/// Declarations are named, never referenced: they belong to no unit.
struct Edges<'a> {
    s: &'a Storage,
    leaf_bytes: u64,
    found: &'a mut Vec<ObjectId>,
    strings: &'a mut [bool],
    bigints: &'a mut [bool],
}
impl Resolver for Edges<'_> {
    fn member(&mut self, _: ObjectId) -> u32 {
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
