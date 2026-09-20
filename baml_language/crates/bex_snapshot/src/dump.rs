//! The JSON state dump (`StateDump` in the component contracts, section 2.5).
//!
//! A dump lists, for every thread, the call frames (innermost first) with
//! their named locals, and the totals of the heap subgraph the run reaches.
//! Values are rendered to a bounded depth and a bounded number of nodes, and a
//! reference back into a value that is being rendered is cut with the kind
//! `"omitted"`.
//!
//! The dump is tolerant: it renders runs that [`crate::write_snapshot`] would
//! refuse (native frames, host resources), so that a blocked pause can still
//! be inspected.
//!
//! Phase 3 additions to the contract's `StateDump`: a thread also has
//! `parent_thread`, `settles_future` (the id of the future it settles, or
//! null), and `cancelled`. `DumpValue.kind` also takes `"future"` (the preview
//! is `#<id> (<state>)`, and a settled future has one child `value` or
//! `error`), `"cancel_token"`, and `"task_group"`.
#![allow(unsafe_code)]

use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
};

use bex_heap::BexHeap;
use bex_vm::{
    BexVm,
    snapshot::{local_name, local_slot},
};
use bex_vm_types::{
    HeapPtr, Object, Program, Value, ValueKind,
    snapshot_ctx::{self, WriterTable},
    types::Function,
};
use borsh::BorshSerialize;
use serde_json::{Value as Json, json};

use crate::{
    ParkedAt, SnapshotError, ThreadInput, program_hash, restore_into,
    walk::{self, Root, kind_name},
};

/// Levels of children below a local before the dump cuts off.
const MAX_DEPTH: usize = 6;
/// Children rendered per container.
const MAX_CHILDREN: usize = 50;
/// Value nodes rendered per local.
const MAX_NODES_PER_LOCAL: usize = 400;
/// Characters of a string shown in a preview.
const MAX_PREVIEW_CHARS: usize = 80;

struct DumpFrame<'a> {
    function: Option<&'a Function>,
    pc: usize,
    compact_pc: bool,
    locals_offset: Option<usize>,
}

struct DumpThread<'a> {
    thread_id: u64,
    parent_thread: Option<u64>,
    settles_future: Option<u64>,
    cancelled: bool,
    name: &'a str,
    parked: &'a ParkedAt,
    frames: Vec<DumpFrame<'a>>,
    stack: &'a [Value],
    roots: Vec<Value>,
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The `Object::Function` behind a callable, from heap objects alone. A
/// generic function value resolves through the VM's globals, which a dump from
/// bytes does not have, so it stays unresolved here.
fn function_of(heap: &BexHeap, callable: HeapPtr) -> Option<&Function> {
    if callable.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a quiescent heap (see `state_dump`).
    let function_ptr = match unsafe { heap.get_object(callable) } {
        Object::Function(function) => return Some(function),
        Object::Closure(closure) => closure.function,
        Object::BoundMethod(bound) => bound.function,
        _ => return None,
    };
    if function_ptr.is_null() {
        return None;
    }
    // SAFETY: as above.
    match unsafe { heap.get_object(function_ptr) } {
        Object::Function(function) => Some(function),
        _ => None,
    }
}

fn source_line(function: &Function, pc: usize, compact_pc: bool) -> usize {
    match (&function.bytecode.compact, compact_pc) {
        (Some(compact), true) => compact.source_line_for_pc(pc),
        // The snapshot came from an engine heap (byte-offset program
        // counters) but this heap holds the unlowered form: lower on demand.
        (None, true) => function.bytecode.lower_to_compact().source_line_for_pc(pc),
        (_, false) => function.bytecode.source_line_for_pc(pc),
    }
}

fn truncate(text: &str) -> String {
    let mut out: String = text.chars().take(MAX_PREVIEW_CHARS).collect();
    if text.chars().count() > MAX_PREVIEW_CHARS {
        out.push('…');
    }
    out.replace('\n', "\\n")
}

fn class_name(heap: &BexHeap, class: HeapPtr) -> String {
    if class.is_null() {
        return "<class>".to_string();
    }
    // SAFETY: quiescent heap; a class reachable from a live instance.
    match unsafe { heap.get_object(class) } {
        Object::Class(class) => class.name.display_name().to_string(),
        _ => "<class>".to_string(),
    }
}

fn variant_name(heap: &BexHeap, enm: HeapPtr, index: usize) -> String {
    if enm.is_null() {
        return "<enum>".to_string();
    }
    // SAFETY: quiescent heap; an enum reachable from a live variant.
    match unsafe { heap.get_object(enm) } {
        Object::Enum(enm) => {
            let variant = enm
                .variants
                .get(index)
                .map_or("?", |variant| variant.name.as_str());
            format!("{}.{variant}", enm.name.display_name())
        }
        _ => "<enum>".to_string(),
    }
}

/// Short static type of a runtime value, for the `type` field of a local.
fn type_name(heap: &BexHeap, value: Value) -> Option<String> {
    Some(match value.kind() {
        ValueKind::Null | ValueKind::OmittedArg => return None,
        ValueKind::Int(_) => "int".to_string(),
        ValueKind::Bool(_) => "bool".to_string(),
        // SAFETY: quiescent heap.
        ValueKind::Object(ptr) => match unsafe { heap.get_object(ptr) } {
            Object::Cell(cell) => return type_name(heap, cell.load()),
            Object::Instance(instance) => class_name(heap, instance.class),
            Object::Variant(variant) => {
                // SAFETY: as above.
                match unsafe { heap.get_object(variant.enm) } {
                    Object::Enum(enm) => enm.name.display_name().to_string(),
                    _ => "enum".to_string(),
                }
            }
            Object::Array(array) => format!("{}[]", array.element_ty),
            Object::Map(map) => format!("map<{}, {}>", map.key_ty, map.value_ty),
            other => kind_name(other).to_string(),
        },
    })
}

struct Renderer<'a> {
    heap: &'a BexHeap,
    visiting: Vec<HeapPtr>,
    budget: usize,
}

fn leaf(kind: &str, preview: impl Into<String>) -> Json {
    json!({ "kind": kind, "preview": preview.into() })
}

impl Renderer<'_> {
    fn children(&mut self, items: Vec<(String, Value)>, depth: usize) -> Vec<Json> {
        let total = items.len();
        let mut out = Vec::with_capacity(total.min(MAX_CHILDREN + 1));
        for (key, value) in items.into_iter().take(MAX_CHILDREN) {
            let value = self.value(value, depth + 1);
            out.push(json!({ "key": key, "value": value }));
        }
        if total > MAX_CHILDREN {
            out.push(json!({
                "key": "…",
                "value": leaf("omitted", format!("{} more", total - MAX_CHILDREN)),
            }));
        }
        out
    }

    fn value(&mut self, value: Value, depth: usize) -> Json {
        let ptr = match value.kind() {
            ValueKind::Null => return leaf("null", "null"),
            ValueKind::OmittedArg => return leaf("null", "<omitted argument>"),
            ValueKind::Bool(b) => return leaf("bool", b.to_string()),
            ValueKind::Int(i) => return leaf("int", i.to_string()),
            ValueKind::Object(ptr) => ptr,
        };
        // SAFETY: quiescent heap (see `state_dump`).
        let object = unsafe { self.heap.get_object(ptr) };

        // Scalars cost nothing to render and cannot recurse.
        match object {
            Object::String(text) => return leaf("string", format!("{:?}", truncate(text))),
            Object::Float(f) => return leaf("float", f.to_string()),
            Object::Bigint(big) => return leaf("bigint", truncate(&big.to_string())),
            _ => {}
        }

        if self.visiting.contains(&ptr) {
            return leaf("omitted", "<cycle>");
        }
        if depth >= MAX_DEPTH {
            return leaf("omitted", "<depth limit>");
        }
        if self.budget == 0 {
            return leaf("omitted", "<truncated>");
        }
        self.budget -= 1;
        self.visiting.push(ptr);

        let rendered = match object {
            // A captured local lives in a cell; show the variable's value.
            Object::Cell(cell) => self.value(cell.load(), depth),
            Object::Array(array) => {
                let items: Vec<(String, Value)> = array
                    .data
                    .lock()
                    .iter()
                    .enumerate()
                    .map(|(i, item)| (format!("[{i}]"), *item))
                    .collect();
                let preview = format!("{}[] (len {})", array.element_ty, items.len());
                let children = self.children(items, depth);
                json!({ "kind": "array", "preview": preview, "children": children })
            }
            Object::Map(map) => {
                let items: Vec<(String, Value)> = map
                    .data
                    .lock()
                    .iter()
                    .map(|(key, item)| (key.to_string(), *item))
                    .collect();
                let preview = format!(
                    "map<{}, {}> (len {})",
                    map.key_ty,
                    map.value_ty,
                    items.len()
                );
                let children = self.children(items, depth);
                json!({ "kind": "map", "preview": preview, "children": children })
            }
            Object::Instance(instance) => {
                let class = class_name(self.heap, instance.class);
                // SAFETY: quiescent heap.
                let field_names: Vec<String> = match (!instance.class.is_null())
                    .then(|| unsafe { self.heap.get_object(instance.class) })
                {
                    Some(Object::Class(class)) => class
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect(),
                    _ => Vec::new(),
                };
                let items: Vec<(String, Value)> = instance
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(i, slot)| {
                        let key = field_names
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("field {i}"));
                        (key, slot.load())
                    })
                    .collect();
                let children = self.children(items, depth);
                json!({ "kind": "instance", "preview": class, "class": class, "children": children })
            }
            Object::Closure(closure) => {
                let name = function_of(self.heap, ptr).map_or("?", |f| f.name.as_str());
                let items: Vec<(String, Value)> = closure
                    .captures
                    .iter()
                    .enumerate()
                    .map(|(i, capture)| (format!("capture {i}"), *capture))
                    .collect();
                let preview = format!("<closure {name}>");
                let children = self.children(items, depth);
                json!({ "kind": "closure", "preview": preview, "children": children })
            }
            Object::BoundMethod(bound) => {
                let name = function_of(self.heap, ptr).map_or("?", |f| f.name.as_str());
                let preview = format!("<bound method {name}>");
                let children = self.children(vec![("self".to_string(), bound.receiver)], depth);
                json!({ "kind": "closure", "preview": preview, "children": children })
            }
            Object::Function(function) => leaf("closure", format!("<fn {}>", function.name)),
            Object::GenericFunction(_) => leaf("closure", "<generic function>"),
            Object::Future(future) => {
                use bex_vm_types::types::FutureRead;
                let (state, child) = match future.read() {
                    FutureRead::Pending(_) => ("pending", None),
                    FutureRead::Ready(settled) => ("ready", Some(("value", settled))),
                    FutureRead::Error(settled) => ("failed", Some(("error", settled))),
                    FutureRead::Cancelled => ("cancelled", None),
                    FutureRead::InternalError(_) => ("internal error", None),
                };
                let preview = format!("#{} ({state})", future.id());
                let children = self.children(
                    child
                        .into_iter()
                        .map(|(key, settled)| (key.to_string(), settled))
                        .collect(),
                    depth,
                );
                json!({ "kind": "future", "preview": preview, "children": children })
            }
            Object::RustData(data) => match walk::Resource::of(data) {
                Some(walk::Resource::CancelToken(token)) => {
                    let state = if token.is_cancelled() {
                        "cancelled"
                    } else {
                        "not cancelled"
                    };
                    let sources = match token.sources().len() {
                        0 => String::new(),
                        count => format!(", any of {count}"),
                    };
                    leaf("cancel_token", format!("<cancel token: {state}{sources}>"))
                }
                Some(walk::Resource::TaskGroup(group)) => {
                    let snapshot = group.snapshot();
                    let running = snapshot.members.iter().filter(|m| m.active).count();
                    leaf(
                        "task_group",
                        format!(
                            "<task group {}limit {}, {running} running, {} queued>",
                            snapshot
                                .name
                                .as_deref()
                                .map(|name| format!("{name:?}, "))
                                .unwrap_or_default(),
                            snapshot.limit,
                            snapshot.members.len() - running,
                        ),
                    )
                }
                None => leaf("opaque", "<rust_data>"),
            },
            Object::Variant(variant) => leaf(
                "opaque",
                variant_name(self.heap, variant.enm, variant.index),
            ),
            other => leaf("opaque", truncate(&other.to_string())),
        };
        self.visiting.pop();
        rendered
    }
}

fn dump_frame(heap: &BexHeap, thread: &DumpThread<'_>, index: usize) -> Json {
    let frame = &thread.frames[index];
    let (name, file, line) = match frame.function {
        Some(function) => (
            function.name.clone(),
            function.source_file.clone(),
            source_line(function, frame.pc, frame.compact_pc),
        ),
        None => ("<unknown>".to_string(), String::new(), 0),
    };

    let mut locals = Vec::new();
    if let (Some(offset), Some(function)) = (frame.locals_offset, frame.function) {
        // The frame owns the stack up to the next frame that owns a region.
        let end = thread.frames[index + 1..]
            .iter()
            .find_map(|next| next.locals_offset)
            .unwrap_or(thread.stack.len())
            .min(thread.stack.len());
        // Positions above the locals region are operand temporaries.
        for position in offset..end {
            let Some(local) =
                local_slot(function, offset, position).and_then(|slot| local_name(function, slot))
            else {
                continue;
            };
            let value = thread.stack[position];
            let mut renderer = Renderer {
                heap,
                visiting: Vec::new(),
                budget: MAX_NODES_PER_LOCAL,
            };
            locals.push(json!({
                "name": local,
                "type": type_name(heap, value),
                "value": renderer.value(value, 0),
            }));
        }
    }
    json!({ "function": name, "file": file, "line": line, "locals": locals })
}

/// The payload is short text the engine wrote (JSON for `sleep` and
/// `remote_call`). It is not shortened, so that it stays parseable.
fn parked_detail(parked: &ParkedAt) -> String {
    match std::str::from_utf8(&parked.payload) {
        Ok(text) => text.replace('\n', "\\n"),
        Err(_) => format!("<{} bytes>", parked.payload.len()),
    }
}

/// Totals of the heap subgraph reachable from the threads: object count,
/// encoded size, and both broken down by object kind. An object that cannot
/// be encoded counts with the size of its heap slot.
fn heap_totals(heap: &BexHeap, threads: &[DumpThread<'_>]) -> Json {
    let roots: Vec<Root> = threads
        .iter()
        .flat_map(|thread| thread.roots.iter())
        .map(|value| Root {
            value: *value,
            path: Vec::new(),
        })
        .collect();
    let Ok(graph) = walk::walk(heap, &roots, true, &mut |_| true) else {
        return json!({ "objects": 0, "bytes": 0, "by_kind": {} });
    };

    let opaque: std::collections::HashSet<u32> = graph.opaque.iter().copied().collect();
    let objects = graph.objects;
    // Resources are written as an id, whatever the id is.
    let resources = objects
        .iter()
        // SAFETY: quiescent heap; `ptr` was reached by the walk.
        .filter_map(|ptr| match unsafe { heap.get_object(*ptr) } {
            Object::RustData(data) => walk::Resource::of(data).map(|resource| (resource.key(), 0)),
            _ => None,
        })
        .collect();
    let table = WriterTable {
        runtime: graph.ids,
        compile_time: heap.snapshot_compile_time_region(),
        resources,
    };
    let (sizes, _table) = snapshot_ctx::with_writer(table, || {
        let mut by_kind: BTreeMap<&'static str, (u64, u64)> = BTreeMap::new();
        let mut blob = Vec::new();
        for (id, ptr) in objects.iter().enumerate() {
            // SAFETY: quiescent heap; `ptr` was reached by the walk.
            let object = unsafe { heap.get_object(*ptr) };
            blob.clear();
            let encodable = !opaque.contains(&u32::try_from(id).unwrap_or(u32::MAX))
                && object.serialize(&mut blob).is_ok();
            let bytes = if encodable {
                blob.len()
            } else {
                std::mem::size_of::<Object>()
            };
            let entry = by_kind.entry(kind_name(object)).or_default();
            entry.0 += 1;
            entry.1 += bytes as u64;
        }
        by_kind
    });

    let total_bytes: u64 = sizes.values().map(|(_, bytes)| bytes).sum();
    let by_kind: serde_json::Map<String, Json> = sizes
        .into_iter()
        .map(|(kind, (count, bytes))| (kind.to_string(), json!({ "count": count, "bytes": bytes })))
        .collect();
    json!({ "objects": objects.len(), "bytes": total_bytes, "by_kind": by_kind })
}

fn dump(
    heap: &BexHeap,
    threads: &[DumpThread<'_>],
    run_id: &str,
    segment: u32,
    created_ts: u64,
) -> Json {
    let thread_dumps: Vec<Json> = threads
        .iter()
        .map(|thread| {
            let frames: Vec<Json> = (0..thread.frames.len())
                .rev()
                .map(|index| dump_frame(heap, thread, index))
                .collect();
            json!({
                "thread": thread.thread_id,
                "parent_thread": thread.parent_thread,
                "settles_future": thread.settles_future,
                "cancelled": thread.cancelled,
                "name": thread.name,
                "parked": { "kind": thread.parked.kind, "detail": parked_detail(thread.parked) },
                "frames": frames,
            })
        })
        .collect();
    json!({
        "run": run_id,
        "segment": segment,
        "created_ts": created_ts,
        "threads": thread_dumps,
        "heap": heap_totals(heap, threads),
    })
}

fn live_thread<'a>(thread: &'a ThreadInput<'a>) -> DumpThread<'a> {
    let vm: &BexVm = thread.vm;
    let frames = vm
        .snapshot_frame_views()
        .into_iter()
        .map(|view| {
            let function = vm.snapshot_function_of(view.function);
            DumpFrame {
                function,
                pc: view.pc,
                compact_pc: function.is_some_and(|f| f.bytecode.compact.is_some()),
                locals_offset: view.locals_offset,
            }
        })
        .collect();
    let mut roots = vm.snapshot_roots();
    roots.extend(thread.extra_roots.iter().copied());
    roots.extend(
        thread
            .settles_future
            .and_then(|future| future.object)
            .map(Value::object),
    );
    DumpThread {
        thread_id: thread.thread_id,
        parent_thread: thread.parent_thread,
        settles_future: thread
            .settles_future
            .map(|future| future.id.as_usize() as u64),
        cancelled: thread.cancel.cancelled,
        name: &thread.name,
        parked: &thread.parked,
        frames,
        stack: &vm.stack.0,
        roots,
    }
}

/// `StateDump` JSON (contracts section 2.5) produced from live VMs. No
/// snapshot bytes are needed, and the run does not have to be at a clean
/// point.
///
/// The caller guarantees that no thread of the run is executing and that the
/// collector cannot move objects for the duration of the call.
#[must_use]
pub fn state_dump(
    heap: &BexHeap,
    threads: &[ThreadInput],
    run_id: &str,
    segment: u32,
) -> serde_json::Value {
    let threads: Vec<DumpThread<'_>> = threads.iter().map(live_thread).collect();
    dump(heap, &threads, run_id, segment, now_unix_ms())
}

/// `StateDump` JSON produced from snapshot bytes plus the program (for
/// `baml snapshot inspect`). The snapshot is restored into a scratch heap
/// built from `program`, so every validation of [`restore_into`] applies.
///
/// # Errors
///
/// As for [`restore_into`], plus [`SnapshotError::Mismatch`] when `program`
/// is not the program the snapshot was taken from.
pub fn state_dump_from_bytes(
    bytes: &[u8],
    program: &Program,
) -> Result<serde_json::Value, SnapshotError> {
    let hash = program_hash(program)?;
    let vm = BexVm::from_program(program.clone(), Arc::new(AtomicBool::new(false)))
        .map_err(|err| SnapshotError::Format(format!("the program does not load: {err:?}")))?;
    let heap = Arc::clone(&vm.heap);
    let restored = restore_into(&heap, bytes, hash)?;

    let threads: Vec<DumpThread<'_>> = restored
        .threads
        .iter()
        .map(|thread| {
            // The live program counter belongs to the innermost bytecode frame.
            let last = thread
                .state
                .frames
                .iter()
                .rposition(|frame| frame.native.is_none())
                .unwrap_or(0);
            let frames = thread
                .state
                .frames
                .iter()
                .enumerate()
                .map(|(index, frame)| DumpFrame {
                    function: function_of(&heap, frame.function),
                    pc: if index == last {
                        thread.state.cur_pc
                    } else {
                        frame.faulting_pc
                    },
                    compact_pc: frame.compact_pc,
                    locals_offset: frame.native.is_none().then_some(frame.locals_offset),
                })
                .collect();
            let mut roots = thread.state.roots();
            roots.extend(thread.extra_roots.iter().copied());
            roots.extend(
                restored
                    .futures
                    .iter()
                    .filter(|future| Some(future.id) == thread.settles_future)
                    .map(|future| Value::object(future.object)),
            );
            DumpThread {
                thread_id: thread.thread_id,
                parent_thread: thread.parent_thread,
                settles_future: thread.settles_future.map(|id| id.as_usize() as u64),
                cancelled: thread.cancel.cancelled,
                name: &thread.name,
                parked: &thread.parked,
                frames,
                stack: &thread.state.stack,
                roots,
            }
        })
        .collect();
    Ok(dump(
        &heap,
        &threads,
        &restored.header.run_id,
        restored.header.segment,
        restored.header.created_unix_ms,
    ))
}
