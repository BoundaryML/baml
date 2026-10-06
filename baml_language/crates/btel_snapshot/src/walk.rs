//! The one enumeration of a blob's content, in encoding order.
//!
//! Hashing, numbering, measuring and writing a blob all run this walk with a
//! different [`Visitor`] and [`Resolver`], so they cannot disagree about what
//! a blob contains or in which order. A visitor decides what each piece
//! becomes (hash input, bytes, a length); a resolver decides how a reference
//! is written (a local number, or a position in another blob).
//!
//! Only values (including map keys) may name another blob by its slot. Enum
//! variant names, descriptive names, media MIME types, URLs and paths, types
//! and declarations are always written in place, and a blob's own root value
//! is never a reference to another blob. Media content is written as a value.
//! A type head or declaration that names a recorded definition holds its
//! group's ID in place; the walk reports it where it is written, so the
//! groups a blob names are found in encoding order.
use std::io::{self, Write};

use baml_type::typetag::TypeTag;
use bex_str::BexStr;
use borsh::BorshSerialize;
use num_bigint::BigInt;

use crate::{
    CasId, TypeIdentity,
    graph::{
        BigintId, Declared, FieldEntry, Graph, LabelId, MapEntry, MediaSource, ObjectId, OwnedType,
        Range, SnapshotObject, SnapshotRoot, SnapshotValue, StringId, TypeId,
    },
    hash::{self, Absorb, Counter, Digest, Hasher, TypeLeaf},
    tags::{self, DeclarationTag, MediaSourceTag, ObjectTag, RootTag, ValueTag},
};

/// How a declaration identifies itself, written in place: by its recorded
/// definition, or by its runtime tag.
pub(crate) struct DeclarationIdentity<'a> {
    pub(crate) tag: TypeTag,
    pub(crate) declared: &'a Declared,
}
impl BorshSerialize for DeclarationIdentity<'_> {
    fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
        match &self.declared.definition {
            Some(definition) => {
                (DeclarationTag::Defined as u8).serialize(w)?;
                self.declared.name.serialize(w)?;
                definition.serialize(w)
            }
            None => {
                (DeclarationTag::Tagged as u8).serialize(w)?;
                self.tag.serialize(w)?;
                self.declared.name.serialize(w)
            }
        }
    }
}

/// How a reference to an object is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// A member of this blob, by its local number.
    Local(u32),
    /// The root value of the child blob in this slot of the child table.
    Child(u32),
    /// Object `node` of the child blob in `slot`. Only a cycle has objects
    /// other than its root that another blob can name.
    ChildNode { slot: u32, node: u32 },
}

/// Decides how references are written in the blob being walked.
pub(crate) trait Resolver {
    /// The local number of an object that is always a member of this blob:
    /// its root object, or a declaration.
    fn member(&mut self, id: ObjectId) -> u32;
    fn object(&mut self, id: ObjectId) -> Reference;
    /// The child slot of a string stored in its own blob, or `None` when it
    /// is written in place.
    fn string(&mut self, id: StringId) -> Option<u32>;
    /// As [`Self::string`], for a bigint.
    fn bigint(&mut self, id: BigintId) -> Option<u32>;
    /// A type head or declaration names the definition group `group`.
    fn definition(&mut self, group: CasId) {
        let _ = group;
    }
    /// The length of the text a media object holds as content. A resolver
    /// that knows the blob a string was stored in answers from that blob, so
    /// the object can be written after its capture gave the string up.
    fn content_len(&mut self, s: &Graph, id: StringId) -> usize {
        s.strings[id.0 as usize].len()
    }
}

/// Receives a blob's content piece by piece.
pub(crate) trait Visitor {
    type Error;
    /// One byte: a tag, a flag, or a small enumeration.
    fn byte(&mut self, byte: u8) -> Result<(), Self::Error>;
    fn int(&mut self, value: i64) -> Result<(), Self::Error>;
    /// A float's raw bits.
    fn bits(&mut self, bits: u64) -> Result<(), Self::Error>;
    /// A local number, child slot or enum variant.
    fn index(&mut self, index: u32) -> Result<(), Self::Error>;
    /// A length that is data, not the size of what follows.
    fn length(&mut self, length: usize) -> Result<(), Self::Error>;
    /// A string as a value or a name. Its digest is the handle's own.
    fn string(&mut self, text: &BexStr) -> Result<(), Self::Error>;
    /// A map key or field name.
    fn key(&mut self, text: &BexStr) -> Result<(), Self::Error>;
    fn bigint(&mut self, value: &BigInt, digest: Digest) -> Result<(), Self::Error>;
    fn ty(&mut self, ty: &OwnedType, leaf: TypeLeaf) -> Result<(), Self::Error>;
    fn bytes(&mut self, bytes: &[u8], digest: Digest) -> Result<(), Self::Error>;
    fn declaration(&mut self, identity: &DeclarationIdentity<'_>) -> Result<(), Self::Error>;
    /// A sequence of `len` items follows.
    fn begin_range(&mut self, len: usize) -> Result<(), Self::Error>;
}

/// Where a value sits in its blob.
#[derive(Clone, Copy)]
enum Place {
    /// The blob's root value.
    Root,
    Nested,
}

pub(crate) fn root<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    root: SnapshotRoot,
) -> Result<(), V::Error> {
    match root {
        SnapshotRoot::Value(root) => {
            v.byte(RootTag::Value as u8)?;
            value(v, r, s, root, Place::Root)
        }
        SnapshotRoot::FunctionArgs(args) => {
            v.byte(RootTag::FunctionArgs as u8)?;
            v.length(args.parameter_count)?;
            values(v, r, s, args.slots)
        }
    }
}

fn child<V: Visitor>(v: &mut V, slot: u32) -> Result<(), V::Error> {
    v.byte(ValueTag::External as u8)?;
    v.index(slot)
}

fn local<V: Visitor>(v: &mut V, number: u32) -> Result<(), V::Error> {
    v.byte(ValueTag::Object as u8)?;
    v.index(number)
}

fn value<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    value: SnapshotValue,
    place: Place,
) -> Result<(), V::Error> {
    match value {
        SnapshotValue::Null => v.byte(ValueTag::Null as u8),
        SnapshotValue::OmittedArg => v.byte(ValueTag::OmittedArg as u8),
        SnapshotValue::Bool(b) => {
            v.byte(ValueTag::Bool as u8)?;
            v.byte(u8::from(b))
        }
        SnapshotValue::Int(n) => {
            v.byte(ValueTag::Int as u8)?;
            v.int(n)
        }
        SnapshotValue::Float(f) => {
            v.byte(ValueTag::Float as u8)?;
            v.bits(f.to_bits())
        }
        SnapshotValue::String(id) => {
            let slot = match place {
                Place::Root => None,
                Place::Nested => r.string(id),
            };
            match slot {
                Some(slot) => child(v, slot),
                None => {
                    v.byte(ValueTag::String as u8)?;
                    v.string(&s.strings[id.0 as usize])
                }
            }
        }
        SnapshotValue::Bigint(id) => {
            let slot = match place {
                Place::Root => None,
                Place::Nested => r.bigint(id),
            };
            match slot {
                Some(slot) => child(v, slot),
                None => {
                    v.byte(ValueTag::Bigint as u8)?;
                    let bigint = &s.bigints[id.0 as usize];
                    v.bigint(&bigint.value, bigint.digest)
                }
            }
        }
        SnapshotValue::Object(id) => {
            let reference = match place {
                Place::Root => Reference::Local(r.member(id)),
                Place::Nested => r.object(id),
            };
            match reference {
                Reference::Local(number) => local(v, number),
                Reference::Child(slot) => child(v, slot),
                Reference::ChildNode { slot, node } => {
                    v.byte(ValueTag::ExternalNode as u8)?;
                    v.index(slot)?;
                    v.index(node)
                }
            }
        }
        SnapshotValue::Type(id) => {
            v.byte(ValueTag::Type as u8)?;
            ty(v, r, s, id)
        }
        SnapshotValue::Enum {
            declaration,
            variant,
            name,
        } => {
            v.byte(ValueTag::Enum as u8)?;
            v.index(r.member(declaration))?;
            v.index(variant)?;
            text(v, s, name)
        }
        SnapshotValue::Truncated(limit) => {
            v.byte(ValueTag::Truncated as u8)?;
            v.byte(tags::limit(limit))
        }
    }
}

/// A name, written in place.
fn text<V: Visitor>(v: &mut V, s: &Graph, id: LabelId) -> Result<(), V::Error> {
    v.string(&s.labels[id.0 as usize])
}

fn ty<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    id: TypeId,
) -> Result<(), V::Error> {
    described(v, r, &s.types[id.0 as usize])
}

fn described<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    ty: &crate::graph::Type,
) -> Result<(), V::Error> {
    v.ty(&ty.ty, ty.leaf)?;
    if ty.defined {
        ty.ty.visit_heads(&mut |head| {
            if let TypeIdentity::Defined(head) = head {
                r.definition(head.definition.group);
            }
        });
    }
    Ok(())
}

/// Media content: its length, then its base64 text as a value, which a large
/// payload leaves to a blob of its own. The length describes the media
/// without reading that blob.
fn payload<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    data: StringId,
) -> Result<(), V::Error> {
    v.length(r.content_len(s, data))?;
    value(v, r, s, SnapshotValue::String(data), Place::Nested)
}

/// Whether content loaded from a URL or file follows, then that content.
fn loaded<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    data: Option<StringId>,
) -> Result<(), V::Error> {
    v.byte(u8::from(data.is_some()))?;
    match data {
        Some(data) => payload(v, r, s, data),
        None => Ok(()),
    }
}

fn values<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    range: Range<SnapshotValue>,
) -> Result<(), V::Error> {
    v.begin_range(range.len())?;
    for item in &s.values[range.indexes()] {
        value(v, r, s, *item, Place::Nested)?;
    }
    Ok(())
}

fn entries<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    range: Range<MapEntry>,
) -> Result<(), V::Error> {
    v.begin_range(range.len())?;
    for entry in &s.entries[range.indexes()] {
        value(v, r, s, entry.key, Place::Nested)?;
        value(v, r, s, entry.value, Place::Nested)?;
    }
    Ok(())
}

fn fields<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    range: Range<FieldEntry>,
) -> Result<(), V::Error> {
    v.begin_range(range.len())?;
    for entry in &s.fields[range.indexes()] {
        v.key(&entry.key)?;
        value(v, r, s, entry.value, Place::Nested)?;
    }
    Ok(())
}

pub(crate) fn object<V: Visitor, R: Resolver>(
    v: &mut V,
    r: &mut R,
    s: &Graph,
    object: &SnapshotObject,
) -> Result<(), V::Error> {
    match object {
        SnapshotObject::Uint8Array { data } => {
            v.byte(ObjectTag::Uint8Array as u8)?;
            v.length(data.len())?;
            v.bytes(&s.bytes[data.range.indexes()], data.digest)
        }
        // The same object with no bytes captured: only the length says more.
        SnapshotObject::Uint8ArrayTruncated { original_len } => {
            v.byte(ObjectTag::Uint8Array as u8)?;
            v.length(*original_len)?;
            v.bytes(&[], hash::no_bytes())
        }
        SnapshotObject::List {
            element_type,
            items,
            original_len,
        } => {
            v.byte(ObjectTag::List as u8)?;
            ty(v, r, s, *element_type)?;
            v.length(*original_len)?;
            values(v, r, s, *items)
        }
        SnapshotObject::Map {
            key_type,
            value_type,
            entries: range,
            original_len,
        } => {
            v.byte(ObjectTag::Map as u8)?;
            ty(v, r, s, *key_type)?;
            ty(v, r, s, *value_type)?;
            v.length(*original_len)?;
            entries(v, r, s, *range)
        }
        SnapshotObject::Instance {
            type_arguments,
            declaration,
            fields: range,
            original_len,
        } => {
            v.byte(ObjectTag::Instance as u8)?;
            v.begin_range(type_arguments.len())?;
            for ty in &s.types[type_arguments.indexes()] {
                described(v, r, ty)?;
            }
            v.index(r.member(*declaration))?;
            v.length(*original_len)?;
            fields(v, r, s, *range)
        }
        SnapshotObject::Declaration { name, tag, is_enum } => {
            v.byte(ObjectTag::Declaration as u8)?;
            let declared = &s.names[name.0 as usize];
            v.declaration(&DeclarationIdentity {
                tag: *tag,
                declared,
            })?;
            if let Some(definition) = &declared.definition {
                r.definition(definition.group);
            }
            v.byte(u8::from(*is_enum))
        }
        SnapshotObject::Cell(inner) => {
            v.byte(ObjectTag::Cell as u8)?;
            value(v, r, s, *inner, Place::Nested)
        }
        SnapshotObject::NonSnapshotableValue {} => v.byte(ObjectTag::NonSnapshotableValue as u8),
        SnapshotObject::Descriptive { kind, name } => {
            v.byte(ObjectTag::Descriptive as u8)?;
            v.byte(tags::description(*kind))?;
            v.byte(u8::from(name.is_some()))?;
            match name {
                Some(name) => text(v, s, *name),
                None => Ok(()),
            }
        }
        SnapshotObject::Media {
            kind,
            mime_type,
            source,
        } => {
            v.byte(ObjectTag::Media as u8)?;
            v.byte(tags::media_kind(*kind))?;
            v.byte(u8::from(mime_type.is_some()))?;
            if let Some(mime_type) = mime_type {
                text(v, s, *mime_type)?;
            }
            match *source {
                MediaSource::Url { url, data } => {
                    v.byte(MediaSourceTag::Url as u8)?;
                    text(v, s, url)?;
                    loaded(v, r, s, data)
                }
                MediaSource::File { path, data } => {
                    v.byte(MediaSourceTag::File as u8)?;
                    text(v, s, path)?;
                    loaded(v, r, s, data)
                }
                MediaSource::Base64 { data } => {
                    v.byte(MediaSourceTag::Base64 as u8)?;
                    payload(v, r, s, data)
                }
            }
        }
        SnapshotObject::Truncated(limit) => {
            v.byte(ObjectTag::Truncated as u8)?;
            v.byte(tags::limit(*limit))
        }
    }
}

/// Hash format 4 input: each piece as the encoding writes it, with a leaf's
/// content replaced by its digest. One stream hashes a whole blob.
impl Visitor for Hasher {
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
    fn string(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        self.digest(hash::string(text));
        Ok(())
    }
    fn key(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        self.size(text.len());
        self.absorb(text.as_bytes());
        Ok(())
    }
    fn bigint(&mut self, _: &BigInt, digest: Digest) -> Result<(), Self::Error> {
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
    fn declaration(&mut self, identity: &DeclarationIdentity<'_>) -> Result<(), Self::Error> {
        self.borsh(identity);
        Ok(())
    }
    fn begin_range(&mut self, len: usize) -> Result<(), Self::Error> {
        self.size(len);
        Ok(())
    }
}

/// The encoded length of each piece. Its sum over a blob's root and objects,
/// plus the header, is the blob's exact size.
///
/// Its methods and [`Both`]'s are `inline(always)`: they run once per piece
/// of every capture on the capturing thread, and left to the optimizer they
/// cost a measurable share of shaping.
#[derive(Default)]
pub(crate) struct Length(pub(crate) u64);

#[expect(
    clippy::inline_always,
    reason = "one call per piece of every capture; measured, see `Length`"
)]
impl Length {
    #[inline(always)]
    fn add(&mut self, bytes: usize) {
        self.0 = self.0.saturating_add(bytes as u64);
    }
}

/// Encoded size of a bigint's magnitude: one u64 limb per 64 significant bits.
pub(crate) fn bigint_limb_bytes(value: &BigInt) -> usize {
    usize::try_from(value.bits().div_ceil(64))
        .unwrap_or(usize::MAX)
        .saturating_mul(8)
}

#[expect(
    clippy::inline_always,
    reason = "one call per piece of every capture; measured, see `Length`"
)]
impl Visitor for Length {
    type Error = std::convert::Infallible;
    #[inline(always)]
    fn byte(&mut self, _: u8) -> Result<(), Self::Error> {
        self.add(1);
        Ok(())
    }
    #[inline(always)]
    fn int(&mut self, _: i64) -> Result<(), Self::Error> {
        self.add(8);
        Ok(())
    }
    #[inline(always)]
    fn bits(&mut self, _: u64) -> Result<(), Self::Error> {
        self.add(8);
        Ok(())
    }
    #[inline(always)]
    fn index(&mut self, _: u32) -> Result<(), Self::Error> {
        self.add(4);
        Ok(())
    }
    #[inline(always)]
    fn length(&mut self, _: usize) -> Result<(), Self::Error> {
        self.add(8);
        Ok(())
    }
    #[inline(always)]
    fn string(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        self.add(4);
        self.add(text.len());
        Ok(())
    }
    #[inline(always)]
    fn key(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        self.add(4);
        self.add(text.len());
        Ok(())
    }
    #[inline(always)]
    fn bigint(&mut self, value: &BigInt, _: Digest) -> Result<(), Self::Error> {
        // Sign, bit length, limbs.
        self.add(1 + 8);
        self.add(bigint_limb_bytes(value));
        Ok(())
    }
    #[inline(always)]
    fn ty(&mut self, _: &OwnedType, leaf: TypeLeaf) -> Result<(), Self::Error> {
        self.add(leaf.encoded_len as usize);
        Ok(())
    }
    #[inline(always)]
    fn bytes(&mut self, bytes: &[u8], _: Digest) -> Result<(), Self::Error> {
        self.add(4);
        self.add(bytes.len());
        Ok(())
    }
    #[inline(always)]
    fn declaration(&mut self, identity: &DeclarationIdentity<'_>) -> Result<(), Self::Error> {
        let mut counter = Counter(0);
        identity
            .serialize(&mut counter)
            .expect("counting cannot fail");
        self.add(counter.0);
        Ok(())
    }
    #[inline(always)]
    fn begin_range(&mut self, _: usize) -> Result<(), Self::Error> {
        self.add(4);
        Ok(())
    }
}

/// A visitor and a second one that cannot fail, run together.
pub(crate) struct Both<A, B>(pub(crate) A, pub(crate) B);

#[expect(
    clippy::inline_always,
    reason = "one call per piece of every capture; measured, see `Length`"
)]
impl<A, B> Visitor for Both<A, B>
where
    A: Visitor,
    B: Visitor<Error = std::convert::Infallible>,
{
    type Error = A::Error;
    #[inline(always)]
    fn byte(&mut self, byte: u8) -> Result<(), Self::Error> {
        infallible(self.1.byte(byte));
        self.0.byte(byte)
    }
    #[inline(always)]
    fn int(&mut self, value: i64) -> Result<(), Self::Error> {
        infallible(self.1.int(value));
        self.0.int(value)
    }
    #[inline(always)]
    fn bits(&mut self, bits: u64) -> Result<(), Self::Error> {
        infallible(self.1.bits(bits));
        self.0.bits(bits)
    }
    #[inline(always)]
    fn index(&mut self, index: u32) -> Result<(), Self::Error> {
        infallible(self.1.index(index));
        self.0.index(index)
    }
    #[inline(always)]
    fn length(&mut self, length: usize) -> Result<(), Self::Error> {
        infallible(self.1.length(length));
        self.0.length(length)
    }
    #[inline(always)]
    fn string(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        infallible(self.1.string(text));
        self.0.string(text)
    }
    #[inline(always)]
    fn key(&mut self, text: &BexStr) -> Result<(), Self::Error> {
        infallible(self.1.key(text));
        self.0.key(text)
    }
    #[inline(always)]
    fn bigint(&mut self, value: &BigInt, digest: Digest) -> Result<(), Self::Error> {
        infallible(self.1.bigint(value, digest));
        self.0.bigint(value, digest)
    }
    #[inline(always)]
    fn ty(&mut self, ty: &OwnedType, leaf: TypeLeaf) -> Result<(), Self::Error> {
        infallible(self.1.ty(ty, leaf));
        self.0.ty(ty, leaf)
    }
    #[inline(always)]
    fn bytes(&mut self, bytes: &[u8], digest: Digest) -> Result<(), Self::Error> {
        infallible(self.1.bytes(bytes, digest));
        self.0.bytes(bytes, digest)
    }
    #[inline(always)]
    fn declaration(&mut self, identity: &DeclarationIdentity<'_>) -> Result<(), Self::Error> {
        infallible(self.1.declaration(identity));
        self.0.declaration(identity)
    }
    #[inline(always)]
    fn begin_range(&mut self, len: usize) -> Result<(), Self::Error> {
        infallible(self.1.begin_range(len));
        self.0.begin_range(len)
    }
}

/// Unwrap the result of a walk whose visitor cannot fail.
pub(crate) fn infallible(result: Result<(), std::convert::Infallible>) {
    match result {
        Ok(()) => {}
        Err(never) => match never {},
    }
}
