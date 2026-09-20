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
//! host resources and other `RustData`, host callbacks, futures, a native
//! continuation frame on a call stack, or a declaration created at runtime.
//! The caller is expected to let the run continue and try again at a later
//! yield.
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

use std::time::Instant;

use bex_heap::BexHeap;
use bex_vm::{BexVm, snapshot::VmThreadState};
use bex_vm_types::{
    Program, Value,
    snapshot_ctx::{self, WriterTable},
};
use borsh::BorshSerialize;
use sha2::{Digest, Sha256};

pub use crate::dump::{state_dump, state_dump_from_bytes};
use crate::walk::Root;

/// Version of the container and state layout. A reader refuses any other
/// version.
pub const FORMAT_VERSION: u32 = 1;

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
}

pub struct WriteOptions {
    pub header: SnapshotHeader,
    /// Borsh bytes of the program, to make the snapshot self-contained.
    pub embed_program: Option<Vec<u8>>,
    /// Compress the sections with zstd level 1.
    pub compress: bool,
    /// Opaque engine state, for example the `call_id` counter.
    pub run_state: Vec<u8>,
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

/// One restored thread. `state` and `extra_roots` point into the target heap.
pub struct RestoredThread {
    pub thread_id: u64,
    pub parent_thread: Option<u64>,
    pub name: String,
    pub state: VmThreadState,
    pub parked: ParkedAt,
    pub extra_roots: Vec<Value>,
}

pub struct Restored {
    pub header: SnapshotHeader,
    pub threads: Vec<RestoredThread>,
    pub run_state: Vec<u8>,
    pub embedded_program: Option<Vec<u8>>,
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
    let table = WriterTable {
        runtime: graph.ids,
        compile_time: heap.snapshot_compile_time_region(),
    };
    let objects = graph.objects;
    let (encoded, _table) = snapshot_ctx::with_writer(table, || -> std::io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(object_count * 48 + 256);
        (heap.compile_time_len() as u64).serialize(&mut out)?;
        // The run state comes before the objects so that `read_run_state` can
        // return it without decoding any object or thread.
        opts.run_state.serialize(&mut out)?;
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
                extra_roots: thread.extra_roots.clone(),
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
    loader::restore_into(heap, bytes, program_hash)
}
