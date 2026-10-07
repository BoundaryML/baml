//! Recorded class and enum definitions, as their declarations keep them.
//!
//! A declaration's definition is copied out of the heap the first time a
//! recorded capture names it, encoded once as a content-addressed blob, and
//! kept on the declaration: every later capture that names it shares the
//! encoded blob and costs one load. It lives exactly as long as the
//! declaration, so collecting a runtime class drops its definition, and the
//! number kept is the number of declarations alive.
//!
//! The blob format is the snapshot crate's; nothing here reads it.
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};

/// A group of definitions encoded as one CAS blob: a declaration with every
/// declaration it is mutually recursive with. Immutable once made.
pub struct DefinitionBlob {
    id: [u8; 16],
    /// The stream of captures that last carried it to its writers; zero for
    /// none. A shortcut for that stream only: see [`Self::carried_by`].
    carried_by: AtomicU64,
    bytes: Box<[u8]>,
    children: Box<[Arc<DefinitionBlob>]>,
}

impl DefinitionBlob {
    /// `bytes` is the whole blob, `id` its content identity, and `children`
    /// the groups it names, in the order of its child table.
    pub fn new(id: [u8; 16], bytes: Box<[u8]>, children: Box<[Arc<DefinitionBlob>]>) -> Self {
        Self {
            id,
            carried_by: AtomicU64::new(0),
            bytes,
            children,
        }
    }
    /// Whether `stream` is the last stream to have carried this group. Only
    /// `stream` marks itself, after carrying it, so true means it did; false
    /// says nothing.
    #[inline]
    pub fn carried_by(&self, stream: u64) -> bool {
        self.carried_by.load(Ordering::Relaxed) == stream
    }
    /// `stream` has carried this group.
    pub fn mark_carried(&self, stream: u64) {
        self.carried_by.store(stream, Ordering::Relaxed);
    }
    pub fn id(&self) -> [u8; 16] {
        self.id
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn children(&self) -> &[Arc<DefinitionBlob>] {
        &self.children
    }
}

impl std::fmt::Debug for DefinitionBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefinitionBlob")
            .field("id", &self.id)
            .field("carried_by", &self.carried_by)
            .field("len", &self.bytes.len())
            .field("children", &self.children.len())
            .finish()
    }
}

/// One declaration's definition: its group and its position there.
#[derive(Clone, Debug)]
pub struct Definition {
    pub group: Arc<DefinitionBlob>,
    pub member: u32,
}

/// Where a declaration keeps its recorded definition. Set once, by the first
/// capture that names the declaration.
///
/// A clone starts empty: a declaration built by copying another is a new
/// declaration, and the collector moves declarations without cloning them.
#[derive(Default)]
pub struct DefinitionCell(OnceLock<Definition>);

impl DefinitionCell {
    #[inline]
    pub fn get(&self) -> Option<&Definition> {
        self.0.get()
    }
    /// Keep `definition` unless the cell holds one already, and return the
    /// one it holds.
    pub fn set(&self, definition: Definition) -> &Definition {
        self.0.get_or_init(|| definition)
    }
}

impl Clone for DefinitionCell {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for DefinitionCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get() {
            Some(definition) => f
                .debug_tuple("DefinitionCell")
                .field(&definition.group.id)
                .field(&definition.member)
                .finish(),
            None => f.write_str("DefinitionCell(None)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(n: u8) -> Arc<DefinitionBlob> {
        Arc::new(DefinitionBlob::new([n; 16], Box::new([n]), Box::new([])))
    }

    #[test]
    fn a_cell_keeps_its_first_definition_and_a_clone_starts_empty() {
        let cell = DefinitionCell::default();
        assert!(cell.get().is_none());
        let first = cell.set(Definition {
            group: blob(1),
            member: 0,
        });
        assert_eq!(first.group.id(), [1; 16]);
        let kept = cell.set(Definition {
            group: blob(2),
            member: 3,
        });
        assert_eq!((kept.group.id(), kept.member), ([1; 16], 0));
        assert!(cell.clone().get().is_none());
    }
}
