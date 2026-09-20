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
    ParkedAt, Restored, RestoredThread, SnapshotError, format, walk::unserializable_reason, wire,
};

/// Smallest possible encoding of one object blob: a `u32` length plus the
/// one-byte variant tag. Used to reject an object count that the remaining
/// input cannot hold, before any slot is reserved.
const MIN_OBJECT_BLOB_LEN: usize = 5;
const MAX_THREADS: usize = 1 << 16;

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

pub(crate) fn restore_into(
    heap: &BexHeap,
    bytes: &[u8],
    program_hash: [u8; 32],
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
            });
        }

        Ok(Restored {
            header: container.header.clone(),
            threads,
            run_state,
            embedded_program: container.program.clone(),
        })
    })
}
