//! Regression coverage for GC ordering around nested native callbacks.
//!
//! The VM's active heap permit spans `exec()`. This test polls a real collection
//! request while a native callback has displaced the predecessor type lanes, then
//! verifies that collection and forwarding happen only after the callback returns.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Waker},
    time::Duration,
};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, GcStats};
use bex_vm::{
    BexVm, NativeFunction,
    package_baml::{Continuation, NativeCallResult},
};
use bex_vm_types::{FunctionKind, HeapPtr, Object, RealizedTy, RootHaver, Value};
use sys_native::SysOpsExt;
use tokio::sync::Notify;

use crate::stdlib_prefix::{OptLevel, compile_source_with_opt};

type Collection = Pin<Box<dyn Future<Output = GcStats> + Send>>;

/// Shared state for the one engine and one collection request in this test.
struct Probe {
    engine: Arc<BexEngine>,
    collection: Mutex<Option<Collection>>,
    ready: Notify,
    callback_active: AtomicBool,
    collection_pending: AtomicBool,
    scans_after_callback: AtomicUsize,
    record_head: Mutex<Option<HeapPtr>>,
    record_moved: AtomicBool,
}

static PROBE: OnceLock<Probe> = OnceLock::new();

/// Return the callback value and mark the nested dispatch interval complete.
struct ReturnValue;

impl Continuation for ReturnValue {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        PROBE
            .get()
            .expect("GC ordering probe must be initialized")
            .callback_active
            .store(false, Ordering::SeqCst);
        NativeCallResult::Done(value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }

    fn apply_forwarding(&mut self, _forwarding: &HashMap<HeapPtr, HeapPtr>) {}
}

/// Record the runtime class head and synchronously yield to the native callback.
fn outer(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
    let [RealizedTy::Class(head, ..), ..] = vm.current_call_type_args() else {
        panic!("outer native must receive the runtime class argument");
    };
    *PROBE
        .get()
        .expect("GC ordering probe must be initialized")
        .record_head
        .lock()
        .expect("record head mutex") = Some(head.ptr());
    NativeCallResult::YieldToCall {
        callee: args[0].as_object_ptr().expect("native callback"),
        args: vec![],
        type_args: vec![],
        continuation: Box::new(ReturnValue),
    }
}

/// Poll the real GC request while the nested callback has displaced predecessors.
fn inner(vm: &mut BexVm, _args: &[Value]) -> NativeCallResult {
    let probe = PROBE.get().expect("GC ordering probe must be initialized");
    assert!(
        vm.current_call_type_args().is_empty(),
        "the non-generic callback must receive an empty type-argument lane",
    );

    // This native is reached from outer's YieldToCall before the callback call
    // setup restores outer's predecessor lanes. The request itself is the
    // production collection future, not a sleep or a separately spawned probe.
    probe.callback_active.store(true, Ordering::SeqCst);
    let engine = Arc::clone(&probe.engine);
    let mut collection: Collection = Box::pin(async move {
        engine
            .collect_garbage(bex_heap::CollectionLevel::Major)
            .await
    });
    assert!(
        collection
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "GC must wait while the VM owns its active heap permit",
    );
    probe.collection_pending.store(true, Ordering::SeqCst);
    *probe.collection.lock().expect("collection mutex") = Some(collection);
    probe.ready.notify_one();
    NativeCallResult::Done(Value::int(42))
}

/// Observe root scanning and forwarding without adding a root of its own.
struct ObserveCollection;

impl RootHaver for ObserveCollection {
    fn collect_roots(&self, _roots: &mut Vec<HeapPtr>) {
        let probe = PROBE.get().expect("GC ordering probe must be initialized");
        assert!(
            !probe.callback_active.load(Ordering::SeqCst),
            "GC root scanning started while the nested native callback was active",
        );
        if probe.collection_pending.load(Ordering::SeqCst) {
            probe.scans_after_callback.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn forward_roots(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        let probe = PROBE.get().expect("GC ordering probe must be initialized");
        let old = *probe.record_head.lock().expect("record head mutex");
        if let Some(old) = old
            && let Some(new) = forwarding.get(&old)
            && old != *new
        {
            probe.record_moved.store(true, Ordering::SeqCst);
        }
    }
}

/// Exercise the real engine permit, nested native dispatch, and moving GC.
#[tokio::test]
async fn gc_waits_for_native_predecessor_restoration() {
    let mut program = compile_source_with_opt(
        r#"
        function outer<T>(callback: () -> int) -> int { callback() }
        function inner() -> int { 42 }
        function main() -> string {
            let package = reflect.Package.compile({"schema.baml": `class Record { value int }`});
            let declaration = package.get_class("root.Record") ?? throw "missing class";
            type Record = unreflect(declaration.as_type())
            let value = json.from_string<Record>(`{"value":42}`);
            let result = outer<Record>(inner);
            if (result != 42) { throw "wrong callback result"; }
            // Keep the runtime declaration reachable until the queued collector runs.
            baml.sys.sleep(baml.time.Duration.from_milliseconds(0n));
            json.to_string(value)
        }
    "#,
        OptLevel::Zero,
    );

    for (name, native) in [
        ("user.outer", outer as NativeFunction),
        ("user.inner", inner as NativeFunction),
    ] {
        let function = program
            .objects
            .iter_mut()
            .find_map(|object| match object {
                Object::Function(function) if function.name == name => Some(function),
                _ => None,
            })
            .expect("compiled test function");
        function.kind = FunctionKind::Native(native as *const ());
    }

    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            bex_project::runtime_compiler(),
        )
        .expect("GC ordering probe engine"),
    );
    assert!(
        PROBE
            .set(Probe {
                engine: Arc::clone(&engine),
                collection: Mutex::new(None),
                ready: Notify::new(),
                callback_active: AtomicBool::new(false),
                collection_pending: AtomicBool::new(false),
                scans_after_callback: AtomicUsize::new(0),
                record_head: Mutex::new(None),
                record_moved: AtomicBool::new(false),
            })
            .is_ok(),
        "the GC ordering probe must run once per test process",
    );

    // Register before the VM call so this observer is the first root holder. In
    // the deliberate MAX_PERMITS - 1 control it fails before VM root scanning or
    // the unsafe moving collector can observe a running VM.
    let _observer = engine
        .heap_permit_manager()
        .new_permit(ObserveCollection)
        .await;

    tokio::time::timeout(Duration::from_secs(30), async move {
        let mut call = tokio::spawn(async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        });
        let probe = PROBE.get().expect("GC ordering probe must be initialized");
        tokio::select! {
            () = probe.ready.notified() => {}
            result = &mut call => panic!("engine finished before the GC probe: {result:?}"),
        }

        let collection = probe
            .collection
            .lock()
            .expect("collection mutex")
            .take()
            .expect("inner native must retain the polled collection");
        collection.await;

        let value = call.await.expect("VM task").expect("engine call");
        assert_eq!(value, BexExternalValue::String("{\"value\":42}".into()),);
        assert!(
            probe.scans_after_callback.load(Ordering::SeqCst) > 0,
            "the retained GC must scan roots after the callback returns",
        );
        assert!(
            probe.record_moved.load(Ordering::SeqCst),
            "the same runtime Record head must have a non-identity forwarding entry",
        );
    })
    .await
    .expect("scheduled GC and VM must finish");
}
