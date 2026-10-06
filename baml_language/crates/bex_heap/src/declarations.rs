//! Declaration identity lookup for telemetry definitions, owned by the heap.
//! Like the function lookup, entries are weak: they never take part in
//! tracing, and the collector repairs or prunes them. A declaration that dies
//! before its definition was resolved has its definition extracted at the
//! safepoint, while from-space is still intact, so collection never loses one.

use std::collections::{HashMap, HashSet};

use baml_type::{DeclarationName, typetag::TypeTag};
use bex_vm_types::{HeapPtr, Object, PermitProof, TypeHead};
use btel_types::{DefinitionHead, TypeDeclaration, TypeDefinition, TypeField, TypeVariant};

use crate::{BexHeap, CollectionLevel, Generation};

/// Definitions one collection extracts at most; the rest of a dying group is
/// dropped (its references keep their ids), so a safepoint's work stays
/// bounded.
const MAX_EXTRACTED_PER_COLLECTION: usize = 1_024;

impl BexHeap {
    /// Register a class or enum a recorded capture names, before the capture
    /// is published. After the first call for a declaration this is one
    /// atomic read for a compile-time declaration, one shard read otherwise.
    ///
    /// # Safety
    /// The caller must exclude GC, and `ptr` must be the live class or enum
    /// object `tag` identifies.
    #[inline]
    pub unsafe fn register_telemetry_declaration(&self, tag: TypeTag, ptr: HeapPtr) {
        let statics = self
            .is_compile_time_ptr(ptr)
            .then(|| self.compile_time_len());
        self.declarations.register(tag, ptr, statics);
    }

    /// Whether definitions are waiting: pending resolution, or settled.
    /// Lock-free.
    pub fn has_type_definition_work(&self) -> bool {
        self.declarations.has_work()
    }

    /// Definitions settled without heap access: extracted at collection.
    /// Needs no permit.
    pub fn take_settled_type_definitions(&self) -> Vec<TypeDefinition> {
        self.declarations.take_settled()
    }

    /// Copy up to `max` pending definitions. Declarations a definition names
    /// are registered as they are found, and resolved in the same call while
    /// the budget lasts; the rest stay pending.
    pub fn resolve_type_definitions(
        &self,
        _proof: PermitProof<'_>,
        max: usize,
    ) -> Vec<TypeDefinition> {
        let mut out = Vec::new();
        let mut budget = max;
        while budget > 0 {
            let batch = self.declarations.take_pending(budget);
            if batch.is_empty() {
                break;
            }
            budget -= batch.len();
            for (tag, ptr) in batch {
                let mut named = Vec::new();
                // SAFETY: the permit excludes GC, and the lookup holds the
                // current pointer of every registered, uncollected
                // declaration; under a permit every head points at a live,
                // current object.
                let declaration = unsafe { copy_declaration(ptr, &|head| head, &mut named) };
                for (head, head_ptr) in named {
                    // SAFETY: as above.
                    unsafe { self.register_telemetry_declaration(head, head_ptr) };
                }
                self.declarations.mark_resolved(tag);
                out.push(TypeDefinition { tag, declaration });
            }
        }
        out
    }

    /// Only called at the GC safepoint, after survivors are copied and fixed
    /// up and before from-space is destroyed. Never marks anything live.
    pub(crate) fn update_declaration_lookup(
        &self,
        forwarding: &HashMap<HeapPtr, HeapPtr>,
        level: CollectionLevel,
    ) {
        // Where an object is now, if it survives: compile-time objects never
        // move, survivors moved to their forwarding address, and a minor
        // collection leaves Gen2 in place.
        let survives = |ptr: HeapPtr| -> Option<HeapPtr> {
            if self.is_compile_time_ptr(ptr) {
                return Some(ptr);
            }
            if let Some(&moved) = forwarding.get(&ptr) {
                return Some(moved);
            }
            (level == CollectionLevel::Minor && self.generation_of(ptr) == Generation::Gen2)
                .then_some(ptr)
        };
        let mut dying = self.declarations.retain(|ptr| match survives(*ptr) {
            Some(current) => {
                *ptr = current;
                true
            }
            None => false,
        });
        if dying.is_empty() {
            return;
        }
        // A survivor's old slot is a tombstone; a dead object's is intact
        // until this phase ends. Read every object where it is readable.
        let readable = |ptr: HeapPtr| survives(ptr).unwrap_or(ptr);
        // Extract each dying definition, and the definitions it names that die
        // with it; register the ones it names that survive, where they are now.
        let mut seen = HashSet::new();
        let mut extracted = 0;
        while let Some((tag, ptr)) = dying.pop() {
            if !seen.insert(tag) || extracted >= MAX_EXTRACTED_PER_COLLECTION {
                continue;
            }
            extracted += 1;
            let mut named = Vec::new();
            // SAFETY: the safepoint excludes mutators, `ptr` is dead (so its
            // from-space slot is intact), and `readable` maps every head to
            // an intact slot.
            let declaration = unsafe { copy_declaration(ptr, &readable, &mut named) };
            self.declarations
                .settle(TypeDefinition { tag, declaration });
            for (head, head_ptr) in named {
                match survives(head_ptr) {
                    // SAFETY: a surviving declaration at its current address.
                    Some(current) => unsafe { self.register_telemetry_declaration(head, current) },
                    None if !self.declarations.contains(head) => dying.push((head, head_ptr)),
                    None => {}
                }
            }
        }
    }
}

/// Copy the definition of the class or enum at `ptr`, pushing every class or
/// enum its fields name to `named` with the pointer `readable` maps its head
/// to. `None` for any other object.
///
/// # Safety
/// GC must be excluded (or this must run at the safepoint), `ptr` must point
/// at an intact declaration object, and `readable` must map every head's
/// pointer to an intact object.
unsafe fn copy_declaration(
    ptr: HeapPtr,
    readable: &dyn Fn(HeapPtr) -> HeapPtr,
    named: &mut Vec<(TypeTag, HeapPtr)>,
) -> Option<TypeDeclaration> {
    // SAFETY: the caller keeps `ptr`'s slot intact.
    match unsafe { ptr.get() } {
        Object::Class(class) => Some(TypeDeclaration {
            is_enum: false,
            name: class.name.clone(),
            type_params: u32::try_from(class.generic_param_count).unwrap_or(u32::MAX),
            description: class.description.clone(),
            alias: class.alias.clone(),
            docstring: class.docstring.clone(),
            attributes: attributes(&class.other),
            stream_done: class.stream_done,
            fields: class
                .fields
                .iter()
                .map(|field| TypeField {
                    name: field.name.clone(),
                    schema: field.field_template.map_heads(&mut |head: &TypeHead| {
                        // SAFETY: as above; `readable` maps the head.
                        unsafe { definition_head(*head, readable, named) }
                    }),
                    description: field.description.clone(),
                    alias: field.alias.clone(),
                    docstring: field.docstring.clone(),
                    attributes: attributes(&field.other),
                    skip: field.skip,
                    stream_done: field.stream_done,
                    must_exist: field.must_exist,
                })
                .collect(),
            variants: Vec::new(),
        }),
        Object::Enum(enm) => Some(TypeDeclaration {
            is_enum: true,
            name: enm.name.clone(),
            type_params: 0,
            description: enm.description.clone(),
            alias: enm.alias.clone(),
            docstring: enm.docstring.clone(),
            attributes: attributes(&enm.other),
            stream_done: false,
            fields: Vec::new(),
            variants: enm
                .variants
                .iter()
                .map(|variant| TypeVariant {
                    name: variant.name.clone(),
                    description: variant.description.clone(),
                    alias: variant.alias.clone(),
                    docstring: variant.docstring.clone(),
                    attributes: attributes(&variant.other),
                    skip: variant.skip,
                })
                .collect(),
        }),
        _ => None,
    }
}

/// A field type's head as a definition carries it, read where `readable`
/// says the head's object is. Classes and enums go to `named`, so their own
/// definitions get recorded too.
///
/// # Safety
/// As [`copy_declaration`].
unsafe fn definition_head(
    head: TypeHead,
    readable: &dyn Fn(HeapPtr) -> HeapPtr,
    named: &mut Vec<(TypeTag, HeapPtr)>,
) -> DefinitionHead {
    if !head.is_resolved() {
        return DefinitionHead {
            tag: head.tag(),
            name: None,
        };
    }
    // SAFETY: `readable` maps the head to an intact object.
    let name = match unsafe { readable(head.ptr()).get() } {
        Object::Class(class) => {
            named.push((head.tag(), head.ptr()));
            Some(class.name.clone())
        }
        Object::Enum(enm) => {
            named.push((head.tag(), head.ptr()));
            Some(enm.name.clone())
        }
        Object::Interface(iface) => Some(DeclarationName::Declared(iface.name.clone())),
        Object::TypeAlias(alias) => Some(DeclarationName::Declared(alias.name.clone())),
        _ => None,
    };
    DefinitionHead {
        tag: head.tag(),
        name,
    }
}

fn attributes(other: &indexmap::IndexMap<String, String>) -> Vec<(String, String)> {
    other
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
