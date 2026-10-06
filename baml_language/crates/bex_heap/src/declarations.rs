//! Declaration identity lookup for telemetry definitions, owned by the heap.
//! Like the function lookup, entries are weak: they never take part in
//! tracing, and the collector repairs or prunes them. A declaration that dies
//! before its definition was resolved has its definition extracted at the
//! safepoint, while from-space is still intact, so collection never loses one.

use std::collections::{HashMap, HashSet};

use baml_type::typetag::TypeTag;
use bex_vm_types::{HeapPtr, Object, PermitProof, TypeHead};
use btel_types::{DefinitionHead, TypeDeclaration, TypeDefinition, TypeField, TypeVariant};

use crate::{BexHeap, CollectionLevel, Generation};

/// Definitions one collection extracts at most; the rest of a dying group is
/// settled as unavailable, so a safepoint's work stays bounded.
const MAX_EXTRACTED_PER_COLLECTION: usize = 1_024;

impl BexHeap {
    /// Register a class or enum a recorded capture names, before the capture
    /// is published. Cheap after the first call for a declaration.
    ///
    /// # Safety
    /// The caller must exclude GC, and `ptr` must be the live class or enum
    /// object `tag` identifies.
    #[inline]
    pub unsafe fn register_telemetry_declaration(&self, tag: TypeTag, ptr: HeapPtr) {
        self.declarations.register(tag, ptr);
    }

    /// Whether definitions are waiting: pending resolution, or settled.
    pub fn has_type_definition_work(&self) -> bool {
        self.declarations.pending_len() > 0 || self.declarations.has_settled()
    }

    /// Definitions settled without heap access: extracted at collection, or
    /// unavailable. Needs no permit.
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
                // current pointer of every registered, uncollected declaration.
                let declaration = unsafe { copy_declaration(ptr, &mut named) };
                for (head, head_ptr) in named {
                    self.declarations.register(head, head_ptr);
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
        // Extract each dying definition, and the definitions it names that die
        // with it; register the ones it names that survive.
        let mut seen = HashSet::new();
        let mut extracted = 0;
        while let Some((tag, ptr)) = dying.pop() {
            if !seen.insert(tag) {
                continue;
            }
            if extracted >= MAX_EXTRACTED_PER_COLLECTION {
                self.declarations.settle(TypeDefinition {
                    tag,
                    declaration: None,
                });
                continue;
            }
            extracted += 1;
            let mut named = Vec::new();
            // SAFETY: the safepoint excludes mutators, and from-space (where a
            // dying object still lies) is intact until this phase ends.
            let declaration = unsafe { copy_declaration(ptr, &mut named) };
            self.declarations
                .settle(TypeDefinition { tag, declaration });
            for (head, head_ptr) in named {
                if self.declarations.contains(head) {
                    continue;
                }
                match survives(head_ptr) {
                    Some(current) => {
                        self.declarations.register(head, current);
                    }
                    None => dying.push((head, head_ptr)),
                }
            }
        }
    }
}

/// Copy the definition of the class or enum at `ptr`, pushing every class or
/// enum its fields name to `named`. `None` for any other object.
///
/// # Safety
/// GC must be excluded (or this must run at the safepoint, with `ptr` in an
/// intact space), and `ptr` must be a declaration object.
unsafe fn copy_declaration(
    ptr: HeapPtr,
    named: &mut Vec<(TypeTag, HeapPtr)>,
) -> Option<TypeDeclaration> {
    // SAFETY: the caller excludes GC and keeps `ptr`'s space intact.
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
                        // SAFETY: as above; a head names a live declaration.
                        unsafe { definition_head(*head, named) }
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

/// A field type's head as a definition carries it. Classes and enums are
/// reported to `named`, so their own definitions get recorded too.
///
/// # Safety
/// As [`copy_declaration`].
unsafe fn definition_head(head: TypeHead, named: &mut Vec<(TypeTag, HeapPtr)>) -> DefinitionHead {
    // SAFETY: the caller excludes GC and keeps the head's space intact; an
    // unresolved head has no object to read.
    if head.is_resolved()
        && matches!(
            unsafe { head.ptr().get() },
            Object::Class(_) | Object::Enum(_)
        )
    {
        named.push((head.tag(), head.ptr()));
    }
    DefinitionHead {
        tag: head.tag(),
        name: head.tagged_name().map(|name| name.name().clone()),
    }
}

fn attributes(other: &indexmap::IndexMap<String, String>) -> Vec<(String, String)> {
    other
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}
