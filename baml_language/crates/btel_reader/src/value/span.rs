//! A captured value across the blobs that hold it.
//!
//! A blob names the blobs it continues in; each is read and verified alone.
//! A child that is missing, unreadable or inconsistent with how its parent
//! names it makes that part of the value unavailable, never the rest.
use std::{collections::HashMap, sync::Arc};

use btel_snapshot::{
    CasId, ChildIndex, DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue, MediaPayload,
    NodeId, TypeDescription,
};
use num_bigint::BigInt;

use super::{Nav, Unavailable};
use crate::cas::{CasStore, CasUnavailable};

/// Where the blobs of a value come from. `load` must return only a blob whose
/// verified ID is `id`, as [`CasStore`] does: walks know blobs by their IDs.
pub trait BlobSource {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable>;
    /// The class and enum definitions of the recording the rendered value
    /// belongs to, for sources that know it. None by default: references then
    /// carry no definition ids.
    fn type_definitions(&self) -> Option<&dyn crate::types::TypeDefinitions> {
        None
    }
}

impl BlobSource for CasStore {
    fn load(&self, id: CasId) -> Result<Arc<DecodedSnapshot>, CasUnavailable> {
        CasStore::load(self, id).snapshot
    }
}

/// A parent names its child in a way the child cannot satisfy. Each verifies
/// alone, so only reading both shows it.
pub(super) const BROKEN_REFERENCE: Unavailable = Unavailable::Blob("cas_corrupt");

/// A value that navigation found: references to other blobs and cells are
/// followed, and what the capture cut or omitted is reported instead.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Bigint(Arc<BigInt>),
    Type(Arc<TypeDescription>),
    Enum {
        declaration: NodeId,
        variant: u32,
        name: Arc<str>,
    },
    /// An object with content: never a cell or a truncation marker.
    Object(NodeId),
}

impl From<&Found> for DecodedValue {
    fn from(found: &Found) -> Self {
        match found {
            Found::Null => Self::Null,
            Found::Bool(b) => Self::Bool(*b),
            Found::Int(n) => Self::Int(*n),
            Found::Float(f) => Self::Float(*f),
            Found::String(text) => Self::String(text.clone()),
            Found::Bigint(n) => Self::Bigint(n.clone()),
            Found::Type(ty) => Self::Type(ty.clone()),
            Found::Enum {
                declaration,
                variant,
                name,
            } => Self::Enum {
                declaration: *declaration,
                variant: *variant,
                name: name.clone(),
            },
            Found::Object(id) => Self::Object(*id),
        }
    }
}

/// A found value and the blob its object references are local to.
#[derive(Clone, Debug, PartialEq)]
pub struct Located {
    blob: Arc<DecodedSnapshot>,
    value: Found,
}

impl Located {
    pub fn blob(&self) -> &Arc<DecodedSnapshot> {
        &self.blob
    }
    pub fn value(&self) -> &Found {
        &self.value
    }
}

/// The value a reference to a child's root stands for.
fn child_root(child: &DecodedSnapshot) -> Result<&DecodedValue, Unavailable> {
    match &child.root {
        // A blob's root value is stored in it, and captured arguments are
        // never part of another value.
        DecodedRoot::Value(DecodedValue::External(_) | DecodedValue::ExternalNode { .. })
        | DecodedRoot::FunctionArgs { .. } => Err(BROKEN_REFERENCE),
        DecodedRoot::Value(value) => Ok(value),
    }
}

/// The media content a child blob holds for a parent that recorded its length.
fn child_content(child: &DecodedSnapshot, text_len: u64) -> Result<&Arc<str>, Unavailable> {
    match child_root(child)? {
        DecodedValue::String(text) if text.len() as u64 == text_len => Ok(text),
        DecodedValue::String(_)
        | DecodedValue::Null
        | DecodedValue::OmittedArg
        | DecodedValue::Bool(_)
        | DecodedValue::Int(_)
        | DecodedValue::Float(_)
        | DecodedValue::Bigint(_)
        | DecodedValue::Object(_)
        | DecodedValue::Type(_)
        | DecodedValue::Enum { .. }
        | DecodedValue::Truncated(_)
        | DecodedValue::External(_)
        | DecodedValue::ExternalNode { .. } => Err(BROKEN_REFERENCE),
    }
}

/// A media value's content as base64 text, read from its own blob when it is
/// stored there. `blob` holds the media object.
pub fn media_content(
    source: &(impl BlobSource + ?Sized),
    blob: &DecodedSnapshot,
    payload: &MediaPayload,
) -> Result<Arc<str>, Unavailable> {
    match payload {
        MediaPayload::Inline(text) => Ok(Arc::clone(text)),
        MediaPayload::External { child, text_len } => {
            // A payload from another blob may name a slot this one lacks.
            let id = blob
                .children
                .get(child.0 as usize)
                .ok_or(BROKEN_REFERENCE)?;
            let child = load(source, *id, &mut 1)?;
            child_content(&child, *text_len).cloned()
        }
    }
}

fn child_node(child: &DecodedSnapshot, node: NodeId) -> Result<NodeId, Unavailable> {
    if (node.0 as usize) < child.objects.len() {
        Ok(node)
    } else {
        Err(BROKEN_REFERENCE)
    }
}

fn load(
    source: &(impl BlobSource + ?Sized),
    id: CasId,
    blobs_left: &mut usize,
) -> Result<Arc<DecodedSnapshot>, Unavailable> {
    *blobs_left = blobs_left.checked_sub(1).ok_or(Unavailable::BlobBudget)?;
    source
        .load(id)
        .map_err(|error| Unavailable::Blob(error.code()))
}

/// Follow `value` through references to other blobs and through cells.
/// Entering a blob spends one of `blobs_left`. Never [`Nav::Arguments`].
pub(super) fn reach(
    source: &(impl BlobSource + ?Sized),
    blob: &Arc<DecodedSnapshot>,
    value: &DecodedValue,
    blobs_left: &mut usize,
) -> Nav {
    let mut blob = Arc::clone(blob);
    let mut value = value.clone();
    // A cycle of cells stays in one blob, so it revisits a cell of that blob.
    let mut cells = 0;
    loop {
        let value_in_blob = match value {
            DecodedValue::External(child) => {
                let child = match load(source, blob.child(child), blobs_left) {
                    Ok(child) => child,
                    Err(reason) => return Nav::Unavailable(reason),
                };
                let root = match child_root(&child) {
                    Ok(root) => root.clone(),
                    Err(reason) => return Nav::Unavailable(reason),
                };
                (child, root)
            }
            DecodedValue::ExternalNode { child, node } => {
                let child = match load(source, blob.child(child), blobs_left) {
                    Ok(child) => child,
                    Err(reason) => return Nav::Unavailable(reason),
                };
                match child_node(&child, node) {
                    Ok(node) => (child, DecodedValue::Object(node)),
                    Err(reason) => return Nav::Unavailable(reason),
                }
            }
            DecodedValue::Object(id) => {
                let inner = match blob.object(id) {
                    DecodedObject::Cell(inner) => inner.clone(),
                    DecodedObject::Truncated(limit) => {
                        return Nav::Unavailable(Unavailable::Truncated(*limit));
                    }
                    DecodedObject::Uint8Array { .. }
                    | DecodedObject::List { .. }
                    | DecodedObject::Map { .. }
                    | DecodedObject::Instance { .. }
                    | DecodedObject::Declaration { .. }
                    | DecodedObject::NonSnapshotable
                    | DecodedObject::Descriptive { .. }
                    | DecodedObject::Media(_) => {
                        return found(blob, Found::Object(id));
                    }
                };
                cells += 1;
                if cells > blob.objects.len() {
                    return Nav::Unavailable(Unavailable::CellCycle);
                }
                value = inner;
                continue;
            }
            DecodedValue::Truncated(limit) => {
                return Nav::Unavailable(Unavailable::Truncated(limit));
            }
            // Not passed by the caller: absent, not null.
            DecodedValue::OmittedArg => return Nav::Missing,
            DecodedValue::Null => return found(blob, Found::Null),
            DecodedValue::Bool(b) => return found(blob, Found::Bool(b)),
            DecodedValue::Int(n) => return found(blob, Found::Int(n)),
            DecodedValue::Float(f) => return found(blob, Found::Float(f)),
            DecodedValue::String(text) => return found(blob, Found::String(text)),
            DecodedValue::Bigint(n) => return found(blob, Found::Bigint(n)),
            DecodedValue::Type(ty) => return found(blob, Found::Type(ty)),
            DecodedValue::Enum {
                declaration,
                variant,
                name,
            } => {
                return found(
                    blob,
                    Found::Enum {
                        declaration,
                        variant,
                        name,
                    },
                );
            }
        };
        (blob, value) = value_in_blob;
        cells = 0;
    }
}

fn found(blob: Arc<DecodedSnapshot>, value: Found) -> Nav {
    Nav::Value(Located { blob, value })
}

/// What a walk over a value examines, and so which of its blobs it reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Examine {
    /// Structure and leaves. Media is only described, so its content is not read.
    Structure,
    /// Everything, media content included.
    Content,
}

/// Every blob a value reaches, read once, so that the value can be walked
/// without reading anything more.
pub(super) struct Span {
    blobs: HashMap<CasId, Result<Arc<DecodedSnapshot>, Unavailable>>,
    /// How often the walked values name each object. Declarations are named
    /// by everything of their type, so they are not counted.
    references: HashMap<(CasId, NodeId), u32>,
    blobs_left: usize,
    bytes_left: u64,
}

impl Span {
    /// Read what `starts` reach, entering at most `max_blobs` blobs beyond
    /// the ones the starts are in, and none once those hold `max_blob_bytes`
    /// encoded bytes.
    pub(super) fn load<'a>(
        source: &(impl BlobSource + ?Sized),
        starts: impl IntoIterator<Item = (&'a Arc<DecodedSnapshot>, &'a DecodedValue)>,
        examine: Examine,
        max_blobs: usize,
        max_blob_bytes: u64,
    ) -> Self {
        let mut span = Self {
            blobs: HashMap::new(),
            references: HashMap::new(),
            blobs_left: max_blobs,
            bytes_left: max_blob_bytes,
        };
        let mut pending = Vec::new();
        for (blob, value) in starts {
            span.blobs
                .entry(blob.id)
                .or_insert_with(|| Ok(Arc::clone(blob)));
            span.visit(source, blob, value, &mut pending);
        }
        while let Some((blob, id)) = pending.pop() {
            match blob.object(id) {
                DecodedObject::List { items, .. } => {
                    for item in items {
                        span.visit(source, &blob, item, &mut pending);
                    }
                }
                DecodedObject::Map { entries, .. } => {
                    for (key, value) in entries {
                        span.visit(source, &blob, key, &mut pending);
                        span.visit(source, &blob, value, &mut pending);
                    }
                }
                DecodedObject::Instance { fields, .. } => {
                    for (_, value) in fields {
                        span.visit(source, &blob, value, &mut pending);
                    }
                }
                DecodedObject::Cell(value) => span.visit(source, &blob, value, &mut pending),
                DecodedObject::Media(media) => {
                    if let (Examine::Content, Some(MediaPayload::External { child, .. })) =
                        (examine, media.source.data())
                    {
                        // Content is text: it names nothing further.
                        let _ = span.read(source, blob.child(*child));
                    }
                }
                DecodedObject::Uint8Array { .. }
                | DecodedObject::Declaration { .. }
                | DecodedObject::NonSnapshotable
                | DecodedObject::Descriptive { .. }
                | DecodedObject::Truncated(_) => {}
            }
        }
        span
    }

    fn visit(
        &mut self,
        source: &(impl BlobSource + ?Sized),
        blob: &Arc<DecodedSnapshot>,
        value: &DecodedValue,
        pending: &mut Vec<(Arc<DecodedSnapshot>, NodeId)>,
    ) {
        let (blob, id) = match value {
            DecodedValue::Object(id) => (Arc::clone(blob), *id),
            DecodedValue::External(child) => {
                let Ok(child) = self.read(source, blob.child(*child)) else {
                    return;
                };
                // A child's root value is stored in the child.
                let Ok(DecodedValue::Object(id)) = child_root(&child) else {
                    return;
                };
                let id = *id;
                (child, id)
            }
            DecodedValue::ExternalNode { child, node } => {
                let Ok(child) = self.read(source, blob.child(*child)) else {
                    return;
                };
                let Ok(node) = child_node(&child, *node) else {
                    return;
                };
                (child, node)
            }
            DecodedValue::Null
            | DecodedValue::OmittedArg
            | DecodedValue::Bool(_)
            | DecodedValue::Int(_)
            | DecodedValue::Float(_)
            | DecodedValue::String(_)
            | DecodedValue::Bigint(_)
            | DecodedValue::Type(_)
            | DecodedValue::Enum { .. }
            | DecodedValue::Truncated(_) => return,
        };
        let count = self.references.entry((blob.id, id)).or_insert(0);
        *count += 1;
        if *count == 1 {
            pending.push((blob, id));
        }
    }

    fn read(
        &mut self,
        source: &(impl BlobSource + ?Sized),
        id: CasId,
    ) -> Result<Arc<DecodedSnapshot>, Unavailable> {
        if let Some(known) = self.blobs.get(&id) {
            return known.clone();
        }
        // A blob's size is known once it is read, so the last one read may
        // pass the limit.
        let read = if self.bytes_left == 0 {
            Err(Unavailable::BlobBudget)
        } else {
            load(source, id, &mut self.blobs_left)
        };
        if let Ok(blob) = &read {
            self.bytes_left = self.bytes_left.saturating_sub(blob.encoded_len);
        }
        self.blobs.insert(id, read.clone());
        read
    }

    fn child(
        &self,
        blob: &DecodedSnapshot,
        child: ChildIndex,
    ) -> Result<&DecodedSnapshot, Unavailable> {
        match self.blobs.get(&blob.child(child)) {
            Some(Ok(child)) => Ok(child),
            Some(Err(reason)) => Err(*reason),
            None => unreachable!("loading walked every reference a value can reach"),
        }
    }

    /// The blob and root value that a reference to a child's root names.
    pub(super) fn root_of(
        &self,
        blob: &DecodedSnapshot,
        child: ChildIndex,
    ) -> Result<(&DecodedSnapshot, &DecodedValue), Unavailable> {
        let child = self.child(blob, child)?;
        Ok((child, child_root(child)?))
    }

    /// The blob and object that a reference into a child names.
    pub(super) fn node_of(
        &self,
        blob: &DecodedSnapshot,
        child: ChildIndex,
        node: NodeId,
    ) -> Result<(&DecodedSnapshot, NodeId), Unavailable> {
        let child = self.child(blob, child)?;
        Ok((child, child_node(child, node)?))
    }

    /// A media value's content. Only a span that examines
    /// [`Examine::Content`] has read the blobs content is stored in.
    pub(super) fn content<'s>(
        &'s self,
        blob: &'s DecodedSnapshot,
        payload: &'s MediaPayload,
    ) -> Result<&'s str, Unavailable> {
        match payload {
            MediaPayload::Inline(text) => Ok(text),
            MediaPayload::External { child, text_len } => {
                Ok(child_content(self.child(blob, *child)?, *text_len)?)
            }
        }
    }

    /// Whether more than one of the walked values names this object.
    pub(super) fn shared(&self, blob: &DecodedSnapshot, id: NodeId) -> bool {
        self.references
            .get(&(blob.id, id))
            .is_some_and(|count| *count > 1)
    }
}

#[cfg(test)]
mod tests;
