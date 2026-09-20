//! Snapshot and restore of paused BEX VM threads.
//!
//! A snapshot holds the state of every BAML thread of one run: call frames,
//! operand stacks, and the subgraph of the runtime heap that those reach. It
//! can be restored into a different heap, in a different process, as long as
//! that heap was built from the identical program. Execution then continues
//! from the yield at which the snapshot was taken.
//!
//! Compile-time objects (functions, classes, string literals, ...) are never
//! copied. A reference to one is written as its index in the heap's
//! compile-time region, which is the same in every heap built from the same
//! program.
//!
//! # Usage
//!
//! Writing, with every thread of the run parked at a yield:
//!
//! ```ignore
//! let (bytes, stats) = bex_snapshot::write_snapshot(&heap, &threads, options)?;
//! ```
//!
//! Restoring, into a fresh heap and fresh VMs:
//!
//! ```ignore
//! let restored = bex_snapshot::restore_into(&heap, &bytes, program_hash)?;
//! for thread in restored.threads {
//!     vm.import_thread_state(thread.state)?;
//!     // Resume according to `thread.parked`.
//! }
//! ```
//!
//! # What blocks a snapshot
//!
//! [`write_snapshot`] returns [`SnapshotError::Blocked`] with a root-to-value
//! path when the run holds something that cannot move to another process:
//! host resources and other opaque `RustData`, host callbacks, a future that
//! ended with an internal engine error, a pending future that no thread of the
//! run settles, a native continuation frame whose continuation has no snapshot
//! form, or a declaration created at runtime. The caller is expected to let
//! the run continue and try again at a later yield.
//!
//! # Threads, futures, and cancellation (format version 2)
//!
//! A snapshot holds every thread of the run. For each thread it records the
//! future the thread settles, whether its cancel token has fired, the thread
//! whose token is the parent of its token, the user cancel tokens linked into
//! it, and its membership in a task group. Futures are heap objects and are
//! written in every state except the internal-error state. Cancel tokens
//! (`baml.spawn.CancelToken`) and task groups (`baml.spawn.TaskGroup`) are
//! *resources*: every handle of one token restores to one token, a cancelled
//! token stays cancelled, and a composite token (`CancelToken.any`) keeps its
//! sources. The snapshot crate rebuilds these values and leaves the process
//! side to the caller: spawning the watcher tasks of composite tokens
//! ([`Restored::cancel_tokens`]), deriving thread tokens, registering group
//! members, and registering pending futures.
//!
//! # Not covered
//!
//! - Globals. A restored engine runs the program's `$init` itself.
//! - Authentication. The checksum detects accidental corruption only. The
//!   loader bounds-checks every reference and length and validates the shape
//!   of every object, but it cannot verify that a value has the static type
//!   the bytecode expects, so snapshots from untrusted parties need a MAC.

mod dump;
mod format;
mod loader;
mod walk;
mod wire;

use std::{collections::HashMap, sync::Arc, time::Instant};

use bex_heap::BexHeap;
use bex_vm::{BexVm, snapshot::VmThreadState};
use bex_vm_types::{
    CancelTokenData, HeapPtr, Object, Program, TaskGroupInner, Value,
    snapshot_ctx::{self, WriterTable},
    types::{FutureId, FutureRead},
};
use borsh::BorshSerialize;
use sha2::{Digest, Sha256};

pub use crate::dump::{state_dump, state_dump_from_bytes};
use crate::walk::{Resource, Root};

/// Version of the container and state layout. A reader refuses any other
/// version. Version 2 added threads with futures, cancel tokens, task groups,
/// and native continuation frames.
pub const FORMAT_VERSION: u32 = 2;

/// Identification of a snapshot, readable without decoding the state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotHeader {
    /// Always [`FORMAT_VERSION`] in a snapshot this build wrote. The writer
    /// overwrites whatever the caller put here.
    pub format_version: u32,
    /// Runtime build that wrote the snapshot; see [`runtime_build`].
    pub runtime_build: String,
    /// SHA-256 of the Borsh-encoded `Program`; see [`program_hash`].
    pub program_hash: [u8; 32],
    pub run_id: String,
    pub segment: u32,
    pub seq: u64,
    pub created_unix_ms: u64,
}

/// How a thread resumes. The snapshot crate stores it and does not interpret
/// it. The kind `"runnable"` means: call `exec()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParkedAt {
    pub kind: String,
    pub payload: Vec<u8>,
}

impl ParkedAt {
    /// The thread continues with a plain `exec()` call.
    #[must_use]
    pub fn runnable() -> Self {
        Self {
            kind: "runnable".to_string(),
            payload: Vec::new(),
        }
    }
}

/// The cancellation state of one thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreadCancel {
    /// The thread's cancel token has fired.
    pub cancelled: bool,
    /// The thread of this snapshot whose token is the parent of this thread's
    /// token. `None` for the root thread and for a detached thread.
    pub parent: Option<u64>,
}

/// The future a spawned thread settles when its body ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SettledFuture {
    pub id: FutureId,
    /// The heap object. `None` when the future was settled from outside
    /// (`f.cancel()`) and nothing refers to it any more.
    pub object: Option<HeapPtr>,
}

/// A thread's seat in a task group.
#[derive(Clone, Debug)]
pub struct GroupMembership {
    pub group: Arc<TaskGroupInner>,
    /// `TaskGroupTicket::member_id` of the thread's registration.
    pub member_id: u64,
}

/// One paused thread handed to the writer.
pub struct ThreadInput<'a> {
    pub thread_id: u64,
    pub parent_thread: Option<u64>,
    pub name: String,
    pub vm: &'a BexVm,
    pub parked: ParkedAt,
    /// Values held outside the VM at the yield, for example the arguments the
    /// VM drained from its stack into `VmExecState::SysOp`.
    pub extra_roots: Vec<Value>,
    /// `None` for a root thread.
    pub settles_future: Option<SettledFuture>,
    pub cancel: ThreadCancel,
    /// User cancel tokens (`spawn with options(cancel = ...)`) that cancel
    /// this thread's token when they fire.
    pub user_cancels: Vec<Arc<CancelTokenData>>,
    pub group: Option<GroupMembership>,
    /// Opaque engine state of this thread, for example its path in the spawn
    /// tree and its counters. The snapshot crate stores it and does not
    /// interpret it.
    pub engine_state: Vec<u8>,
}

impl<'a> ThreadInput<'a> {
    /// A root thread without cancellation links or a task group.
    #[must_use]
    pub fn new(thread_id: u64, vm: &'a BexVm, parked: ParkedAt) -> Self {
        Self {
            thread_id,
            parent_thread: None,
            name: String::new(),
            vm,
            parked,
            extra_roots: Vec::new(),
            settles_future: None,
            cancel: ThreadCancel::default(),
            user_cancels: Vec::new(),
            group: None,
            engine_state: Vec::new(),
        }
    }
}

pub struct WriteOptions {
    pub header: SnapshotHeader,
    /// Borsh bytes of the program, to make the snapshot self-contained.
    pub embed_program: Option<Vec<u8>>,
    /// Compress the sections with zstd level 1.
    pub compress: bool,
    /// Opaque engine state, for example the `call_id` counter.
    pub run_state: Vec<u8>,
    /// An upper bound (exclusive) of the future ids the source engine has
    /// issued. A restoring engine reserves this many ids above its own, so
    /// that a restored future keeps a unique id.
    pub future_id_span: u64,
}

/// Cost of one [`write_snapshot`] call.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WriteStats {
    /// Exporting thread states and walking the object graph.
    pub walk_ms: f64,
    /// Encoding objects and thread states.
    pub encode_ms: f64,
    /// Compressing the sections (0 when compression is off).
    pub compress_ms: f64,
    /// Runtime objects in the snapshot.
    pub objects: u64,
    /// Size of the state section before compression.
    pub raw_bytes: u64,
    /// Size of the state section as stored. The embedded program is not
    /// included; its size is the length of `WriteOptions::embed_program`.
    pub compressed_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The run is not at a clean point. `path` leads from a root to the value
    /// that cannot be serialized, outermost element first.
    #[error("snapshot blocked: {reason} (at {})", path.join(" > "))]
    Blocked { reason: String, path: Vec<String> },
    #[error("snapshot I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The bytes are not a well-formed snapshot.
    #[error("malformed snapshot: {0}")]
    Format(String),
    /// The snapshot is well-formed but does not belong to this program, heap,
    /// or build.
    #[error("snapshot mismatch: {0}")]
    Mismatch(String),
}

/// A restored thread's seat in a rebuilt task group.
#[derive(Clone, Debug)]
pub struct RestoredGroupMembership {
    /// The rebuilt group. It has the recorded limit and name and no members.
    pub group: Arc<TaskGroupInner>,
    /// True when the thread was running, false when it was queued.
    pub active: bool,
    /// Position among the group's members: the caller registers the members
    /// of one group in this order (`TaskGroupInner::register_restored`) and
    /// then calls `TaskGroupInner::admit_waiters`.
    pub order: usize,
}

/// One restored thread. `state` and `extra_roots` point into the target heap.
pub struct RestoredThread {
    pub thread_id: u64,
    pub parent_thread: Option<u64>,
    pub name: String,
    pub state: VmThreadState,
    pub parked: ParkedAt,
    pub extra_roots: Vec<Value>,
    /// Id (in the target engine's id space) of the future the thread settles.
    pub settles_future: Option<FutureId>,
    pub cancel: ThreadCancel,
    pub user_cancels: Vec<Arc<CancelTokenData>>,
    /// `None` also for a thread whose registration the group had already
    /// dropped (a queued member that was cancelled).
    pub group: Option<RestoredGroupMembership>,
    /// [`ThreadInput::engine_state`] as it was written.
    pub engine_state: Vec<u8>,
}

/// One future object of the snapshot, in the target heap.
#[derive(Clone, Copy, Debug)]
pub struct RestoredFuture {
    pub object: HeapPtr,
    /// Id in the target engine's id space.
    pub id: FutureId,
    /// True when the future has not settled. Exactly one restored thread
    /// settles it. The caller registers it with the engine and gives it the
    /// cancel token of that thread.
    pub pending: bool,
}

pub struct Restored {
    pub header: SnapshotHeader,
    pub threads: Vec<RestoredThread>,
    pub run_state: Vec<u8>,
    pub embedded_program: Option<Vec<u8>>,
    pub futures: Vec<RestoredFuture>,
    /// The restoring engine must not issue future ids below
    /// `future_id_base + future_id_span`.
    pub future_id_span: usize,
    /// Every rebuilt cancel token. The caller spawns
    /// `CancelTokenData::watchers` for each.
    pub cancel_tokens: Vec<Arc<CancelTokenData>>,
}

/// Parameters of [`restore_into_with`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RestoreOptions {
    /// Added to every future id of the snapshot: the number of future ids
    /// the target engine has already issued.
    pub future_id_base: usize,
}

/// Identifier of the runtime build, for `SnapshotHeader::runtime_build`: the
/// crate version, plus the git revision when the build environment provides
/// one in `BAML_GIT_SHA`.
#[must_use]
pub fn runtime_build() -> String {
    match option_env!("BAML_GIT_SHA") {
        Some(sha) if !sha.is_empty() => format!("{}+{sha}", env!("CARGO_PKG_VERSION")),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// SHA-256 of the Borsh-encoded program, for `SnapshotHeader::program_hash`
/// and for the `program_hash` argument of [`restore_into`].
///
/// # Errors
///
/// Fails when the program does not serialize.
pub fn program_hash(program: &Program) -> Result<[u8; 32], SnapshotError> {
    Ok(program_hash_of_bytes(&borsh::to_vec(program)?))
}

/// [`program_hash`] for a caller that already holds the Borsh bytes.
#[must_use]
pub fn program_hash_of_bytes(program_bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(program_bytes).into()
}

fn thread_label(thread_id: u64, name: &str) -> String {
    if name.is_empty() {
        format!("thread {thread_id}")
    } else {
        format!("thread {thread_id} ({name})")
    }
}

/// Labelled roots of one thread: what its VM state refers to, then the values
/// the engine holds for it.
fn thread_roots(thread: &ThreadInput<'_>, state: Option<&VmThreadState>) -> Vec<Root> {
    let label = thread_label(thread.thread_id, &thread.name);
    let mut roots: Vec<Root> = thread
        .vm
        .snapshot_root_paths()
        .into_iter()
        .map(|root| {
            let mut path = vec![label.clone()];
            path.extend(root.path);
            Root {
                value: root.value,
                path,
            }
        })
        .collect();
    for (index, value) in thread.extra_roots.iter().enumerate() {
        roots.push(Root {
            value: *value,
            path: vec![
                label.clone(),
                format!("value {index} held by the engine at the yield"),
            ],
        });
    }
    // Nothing else may refer to the future of a fire-and-forget spawn, and
    // the thread must still find it after a restore.
    if let Some(object) = thread.settles_future.and_then(|future| future.object) {
        roots.push(Root {
            value: Value::object(object),
            path: vec![label.clone(), "the future the thread settles".to_string()],
        });
    }
    // The exported state is what gets encoded, so make sure that everything
    // it refers to is in the table even if the labelled listing above ever
    // drifts from it.
    if let Some(state) = state {
        for value in state.roots() {
            roots.push(Root {
                value,
                path: vec![label.clone(), "thread state".to_string()],
            });
        }
    }
    roots
}

/// The resources of one snapshot, in id order. A token's sources are added
/// before the token, so every source id is lower than the composite's id.
#[derive(Default)]
struct ResourceTable {
    ids: HashMap<usize, u32>,
    items: Vec<Resource>,
}

impl ResourceTable {
    fn add(&mut self, resource: &Resource) {
        if self.ids.contains_key(&resource.key()) {
            return;
        }
        if let Resource::CancelToken(token) = resource {
            for source in token.sources() {
                self.add(&Resource::CancelToken(Arc::clone(source)));
            }
        }
        let id = u32::try_from(self.items.len()).unwrap_or(u32::MAX);
        self.ids.insert(resource.key(), id);
        self.items.push(resource.clone());
    }

    fn wires(&self) -> Vec<wire::ResourceWire> {
        self.items
            .iter()
            .map(|resource| match resource {
                Resource::CancelToken(token) => wire::ResourceWire::CancelToken {
                    cancelled: token.is_cancelled(),
                    sources: token
                        .sources()
                        .iter()
                        .filter_map(|source| {
                            self.ids
                                .get(&snapshot_ctx::resource_key_of(source))
                                .copied()
                        })
                        .collect(),
                },
                Resource::TaskGroup(group) => {
                    let snapshot = group.snapshot();
                    wire::ResourceWire::TaskGroup {
                        limit: snapshot.limit as u64,
                        name: snapshot.name,
                        members: snapshot
                            .members
                            .iter()
                            .map(|member| (member.member_id, member.active))
                            .collect(),
                    }
                }
            })
            .collect()
    }
}

/// Serialize the state of the paused threads of one run.
///
/// The caller guarantees that no thread of the run is executing and that the
/// collector cannot move objects for the duration of the call.
///
/// # Errors
///
/// [`SnapshotError::Blocked`] when the run holds a value that cannot be
/// serialized or a thread has a native continuation frame on its stack.
pub fn write_snapshot(
    heap: &BexHeap,
    threads: &[ThreadInput],
    opts: WriteOptions,
) -> Result<(Vec<u8>, WriteStats), SnapshotError> {
    let walk_started = Instant::now();

    let mut states = Vec::with_capacity(threads.len());
    for thread in threads {
        let state = thread
            .vm
            .export_thread_state()
            .map_err(|reason| SnapshotError::Blocked {
                reason,
                path: vec![thread_label(thread.thread_id, &thread.name)],
            })?;
        states.push(state);
    }

    let roots: Vec<Root> = threads
        .iter()
        .zip(&states)
        .flat_map(|(thread, state)| thread_roots(thread, Some(state)))
        .collect();

    let mut declarations = None;
    let graph = walk::walk(heap, &roots, false, &mut |head| {
        declarations
            .get_or_insert_with(|| heap.compile_time_declaration_index())
            .contains_key(&head.tag())
    })
    .map_err(|blocked| SnapshotError::Blocked {
        reason: blocked.reason,
        path: blocked.path,
    })?;
    // A pending future is settled by a thread. When that thread is not part
    // of the snapshot, nothing would ever settle the restored future.
    let producers: std::collections::HashSet<HeapPtr> = threads
        .iter()
        .filter_map(|thread| thread.settles_future.and_then(|future| future.object))
        .collect();
    let mut resources = ResourceTable::default();
    for (id, ptr) in graph.objects.iter().enumerate() {
        // SAFETY: the caller guarantees a quiescent heap; the walk reached `ptr`.
        #[allow(unsafe_code)]
        match unsafe { heap.get_object(*ptr) } {
            Object::Future(future)
                if matches!(future.read(), FutureRead::Pending(_)) && !producers.contains(ptr) =>
            {
                return Err(SnapshotError::Blocked {
                    reason: "the run holds a pending future that no thread of the run settles"
                        .to_string(),
                    path: graph.path_to(heap, &roots, u32::try_from(id).unwrap_or(u32::MAX)),
                });
            }
            Object::RustData(data) => {
                if let Some(resource) = Resource::of(data) {
                    resources.add(&resource);
                }
            }
            _ => {}
        }
    }
    for thread in threads {
        for token in &thread.user_cancels {
            resources.add(&Resource::CancelToken(Arc::clone(token)));
        }
        if let Some(membership) = &thread.group {
            resources.add(&Resource::TaskGroup(Arc::clone(&membership.group)));
        }
    }

    // Frame type arguments are not heap objects, so the walk did not see
    // their unresolved heads. The loader must be able to bind them too.
    for (thread, state) in threads.iter().zip(&states) {
        for frame in &state.frames {
            let mut undeclared = None;
            for ty in frame.frame_types() {
                ty.visit_heads(&mut |head| {
                    if !head.is_resolved()
                        && !declarations
                            .get_or_insert_with(|| heap.compile_time_declaration_index())
                            .contains_key(&head.tag())
                    {
                        undeclared = Some(head.tag().as_i64());
                    }
                });
            }
            if let Some(tag) = undeclared {
                return Err(SnapshotError::Blocked {
                    reason: format!(
                        "a type argument names a type (tag {tag}) that the program does not declare"
                    ),
                    path: vec![
                        thread_label(thread.thread_id, &thread.name),
                        format!("frame {}", frame.function_name),
                    ],
                });
            }
        }
    }
    let walk_ms = walk_started.elapsed().as_secs_f64() * 1000.0;

    let encode_started = Instant::now();
    let object_count = graph.objects.len();
    let resource_wires = resources.wires();
    let table = WriterTable {
        runtime: graph.ids,
        compile_time: heap.snapshot_compile_time_region(),
        resources: resources.ids.clone(),
    };
    let objects = graph.objects;
    let (encoded, _table) = snapshot_ctx::with_writer(table, || -> std::io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(object_count * 48 + 256);
        (heap.compile_time_len() as u64).serialize(&mut out)?;
        // The run state comes before the objects so that `read_run_state` can
        // return it without decoding any object or thread.
        opts.run_state.serialize(&mut out)?;
        opts.future_id_span.serialize(&mut out)?;
        resource_wires.serialize(&mut out)?;
        u32::try_from(object_count)
            .map_err(|_| std::io::Error::other("too many objects"))?
            .serialize(&mut out)?;
        let mut blob = Vec::new();
        for ptr in &objects {
            blob.clear();
            // SAFETY: the caller guarantees that no thread runs and that the
            // collector cannot move objects; `ptr` was reached by the walk.
            #[allow(unsafe_code)]
            let object = unsafe { heap.get_object(*ptr) };
            object.serialize(&mut blob)?;
            blob.serialize(&mut out)?;
        }
        let thread_wires: Vec<wire::ThreadWire> = threads
            .iter()
            .zip(&states)
            .map(|(thread, state)| wire::ThreadWire {
                thread_id: thread.thread_id,
                parent_thread: thread.parent_thread,
                name: thread.name.clone(),
                parked_kind: thread.parked.kind.clone(),
                parked_payload: thread.parked.payload.clone(),
                engine_state: thread.engine_state.clone(),
                extra_roots: thread.extra_roots.clone(),
                settles_future_id: thread
                    .settles_future
                    .map(|future| future.id.as_usize() as u64),
                settles_future: thread
                    .settles_future
                    .and_then(|future| future.object)
                    .unwrap_or_else(HeapPtr::null),
                cancelled: thread.cancel.cancelled,
                cancel_parent: thread.cancel.parent,
                user_cancels: thread
                    .user_cancels
                    .iter()
                    .filter_map(|token| {
                        resources
                            .ids
                            .get(&snapshot_ctx::resource_key_of(token))
                            .copied()
                    })
                    .collect(),
                group: thread.group.as_ref().and_then(|membership| {
                    resources
                        .ids
                        .get(&snapshot_ctx::resource_key_of(&membership.group))
                        .map(|id| (*id, membership.member_id))
                }),
                state: wire::VmStateWire::from_state(state),
            })
            .collect();
        thread_wires.serialize(&mut out)?;
        Ok(out)
    });
    let state_bytes = encoded
        .map_err(|err| SnapshotError::Format(format!("the run state does not serialize: {err}")))?;
    let encode_ms = encode_started.elapsed().as_secs_f64() * 1000.0;

    let mut header = opts.header;
    header.format_version = FORMAT_VERSION;
    let (bytes, sizes) = format::encode(
        &header,
        opts.embed_program.as_deref(),
        &state_bytes,
        opts.compress,
    )?;

    Ok((
        bytes,
        WriteStats {
            walk_ms,
            encode_ms,
            compress_ms: sizes.compress_ms,
            objects: object_count as u64,
            raw_bytes: state_bytes.len() as u64,
            compressed_bytes: sizes.state_stored,
        },
    ))
}

/// Read the header of a snapshot. This parses the first few hundred bytes and
/// does not verify the checksum; [`restore_into`] does.
///
/// # Errors
///
/// [`SnapshotError::Format`] for bytes that are not a snapshot,
/// [`SnapshotError::Mismatch`] for an unsupported format version.
pub fn read_header(bytes: &[u8]) -> Result<SnapshotHeader, SnapshotError> {
    format::read_header(bytes)
}

/// The program bytes embedded in a snapshot, if any. A process that starts
/// from a self-contained snapshot needs them before it can build the heap that
/// [`restore_into`] takes.
///
/// # Errors
///
/// As for [`restore_into`], for the container level.
pub fn read_embedded_program(bytes: &[u8]) -> Result<Option<Vec<u8>>, SnapshotError> {
    Ok(format::decode(bytes, false)?.program)
}

/// The opaque engine state a snapshot was written with
/// (`WriteOptions::run_state`), without a heap: a process that resumes a run
/// reads the function name from it before it has compiled the program.
///
/// # Errors
///
/// As for [`restore_into`], for the container level.
pub fn read_run_state(bytes: &[u8]) -> Result<Vec<u8>, SnapshotError> {
    use borsh::BorshDeserialize as _;
    let container = format::decode(bytes, true)?;
    let mut input: &[u8] = &container.state;
    let malformed = |err: std::io::Error| {
        SnapshotError::Format(format!("the state section does not decode: {err}"))
    };
    let _compile_time_len = u64::deserialize(&mut input).map_err(malformed)?;
    Vec::<u8>::deserialize(&mut input).map_err(malformed)
}

/// Allocate every snapshot object in `heap` and return thread states whose
/// values point into `heap`. `program_hash` is checked against the header.
///
/// `heap` must have been built from the identical program. The restored
/// objects are not rooted until the caller installs the thread states in VMs
/// that are registered with the collector, so the caller must keep the
/// collector from running until then (see
/// `BexHeap::alloc_snapshot_placeholders`).
///
/// # Errors
///
/// [`SnapshotError::Format`] when the bytes are corrupt or fail validation,
/// [`SnapshotError::Mismatch`] when the snapshot belongs to another program or
/// format version.
pub fn restore_into(
    heap: &BexHeap,
    bytes: &[u8],
    program_hash: [u8; 32],
) -> Result<Restored, SnapshotError> {
    loader::restore_into(heap, bytes, program_hash, RestoreOptions::default())
}

/// [`restore_into`] for an engine that has already issued future ids.
///
/// # Errors
///
/// As for [`restore_into`].
pub fn restore_into_with(
    heap: &BexHeap,
    bytes: &[u8],
    program_hash: [u8; 32],
    options: RestoreOptions,
) -> Result<Restored, SnapshotError> {
    loader::restore_into(heap, bytes, program_hash, options)
}
