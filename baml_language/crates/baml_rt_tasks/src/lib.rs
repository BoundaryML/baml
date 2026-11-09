//! Single-owner stackful task scheduler. No interpreter or heap dependencies.
use corosensei::{
    Coroutine, CoroutineResult, Yielder,
    stack::{DefaultStack, Stack},
};
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    collections::VecDeque,
    future::Future,
    rc::{Rc, Weak},
    sync::mpsc,
    time::{Duration, Instant},
};
pub use tokio_util::sync::CancellationToken;

mod roots;
mod slice_clock;
mod tasks;
pub use roots::{IntegrationRoot, IntegrationRoots, RootSource};

/// Placeholder boundary values; unlike `Value`, these contain no heap addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnedValue {
    Unit,
    Int(i64),
    Text(String),
}
#[derive(Debug)]
pub enum Value {
    Owned(OwnedValue),
    Pointer {
        slot: UnsafeCell<usize>,
        owner: std::marker::PhantomData<Rc<()>>,
    },
}
impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Self::Owned(v) => Self::Owned(v.clone()),
            // SAFETY: loop-only, synchronous, non-moving root visitation.
            Self::Pointer { slot, .. } => Self::pointer(unsafe { *slot.get() }),
        }
    }
}
impl Value {
    pub fn pointer(address: usize) -> Self {
        Self::Pointer {
            slot: UnsafeCell::new(address),
            owner: std::marker::PhantomData,
        }
    }
    pub fn int(n: i64) -> Self {
        Self::Owned(OwnedValue::Int(n))
    }
    pub fn as_int(&self) -> i64 {
        match self {
            Self::Owned(OwnedValue::Int(n)) => *n,
            _ => panic!("expected int"),
        }
    }
    fn roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        if let Self::Pointer { slot, .. } = self {
            visit(slot.get());
        }
    }
    fn owned(self) -> Result<OwnedValue, Err> {
        match self {
            Self::Owned(v) => Ok(v),
            Self::Pointer { .. } => Err(Err::InternalError(
                "heap value needs host serialization".into(),
            )),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Err {
    Error { message: String, trace: Vec<String> },
    Cancelled,
    InternalError(String),
}
pub type Outcome = Result<Value, Err>;
type RawBody = unsafe fn(&mut TaskCx<'_>, [usize; 4]) -> Outcome;
enum Body {
    Boxed(Box<dyn FnOnce(&mut TaskCx<'_>) -> Outcome>),
    Managed { code: RawBody, data: [usize; 4] },
}
impl Body {
    fn run(self, cx: &mut TaskCx<'_>) -> Outcome {
        match self {
            Self::Boxed(body) => {
                cx.admit()?;
                body(cx)
            }
            Self::Managed { code, data } => unsafe { code(cx, data) },
        }
    }
}
#[derive(Clone, Debug)]
pub enum FutureState {
    Pending,
    Ready(Value),
    Error { message: String, trace: Vec<String> },
    Cancelled,
    InternalError(String),
}
impl FutureState {
    fn result(&self) -> Option<Outcome> {
        Some(match self {
            Self::Pending => return None,
            Self::Ready(v) => Ok(v.clone()),
            Self::Error { message, trace } => Err(Err::Error {
                message: message.clone(),
                trace: trace.clone(),
            }),
            Self::Cancelled => Err(Err::Cancelled),
            Self::InternalError(e) => Err(Err::InternalError(e.clone())),
        })
    }
}
/// Loop-local cancellation authority. Cross-thread cancellation uses linked
/// CancellationTokens; the common parent/child path needs no atomic or lock.
#[derive(Clone)]
pub struct TaskCancel {
    node: Rc<CancelNode>,
    deadline: Option<Instant>,
}
struct CancelNode {
    cancelled: Cell<bool>,
    parent: Option<Rc<CancelNode>>,
    linked: Vec<CancellationToken>,
    local_linked: Vec<TaskCancel>,
    external: bool,
    epoch: Rc<Cell<usize>>,
    poll_word: Rc<Cell<u8>>,
}
impl Drop for CancelNode {
    fn drop(&mut self) {
        let mut pending: Vec<_> = self.local_linked.drain(..).map(|t| t.node).collect();
        let mut parent = self.parent.take();
        loop {
            let Some(node) = parent.take().or_else(|| pending.pop()) else {
                break;
            };
            if let Ok(mut node) = Rc::try_unwrap(node) {
                parent = node.parent.take();
                pending.extend(node.local_linked.drain(..).map(|t| t.node));
            }
        }
    }
}
impl Default for TaskCancel {
    fn default() -> Self {
        Self::detached(vec![])
    }
}
impl TaskCancel {
    pub fn detached(linked: Vec<CancellationToken>) -> Self {
        Self::root(linked, Rc::default())
    }
    fn root(linked: Vec<CancellationToken>, epoch: Rc<Cell<usize>>) -> Self {
        Self {
            node: Rc::new(CancelNode {
                cancelled: Cell::new(false),
                parent: None,
                external: !linked.is_empty(),
                linked,
                local_linked: Vec::new(),
                epoch,
                poll_word: Rc::default(),
            }),
            deadline: None,
        }
    }
    pub fn child(&self, linked: Vec<CancellationToken>) -> Self {
        Self {
            node: Rc::new(CancelNode {
                cancelled: Cell::new(false),
                parent: Some(self.node.clone()),
                external: self.node.external || !linked.is_empty(),
                linked,
                local_linked: Vec::new(),
                epoch: self.node.epoch.clone(),
                poll_word: self.node.poll_word.clone(),
            }),
            deadline: self.deadline,
        }
    }
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(self.deadline.map_or(deadline, |d| d.min(deadline)));
        self
    }
    pub fn request_cancel(&self) -> bool {
        let was = self.node.cancelled.get();
        self.cancel();
        !was
    }
    pub fn cancel(&self) {
        self.node.poll_word.set(self.node.poll_word.get() | 1);
        if !self.node.cancelled.replace(true) {
            self.node.epoch.set(self.node.epoch.get().wrapping_add(1));
        }
    }
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        let mut pending = Vec::new();
        let mut token = self;
        loop {
            if token.deadline.is_some_and(|d| Instant::now() >= d) {
                return true;
            }
            let mut node = Some(&*token.node);
            while let Some(n) = node {
                if n.cancelled.get() || n.linked.iter().any(CancellationToken::is_cancelled) {
                    return true;
                }
                pending.extend(n.local_linked.iter());
                node = n.parent.as_deref();
            }
            match pending.pop() {
                Some(next) => token = next,
                None => return false,
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitReason {
    Futures,
    Io,
    Timer,
    Admission,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Parked(WaitReason),
    Done,
}
#[derive(Clone)]
pub struct FutureHandle(Rc<RefCell<FutureData>>);
struct FuturePool {
    records: Vec<Rc<RefCell<FutureData>>>,
    capacity: usize,
}
struct FutureData {
    pool: Weak<RefCell<FuturePool>>,
    id: usize,
    owner: Weak<RefCell<Core>>,
    state: FutureState,
    cancel: TaskCancel,
    waiters: Vec<usize>,
    observed: bool,
    cancel_requested: bool,
    errors: Rc<RefCell<Vec<(usize, Err)>>>,
}
impl Drop for FutureHandle {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) != 1 {
            return;
        }
        let pool = self.0.borrow().pool.upgrade();
        if let Some(pool) = pool {
            let mut pool = pool.borrow_mut();
            if pool.records.len() == pool.capacity {
                return;
            }
            let mut f = self.0.borrow_mut();
            if !f.observed
                && let Some(Err(e @ (Err::Error { .. } | Err::InternalError(_)))) = f.state.result()
            {
                f.errors.borrow_mut().push((f.id, e));
            }
            f.observed = true;
            f.state = FutureState::Pending;
            f.waiters.clear();
            // Only detach ancestry when no live descendants/cancel handles can
            // observe it. Otherwise a fresh node is allocated on reuse.
            if let Some(node) = Rc::get_mut(&mut f.cancel.node) {
                node.parent = None;
                node.linked.clear();
                node.local_linked.clear();
            }
            drop(f);
            pool.records.push(self.0.clone());
        }
    }
}
impl Drop for FutureData {
    fn drop(&mut self) {
        if !self.observed
            && let Some(Err(e @ (Err::Error { .. } | Err::InternalError(_)))) = self.state.result()
        {
            self.errors.borrow_mut().push((self.id, e));
        }
    }
}
impl FutureHandle {
    pub fn state(&self) -> FutureState {
        self.0.borrow().state.clone()
    }
    /// Requests only: settlement occurs when the producer actually exits.
    pub fn request_cancel(&self) {
        let _ = self.try_cancel();
    }
    /// Fire the producer token once, without settling its future early.
    pub fn try_cancel(&self) -> bool {
        let mut future = self.0.borrow_mut();
        let previous = std::mem::replace(&mut future.cancel_requested, true);
        if previous || !matches!(future.state, FutureState::Pending) {
            return false;
        }
        future.cancel.cancel();
        true
    }
    pub fn id(&self) -> usize {
        self.0.borrow().id
    }
    pub fn was_cancel_requested(&self) -> bool {
        self.0.borrow().cancel_requested
    }
    pub fn is_observed(&self) -> bool {
        self.0.borrow().observed
    }
    pub fn result(&self) -> Option<Outcome> {
        let mut f = self.0.borrow_mut();
        let result = f.state.result();
        if result.is_some() {
            f.observed = true;
        }
        result
    }
}
/// FIFO admission with synchronous registration and lifetime-held permits.
#[derive(Clone)]
pub struct Gate(Rc<RefCell<GateData>>);
struct GateData {
    limit: usize,
    active: usize,
    queue: VecDeque<usize>,
}
impl Gate {
    pub fn new(limit: usize) -> Self {
        assert!(limit > 0);
        Self(Rc::new(RefCell::new(GateData {
            limit,
            active: 0,
            queue: VecDeque::new(),
        })))
    }
    pub fn capacity(&self) -> usize {
        self.0.borrow().limit
    }
    pub fn counts(&self) -> (usize, usize) {
        let g = self.0.borrow();
        (g.active, g.queue.len())
    }
}
struct Ticket {
    gate: Gate,
    id: usize,
    admitted: bool,
}
impl Ticket {
    fn acquire(&mut self) -> bool {
        if self.admitted {
            return true;
        }
        let mut g = self.gate.0.borrow_mut();
        if g.active < g.limit && g.queue.front() == Some(&self.id) {
            g.queue.pop_front();
            g.active += 1;
            self.admitted = true;
        }
        self.admitted
    }
}
struct Tickets {
    tickets: Vec<Ticket>,
}
impl Tickets {
    fn acquire(&mut self) -> bool {
        if self.tickets.iter().any(|ticket| {
            !ticket.admitted && {
                let gate = ticket.gate.0.borrow();
                gate.active >= gate.limit || gate.queue.front() != Some(&ticket.id)
            }
        }) {
            return false;
        }
        self.tickets.iter_mut().all(Ticket::acquire)
    }
}
impl Drop for Ticket {
    fn drop(&mut self) {
        let mut g = self.gate.0.borrow_mut();
        if self.admitted {
            g.active -= 1;
        } else {
            g.queue.retain(|id| *id != self.id);
        }
    }
}
#[derive(Default)]
pub struct SpawnOptions {
    pub detached: bool,
    pub linked: Vec<CancellationToken>,
    pub local_linked: Vec<TaskCancel>,
    pub gates: Vec<Gate>,
    pub deadline: Option<Instant>,
    pub gate: Option<Gate>,
    pub roots: Vec<Value>,
    /// Managed environments/results held by inline entry data before execution.
    pub managed_roots: [usize; 2],
}
#[derive(Clone)]
pub struct Config {
    pub stack_size: usize,
    pub time_slice: Duration,
    pub pool_capacity: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            stack_size: 64 * 1024,
            time_slice: Duration::from_millis(1),
            pool_capacity: 128,
        }
    }
}
/// The independently built heap implements this seam; collection is synchronous.
pub trait LoopHeap {
    fn collect(&mut self, roots: &dyn RootSource);
}
impl LoopHeap for () {
    fn collect(&mut self, _: &dyn RootSource) {}
}
/// Owner-thread bindings installed whenever a fiber resumes. The pointed-to
/// context must outlive registration, including forced shutdown unwinding.
#[derive(Clone, Copy)]
pub struct SwitchHook {
    pub data: *mut (),
    pub enter: unsafe fn(*mut ()),
    pub leave: unsafe fn(*mut ()),
}
type Fiber = Coroutine<(), (), Outcome>;
struct Task {
    hook: Option<SwitchHook>,
    fiber: Option<Fiber>,
    base: usize,
    parked_sp: Option<usize>,
    stack_limit: usize,
    parent: Option<usize>,
    state: TaskState,
    future: FutureHandle,
    cancel: TaskCancel,
    shield: usize,
    io_cancel: Option<CancellationToken>,
    needs_service: bool,
    io_epoch: u64,
    completion: Option<Result<OwnedValue, Err>>,
    timer: Option<Instant>,
    ticket: Option<Tickets>,
    roots: Vec<Value>,
    managed_roots: [UnsafeCell<usize>; 2],
    host: Option<mpsc::Sender<Result<OwnedValue, Err>>>,
}
#[cfg(feature = "instrument")]
#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub resumes: usize,
    pub switches: usize,
    pub spawns: usize,
    pub pool_hits: usize,
    pub queue_probes: usize,
    pub service_scans: usize,
    pub checkpoints: usize,
    pub clock_reads: usize,
    pub slice_reads: usize,
    pub future_pool_hits: usize,
    pub future_allocations: usize,
    pub cancel_allocations: usize,
}
struct Core {
    active_hook: Option<SwitchHook>,
    inline_child: bool,
    slow_tasks: usize,
    io_pending: usize,
    host_enabled: bool,
    cancel_epoch: Rc<Cell<usize>>,
    last_cancel_epoch: usize,
    poll_word: Rc<Cell<u8>>,
    slice: slice_clock::Slice,
    #[cfg(feature = "instrument")]
    metrics: Metrics,
    tasks: tasks::Tasks,
    ready: VecDeque<usize>,
    pool: Vec<DefaultStack>,
    config: Config,
    tx: mpsc::Sender<Message>,
    runtime: tokio::runtime::Handle,
    rx: Rc<mpsc::Receiver<Message>>,
    futures: Vec<Weak<RefCell<FutureData>>>,
    future_pool: Rc<RefCell<FuturePool>>,
    errors: Rc<RefCell<Vec<(usize, Err)>>>,
    host_gate: std::sync::Arc<std::sync::Mutex<Option<mpsc::Sender<Message>>>>,
    heap: Rc<RefCell<dyn LoopHeap>>,
    running_base: Option<usize>,
    integration_slots: Vec<Weak<Cell<usize>>>,
}
/// !Send and !Sync: construction, execution, GC and destruction belong to one thread.
/// ```compile_fail
/// fn needs_send<T: Send>() {}
/// needs_send::<baml_rt_tasks::TaskLoop>();
/// ```
/// ```compile_fail
/// fn needs_send<T: Send>() {}
/// needs_send::<baml_rt_tasks::Value>();
/// ```
pub struct TaskLoop {
    core: Rc<RefCell<Core>>,
    rx: Rc<mpsc::Receiver<Message>>,
}
type HostBody = Box<dyn FnOnce(&mut TaskCx<'_>, Vec<OwnedValue>) -> Outcome + Send>;
enum Message {
    Io {
        id: usize,
        epoch: u64,
        result: Result<OwnedValue, Err>,
    },
    Submit {
        body: HostBody,
        args: Vec<OwnedValue>,
        timeout: Option<Duration>,
        reply: mpsc::Sender<Result<OwnedValue, Err>>,
    },
}
// A runtime may drop the monitor before ever polling it. Its destructor must
// still notify the loop, otherwise a parked producer would wait forever.
struct IoCompletion {
    tx: mpsc::Sender<Message>,
    id: usize,
    epoch: u64,
    sent: bool,
}
impl IoCompletion {
    fn finish(mut self, result: Result<OwnedValue, Err>) {
        self.sent = true;
        let _ = self.tx.send(Message::Io {
            id: self.id,
            epoch: self.epoch,
            result,
        });
    }
}
impl Drop for IoCompletion {
    fn drop(&mut self) {
        if !self.sent {
            let _ = self.tx.send(Message::Io {
                id: self.id,
                epoch: self.epoch,
                result: Err(Err::InternalError("I/O runtime stopped".into())),
            });
        }
    }
}
#[derive(Clone)]
pub struct SubmissionHandle {
    gate: std::sync::Arc<std::sync::Mutex<Option<mpsc::Sender<Message>>>>,
}
impl SubmissionHandle {
    pub fn submit(
        &self,
        args: Vec<OwnedValue>,
        timeout: Option<Duration>,
        body: impl FnOnce(&mut TaskCx<'_>, Vec<OwnedValue>) -> Outcome + Send + 'static,
    ) -> mpsc::Receiver<Result<OwnedValue, Err>> {
        let (reply, rx) = mpsc::channel();
        let gate = self.gate.lock().unwrap();
        let Some(tx) = &*gate else {
            let _ = reply.send(Err(Err::InternalError("loop closed".into())));
            return rx;
        };
        let msg = Message::Submit {
            body: Box::new(body),
            args,
            timeout,
            reply,
        };
        if let Err(mpsc::SendError(Message::Submit { reply, .. })) = tx.send(msg) {
            let _ = reply.send(Err(Err::InternalError("loop closed".into())));
        }
        rx
    }
}
#[derive(Debug, Default)]
pub struct ShutdownReport {
    pub unhandled: Vec<(usize, Err)>,
    pub leaked: Vec<usize>,
}
impl TaskLoop {
    #[cfg(feature = "instrument")]
    pub fn metrics(&self) -> Metrics {
        self.core.borrow().metrics.clone()
    }
    #[cfg(feature = "instrument")]
    pub fn reset_metrics(&mut self) {
        self.core.borrow_mut().metrics = Metrics::default();
    }

    pub fn new(
        config: Config,
        runtime: tokio::runtime::Handle,
        heap: Rc<RefCell<dyn LoopHeap>>,
    ) -> Self {
        assert!(config.stack_size >= 16 * 1024);
        assert!(
            matches!(
                runtime.runtime_flavor(),
                tokio::runtime::RuntimeFlavor::MultiThread
            ),
            "I/O requires a runtime on other threads"
        );
        let pool_capacity = config.pool_capacity;
        let (tx, rx) = mpsc::channel();
        let rx = Rc::new(rx);
        let host_gate = std::sync::Arc::new(std::sync::Mutex::new(Some(tx.clone())));
        Self {
            core: Rc::new(RefCell::new(Core {
                active_hook: None,
                #[cfg(feature = "instrument")]
                metrics: Metrics::default(),
                inline_child: false,
                slow_tasks: 0,
                io_pending: 0,
                host_enabled: false,
                cancel_epoch: Rc::default(),
                last_cancel_epoch: 0,
                poll_word: Rc::new(Cell::new(1)),
                slice: slice_clock::Slice::new(config.time_slice),
                tasks: tasks::Tasks::default(),
                ready: VecDeque::new(),
                pool: vec![],
                config,
                tx,
                host_gate,
                runtime,
                rx: rx.clone(),
                futures: vec![],
                future_pool: Rc::new(RefCell::new(FuturePool {
                    records: vec![],
                    capacity: pool_capacity,
                })),
                errors: Rc::default(),
                heap,
                running_base: None,
                integration_slots: Vec::new(),
            })),
            rx,
        }
    }
    pub fn submission_handle(&self) -> SubmissionHandle {
        self.core.borrow_mut().host_enabled = true;
        SubmissionHandle {
            gate: self.core.borrow().host_gate.clone(),
        }
    }
    pub fn spawn(
        &mut self,
        body: impl FnOnce(&mut TaskCx<'_>) -> Outcome + 'static,
    ) -> FutureHandle {
        spawn(
            &self.core,
            None,
            SpawnOptions::default(),
            Body::Boxed(Box::new(body)),
        )
    }
    pub fn spawn_with(
        &mut self,
        options: SpawnOptions,
        body: impl FnOnce(&mut TaskCx<'_>) -> Outcome + 'static,
    ) -> FutureHandle {
        spawn(&self.core, None, options, Body::Boxed(Box::new(body)))
    }
    pub fn task_count(&self) -> usize {
        self.core.borrow().tasks.len()
    }
    /// Live and pooled mmap reservations, including guard pages. Diagnostic
    /// pointers are valid only during the synchronous callback.
    pub fn stack_reservations(&self, visit: &mut dyn FnMut(*const u8, usize)) {
        let c = self.core.borrow();
        for t in c.tasks.values() {
            visit(t.stack_limit as *const u8, t.base - t.stack_limit);
        }
        for stack in &c.pool {
            visit(
                stack.limit().get() as *const u8,
                stack.base().get() - stack.limit().get(),
            );
        }
    }
    pub fn pooled_stacks(&self) -> usize {
        self.core.borrow().pool.len()
    }
    pub fn task_state(&self, handle: &FutureHandle) -> TaskState {
        self.core
            .borrow()
            .tasks
            .get(&handle.id())
            .map_or(TaskState::Done, |t| t.state.clone())
    }
    pub fn run_until(&mut self, future: &FutureHandle) -> Outcome {
        if !future.0.borrow().owner.ptr_eq(&Rc::downgrade(&self.core)) {
            return Err(Err::InternalError("future belongs to another loop".into()));
        }
        while matches!(future.state(), FutureState::Pending) {
            self.tick(true);
        }
        future.result().unwrap()
    }
    /// Retire reported failures between isolated host invocations without
    /// closing submissions or discarding reusable fibers.
    pub fn take_unhandled(&mut self) -> Vec<(usize, Err)> {
        let core = self.core.borrow();
        for weak in &core.futures {
            if let Some(future) = weak.upgrade() {
                let mut future = future.borrow_mut();
                if !future.observed
                    && let Some(Err(error @ (Err::Error { .. } | Err::InternalError(_)))) =
                        future.state.result()
                {
                    core.errors.borrow_mut().push((future.id, error));
                    future.observed = true;
                }
            }
        }
        std::mem::take(&mut *core.errors.borrow_mut())
    }
    pub fn run_until_idle(&mut self) {
        while self.tick(false) {}
    }
    /// Executes at most one fiber slice; `block` waits for queue/timer/cancellation.
    /// External token cancellation is observed within 1ms while idle.
    pub fn tick(&mut self, block: bool) -> bool {
        loop {
            maintenance(&self.core);
            let id = {
                let mut c = self.core.borrow_mut();
                c.ready.pop_front()
            };
            if let Some(id) = id {
                self.run_one(id);
                return true;
            }
            if !block {
                return false;
            }
            let delay = self
                .core
                .borrow()
                .tasks
                .values()
                .filter_map(|t| {
                    t.timer
                        .into_iter()
                        .chain(t.cancel.deadline.filter(|_| t.shield == 0))
                        .min()
                })
                .map(|d| d.saturating_duration_since(Instant::now()))
                .min()
                .unwrap_or(Duration::from_millis(1))
                .min(Duration::from_millis(1));
            if let Ok(msg) = self.rx.recv_timeout(delay) {
                message(&self.core, msg);
            }
        }
    }
    fn run_one(&mut self, id: usize) {
        run_one(&self.core, id);
    }
    /// Cooperative grace, followed by forced fiber unwinding. Non-polling CPU
    /// code cannot be preempted; callers must honor the poll contract.
    pub fn shutdown(&mut self, grace: Duration) -> ShutdownReport {
        self.core.borrow().host_gate.lock().unwrap().take();
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Message::Submit { reply, .. } => {
                    let _ = reply.send(Err(Err::InternalError("loop closed".into())));
                }
                other => message(&self.core, other),
            }
        }
        let end = Instant::now() + grace;
        while self.task_count() > 0 && Instant::now() < end {
            self.tick(false);
            std::thread::yield_now();
        }
        let leaked = self.core.borrow().tasks.keys().copied().collect();
        for t in self.core.borrow().tasks.values() {
            t.cancel.cancel();
        }
        let cancel_end = Instant::now() + grace;
        while self.task_count() > 0 && Instant::now() < cancel_end {
            self.tick(false);
            std::thread::yield_now();
        }
        // Drop outside Core borrow: forced unwinding runs arbitrary Rust drops.
        let tasks = self.core.borrow_mut().tasks.drain();
        for (_, mut t) in tasks {
            if let Some(token) = t.io_cancel.take() {
                token.cancel();
            }
            // Forced unwinding executes code on this fiber too. Install its
            // owner-thread bindings before running destructors, just as on a
            // normal resume. A binding may unregister itself while unwinding;
            // read active_hook afterwards instead of retaining its stack pointer.
            let previous = self.core.borrow_mut().active_hook.take();
            if let Some(hook) = previous {
                unsafe { (hook.leave)(hook.data) };
            }
            self.core.borrow_mut().active_hook = t.hook;
            if let Some(hook) = t.hook {
                unsafe { (hook.enter)(hook.data) };
            }
            drop(t.fiber.take());
            let current = self.core.borrow_mut().active_hook.take();
            if let Some(hook) = current {
                unsafe { (hook.leave)(hook.data) };
            }
            self.core.borrow_mut().active_hook = previous;
            if let Some(hook) = previous {
                unsafe { (hook.enter)(hook.data) };
            }
            t.future.0.borrow_mut().state = FutureState::Cancelled;
            if let Some(reply) = t.host.take() {
                let _ = reply.send(Err(Err::Cancelled));
            }
        }
        let mut c = self.core.borrow_mut();
        c.slow_tasks = 0;
        c.ready.clear();
        c.pool.clear();
        c.future_pool.borrow_mut().records.clear();
        for weak in &c.futures {
            if let Some(f) = weak.upgrade() {
                let mut f = f.borrow_mut();
                if !f.observed
                    && let Some(Err(e @ (Err::Error { .. } | Err::InternalError(_)))) =
                        f.state.result()
                {
                    c.errors.borrow_mut().push((f.id, e));
                    f.observed = true;
                }
            }
        }
        ShutdownReport {
            leaked,
            unhandled: std::mem::take(&mut *c.errors.borrow_mut()),
        }
    }
}
impl Drop for TaskLoop {
    fn drop(&mut self) {
        self.shutdown(Duration::ZERO);
    }
}
fn run_one(core: &Rc<RefCell<Core>>, id: usize) {
    #[cfg(feature = "instrument")]
    {
        let mut c = core.borrow_mut();
        c.metrics.resumes += 1;
        c.metrics.switches += 2;
    }
    let previous = core.borrow_mut().active_hook.take();
    if let Some(hook) = previous {
        unsafe { (hook.leave)(hook.data) };
    }
    let hook = core.borrow().tasks[&id].hook;
    core.borrow_mut().active_hook = hook;
    if let Some(hook) = hook {
        unsafe { (hook.enter)(hook.data) };
    }
    let mut fiber = {
        let mut c = core.borrow_mut();
        refresh_poll_word(&c, id);
        let t = c.tasks.get_mut(&id).unwrap();
        t.state = TaskState::Running;
        let base = t.base;
        let fiber = t.fiber.take().unwrap();
        c.running_base = Some(base);
        fiber
    };
    // No Core borrow survives a context switch: TaskCx may freely access it.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fiber.resume(())))
        .unwrap_or_else(|_| {
            CoroutineResult::Return(Err(Err::InternalError("task panicked".into())))
        });
    let current = core.borrow_mut().active_hook.take();
    if let Some(hook) = current {
        unsafe { (hook.leave)(hook.data) };
    }
    core.borrow_mut().active_hook = previous;
    if let Some(hook) = previous {
        unsafe { (hook.enter)(hook.data) };
    }
    let mut c = core.borrow_mut();
    c.running_base = None;
    match result {
        CoroutineResult::Yield(()) => {
            c.tasks.get_mut(&id).unwrap().fiber = Some(fiber);
        }
        CoroutineResult::Return(result) => {
            let mut task = c.tasks.remove(&id).unwrap();
            task.state = TaskState::Done;
            if result.is_err() {
                // Preserve cancellation from ancestors, linked tokens and
                // deadlines before firing the producer's own token below.
                task.future.0.borrow_mut().cancel_requested |= task.cancel.is_cancelled();
                task.cancel.cancel();
            }
            if let Some(token) = task.io_cancel.take() {
                token.cancel();
            }
            c.slow_tasks -= usize::from(task.needs_service);
            let state = match &result {
                Ok(v) => FutureState::Ready(v.clone()),
                Err(Err::Cancelled) => FutureState::Cancelled,
                Err(Err::Error { message, trace }) => FutureState::Error {
                    message: message.clone(),
                    trace: trace.clone(),
                },
                Err(Err::InternalError(e)) => FutureState::InternalError(e.clone()),
            };
            {
                let mut f = task.future.0.borrow_mut();
                f.state = state;
                for waiter in f.waiters.drain(..) {
                    wake(&mut c, waiter);
                }
            }
            if let Some(reply) = task.host.take() {
                task.future.0.borrow_mut().observed = true;
                let _ = reply.send(result.and_then(Value::owned));
            }
            let stack = fiber.into_stack();
            if c.pool.len() < c.config.pool_capacity {
                c.pool.push(stack);
            }
            // Ticket drops here, after producer frames have unwound.
        }
    }
    if c.futures.len() > c.tasks.len() * 2 + 1024 {
        c.futures.retain(|f| f.strong_count() > 0);
    }
}

fn wake(c: &mut Core, id: usize) {
    if let Some(t) = c.tasks.get_mut(&id)
        && matches!(t.state, TaskState::Parked(_))
    {
        t.state = TaskState::Ready;
        c.ready.push_back(id);
    }
}
fn spawn(
    core: &Rc<RefCell<Core>>,
    parent: Option<usize>,
    options: SpawnOptions,
    body: Body,
) -> FutureHandle {
    let mut c = core.borrow_mut();
    #[cfg(feature = "instrument")]
    {
        c.metrics.spawns += 1;
        c.metrics.pool_hits += usize::from(!c.pool.is_empty());
    }
    let id = c.tasks.reserve();
    let inherited = parent
        .and_then(|id| c.tasks.get(&id))
        .filter(|t| !options.detached && t.shield == 0)
        .map(|t| t.cancel.clone());
    let deadline = match (
        inherited.as_ref().and_then(|p| p.deadline),
        options.deadline,
    ) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    let node = CancelNode {
        cancelled: Cell::new(false),
        parent: inherited.as_ref().map(|p| p.node.clone()),
        external: !options.linked.is_empty() || inherited.as_ref().is_some_and(|p| p.node.external),
        linked: options.linked,
        local_linked: options.local_linked,
        epoch: c.cancel_epoch.clone(),
        poll_word: c.poll_word.clone(),
    };
    let needs_service =
        node.external || deadline.is_some() || options.gate.is_some() || !options.gates.is_empty();
    c.slow_tasks += usize::from(needs_service);
    let recycled = c.future_pool.borrow_mut().records.pop();
    let future = if let Some(f) = recycled {
        #[cfg(feature = "instrument")]
        {
            c.metrics.future_pool_hits += 1;
        }
        {
            let mut data = f.borrow_mut();
            if let Some(old) = Rc::get_mut(&mut data.cancel.node) {
                *old = node;
            } else {
                #[cfg(feature = "instrument")]
                {
                    c.metrics.cancel_allocations += 1;
                }
                data.cancel.node = Rc::new(node);
            }
            data.cancel.deadline = deadline;
            data.id = id;
            data.observed = false;
            data.cancel_requested = false;
        }
        FutureHandle(f)
    } else {
        #[cfg(feature = "instrument")]
        {
            c.metrics.future_allocations += 1;
            c.metrics.cancel_allocations += 1;
        }
        let f = Rc::new(RefCell::new(FutureData {
            pool: Rc::downgrade(&c.future_pool),
            id,
            owner: Rc::downgrade(core),
            state: FutureState::Pending,
            cancel: TaskCancel {
                node: Rc::new(node),
                deadline,
            },
            waiters: vec![],
            observed: false,
            cancel_requested: false,
            errors: c.errors.clone(),
        }));
        c.futures.push(Rc::downgrade(&f));
        FutureHandle(f)
    };
    let cancel = future.0.borrow().cancel.clone();
    let mut gates = options.gates;
    if let Some(gate) = options.gate {
        gates.push(gate);
    }
    let mut unique: Vec<Gate> = Vec::new();
    for gate in gates {
        if !unique.iter().any(|g| Rc::ptr_eq(&g.0, &gate.0)) {
            unique.push(gate);
        }
    }
    let ticket = if unique.is_empty() {
        None
    } else {
        Some(Tickets {
            tickets: unique
                .into_iter()
                .map(|gate| {
                    gate.0.borrow_mut().queue.push_back(id);
                    Ticket {
                        gate,
                        id,
                        admitted: false,
                    }
                })
                .collect(),
        })
    };
    let stack = c.pool.pop().unwrap_or_else(|| {
        DefaultStack::new(c.config.stack_size).expect("fiber stack allocation failed")
    });
    let base = stack.base().get();
    let stack_limit = stack.limit().get();
    let weak = Rc::downgrade(core);
    let fiber = Coroutine::with_stack(stack, move |yielder, ()| {
        let core = weak.upgrade().expect("loop alive");
        let cancel = core.borrow().tasks[&id].cancel.clone();
        let mut cx = TaskCx {
            core,
            id,
            yielder,
            cancel,
            shield: 0,
            scheduled: slice_clock::now(),
        };
        // Driver catches user panics after corosensei unwinds the fiber. Forced
        // unwind on shutdown runs through this boundary without being caught.
        #[cfg(feature = "instrument")]
        {
            cx.core.borrow_mut().metrics.slice_reads += 1;
        }
        body.run(&mut cx)
    });
    c.tasks.insert(
        id,
        Task {
            hook: None,
            fiber: Some(fiber),
            base,
            parked_sp: None,
            stack_limit,
            parent,
            state: TaskState::Ready,
            future: future.clone(),
            cancel,
            shield: 0,
            io_cancel: None,
            needs_service,
            io_epoch: 0,
            completion: None,
            timer: None,
            ticket,
            roots: options.roots,
            managed_roots: options.managed_roots.map(UnsafeCell::new),
            host: None,
        },
    );
    c.ready.push_back(id);
    c.poll_word.set(c.poll_word.get() | 1);
    future
}

pub struct TaskCx<'a> {
    core: Rc<RefCell<Core>>,
    id: usize,
    yielder: &'a Yielder<(), ()>,
    cancel: TaskCancel,
    shield: usize,
    scheduled: slice_clock::Stamp,
}
impl TaskCx<'_> {
    pub fn id(&self) -> usize {
        self.id
    }
    /// # Safety
    /// A registered context stays alive until unregistered, on this fiber. Its
    /// callbacks must only update thread-local bindings, must not unwind, and
    /// may not reenter the loop.
    pub unsafe fn set_switch_hook(&mut self, hook: Option<SwitchHook>) {
        let mut core = self.core.borrow_mut();
        if let Some(task) = core.tasks.get_mut(&self.id) {
            task.hook = hook;
        } else {
            // Shutdown removes records before force-unwinding their fibers.
            // A binding's destructor must still be able to unregister itself.
            assert!(hook.is_none(), "registering a hook on a retired task");
        }
        core.active_hook = hook;
    }
    pub fn parent_id(&self) -> Option<usize> {
        self.core.borrow().tasks[&self.id].parent
    }
    /// A loop-owned token wakes checkpoints without cross-thread atomics.
    pub fn new_cancel_token(&self, linked: Vec<TaskCancel>) -> TaskCancel {
        let external = linked.iter().any(|token| token.node.external);
        let core = self.core.borrow();
        TaskCancel {
            node: Rc::new(CancelNode {
                cancelled: Cell::new(false),
                parent: None,
                linked: Vec::new(),
                local_linked: linked,
                external,
                epoch: core.cancel_epoch.clone(),
                poll_word: core.poll_word.clone(),
            }),
            deadline: None,
        }
    }
    pub fn cancel(&self) -> TaskCancel {
        self.cancel.clone()
    }
    pub fn spawn(
        &mut self,
        body: impl FnOnce(&mut TaskCx<'_>) -> Outcome + 'static,
    ) -> FutureHandle {
        self.spawn_with(SpawnOptions::default(), body)
    }
    pub fn spawn_with(
        &mut self,
        options: SpawnOptions,
        body: impl FnOnce(&mut TaskCx<'_>) -> Outcome + 'static,
    ) -> FutureHandle {
        spawn(
            &self.core,
            Some(self.id),
            options,
            Body::Boxed(Box::new(body)),
        )
    }
    /// Enter a managed fiber without boxing its captured native entry data.
    ///
    /// # Safety
    /// Code/data must outlive execution. `code` installs runtime bindings and
    /// calls `admit` before executing any user code, including cancelled tasks.
    /// Managed addresses in data must also be in options.managed_roots or roots.
    pub unsafe fn spawn_managed(
        &mut self,
        options: SpawnOptions,
        code: RawBody,
        data: [usize; 4],
    ) -> FutureHandle {
        spawn(
            &self.core,
            Some(self.id),
            options,
            Body::Managed { code, data },
        )
    }
    /// A ready await checks cancellation but does not yield or consume a slice.
    pub fn check_cancel(&mut self) -> Result<(), Err> {
        self.checkpoint()
    }
    fn enable_service(&mut self) {
        let mut c = self.core.borrow_mut();
        c.poll_word.set(c.poll_word.get() | 1);
        let t = c.tasks.get_mut(&self.id).unwrap();
        if !t.needs_service {
            t.needs_service = true;
            c.slow_tasks += 1;
        }
    }
    #[inline]
    fn checkpoint(&mut self) -> Result<(), Err> {
        #[cfg(feature = "instrument")]
        {
            self.core.borrow_mut().metrics.checkpoints += 1;
        }
        if self.shield == 0 && self.cancel.is_cancelled() {
            let mut c = self.core.borrow_mut();
            let t = c.tasks.get_mut(&self.id).unwrap();
            if let Some(token) = t.io_cancel.take() {
                token.cancel();
            }
            t.io_epoch += 1;
            t.completion = None;
            t.timer = None;
            return Err(Err::Cancelled);
        }
        Ok(())
    }
    // Keep the spill frame alive until resume. No Core borrow crosses the switch.
    #[inline(never)]
    fn yield_with_roots(&self) {
        roots::with_registers(|sp, _registers| {
            self.core
                .borrow_mut()
                .tasks
                .get_mut(&self.id)
                .unwrap()
                .parked_sp = Some(sp);
            self.yielder.suspend(());
            self.core
                .borrow_mut()
                .tasks
                .get_mut(&self.id)
                .unwrap()
                .parked_sp = None;
        });
    }
    fn suspend(&mut self, reason: WaitReason) -> Result<(), Err> {
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .state = TaskState::Parked(reason);
        self.yield_with_roots();
        #[cfg(feature = "instrument")]
        {
            self.core.borrow_mut().metrics.slice_reads += 1;
        }
        self.scheduled = slice_clock::now();
        self.checkpoint()
    }
    pub fn admit(&mut self) -> Result<(), Err> {
        loop {
            self.checkpoint()?;
            let admitted = self
                .core
                .borrow_mut()
                .tasks
                .get_mut(&self.id)
                .unwrap()
                .ticket
                .as_mut()
                .is_none_or(Tickets::acquire);
            if admitted {
                return Ok(());
            }
            self.suspend(WaitReason::Admission)?;
        }
    }
    pub fn await_future(&mut self, future: &FutureHandle) -> Outcome {
        if !future.0.borrow().owner.ptr_eq(&Rc::downgrade(&self.core)) {
            return Err(Err::InternalError("future belongs to another loop".into()));
        }
        self.checkpoint()?;
        // At a pending await, the FIFO head may run directly on its own fiber.
        // The awaiting parent's stack is parked and scanned, exactly as at a
        // switch through tick(). Bound nesting so a spawn chain cannot build a
        // recursive scheduler stack. Spawn itself still never runs the child.
        let direct = {
            let c = self.core.borrow();
            !c.inline_child && c.ready.front() == Some(&future.id())
        };
        if direct {
            maintenance(&self.core);
            let child = {
                let mut c = self.core.borrow_mut();
                c.inline_child = true;
                c.ready.pop_front().expect("FIFO head")
            };
            roots::with_registers(|sp, _| {
                self.core
                    .borrow_mut()
                    .tasks
                    .get_mut(&self.id)
                    .unwrap()
                    .parked_sp = Some(sp);
                run_one(&self.core, child);
                let mut c = self.core.borrow_mut();
                let t = c.tasks.get_mut(&self.id).unwrap();
                t.parked_sp = None;
                let base = t.base;
                c.running_base = Some(base);
                c.inline_child = false;
                refresh_poll_word(&c, self.id);
            });
            // Settlement readies the waiter behind tasks already queued. A
            // direct return is legal only when that FIFO has no predecessors.
            let (yield_ready, settled) = {
                let mut c = self.core.borrow_mut();
                let settled = !matches!(future.0.borrow().state, FutureState::Pending);
                let cancelled = self.shield == 0 && self.cancel.is_cancelled();
                if (settled || cancelled) && !c.ready.is_empty() {
                    c.tasks.get_mut(&self.id).unwrap().state = TaskState::Ready;
                    c.ready.push_back(self.id);
                    (true, settled)
                } else {
                    (false, settled)
                }
            };
            #[cfg(feature = "instrument")]
            if yield_ready || settled {
                self.core.borrow_mut().metrics.slice_reads += 1;
            }
            if yield_ready {
                self.yield_with_roots();
                self.scheduled = slice_clock::now();
            } else if settled {
                // A completed handoff is a new parent slice, just like a
                // resume through tick(); child CPU time is not its budget.
                self.scheduled = slice_clock::now();
            }
            self.checkpoint()?;
        }
        {
            let mut f = future.0.borrow_mut();
            if let Some(result) = f.state.result() {
                f.observed = true;
                return result;
            }
            // A task has only one active suspension; await_any unregisters all
            // its inputs before returning, so this id cannot already be here.
            debug_assert!(!f.waiters.contains(&self.id));
            f.waiters.push(self.id);
        }
        let resumed = self.suspend(WaitReason::Futures);
        future.0.borrow_mut().waiters.retain(|id| *id != self.id);
        resumed?;
        future
            .result()
            .expect("await resumed only by settlement or cancellation")
    }
    pub fn await_any(&mut self, futures: &[FutureHandle]) -> Result<(usize, Value), Err> {
        let index = self.await_settled(futures)?;
        futures[index]
            .result()
            .expect("settled future")
            .map(|v| (index, v))
    }
    /// BAML's __await_any reports the first settled input in input order,
    /// including failed/cancelled inputs. Only awaiting the selected future
    /// observes its result; choosing an index must not consume an error.
    pub fn await_settled(&mut self, futures: &[FutureHandle]) -> Result<usize, Err> {
        if futures
            .iter()
            .any(|f| !f.0.borrow().owner.ptr_eq(&Rc::downgrade(&self.core)))
        {
            return Err(Err::InternalError("future belongs to another loop".into()));
        }
        if futures.is_empty() {
            return Err(Err::InternalError("await_any requires inputs".into()));
        }
        self.checkpoint()?;
        loop {
            for (index, f) in futures.iter().enumerate() {
                if !matches!(f.0.borrow().state, FutureState::Pending) {
                    return Ok(index);
                }
            }
            for f in futures {
                let mut f = f.0.borrow_mut();
                if !f.waiters.contains(&self.id) {
                    f.waiters.push(self.id);
                }
            }
            let result = self.suspend(WaitReason::Futures);
            for f in futures {
                f.0.borrow_mut().waiters.retain(|id| *id != self.id);
            }
            result?;
        }
    }
    #[inline(always)]
    pub fn poll(&mut self) -> Result<(), Err> {
        if self.cancel.node.poll_word.get() & 1 == 0 {
            return Ok(());
        }
        self.poll_slow()
    }
    #[cold]
    #[inline(never)]
    fn poll_slow(&mut self) -> Result<(), Err> {
        self.checkpoint()?;
        maintenance(&self.core);
        let yield_now = {
            let c = self.core.borrow();
            !c.ready.is_empty() && c.slice.expired(self.scheduled)
        };
        if yield_now {
            {
                let mut c = self.core.borrow_mut();
                c.tasks.get_mut(&self.id).unwrap().state = TaskState::Ready;
                c.ready.push_back(self.id);
            }
            self.yield_with_roots();
            #[cfg(feature = "instrument")]
            {
                self.core.borrow_mut().metrics.slice_reads += 1;
            }
            self.scheduled = slice_clock::now();
            self.checkpoint()?;
        }
        refresh_poll_word(&self.core.borrow(), self.id);
        Ok(())
    }
    pub fn sleep(&mut self, duration: Duration) -> Result<(), Err> {
        self.checkpoint()?;
        self.enable_service();
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .timer = Some(Instant::now() + duration);
        let result = self.suspend(WaitReason::Timer);
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .timer = None;
        result
    }
    pub fn io<F, Fut>(&mut self, request: F) -> Outcome
    where
        F: FnOnce(CancellationToken) -> Fut + Send + 'static,
        Fut: Future<Output = Result<OwnedValue, Err>> + Send + 'static,
    {
        self.checkpoint()?;
        let (runtime, tx, token, epoch) = {
            let mut c = self.core.borrow_mut();
            let t = c.tasks.get_mut(&self.id).unwrap();
            t.io_epoch += 1;
            let epoch = t.io_epoch;
            let token = t
                .io_cancel
                .get_or_insert_with(CancellationToken::new)
                .clone();
            c.io_pending += 1;
            (c.runtime.clone(), c.tx.clone(), token, epoch)
        };
        let id = self.id;
        let job = runtime.spawn(async move {
            tokio::select! { biased; _ = token.cancelled() => Err(Err::Cancelled), result = async { request(token.clone()).await } => result }
        });
        let completion = IoCompletion {
            tx,
            id,
            epoch,
            sent: false,
        };
        runtime.spawn(async move {
            let result = job
                .await
                .unwrap_or_else(|e| Err(Err::InternalError(format!("I/O task failed: {e}"))));
            completion.finish(result);
        });
        self.suspend(WaitReason::Io)?;
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .completion
            .take()
            .expect("I/O completion before resume")
            .map(Value::Owned)
    }
    /// Generated MIR shields one block at a time, with nested calls inheriting
    /// the depth. The caller pairs this with `leave_shield` on every exit.
    pub fn enter_shield(&mut self) {
        self.shield += 1;
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .shield += 1;
    }
    pub fn leave_shield(&mut self) {
        self.shield -= 1;
        if let Some(task) = self.core.borrow_mut().tasks.get_mut(&self.id) {
            task.shield -= 1;
        }
    }
    pub fn shield<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        struct Shield<'a, 'b>(&'a mut TaskCx<'b>);
        impl Drop for Shield<'_, '_> {
            fn drop(&mut self) {
                self.0.shield -= 1;
                if let Some(t) = self.0.core.borrow_mut().tasks.get_mut(&self.0.id) {
                    t.shield -= 1;
                }
            }
        }
        self.shield += 1;
        self.core
            .borrow_mut()
            .tasks
            .get_mut(&self.id)
            .unwrap()
            .shield += 1;
        let guard = Shield(self);
        f(guard.0)
    }
    /// Rust closures' heap-allocated captures are not stack roots: register
    /// pointer-bearing captures in SpawnOptions::roots before first execution.
    pub fn collect(&mut self) {
        let c = self.core.borrow();
        let heap = c.heap.clone();
        heap.borrow_mut().collect(&*c);
    }
}

fn message(core: &Rc<RefCell<Core>>, msg: Message) {
    match msg {
        Message::Io { id, epoch, result } => {
            let mut c = core.borrow_mut();
            c.io_pending -= 1;
            if let Some(t) = c.tasks.get_mut(&id)
                && t.io_epoch == epoch
                && t.state == TaskState::Parked(WaitReason::Io)
            {
                t.completion = Some(result);
                wake(&mut c, id);
            }
        }
        Message::Submit {
            body,
            args,
            timeout,
            reply,
        } => {
            let opts = SpawnOptions {
                deadline: timeout.map(|d| Instant::now() + d),
                ..Default::default()
            };
            let f = spawn(
                core,
                None,
                opts,
                Body::Boxed(Box::new(move |cx| body(cx, args))),
            );
            core.borrow_mut().tasks.get_mut(&f.id()).unwrap().host = Some(reply);
        }
    }
}

#[inline]
fn maintenance(core: &Rc<RefCell<Core>>) {
    let c = core.borrow();
    if c.io_pending == 0
        && !c.host_enabled
        && c.slow_tasks == 0
        && c.last_cancel_epoch == c.cancel_epoch.get()
    {
        return;
    }
    drop(c);
    drain(core);
    service(core);
}
fn drain(core: &Rc<RefCell<Core>>) {
    let c = core.borrow();
    if c.io_pending == 0 && !c.host_enabled {
        return;
    }
    let rx = c.rx.clone();
    drop(c);
    #[cfg(feature = "instrument")]
    {
        core.borrow_mut().metrics.queue_probes += 1;
    }
    while let Ok(msg) = rx.try_recv() {
        message(core, msg);
    }
}
fn service(core: &Rc<RefCell<Core>>) {
    let mut c = core.borrow_mut();
    let epoch = c.cancel_epoch.get();
    if c.slow_tasks == 0 && c.last_cancel_epoch == epoch {
        return;
    }
    c.last_cancel_epoch = epoch;
    #[cfg(feature = "instrument")]
    {
        c.metrics.service_scans += 1;
        c.metrics.clock_reads += 1;
    }
    let now = Instant::now();
    let wakeups: Vec<_> = c
        .tasks
        .iter_mut()
        .filter_map(|(&id, t)| {
            if !matches!(t.state, TaskState::Parked(_)) {
                return None;
            }
            let cancelled = t.shield == 0 && t.cancel.is_cancelled();
            let timed = t.timer.is_some_and(|d| now >= d);
            let admitted = t.state == TaskState::Parked(WaitReason::Admission)
                && t.ticket.as_mut().is_none_or(Tickets::acquire);
            (cancelled || timed || admitted).then_some(id)
        })
        .collect();
    for id in wakeups {
        wake(&mut c, id);
    }
}

// Bit zero is loop work. Other bits belong to the heap/runtime integration.
fn refresh_poll_word(c: &Core, running: usize) {
    let required = !c.ready.is_empty()
        || c.slow_tasks != 0
        || c.io_pending != 0
        || c.host_enabled
        || c.tasks[&running].cancel.is_cancelled();
    c.poll_word
        .set((c.poll_word.get() & !1) | u8::from(required));
}
