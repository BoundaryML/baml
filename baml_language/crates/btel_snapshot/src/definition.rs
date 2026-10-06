//! Recorded class and enum definitions, one CAS blob per group of them.
//!
//! A capture that names a class or enum names its definition: the name, the
//! number of generic parameters, the metadata, and the fields or variants in
//! order. The runtime copies a definition out of the heap the first time a
//! recorded capture names its declaration and keeps the blob made here on the
//! declaration, so every later capture shares it.
//!
//! # Groups
//!
//! A definition's ID cannot depend on itself, so declarations that reach
//! each other through their field types are one group, encoded as one blob.
//! Members refer to each other by position. A definition outside the group is
//! named by its group's blob ID and its position there, and that blob is a
//! child of this one, so whatever stores this blob stores those first.
//!
//! # Canonical order
//!
//! Equal groups hash equally however the runtime found them. Members are
//! numbered in first-reference order from a starting member: walking the
//! members in that order, each field's type heads in order, a member takes
//! the next number the first time it is named. The start is the member whose
//! encoding, with references to members left out, has the smallest digest;
//! among members that share it, the one that makes the smallest encoding of
//! the whole group. The blob therefore depends on the group's content alone,
//! never on discovery order or runtime tags.
//!
//! Starts that make that same smallest encoding are members the group cannot
//! tell apart (an automorphism maps one to the other), and each start places
//! the members differently. A member's position, which is what a definition
//! outside the group names it by, is the smallest it takes from any of
//! those starts: the same for every member it cannot be told apart from,
//! whichever the runtime found first.
//!
//! # Encoding (blob format 5, root tag 2)
//!
//! After the root tag: a u32 member count, then each member: its kind (0
//! class, 1 enum), its Borsh `DeclarationName`, its metadata, then for a
//! class a u32 generic parameter count, a `@@stream.done` byte and its
//! fields, for an enum its variants, each sequence with a u32 count.
//! Metadata is a Borsh `Option<String>` each for the description, alias and
//! docstring, then a u32 count of `(key, value)` attribute strings. A field
//! is its name, its type as Borsh `TyTemplate<DefinitionHead>`, its metadata,
//! then `skip`, `@stream.done` and `@stream.must_exist` bytes; a variant its
//! name, metadata and `skip` byte. Strings are a u32 length and UTF-8. A head
//! is tag 0 and a u32 member position, tag 1, a `DeclarationName`, a 16-byte
//! group ID and a u32 position, or tag 2 and a `DeclarationName` (an
//! interface or type alias, which has no recorded definition). The child
//! table lists the groups tag 1 heads name, in first-use order, and the
//! object count is zero.
use std::{
    io::{self, Write},
    sync::Arc,
};

use baml_type::{DeclarationName, TyTemplate};
use borsh::{BorshDeserialize, BorshSerialize};
use btel_types::{Definition, DefinitionBlob};

use crate::{
    CasId,
    encoding::{BLOB_MAGIC, BLOB_VERSION},
    hash::{Absorb, Hasher},
    tags::{DefinitionHeadTag, DefinitionKind, HashDomain, RootTag},
};

/// One definition: the blob of its group and its position there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DefinitionRef {
    pub group: CasId,
    pub member: u32,
}

impl DefinitionRef {
    pub fn of(definition: &Definition) -> Self {
        Self {
            group: CasId::from_bytes(definition.group.id()),
            member: definition.member,
        }
    }
}

impl BorshSerialize for DefinitionRef {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(self.group.as_bytes())?;
        self.member.serialize(w)
    }
}

impl BorshDeserialize for DefinitionRef {
    fn deserialize_reader<R: io::Read>(r: &mut R) -> io::Result<Self> {
        let group = CasId::from_bytes(<[u8; 16]>::deserialize_reader(r)?);
        let member = u32::deserialize_reader(r)?;
        Ok(Self { group, member })
    }
}

/// A class or enum named by its recorded definition, with its name for
/// display. The definition is its identity; the name rides along.
#[derive(Clone, Debug)]
pub struct DefinedHead {
    pub name: DeclarationName,
    pub definition: DefinitionRef,
}

impl PartialEq for DefinedHead {
    fn eq(&self, other: &Self) -> bool {
        self.definition == other.definition
    }
}
impl Eq for DefinedHead {}
impl PartialOrd for DefinedHead {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for DefinedHead {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.definition.cmp(&other.definition)
    }
}
impl std::hash::Hash for DefinedHead {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.definition.hash(state);
    }
}

/// A head in a recorded definition's field type.
#[derive(Clone, Debug)]
pub enum DefinitionHead {
    /// Another member of the same group, by its position.
    Member(u32),
    /// A class or enum of another group.
    Defined(DefinedHead),
    /// A head with no recorded definition: an interface or a type alias.
    Named(DeclarationName),
}

impl PartialEq for DefinitionHead {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Member(a), Self::Member(b)) => a == b,
            (Self::Defined(a), Self::Defined(b)) => a == b,
            (Self::Named(a), Self::Named(b)) => a.to_string() == b.to_string(),
            (Self::Member(_) | Self::Defined(_) | Self::Named(_), _) => false,
        }
    }
}

impl BorshSerialize for DefinitionHead {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match self {
            Self::Member(position) => {
                (DefinitionHeadTag::Member as u8).serialize(w)?;
                position.serialize(w)
            }
            Self::Defined(head) => {
                (DefinitionHeadTag::Defined as u8).serialize(w)?;
                head.name.serialize(w)?;
                head.definition.serialize(w)
            }
            Self::Named(name) => {
                (DefinitionHeadTag::Named as u8).serialize(w)?;
                name.serialize(w)
            }
        }
    }
}

impl BorshDeserialize for DefinitionHead {
    fn deserialize_reader<R: io::Read>(r: &mut R) -> io::Result<Self> {
        match u8::deserialize_reader(r)? {
            0 => Ok(Self::Member(u32::deserialize_reader(r)?)),
            1 => {
                let name = DeclarationName::deserialize_reader(r)?;
                let definition = DefinitionRef::deserialize_reader(r)?;
                Ok(Self::Defined(DefinedHead { name, definition }))
            }
            2 => Ok(Self::Named(DeclarationName::deserialize_reader(r)?)),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("definition head tag {other}"),
            )),
        }
    }
}

/// A field type as recorded.
pub type DefinitionType = TyTemplate<DefinitionHead>;

/// A head in a field type, as the runtime hands a group over.
#[derive(Clone, Debug)]
pub enum Head {
    /// Another member of the group, by its position among the members
    /// handed over.
    Member(u32),
    /// A class or enum of a group made before.
    Defined(DeclarationName, Definition),
    /// A head with no recorded definition: an interface or a type alias.
    Named(DeclarationName),
}

/// Description, alias, docstring and other attributes, as declared.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    pub description: Option<String>,
    pub alias: Option<String>,
    pub docstring: Option<String>,
    pub attributes: Vec<(String, String)>,
}

/// A class's definition. `T` is how its field types are held.
#[derive(Clone, Debug)]
pub struct Class<T> {
    pub name: DeclarationName,
    pub type_params: u32,
    pub meta: Meta,
    /// `@@stream.done`.
    pub stream_done: bool,
    pub fields: Vec<Field<T>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field<T> {
    pub name: String,
    /// Generic parameter `N` is `TypeArgRef(N)`.
    pub ty: T,
    pub meta: Meta,
    pub skip: bool,
    /// `@stream.done`.
    pub stream_done: bool,
    /// `@stream.must_exist`.
    pub must_exist: bool,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub name: DeclarationName,
    pub meta: Meta,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub meta: Meta,
    pub skip: bool,
}

#[derive(Clone, Debug)]
pub enum Declaration<T> {
    Class(Class<T>),
    Enum(Enum),
}

impl<T> Declaration<T> {
    pub fn name(&self) -> &DeclarationName {
        match self {
            Self::Class(class) => &class.name,
            Self::Enum(enm) => &enm.name,
        }
    }
    fn fields(&self) -> &[Field<T>] {
        match self {
            Self::Class(class) => &class.fields,
            Self::Enum(_) => &[],
        }
    }
}

/// Names compare by spelling, as decoded declarations do.
impl<T: PartialEq> PartialEq for Declaration<T> {
    fn eq(&self, other: &Self) -> bool {
        let same_name = |a: &DeclarationName, b: &DeclarationName| a.to_string() == b.to_string();
        match (self, other) {
            (Self::Class(a), Self::Class(b)) => {
                same_name(&a.name, &b.name)
                    && a.type_params == b.type_params
                    && a.meta == b.meta
                    && a.stream_done == b.stream_done
                    && a.fields == b.fields
            }
            (Self::Enum(a), Self::Enum(b)) => {
                same_name(&a.name, &b.name) && a.meta == b.meta && a.variants == b.variants
            }
            (Self::Class(_) | Self::Enum(_), _) => false,
        }
    }
}

/// Make the blob of one group: `members` reach each other through their
/// field types, and name any other declaration by a definition made before.
/// Returns each member's definition, in the order given.
pub fn group(members: &[Declaration<TyTemplate<Head>>]) -> Vec<Definition> {
    assert!(!members.is_empty(), "a group has a member");
    // One member starts its group, at position 0: nothing to choose.
    if let [member] = members {
        let mut content = Vec::new();
        let mut children = Vec::new();
        put(&mut content, &1_u32);
        encode(member, Some(&[0]), &mut content, &mut children);
        return seal(&content, children, [0].into_iter());
    }
    let digests: Vec<[u8; 16]> = members
        .iter()
        .map(|member| {
            let mut content = Vec::new();
            encode(member, None, &mut content, &mut Vec::new());
            let mut h = Hasher::new(HashDomain::CycleRoot);
            h.absorb(&content);
            h.finish().0
        })
        .collect();
    let least = *digests
        .iter()
        .min()
        .unwrap_or_else(|| unreachable!("a member"));
    let mut best: Option<Encoding> = None;
    // The placements of every start that makes the best encoding so far.
    let mut tied: Vec<Vec<u32>> = Vec::new();
    for start in (0..members.len()).filter(|&at| digests[at] == least) {
        let (order, positions) = first_reference(members, start);
        let mut encoding = Encoding {
            content: Vec::new(),
            children: Vec::new(),
            positions,
        };
        put(
            &mut encoding.content,
            &u32::try_from(members.len()).expect("bounded group"),
        );
        for &at in &order {
            encode(
                &members[at],
                Some(&encoding.positions),
                &mut encoding.content,
                &mut encoding.children,
            );
        }
        match best
            .as_ref()
            .map(|least| encoding.content.cmp(&least.content))
        {
            None | Some(std::cmp::Ordering::Less) => {
                tied.clear();
                tied.push(encoding.positions.clone());
                best = Some(encoding);
            }
            Some(std::cmp::Ordering::Equal) => tied.push(encoding.positions),
            Some(std::cmp::Ordering::Greater) => {}
        }
    }
    let Encoding {
        content, children, ..
    } = best.unwrap_or_else(|| unreachable!("a start"));
    let positions = (0..members.len()).map(|member| {
        tied.iter()
            .map(|positions| positions[member])
            .min()
            .unwrap_or_else(|| unreachable!("a start"))
    });
    seal(&content, children, positions)
}

/// The blob of a group whose content is `content` and which names
/// `children`, and each member's definition at `positions`.
fn seal(
    content: &[u8],
    children: Vec<Arc<DefinitionBlob>>,
    positions: impl Iterator<Item = u32>,
) -> Vec<Definition> {
    let mut h = Hasher::new(HashDomain::Blob);
    h.byte(RootTag::Definitions as u8);
    h.absorb(content);
    h.size(children.len());
    for child in &children {
        h.absorb(&child.id());
    }
    h.size(0);
    let id = h.finish().0;

    let mut bytes =
        Vec::with_capacity(8 + 4 + 16 + 4 + 16 * children.len() + 4 + 1 + content.len());
    bytes.extend_from_slice(&BLOB_MAGIC);
    bytes.extend_from_slice(&BLOB_VERSION.to_le_bytes());
    bytes.extend_from_slice(&id);
    let child_count = u32::try_from(children.len()).expect("bounded children");
    bytes.extend_from_slice(&child_count.to_le_bytes());
    for child in &children {
        bytes.extend_from_slice(&child.id());
    }
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.push(RootTag::Definitions as u8);
    bytes.extend_from_slice(content);
    let blob = Arc::new(DefinitionBlob::new(
        id,
        bytes.into_boxed_slice(),
        children.into_boxed_slice(),
    ));
    positions
        .map(|member| Definition {
            group: Arc::clone(&blob),
            member,
        })
        .collect()
}

/// A group's content from one starting member: the groups it names, and
/// each member's place.
struct Encoding {
    content: Vec<u8>,
    children: Vec<Arc<DefinitionBlob>>,
    positions: Vec<u32>,
}

/// Members in first-reference order from `start`, and each member's place
/// in it. A member nothing reaches, which a group never has, comes after
/// the rest, in the order given.
fn first_reference(
    members: &[Declaration<TyTemplate<Head>>],
    start: usize,
) -> (Vec<usize>, Vec<u32>) {
    let unplaced = u32::MAX;
    let mut positions = vec![unplaced; members.len()];
    let mut order = vec![start];
    positions[start] = 0;
    let mut next = 0;
    loop {
        while let Some(&at) = order.get(next) {
            for field in members[at].fields() {
                field.ty.visit_heads(&mut |head| {
                    if let Head::Member(member) = head
                        && let Some(position) = positions.get_mut(*member as usize)
                        && *position == unplaced
                    {
                        *position = u32::try_from(order.len()).expect("bounded group");
                        order.push(*member as usize);
                    }
                });
            }
            next += 1;
        }
        match positions.iter().position(|position| *position == unplaced) {
            Some(at) => {
                positions[at] = u32::try_from(order.len()).expect("bounded group");
                order.push(at);
            }
            None => break,
        }
    }
    (order, positions)
}

fn put(out: &mut Vec<u8>, value: &impl BorshSerialize) {
    value.serialize(out).expect("writing to memory");
}

/// One member's encoding. With `positions`, a member reference is written
/// as its place; without, references to members are left out, as the
/// starting member is chosen. Groups named for the first time are added to
/// `children`.
fn encode(
    member: &Declaration<TyTemplate<Head>>,
    positions: Option<&[u32]>,
    out: &mut Vec<u8>,
    children: &mut Vec<Arc<DefinitionBlob>>,
) {
    let meta = |out: &mut Vec<u8>, meta: &Meta| {
        put(out, &meta.description);
        put(out, &meta.alias);
        put(out, &meta.docstring);
        put(
            out,
            &u32::try_from(meta.attributes.len()).expect("bounded attributes"),
        );
        for (key, value) in &meta.attributes {
            put(out, key);
            put(out, value);
        }
    };
    let count = |out: &mut Vec<u8>, n: usize| put(out, &u32::try_from(n).expect("bounded"));
    match member {
        Declaration::Class(class) => {
            out.push(DefinitionKind::Class as u8);
            put(out, &class.name);
            meta(out, &class.meta);
            put(out, &class.type_params);
            put(out, &class.stream_done);
            count(out, class.fields.len());
            for field in &class.fields {
                put(out, &field.name);
                let ty = field.ty.map_heads(&mut |head| match head {
                    Head::Member(member) => DefinitionHead::Member(
                        positions.map_or(u32::MAX, |positions| positions[*member as usize]),
                    ),
                    Head::Defined(name, definition) => DefinitionHead::Defined(DefinedHead {
                        name: name.clone(),
                        definition: DefinitionRef::of(definition),
                    }),
                    Head::Named(name) => DefinitionHead::Named(name.clone()),
                });
                put(out, &ty);
                // The same walk as decoding's, so the child table's order is
                // the one a reader checks.
                field.ty.visit_heads(&mut |head| {
                    if let Head::Defined(_, definition) = head
                        && !children
                            .iter()
                            .any(|child| child.id() == definition.group.id())
                    {
                        children.push(Arc::clone(&definition.group));
                    }
                });
                meta(out, &field.meta);
                put(out, &field.skip);
                put(out, &field.stream_done);
                put(out, &field.must_exist);
            }
        }
        Declaration::Enum(enm) => {
            out.push(DefinitionKind::Enum as u8);
            put(out, &enm.name);
            meta(out, &enm.meta);
            count(out, enm.variants.len());
            for variant in &enm.variants {
                put(out, &variant.name);
                meta(out, &variant.meta);
                put(out, &variant.skip);
            }
        }
    }
}

#[cfg(test)]
mod tests;
