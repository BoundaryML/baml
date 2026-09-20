//! Loading a snapshot into a heap.
//!
//! The loader treats the bytes as untrusted. It verifies the checksum, bounds
//! every length, translates every reference through a bounds-checked table,
//! and validates the shape of each decoded object against the target heap
//! before the object becomes reachable.
#![allow(unsafe_code)]

use std::collections::HashMap;

use baml_type::typetag::TypeTag;
use bex_heap::BexHeap;
use bex_vm_types::{
    HeapPtr, Object, TypeHead,
    head_walk::visit_object_heads_mut,
    snapshot_ctx::{self, LoaderTable},
};
use borsh::BorshDeserialize;

use crate::{
    ParkedAt, RestoreOptions, Restored, RestoredFuture, RestoredGroupMembership, RestoredThread,
    SnapshotError, ThreadCancel, format,
    walk::{Resource, unserializable_reason},
    wire,
};

/// Smallest possible encoding of one object blob: a `u32` length plus the
/// one-byte variant tag. Used to reject an object count that the remaining
/// input cannot hold, before any slot is reserved.
const MIN_OBJECT_BLOB_LEN: usize = 5;
const MAX_THREADS: usize = 1 << 16;
const MAX_RESOURCES: usize = 1 << 20;

fn format_err(message: impl Into<String>) -> SnapshotError {
    SnapshotError::Format(message.into())
}

fn io_format(what: &str) -> impl Fn(std::io::Error) -> SnapshotError + '_ {
    move |err| format_err(format!("{what} does not decode: {err}"))
}

/// Binds the unresolved heads of decoded objects and frames to the target
/// heap's compile-time declarations.
struct HeadBinder {
    declarations: HashMap<TypeTag, HeapPtr>,
    missing: Option<i64>,
}

impl HeadBinder {
    fn bind(&mut self, head: &mut TypeHead) {
        if head.is_resolved() {
            return;
        }
        match self.declarations.get(&head.tag()) {
            Some(ptr) => head.resolve(*ptr),
            None => self.missing = Some(head.tag().as_i64()),
        }
    }

    fn check(&mut self, what: &str) -> Result<(), SnapshotError> {
        match self.missing.take() {
            Some(tag) => Err(SnapshotError::Mismatch(format!(
                "{what} names a type (tag {tag}) that the program does not declare"
            ))),
            None => Ok(()),
        }
    }
}

fn compile_time_object<'a>(
    heap: &'a BexHeap,
    ptr: HeapPtr,
    what: &str,
) -> Result<&'a Object, SnapshotError> {
    if !heap.is_compile_time_ptr(ptr) {
        return Err(format_err(format!(
            "{what} must refer to a compile-time object"
        )));
    }
    // SAFETY: compile-time objects are immutable and live as long as the heap.
    Ok(unsafe { heap.get_object(ptr) })
}

/// Reject any decoded object that the writer would never have produced, and
/// any whose shape disagrees with the declaration it points at.
fn validate_object(heap: &BexHeap, index: usize, object: &Object) -> Result<(), SnapshotError> {
    if let Some(reason) = unserializable_reason(object) {
        return Err(format_err(format!(
            "object {index} is {reason}, which a snapshot cannot contain"
        )));
    }
    match object {
        Object::Instance(instance) => {
            let what = format!("the class of object {index}");
            let Object::Class(class) = compile_time_object(heap, instance.class, &what)? else {
                return Err(format_err(format!("{what} is not a class")));
            };
            if class.fields.len() != instance.fields.len() {
                return Err(format_err(format!(
                    "object {index} has {} fields but class {} declares {}",
                    instance.fields.len(),
                    class.name,
                    class.fields.len()
                )));
            }
        }
        Object::Variant(variant) => {
            let what = format!("the enum of object {index}");
            let Object::Enum(enm) = compile_time_object(heap, variant.enm, &what)? else {
                return Err(format_err(format!("{what} is not an enum")));
            };
            if variant.index >= enm.variants.len() {
                return Err(format_err(format!(
                    "object {index} is variant {} of {}, which has {}",
                    variant.index,
                    enm.name,
                    enm.variants.len()
                )));
            }
        }
        Object::Closure(closure) => {
            let what = format!("the function of closure {index}");
            if !matches!(
                compile_time_object(heap, closure.function, &what)?,
                Object::Function(_)
            ) {
                return Err(format_err(format!("{what} is not a function")));
            }
        }
        Object::BoundMethod(bound) => {
            let what = format!("the function of bound method {index}");
            if !matches!(
                compile_time_object(heap, bound.function, &what)?,
                Object::Function(_)
            ) {
                return Err(format_err(format!("{what} is not a function")));
            }
        }
        _ => {}
    }
    Ok(())
}

/// The rebuilt resources plus, for each task group, its recorded members.
struct Resources {
    items: Vec<Resource>,
    /// `(member id, running)` per resource id; empty for a cancel token.
    members: Vec<Vec<(u64, bool)>>,
}

/// Rebuild cancel tokens and task groups. A token may only name sources with
/// a lower id, which rules out cycles and forward references.
fn rebuild_resources(wires: Vec<wire::ResourceWire>) -> Result<Resources, SnapshotError> {
    if wires.len() > MAX_RESOURCES {
        return Err(format_err("the snapshot has too many resources"));
    }
    let mut resources = Resources {
        items: Vec::with_capacity(wires.len()),
        members: Vec::with_capacity(wires.len()),
    };
    for (id, resource) in wires.into_iter().enumerate() {
        match resource {
            wire::ResourceWire::CancelToken { cancelled, sources } => {
                let sources = sources
                    .into_iter()
                    .map(|source| match resources.items.get(source as usize) {
                        Some(Resource::CancelToken(token)) if (source as usize) < id => {
                            Ok(std::sync::Arc::clone(token))
                        }
                        _ => Err(format_err(format!(
                            "cancel token {id} names source {source}, which is not an earlier \
                             cancel token"
                        ))),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                resources.items.push(Resource::CancelToken(
                    bex_vm_types::CancelTokenData::restored(cancelled, sources),
                ));
                resources.members.push(Vec::new());
            }
            wire::ResourceWire::TaskGroup {
                limit,
                name,
                members,
            } => {
                let limit = usize::try_from(limit).unwrap_or(usize::MAX);
                resources
                    .items
                    .push(Resource::TaskGroup(bex_vm_types::TaskGroupInner::new(
                        limit, name,
                    )));
                resources.members.push(members);
            }
        }
    }
    Ok(resources)
}

pub(crate) fn restore_into(
    heap: &BexHeap,
    bytes: &[u8],
    program_hash: [u8; 32],
    options: RestoreOptions,
) -> Result<Restored, SnapshotError> {
    let container = format::decode(bytes, true)?;
    if container.header.program_hash != program_hash {
        return Err(SnapshotError::Mismatch(
            "the snapshot was taken from a different program (program hash differs)".to_string(),
        ));
    }

    let mut input: &[u8] = &container.state;
    let compile_time_len = u64::deserialize(&mut input).map_err(io_format("the state section"))?;
    if compile_time_len != heap.compile_time_len() as u64 {
        return Err(SnapshotError::Mismatch(format!(
            "the snapshot expects {compile_time_len} compile-time objects but this heap has {}",
            heap.compile_time_len()
        )));
    }
    let run_state = Vec::<u8>::deserialize(&mut input).map_err(io_format("the run state"))?;
    let future_id_span = u64::deserialize(&mut input).map_err(io_format("the future id span"))?;
    let future_id_span = usize::try_from(future_id_span)
        .ok()
        .filter(|span| options.future_id_base.checked_add(*span).is_some())
        .ok_or_else(|| format_err("the future id span does not fit this engine's id space"))?;
    let resources = rebuild_resources(
        Vec::<wire::ResourceWire>::deserialize(&mut input).map_err(io_format("the resources"))?,
    )?;
    let object_count =
        u32::deserialize(&mut input).map_err(io_format("the object count"))? as usize;
    if object_count > input.len() / MIN_OBJECT_BLOB_LEN {
        return Err(format_err(
            "the object count is larger than the snapshot can hold",
        ));
    }

    let slots = heap.alloc_snapshot_placeholders(object_count);
    let mut binder = HeadBinder {
        declarations: heap.compile_time_declaration_index(),
        missing: None,
    };
    let table = LoaderTable {
        runtime: slots.clone(),
        compile_time: heap.snapshot_compile_time_region(),
        resources: resources
            .items
            .iter()
            .cloned()
            .map(Resource::into_rust_data)
            .collect(),
        future_id_base: options.future_id_base,
    };

    snapshot_ctx::with_loader(table, || {
        for (index, slot) in slots.iter().enumerate() {
            let blob = Vec::<u8>::deserialize(&mut input)
                .map_err(|err| format_err(format!("object {index} does not decode: {err}")))?;
            // `try_from_slice` also rejects trailing bytes inside the blob.
            let mut object = Object::try_from_slice(&blob)
                .map_err(|err| format_err(format!("object {index} does not decode: {err}")))?;
            validate_object(heap, index, &object)?;
            visit_object_heads_mut(&mut object, &mut |head| binder.bind(head));
            binder.check(&format!("object {index}"))?;
            // SAFETY: `slot` came from `alloc_snapshot_placeholders` above, no
            // collection has run since (the caller's contract), and nothing
            // can reach the slot until this function returns.
            unsafe { heap.overwrite_snapshot_object(*slot, object) };
        }

        let thread_wires = Vec::<wire::ThreadWire>::deserialize(&mut input)
            .map_err(io_format("the thread table"))?;
        if thread_wires.len() > MAX_THREADS {
            return Err(format_err("the snapshot has too many threads"));
        }
        if !input.is_empty() {
            return Err(format_err("the state section has trailing bytes"));
        }

        // Futures, by object. Ids are already in the target engine's id space.
        let mut futures = Vec::new();
        let mut pending: HashMap<HeapPtr, bool> = HashMap::new();
        for slot in &slots {
            // SAFETY: every slot was overwritten above and nothing else can
            // reach it yet.
            if let Object::Future(future) = unsafe { heap.get_object(*slot) } {
                let id = future.id();
                if id.as_usize() >= options.future_id_base + future_id_span {
                    return Err(format_err(format!(
                        "future {id} lies outside the snapshot's future id span"
                    )));
                }
                let is_pending =
                    matches!(future.read(), bex_vm_types::types::FutureRead::Pending(_));
                if is_pending {
                    pending.insert(*slot, false);
                }
                futures.push(RestoredFuture {
                    object: *slot,
                    id,
                    pending: is_pending,
                });
            }
        }

        let mut threads = Vec::with_capacity(thread_wires.len());
        for thread in thread_wires {
            let mut state = thread
                .state
                .into_state()
                .map_err(io_format("a thread state"))?;
            for frame in &mut state.frames {
                for ty in frame.frame_types_mut() {
                    ty.visit_heads_mut(&mut |head| binder.bind(head));
                }
                binder.check(&format!("frame {}", frame.function_name))?;
                if frame.function.is_null() {
                    return Err(format_err(format!(
                        "frame {} has no function",
                        frame.function_name
                    )));
                }
            }
            let settles_future = thread
                .settles_future_id
                .map(|id| {
                    usize::try_from(id)
                        .ok()
                        .filter(|id| *id < future_id_span)
                        .map(|id| {
                            bex_vm_types::types::FutureId::from_usize(options.future_id_base + id)
                        })
                        .ok_or_else(|| {
                            format_err(format!(
                                "thread {} settles future {id}, which lies outside the \
                                 snapshot's future id span",
                                thread.thread_id
                            ))
                        })
                })
                .transpose()?;
            if !thread.settles_future.is_null() {
                // SAFETY: the loader context only hands out restored slots and
                // compile-time objects, all of which are initialized.
                let object = unsafe { heap.get_object(thread.settles_future) };
                let matches = matches!(
                    object,
                    Object::Future(future) if Some(future.id()) == settles_future
                );
                if !matches {
                    return Err(format_err(format!(
                        "thread {} settles an object that is not its future",
                        thread.thread_id
                    )));
                }
                if let Some(claimed) = pending.get_mut(&thread.settles_future) {
                    if *claimed {
                        return Err(format_err("two threads settle one future"));
                    }
                    *claimed = true;
                }
            }
            let user_cancels = thread
                .user_cancels
                .iter()
                .map(|id| match resources.items.get(*id as usize) {
                    Some(Resource::CancelToken(token)) => Ok(std::sync::Arc::clone(token)),
                    _ => Err(format_err(format!(
                        "thread {} links resource {id}, which is not a cancel token",
                        thread.thread_id
                    ))),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let group = match thread.group {
                None => None,
                Some((id, member_id)) => match resources.items.get(id as usize) {
                    Some(Resource::TaskGroup(group)) => resources.members[id as usize]
                        .iter()
                        .position(|(member, _)| *member == member_id)
                        .map(|order| RestoredGroupMembership {
                            group: std::sync::Arc::clone(group),
                            active: resources.members[id as usize][order].1,
                            order,
                        }),
                    _ => {
                        return Err(format_err(format!(
                            "thread {} is a member of resource {id}, which is not a task group",
                            thread.thread_id
                        )));
                    }
                },
            };
            threads.push(RestoredThread {
                thread_id: thread.thread_id,
                parent_thread: thread.parent_thread,
                name: thread.name,
                state,
                parked: ParkedAt {
                    kind: thread.parked_kind,
                    payload: thread.parked_payload,
                },
                extra_roots: thread.extra_roots,
                settles_future,
                cancel: ThreadCancel {
                    cancelled: thread.cancelled,
                    parent: thread.cancel_parent,
                },
                user_cancels,
                group,
                engine_state: thread.engine_state,
            });
        }
        if pending.values().any(|claimed| !claimed) {
            return Err(format_err(
                "the snapshot holds a pending future that no thread settles",
            ));
        }
        let thread_ids: std::collections::HashSet<u64> =
            threads.iter().map(|thread| thread.thread_id).collect();
        if thread_ids.len() != threads.len() {
            return Err(format_err("two threads share one id"));
        }
        if let Some(thread) = threads.iter().find(|thread| {
            thread
                .cancel
                .parent
                .is_some_and(|p| !thread_ids.contains(&p))
        }) {
            return Err(format_err(format!(
                "the cancel token of thread {} names a parent that is not in the snapshot",
                thread.thread_id
            )));
        }

        Ok(Restored {
            header: container.header.clone(),
            threads,
            run_state,
            embedded_program: container.program.clone(),
            futures,
            future_id_span,
            cancel_tokens: resources
                .items
                .iter()
                .filter_map(|resource| match resource {
                    Resource::CancelToken(token) => Some(std::sync::Arc::clone(token)),
                    Resource::TaskGroup(_) => None,
                })
                .collect(),
        })
    })
}
