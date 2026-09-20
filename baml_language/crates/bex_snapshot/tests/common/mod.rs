//! Shared harness for the snapshot acceptance tests.
//!
//! The tests work at the VM level, without `bex_engine`. A small driver plays
//! the engine's part: it answers every sys-op with a deterministic value and
//! resumes the VM after every yield. In "hopping" mode the driver also moves
//! the run at each yield: it writes a snapshot, builds a second heap and VM
//! from the same program, restores the snapshot there, and continues in the
//! new VM. A hopped run must produce the same result as a plain run.

#![allow(dead_code)]

use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bex_heap::CollectionLevel;

use bex_snapshot::{
    ParkedAt, RestoredThread, SnapshotError, SnapshotHeader, ThreadInput, WriteOptions, WriteStats,
};
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::{Object, Program, RootHaver, Value, ValueKind};

/// Cap on `exec()` calls per run, so that a regression fails instead of
/// hanging.
const MAX_EXEC_CALLS: usize = 100_000;
/// Small early-yield interval, so that compute loops yield quickly in tests.
pub(crate) const TEST_YIELD_INTERVAL: u64 = 1 << 10;

pub(crate) fn compile(source: &str) -> Program {
    baml_db::testing::compile_source(source)
}

pub(crate) struct Harness {
    pub(crate) program: Program,
    pub(crate) program_hash: [u8; 32],
    pub(crate) park: Arc<AtomicBool>,
    /// When set, [`Harness::restore`] runs a collection of this level on the
    /// target heap right after the import, the way the engine's collector
    /// would: roots gathered from the VM plus the restored extra roots, then
    /// every root forwarded. The restored objects must survive being moved.
    pub(crate) gc_after_restore: Cell<Option<CollectionLevel>>,
    /// Early-yield check interval of every VM the harness builds. With the
    /// park flag set, an interval of 1 makes the VM yield at every check,
    /// including the checks right after a call and right after a return.
    pub(crate) yield_interval: Cell<u64>,
}

/// Collect garbage on `vm`'s heap with `vm` and `extra` as the only roots.
#[allow(unsafe_code)]
pub(crate) fn collect_garbage(vm: &mut BexVm, extra: &mut [Value], level: CollectionLevel) {
    let mut roots = Vec::new();
    vm.collect_roots(&mut roots);
    roots.extend(extra.iter().filter_map(Value::as_object_ptr));
    // SAFETY: the VM is parked and this test thread is the only user of the heap.
    let (_stats, _roots, forwarding) =
        unsafe { vm.heap.collect_garbage_generational(&roots, level) };
    assert!(
        roots.is_empty() || !forwarding.is_empty(),
        "a collection with live runtime roots moves at least one object"
    );
    vm.forward_roots(&forwarding);
    for value in extra.iter_mut() {
        if let Some(moved) = value.as_object_ptr().and_then(|ptr| forwarding.get(&ptr)) {
            *value = Value::object(*moved);
        }
    }
}

impl Harness {
    pub(crate) fn new(source: &str) -> Self {
        Self::from_program(compile(source))
    }

    /// Compile without optimizations, which keeps every named local in its
    /// own slot. Tests that look at locals by name use this.
    pub(crate) fn new_unoptimized(source: &str) -> Self {
        Self::from_program(baml_db::testing::compile_source_with_opt(
            source,
            baml_db::testing::OptLevel::Zero,
        ))
    }

    fn from_program(program: Program) -> Self {
        let program_hash = bex_snapshot::program_hash(&program).expect("program hashes");
        Self {
            program,
            program_hash,
            park: Arc::new(AtomicBool::new(false)),
            gc_after_restore: Cell::new(None),
            yield_interval: Cell::new(TEST_YIELD_INTERVAL),
        }
    }

    /// A fresh heap and VM built from the program.
    pub(crate) fn new_vm(&self) -> BexVm {
        let mut vm = BexVm::from_program(self.program.clone(), Arc::clone(&self.park))
            .expect("program loads");
        vm.early_yield = bex_vm_types::EarlyYieldCheck::with_interval(
            Arc::clone(&self.park),
            self.yield_interval.get(),
        );
        vm
    }

    pub(crate) fn start(&self, function: &str) -> BexVm {
        let mut vm = self.new_vm();
        let index = self
            .program
            .function_index(function)
            .unwrap_or_else(|| panic!("function {function} not found"));
        let ptr = vm.heap.compile_time_ptr(index);
        vm.set_entry_point(ptr, &[]);
        vm
    }

    pub(crate) fn header(&self, seq: u64) -> SnapshotHeader {
        SnapshotHeader {
            format_version: bex_snapshot::FORMAT_VERSION,
            runtime_build: bex_snapshot::runtime_build(),
            program_hash: self.program_hash,
            run_id: "r-test".to_string(),
            segment: 1,
            seq,
            created_unix_ms: 1_700_000_000_000,
        }
    }

    pub(crate) fn write(
        &self,
        vm: &BexVm,
        parked: ParkedAt,
        extra_roots: Vec<Value>,
        compress: bool,
        seq: u64,
    ) -> Result<(Vec<u8>, WriteStats), SnapshotError> {
        let threads = [ThreadInput {
            thread_id: 1,
            parent_thread: None,
            name: "main".to_string(),
            vm,
            parked,
            extra_roots,
        }];
        bex_snapshot::write_snapshot(
            &vm.heap,
            &threads,
            WriteOptions {
                header: self.header(seq),
                embed_program: None,
                compress,
                run_state: seq.to_le_bytes().to_vec(),
            },
        )
    }

    /// Restore a one-thread snapshot into a fresh heap and VM.
    pub(crate) fn restore(&self, bytes: &[u8]) -> (BexVm, RestoredThread) {
        let mut vm = self.new_vm();
        let mut restored =
            bex_snapshot::restore_into(&vm.heap, bytes, self.program_hash).expect("restores");
        assert_eq!(restored.threads.len(), 1);
        let mut thread = restored.threads.remove(0);
        let state = std::mem::take(&mut thread.state);
        vm.import_thread_state(state).expect("imports");
        if let Some(level) = self.gc_after_restore.get() {
            collect_garbage(&mut vm, &mut thread.extra_roots, level);
        }
        (vm, thread)
    }

    /// Run `function` to completion in one VM and render the result.
    pub(crate) fn run_plain(&self, function: &str) -> String {
        let mut vm = self.start(function);
        let mut sysops = 0usize;
        for _ in 0..MAX_EXEC_CALLS {
            match vm.exec().expect("exec") {
                VmExecState::Complete(value) => return render(&vm, value),
                VmExecState::SysOp { operation, args } => {
                    let result = sysop_result(&mut vm, &format!("{operation:?}"), &args, sysops);
                    sysops += 1;
                    vm.stack.push(result);
                }
                VmExecState::EarlyYield => {}
                other => panic!("unexpected yield: {other:?}"),
            }
        }
        panic!("run did not complete");
    }

    /// Run `function` to completion, moving the run to a fresh heap and VM at
    /// every sys-op yield and at the first `max_early_hops` early yields.
    /// Returns the rendered result and the number of hops.
    pub(crate) fn run_hopping(
        &self,
        function: &str,
        compress: bool,
        max_early_hops: usize,
    ) -> (String, usize) {
        let mut vm = self.start(function);
        let mut sysops = 0usize;
        let mut hops = 0usize;
        let mut early_hops = 0usize;
        for _ in 0..MAX_EXEC_CALLS {
            match vm.exec().expect("exec") {
                VmExecState::Complete(value) => return (render(&vm, value), hops),
                VmExecState::SysOp { operation, args } => {
                    let op_name = format!("{operation:?}");
                    // The VM drained the arguments from its stack, so they are
                    // rooted only by this yield value: record them.
                    let parked = ParkedAt {
                        kind: "sysop".to_string(),
                        payload: op_name.clone().into_bytes(),
                    };
                    let (bytes, stats) = self
                        .write(&vm, parked, args, compress, hops as u64)
                        .expect("snapshot at a sys-op yield");
                    assert!(stats.raw_bytes > 0);
                    drop(vm);

                    let (new_vm, thread) = self.restore(&bytes);
                    vm = new_vm;
                    hops += 1;
                    assert_eq!(thread.parked.kind, "sysop");
                    assert_eq!(thread.parked.payload, op_name.as_bytes());
                    // Emulate the engine: run the operation on the restored
                    // arguments and push its result.
                    let result = sysop_result(&mut vm, &op_name, &thread.extra_roots, sysops);
                    sysops += 1;
                    vm.stack.push(result);
                }
                VmExecState::EarlyYield => {
                    if early_hops >= max_early_hops {
                        self.park.store(false, Ordering::Release);
                        continue;
                    }
                    early_hops += 1;
                    let (bytes, _) = self
                        .write(&vm, ParkedAt::runnable(), Vec::new(), compress, hops as u64)
                        .expect("snapshot at an early yield");
                    drop(vm);
                    let (new_vm, thread) = self.restore(&bytes);
                    vm = new_vm;
                    hops += 1;
                    assert_eq!(thread.parked, ParkedAt::runnable());
                }
                other => panic!("unexpected yield: {other:?}"),
            }
        }
        panic!("run did not complete");
    }

    /// Run `function` until its `n`-th sys-op yield (0-based) and return the
    /// parked VM with the yield's operation name and arguments.
    pub(crate) fn run_to_sysop(&self, function: &str, n: usize) -> (BexVm, String, Vec<Value>) {
        let mut vm = self.start(function);
        let mut sysops = 0usize;
        for _ in 0..MAX_EXEC_CALLS {
            match vm.exec().expect("exec") {
                VmExecState::SysOp { operation, args } => {
                    let op_name = format!("{operation:?}");
                    if sysops == n {
                        return (vm, op_name, args);
                    }
                    let result = sysop_result(&mut vm, &op_name, &args, sysops);
                    sysops += 1;
                    vm.stack.push(result);
                }
                VmExecState::EarlyYield => {}
                other => panic!("unexpected state before sys-op {n}: {other:?}"),
            }
        }
        panic!("run did not reach sys-op {n}");
    }
}

/// The deterministic "engine": `baml.io.input` answers with a string derived
/// from its prompt argument and the call's ordinal, everything else with
/// null. Reading the argument proves that the recorded extra roots survive a
/// restore.
fn sysop_result(vm: &mut BexVm, op_name: &str, args: &[Value], ordinal: usize) -> Value {
    if op_name.to_lowercase().contains("input") {
        let prompt = args
            .first()
            .map_or_else(String::new, |arg| render(vm, *arg));
        let ptr = vm
            .tlab
            .alloc_string(format!("answer {ordinal} to {prompt}"));
        Value::object(ptr)
    } else {
        Value::NULL
    }
}

/// Structural rendering of a value, independent of addresses, so that results
/// from different heaps compare.
pub(crate) fn render(vm: &BexVm, value: Value) -> String {
    fn go(vm: &BexVm, value: Value, depth: usize, out: &mut String) {
        use std::fmt::Write as _;
        if depth > 16 {
            out.push('…');
            return;
        }
        let ptr = match value.kind() {
            ValueKind::Null => return out.push_str("null"),
            ValueKind::OmittedArg => return out.push_str("<omitted>"),
            ValueKind::Bool(b) => return out.push_str(&b.to_string()),
            ValueKind::Int(i) => return out.push_str(&i.to_string()),
            ValueKind::Object(ptr) => ptr,
        };
        match vm.get_object(ptr) {
            Object::String(s) => write!(out, "{:?}", s.to_string()).expect("write"),
            Object::Float(f) => write!(out, "{f:?}").expect("write"),
            Object::Bigint(b) => write!(out, "{b}n").expect("write"),
            Object::Array(array) => {
                out.push('[');
                for (i, item) in array.data.lock().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    go(vm, *item, depth + 1, out);
                }
                out.push(']');
            }
            Object::Map(map) => {
                out.push('{');
                for (i, (key, item)) in map.data.lock().iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    write!(out, "{:?}: ", key.to_string()).expect("write");
                    go(vm, *item, depth + 1, out);
                }
                out.push('}');
            }
            Object::Instance(instance) => {
                let Object::Class(class) = vm.get_object(instance.class) else {
                    panic!("instance without a class");
                };
                write!(out, "{} {{", class.name.display_name()).expect("write");
                for (i, slot) in instance.fields.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&class.fields[i].name);
                    out.push_str(": ");
                    go(vm, slot.load(), depth + 1, out);
                }
                out.push('}');
            }
            Object::Variant(variant) => {
                let Object::Enum(enm) = vm.get_object(variant.enm) else {
                    panic!("variant without an enum");
                };
                write!(
                    out,
                    "{}.{}",
                    enm.name.display_name(),
                    enm.variants[variant.index].name
                )
                .expect("write");
            }
            other => write!(out, "<{other}>").expect("write"),
        }
    }
    let mut out = String::new();
    go(vm, value, 0, &mut out);
    out
}
