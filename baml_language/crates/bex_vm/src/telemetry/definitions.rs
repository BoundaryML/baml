//! Recorded definitions of the classes and enums captures name.
//!
//! A declaration's definition is copied out of the heap the first time a
//! recorded capture names it, on the capturing VM thread, which holds the
//! heap permit, and kept on the declaration (`Class::telemetry_definition`):
//! every later capture costs one load. It lives as long as the declaration,
//! so the cache is bounded by the declarations alive and pruned by
//! collection itself; nothing else holds it.
//!
//! Declarations that reach each other through their field types form one
//! group and one blob ([`btel_snapshot::definition`]). Making a definition
//! finds, from the declaration a capture names, the groups of every
//! declaration it reaches that has no definition yet, each after the groups
//! it uses (Tarjan's algorithm, without recursion), and makes them in that
//! order. A declaration that has a definition already ends the search: its
//! whole group has one.
//!
//! One lock, taken only to make definitions, keeps two threads from making
//! overlapping groups at once: a search never sees part of a group, which
//! would make it give a member a second, different definition. Lookups never
//! take it.
use std::sync::{Mutex, PoisonError};

use baml_type::DeclarationName;
use bex_vm_types::{Class, Enum, HeapPtr, Object, TypeHead};
use btel_snapshot::definition::{self, Declaration, Field, Head, Meta, Variant};
use btel_types::{Definition, DefinitionCell};
use rustc_hash::FxHashMap;

static MAKING: Mutex<()> = Mutex::new(());

/// Where the class or enum at `ptr` keeps its definition; `None` for any
/// other object.
///
/// # Safety
/// The caller holds the heap permit, and `ptr` is live.
unsafe fn cell<'a>(ptr: HeapPtr) -> Option<&'a DefinitionCell> {
    // SAFETY: the caller's permit keeps the object live and unmoved.
    match unsafe { ptr.get() } {
        Object::Class(class) => Some(&class.telemetry_definition),
        Object::Enum(enm) => Some(&enm.telemetry_definition),
        _ => None,
    }
}

/// The recorded definition of the class or enum at `ptr`, made now if it
/// has none. `None` when `ptr` is neither.
///
/// # Safety
/// The caller holds the heap permit throughout, and `ptr` is live.
#[inline]
pub(super) unsafe fn of<'a>(ptr: HeapPtr) -> Option<&'a Definition> {
    // SAFETY: inherited.
    let cell = unsafe { cell(ptr) }?;
    match cell.get() {
        Some(definition) => Some(definition),
        // SAFETY: inherited.
        None => Some(unsafe { make(ptr, cell) }),
    }
}

/// # Safety
/// As [`of`]; `cell` is the declaration's at `ptr`.
#[cold]
#[inline(never)]
unsafe fn make<'a>(ptr: HeapPtr, cell: &'a DefinitionCell) -> &'a Definition {
    let _making = MAKING.lock().unwrap_or_else(PoisonError::into_inner);
    // Another thread may have made it while this one waited.
    if let Some(definition) = cell.get() {
        return definition;
    }
    // SAFETY: inherited.
    unsafe { Search::default().run(ptr) };
    cell.get()
        .unwrap_or_else(|| unreachable!("the search makes the group of its start"))
}

/// One declaration during the search.
struct Node {
    ptr: HeapPtr,
    index: u32,
    lowlink: u32,
    on_stack: bool,
}

#[derive(Default)]
struct Search {
    nodes: Vec<Node>,
    by_ptr: FxHashMap<HeapPtr, usize>,
    /// Visited declarations whose group is incomplete.
    stack: Vec<usize>,
    /// The path being explored: a node, the declarations it names without
    /// a definition, and the next of them to visit.
    path: Vec<(usize, Vec<HeapPtr>, usize)>,
}

impl Search {
    /// Make the definition of every declaration without one that `start`
    /// reaches, `start` included.
    ///
    /// # Safety
    /// As [`of`].
    unsafe fn run(&mut self, start: HeapPtr) {
        // SAFETY: inherited, here and for every pointer reached below.
        unsafe { self.enter(start) };
        while let Some((node, successors, next)) = self.path.last_mut() {
            let node = *node;
            let target = successors.get(*next).copied();
            if let Some(target) = target {
                *next += 1;
                match self.by_ptr.get(&target) {
                    None => unsafe { self.enter(target) },
                    Some(&seen) if self.nodes[seen].on_stack => {
                        let reached = self.nodes[seen].index;
                        let lowlink = &mut self.nodes[node].lowlink;
                        *lowlink = (*lowlink).min(reached);
                    }
                    // A group completed earlier in this search.
                    Some(_) => {}
                }
                continue;
            }
            self.path.pop();
            if self.nodes[node].lowlink == self.nodes[node].index {
                unsafe { self.complete(node) };
            }
            if let Some((parent, ..)) = self.path.last() {
                let lowlink = self.nodes[node].lowlink;
                let parent = &mut self.nodes[*parent].lowlink;
                *parent = (*parent).min(lowlink);
            }
        }
    }

    /// # Safety
    /// As [`of`].
    unsafe fn enter(&mut self, ptr: HeapPtr) {
        let at = self.nodes.len();
        let index = u32::try_from(at).expect("bounded declarations");
        self.nodes.push(Node {
            ptr,
            index,
            lowlink: index,
            on_stack: true,
        });
        self.by_ptr.insert(ptr, at);
        self.stack.push(at);
        let mut successors = Vec::new();
        // SAFETY: inherited.
        if let Object::Class(class) = unsafe { ptr.get() } {
            for field in &class.fields {
                field.field_template.visit_heads(&mut |head| {
                    // SAFETY: inherited; a resolved head points at a live
                    // declaration.
                    if head.is_resolved()
                        && unsafe { cell(head.ptr()) }.is_some_and(|cell| cell.get().is_none())
                    {
                        successors.push(head.ptr());
                    }
                });
            }
        }
        self.path.push((at, successors, 0));
    }

    /// `first` was the first of its group to be visited, and the group's
    /// other members are above it on the stack.
    ///
    /// # Safety
    /// As [`of`].
    unsafe fn complete(&mut self, first: usize) {
        let mut members = Vec::new();
        loop {
            let member = self
                .stack
                .pop()
                .unwrap_or_else(|| unreachable!("a group's first member is on the stack"));
            self.nodes[member].on_stack = false;
            members.push(self.nodes[member].ptr);
            if member == first {
                break;
            }
        }
        let positions: FxHashMap<HeapPtr, u32> = members
            .iter()
            .enumerate()
            .map(|(at, ptr)| (*ptr, u32::try_from(at).expect("bounded group")))
            .collect();
        let declarations: Vec<_> = members
            .iter()
            // SAFETY: inherited.
            .map(|ptr| match unsafe { ptr.get() } {
                Object::Class(class) => Declaration::Class(class_definition(class, &positions)),
                Object::Enum(enm) => Declaration::Enum(enum_definition(enm)),
                _ => unreachable!("only declarations are searched"),
            })
            .collect();
        for (ptr, definition) in members.iter().zip(definition::group(&declarations)) {
            // SAFETY: inherited.
            if let Some(cell) = unsafe { cell(*ptr) } {
                cell.set(definition);
            }
        }
    }
}

fn meta(
    description: Option<&String>,
    alias: Option<&String>,
    docstring: Option<&String>,
    other: &indexmap::IndexMap<String, String>,
) -> Meta {
    Meta {
        description: description.cloned(),
        alias: alias.cloned(),
        docstring: docstring.cloned(),
        attributes: other
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    }
}

/// `class` with its field types' heads as the group sees them: a member by
/// its position, any other declaration by the definition it already has.
fn class_definition(
    class: &Class,
    positions: &FxHashMap<HeapPtr, u32>,
) -> definition::Class<baml_type::TyTemplate<Head>> {
    definition::Class {
        name: class.name.clone(),
        type_params: u32::try_from(class.generic_param_count).expect("bounded generics"),
        meta: meta(
            class.description.as_ref(),
            class.alias.as_ref(),
            class.docstring.as_ref(),
            &class.other,
        ),
        stream_done: class.stream_done,
        fields: class
            .fields
            .iter()
            .map(|field| Field {
                name: field.name.clone(),
                ty: field
                    .field_template
                    .map_heads(&mut |head: &TypeHead| group_head(head, positions)),
                meta: meta(
                    field.description.as_ref(),
                    field.alias.as_ref(),
                    field.docstring.as_ref(),
                    &field.other,
                ),
                skip: field.skip,
                stream_done: field.stream_done,
                must_exist: field.must_exist,
            })
            .collect(),
    }
}

fn enum_definition(enm: &Enum) -> definition::Enum {
    definition::Enum {
        name: enm.name.clone(),
        meta: meta(
            enm.description.as_ref(),
            enm.alias.as_ref(),
            enm.docstring.as_ref(),
            &enm.other,
        ),
        variants: enm
            .variants
            .iter()
            .map(|variant| Variant {
                name: variant.name.clone(),
                meta: meta(
                    variant.description.as_ref(),
                    variant.alias.as_ref(),
                    variant.docstring.as_ref(),
                    &variant.other,
                ),
                skip: variant.skip,
            })
            .collect(),
    }
}

/// Total by invariant: a live declaration's field heads are resolved
/// pointers to declarations, and the search makes every group a group uses
/// before it.
fn group_head(head: &TypeHead, positions: &FxHashMap<HeapPtr, u32>) -> Head {
    // `None` for an unresolved head, before anything reads its pointer.
    let name: DeclarationName = head
        .tagged_name()
        .unwrap_or_else(|| unreachable!("a live declaration's field names a declaration"))
        .name()
        .clone();
    if let Some(position) = positions.get(&head.ptr()) {
        return Head::Member(*position);
    }
    // SAFETY: the caller of the search holds the heap permit, and the head
    // is resolved.
    match unsafe { cell(head.ptr()) } {
        Some(cell) => {
            let definition = cell
                .get()
                .unwrap_or_else(|| unreachable!("a group's dependencies are made before it"));
            Head::Defined(name, definition.clone())
        }
        // An interface or a type alias.
        None => Head::Named(name),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier, Weak};

    use baml_type::{Name, RuntimeTy, TyTemplate as Template, typetag::TypeTag};
    use bex_vm_types::{ClassField, types::Owner};
    use btel_types::DefinitionBlob;
    use indexmap::IndexMap;

    use super::*;

    fn class(name: &str, tag: i64) -> Object {
        Object::Class(Box::new(Class {
            name: DeclarationName::Anonymous(Name::new(name)),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            stream_done: false,
            type_tag: TypeTag::from_i64(tag),
            has_cleanup: false,
            generic_param_count: 0,
            owner: Owner::anonymous(),
            methods: IndexMap::new(),
            telemetry_definition: DefinitionCell::default(),
        }))
    }

    /// Add `name: target?` to the class at `ptr`.
    fn add_optional_field(ptr: HeapPtr, name: &str, target: HeapPtr, tag: i64) {
        let head = TypeHead::new(target, TypeTag::from_i64(tag));
        // SAFETY: test heap, no collection running.
        let Object::Class(class) = (unsafe { ptr.get_mut() }) else {
            unreachable!()
        };
        class.fields.push(ClassField {
            name: name.into(),
            field_type: RuntimeTy::Null,
            field_template: Template::Union(Box::new([
                Template::Class(head, Box::new([])),
                Template::Null,
            ])),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip: false,
            stream_done: false,
            must_exist: false,
            runtime_type: None,
        });
    }

    /// `Left { right: Right? }` and `Right { left: Left? }`, with tags from
    /// `tag`.
    fn left_right(vm: &mut crate::BexVm, tag: i64) -> (HeapPtr, HeapPtr) {
        let left = vm.tlab.alloc(class("Left", tag));
        let right = vm.tlab.alloc(class("Right", tag + 1));
        add_optional_field(left, "right", right, tag + 1);
        add_optional_field(right, "left", left, tag);
        (left, right)
    }

    fn definition(ptr: HeapPtr) -> Definition {
        // SAFETY: test heap, no collection running.
        unsafe { of(ptr) }.unwrap().clone()
    }

    #[test]
    fn a_definition_is_made_once_and_kept_on_its_declaration() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let person = vm.tlab.alloc(class("Person", 1000));
        let first = definition(person);
        let again = definition(person);
        assert!(Arc::ptr_eq(&first.group, &again.group));
        // SAFETY: test heap.
        let cell = unsafe { cell(person) }.unwrap();
        assert!(Arc::ptr_eq(&cell.get().unwrap().group, &first.group));
        let text = vm.tlab.alloc_string("not a declaration");
        // SAFETY: test heap.
        assert!(unsafe { of(text) }.is_none());
    }

    /// Whichever member a capture names first, a group is one blob and
    /// each member keeps its place in it; making one member's definition
    /// makes the whole group's.
    #[test]
    fn mutually_recursive_declarations_share_one_group_from_either_start() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let (left, right) = left_right(&mut vm, 2000);
        let (other_left, other_right) = left_right(&mut vm, 3000);
        let from_left = definition(left);
        // SAFETY: test heap.
        assert!(unsafe { cell(right) }.unwrap().get().is_some());
        let from_right = definition(other_right);
        // SAFETY: test heap.
        assert!(unsafe { cell(other_left) }.unwrap().get().is_some());
        assert_eq!(from_left.group.id(), from_right.group.id());
        assert_eq!(from_left.member, definition(other_left).member);
        assert_eq!(definition(right).member, from_right.member);
        assert_ne!(from_left.member, from_right.member);
        assert!(from_left.group.children().is_empty());
    }

    /// A declaration that names another group's member names that group,
    /// which becomes its blob's child.
    #[test]
    fn a_group_names_the_groups_its_fields_use() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let (left, _) = left_right(&mut vm, 4000);
        let holder = vm.tlab.alloc(class("Holder", 4100));
        add_optional_field(holder, "left", left, 4000);
        let holder = definition(holder);
        let children: Vec<_> = holder.group.children().iter().map(|c| c.id()).collect();
        assert_eq!(children, vec![definition(left).group.id()]);
    }

    fn collect(vm: &mut crate::BexVm, roots: &[HeapPtr]) -> Vec<HeapPtr> {
        // SAFETY: test heap; nothing else holds a pointer across it.
        let (_, roots, _) = unsafe {
            vm.heap
                .collect_garbage_generational(roots, bex_heap::CollectionLevel::Major)
        };
        roots
    }

    /// The cache is the declarations: a collected class drops its
    /// definition, and one that survives keeps it.
    #[test]
    fn a_collected_declaration_drops_its_definition() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        let kept = vm.tlab.alloc(class("Kept", 5000));
        let kept_group = Arc::downgrade(&definition(kept).group);
        let mut dropped: Vec<Weak<DefinitionBlob>> = Vec::new();
        for n in 0..200 {
            let temporary = vm.tlab.alloc(class(&format!("Temp{n}"), 5100 + n));
            dropped.push(Arc::downgrade(&definition(temporary).group));
        }
        let mut roots = vec![kept];
        // Twice: what one collection leaves behind, the next frees.
        for _ in 0..2 {
            roots = collect(&mut vm, &roots);
        }
        assert!(dropped.iter().all(|group| group.upgrade().is_none()));
        let kept_group = kept_group
            .upgrade()
            .expect("a live class keeps its definition");
        assert!(Arc::ptr_eq(&definition(roots[0]).group, &kept_group));
    }

    /// Threads that first see members of the same group at once publish one
    /// whole group: every member has exactly one definition.
    #[test]
    fn concurrent_first_sightings_publish_whole_groups() {
        let mut vm = crate::vm::tests::test_vm(Vec::new());
        for round in 0..20 {
            let (left, right) = left_right(&mut vm, 6000 + round * 2);
            let barrier = Barrier::new(8);
            let seen: Vec<(Definition, Definition)> = std::thread::scope(|scope| {
                let threads: Vec<_> = (0..8)
                    .map(|thread| {
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            let (first, second) = if thread % 2 == 0 {
                                (left, right)
                            } else {
                                (right, left)
                            };
                            let first = definition(first);
                            let second = definition(second);
                            if thread % 2 == 0 {
                                (first, second)
                            } else {
                                (second, first)
                            }
                        })
                    })
                    .collect();
                threads.into_iter().map(|t| t.join().unwrap()).collect()
            });
            let (left, right) = &seen[0];
            assert!(Arc::ptr_eq(&left.group, &right.group));
            assert_ne!(left.member, right.member);
            for (l, r) in &seen {
                assert!(Arc::ptr_eq(&l.group, &left.group) && l.member == left.member);
                assert!(Arc::ptr_eq(&r.group, &right.group) && r.member == right.member);
            }
        }
    }
}
