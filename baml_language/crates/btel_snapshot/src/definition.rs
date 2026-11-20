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
//! class, 1 enum), its name, its metadata, then for a class a u32 generic
//! parameter count, a `@@stream.done` byte and its fields, for an enum its
//! variants, each sequence with a u32 count. Metadata is an optional string
//! each for the description, alias and docstring, then a u32 count of
//! `(key, value)` attribute strings. A field is its name, its type, its
//! metadata, then `skip`, `@stream.done` and `@stream.must_exist` bytes; a
//! variant its name, metadata and `skip` byte. A head in a field type is
//! another member by its position, a definition made before by its name, its
//! group's ID and its position, or a name alone (an interface or type alias,
//! which has no recorded definition). Every piece of that is written by
//! `btel_core::cas::encode`, whose layout `btel_core::cas::decode_blob` reads; the
//! order of the members and the group's start are decided here. The child
//! table lists the groups those heads name, in first-use order, and the
//! object count is zero.
use std::{borrow::Borrow, sync::Arc};

use baml_type::{DeclarationName, TyTemplate};
pub use btel_core::cas::DefinitionRef;
use btel_core::cas::{
    encode::{self, encode_type, infallible},
    hash::{Absorb, Hasher},
    tags::{DefinitionKind, HashDomain, RootTag},
};
use btel_types::{Definition, DefinitionBlob};

use crate::describe::{self, Placed, Placement};

/// A head in a field type, as the runtime hands a group over, borrowed from
/// the declarations it names.
#[derive(Clone, Debug)]
pub enum Head<'a> {
    /// Another member of the group, by its position among the members
    /// handed over.
    Member(u32),
    /// A class or enum of a group made before, and its name.
    Defined(DeclarationName, &'a Definition),
    /// A head with no recorded definition: an interface or a type alias.
    Named(DeclarationName),
}

/// Description, alias, docstring and other attributes, as declared. `S` is
/// how its text is held: owned once decoded, borrowed from the declaration
/// while the runtime hands a group over. Either encodes the same.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta<S = String> {
    pub description: Option<S>,
    pub alias: Option<S>,
    pub docstring: Option<S>,
    pub attributes: Vec<(S, S)>,
}

/// A class's definition. `T` is how its field types are held, `S` its text
/// (see [`Meta`]) and `N` its name.
#[derive(Clone, Debug)]
pub struct Class<T, S = String, N = DeclarationName> {
    pub name: N,
    pub type_params: u32,
    pub meta: Meta<S>,
    /// `@@stream.done`.
    pub stream_done: bool,
    pub fields: Vec<Field<T, S>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field<T, S = String> {
    pub name: S,
    /// Generic parameter `N` is `TypeArgRef(N)`.
    pub ty: T,
    pub meta: Meta<S>,
    pub skip: bool,
    /// `@stream.done`.
    pub stream_done: bool,
    /// `@stream.must_exist`.
    pub must_exist: bool,
}

#[derive(Clone, Debug)]
pub struct Enum<S = String, N = DeclarationName> {
    pub name: N,
    pub meta: Meta<S>,
    pub variants: Vec<Variant<S>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant<S = String> {
    pub name: S,
    pub meta: Meta<S>,
    pub skip: bool,
}

#[derive(Clone, Debug)]
pub enum Declaration<T, S = String, N = DeclarationName> {
    Class(Class<T, S, N>),
    Enum(Enum<S, N>),
}

impl<T, S, N> Declaration<T, S, N> {
    pub fn name(&self) -> &N {
        match self {
            Self::Class(class) => &class.name,
            Self::Enum(enm) => &enm.name,
        }
    }
    fn fields(&self) -> &[Field<T, S>] {
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

/// A group member as the runtime hands it over.
pub type Member<'a, S, N> = Declaration<TyTemplate<Head<'a>>, S, N>;

/// What a group's content starts out holding: room for a small one.
const CONTENT_CAPACITY: usize = 256;

/// Make the blob of one group: `members` reach each other through their
/// field types, and name any other declaration by a definition made before.
/// Returns each member's definition, in the order given.
pub fn group<S, N>(members: &[Member<'_, S, N>]) -> Vec<Definition>
where
    S: AsRef<str>,
    N: Borrow<DeclarationName>,
{
    assert!(!members.is_empty(), "a group has a member");
    // One member starts its group, at position 0: nothing to choose.
    if let [member] = members {
        return vec![single(member)];
    }
    let mut content = Vec::with_capacity(CONTENT_CAPACITY);
    let mut children = Vec::new();
    let digests: Vec<[u8; 16]> = members
        .iter()
        .map(|member| {
            content.clear();
            children.clear();
            encode(member, Placement::Unplaced, &mut content, &mut children);
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
        let capacity = best
            .as_ref()
            .map_or(CONTENT_CAPACITY, |best| best.content.len());
        let mut encoding = Encoding {
            content: Vec::with_capacity(capacity),
            children: Vec::new(),
            positions,
        };
        infallible(encode::count(&mut encoding.content, members.len()));
        for &at in &order {
            encode(
                &members[at],
                Placement::At(&encoding.positions),
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
    let blob = seal(&content, children);
    (0..members.len())
        .map(|member| Definition {
            group: Arc::clone(&blob),
            member: tied
                .iter()
                .map(|positions| positions[member])
                .min()
                .unwrap_or_else(|| unreachable!("a start")),
        })
        .collect()
}

/// [`group`] of one member: a declaration that names no declaration naming
/// it back, so that a reference to a member is to itself, at position 0.
pub fn single<S, N>(member: &Member<'_, S, N>) -> Definition
where
    S: AsRef<str>,
    N: Borrow<DeclarationName>,
{
    let mut content = Vec::with_capacity(CONTENT_CAPACITY);
    let mut children = Vec::new();
    infallible(encode::u32(&mut content, 1));
    encode(member, Placement::At(&[0]), &mut content, &mut children);
    Definition {
        group: seal(&content, children),
        member: 0,
    }
}

/// The blob of a group whose content is `content` and which names
/// `children`.
fn seal(content: &[u8], children: Vec<Arc<DefinitionBlob>>) -> Arc<DefinitionBlob> {
    let ids = || {
        children
            .iter()
            .map(|child| btel_core::cas::CasId::from_bytes(child.id()))
    };
    let mut h = Hasher::new(HashDomain::Definitions);
    h.byte(RootTag::Definitions as u8);
    h.absorb(content);
    h.table(children.len(), ids(), 0);
    let id = h.finish().0;

    let framing = usize::try_from(encode::framing_len(children.len())).expect("bounded children");
    let mut bytes = Vec::with_capacity(framing + 1 + content.len());
    infallible(encode::header(
        &mut bytes,
        btel_core::cas::CasId::from_bytes(id),
        children.len(),
        ids(),
        0,
    ));
    bytes.push(RootTag::Definitions as u8);
    bytes.extend_from_slice(content);
    Arc::new(DefinitionBlob::new(
        id,
        bytes.into_boxed_slice(),
        children.into_boxed_slice(),
    ))
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
fn first_reference<S, N>(members: &[Member<'_, S, N>], start: usize) -> (Vec<usize>, Vec<u32>) {
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

/// One member's encoding. Groups named for the first time are added to
/// `children`.
fn encode<S, N>(
    member: &Member<'_, S, N>,
    placement: Placement<'_>,
    out: &mut Vec<u8>,
    children: &mut Vec<Arc<DefinitionBlob>>,
) where
    S: AsRef<str>,
    N: Borrow<DeclarationName>,
{
    let text = |out: &mut Vec<u8>, text: &S| infallible(encode::string(out, text.as_ref()));
    let optional = |out: &mut Vec<u8>, text: &Option<S>| {
        infallible(encode::optional_string(
            out,
            text.as_ref().map(AsRef::as_ref),
        ));
    };
    let meta = |out: &mut Vec<u8>, meta: &Meta<S>| {
        optional(out, &meta.description);
        optional(out, &meta.alias);
        optional(out, &meta.docstring);
        infallible(encode::count(out, meta.attributes.len()));
        for (key, value) in &meta.attributes {
            text(out, key);
            text(out, value);
        }
    };
    let flag = |out: &mut Vec<u8>, value: bool| infallible(encode::flag(out, value));
    let name = |out: &mut Vec<u8>, name: &N| infallible(describe::name(name.borrow()).write(out));
    match member {
        Declaration::Class(class) => {
            out.push(DefinitionKind::Class as u8);
            name(out, &class.name);
            meta(out, &class.meta);
            infallible(encode::u32(out, class.type_params));
            flag(out, class.stream_done);
            infallible(encode::count(out, class.fields.len()));
            for field in &class.fields {
                text(out, &field.name);
                infallible(encode_type(
                    Placed {
                        ty: &field.ty,
                        at: placement,
                    },
                    out,
                ));
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
                flag(out, field.skip);
                flag(out, field.stream_done);
                flag(out, field.must_exist);
            }
        }
        Declaration::Enum(enm) => {
            out.push(DefinitionKind::Enum as u8);
            name(out, &enm.name);
            meta(out, &enm.meta);
            infallible(encode::count(out, enm.variants.len()));
            for variant in &enm.variants {
                text(out, &variant.name);
                meta(out, &variant.meta);
                flag(out, variant.skip);
            }
        }
    }
}

#[cfg(test)]
mod tests;
