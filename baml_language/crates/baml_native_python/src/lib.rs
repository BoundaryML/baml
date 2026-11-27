//! Calling a BAML program compiled to a native library from Python.
//!
//! A pyo3 module over the core of `baml_native_host`
//! (`baml_native_rt/design/bridges.md`). It loads the library and checks its
//! interface hash ([`load`]), joins the asyncio loop of the first call that
//! has to wait, starts calls from Python arguments and copies results into
//! the classes of the generated package (`plan.rs`).
//!
//! - **Who turns.** The library's `wake`, from its I/O thread, sets a flag
//!   and writes one byte to a socket pair, without the GIL. The joined loop
//!   watches the socket (`add_reader`) and gives the library a turn on its
//!   own thread. A BAML timer becomes a loop timer (`call_later`) from what a
//!   turn returns. The flag is cleared before a turn, so a wake that comes
//!   during one asks for another.
//! - **A call** returns a [`Call`], a coroutine object. Its first step
//!   builds the arguments, starts the call and gives the library a turn: a
//!   call that ends in that turn returns without a trip around the loop.
//!   Otherwise it waits on a future of the joined loop.
//! - **One thread.** The library is used by the thread that loaded it;
//!   another thread gets `RuntimeError`, never a block. One loop at a time:
//!   a call awaited on another loop moves the reader there when the old
//!   loop is closed or has no call in flight, and is refused otherwise.
//! - **Sync calls** start the call and turn until it ends, waiting for
//!   `wake` or the next timer with the GIL released.

mod plan;

use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{
        Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

use baml_native_host::{BamlValue, Event, Library, Outcome, Type, tag};
use plan::{ModelMode, Plans};
use pyo3::{
    exceptions::{PyNotImplementedError, PyRuntimeError, PyStopIteration, PyTypeError},
    intern,
    prelude::*,
    sync::PyOnceLock,
    types::{PyDict, PyString, PyTuple},
};

/// The Python objects the module raises and calls, from `baml_native`'s
/// `__init__.py`: today's SDK's error classes and the unhandled hook.
struct Hooks {
    baml_error: Py<PyAny>,
    baml_panic: Py<PyAny>,
    sdk_panic: Py<PyAny>,
    class_instance: Py<PyAny>,
    unset: Py<PyAny>,
    report_unhandled: Py<PyAny>,
}

static HOOKS: PyOnceLock<Hooks> = PyOnceLock::new();
static RUNTIME: PyOnceLock<Runtime> = PyOnceLock::new();
/// Whether a turn releases the GIL while BAML runs.
static RELEASE_GIL: AtomicBool = AtomicBool::new(false);

/// What the library's `wake` reaches, from any thread.
struct Wake {
    owed: AtomicBool,
    reader: UnixStream,
    writer: UnixStream,
    lock: Mutex<()>,
    changed: Condvar,
}

impl Wake {
    fn wake(&self) {
        if !self.owed.swap(true, Ordering::AcqRel) {
            // One byte per wake that found the flag clear; the reader
            // drains them. A full socket only means one is unread already.
            let _ = (&self.writer).write(&[1]);
            let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
            self.changed.notify_all();
        }
    }

    fn drain(&self) {
        let mut bytes = [0_u8; 64];
        while matches!((&self.reader).read(&mut bytes), Ok(n) if n > 0) {}
    }
}

/// A call the library has not reported ended.
enum Waiter {
    /// Its first turn, or a sync call: whoever turns takes the result.
    Turning,
    /// An async call waiting on this future of the joined loop.
    Future(Py<PyAny>),
    /// Nobody waits any more: its result is freed unread.
    Abandoned,
}

struct Entry {
    export: u32,
    waiter: Waiter,
}

/// The loop calls wait on.
struct Joined {
    event_loop: Py<PyAny>,
    /// The loop timer armed for the library's next timer, and when.
    timer: Option<(Instant, Py<PyAny>)>,
}

#[derive(Default)]
struct Inner {
    calls: HashMap<u64, Entry>,
    /// How many of `calls` wait on a future of the joined loop.
    on_loop: usize,
    joined: Option<Joined>,
}

struct Runtime {
    library: &'static Library,
    plans: Plans,
    owner: ThreadId,
    wake: &'static Wake,
    /// Held by whoever turns or starts; on the owner thread a held lock
    /// means re-entry.
    inner: Mutex<Inner>,
}

fn hooks(py: Python<'_>) -> PyResult<&'static Hooks> {
    HOOKS
        .get(py)
        .ok_or_else(|| PyRuntimeError::new_err("baml_native is not configured"))
}

fn runtime(py: Python<'_>) -> PyResult<&'static Runtime> {
    let runtime = RUNTIME.get(py).ok_or_else(|| {
        PyRuntimeError::new_err("no BAML library is loaded: call baml_native.load first")
    })?;
    if std::thread::current().id() != runtime.owner {
        return Err(PyRuntimeError::new_err(
            "the BAML library is used by the thread that loaded it; calls from another thread \
             are refused",
        ));
    }
    Ok(runtime)
}

/// What a turn hands back.
struct Turned {
    /// The result of the call the turner waits for, if it ended.
    mine: Option<PyResult<Py<PyAny>>>,
    /// Until the library's next timer.
    next: Option<Duration>,
}

impl Runtime {
    fn inner(&self) -> PyResult<MutexGuard<'_, Inner>> {
        self.inner.try_lock().map_err(|_| {
            PyRuntimeError::new_err(
                "a BAML call was made while the library was busy on this thread (from code run \
                 while a result was being built): it would never end",
            )
        })
    }

    /// Give the library a turn and deliver what ended: `mine` to the
    /// caller, the rest to their futures, unhandled failures to the hook.
    fn turn(&'static self, py: Python<'_>, mine: Option<u64>) -> PyResult<Turned> {
        self.wake.owed.store(false, Ordering::Release);
        let mut inner = self.inner()?;
        let mut taken = None;
        let mut resolved: Vec<(Py<PyAny>, PyResult<Py<PyAny>>)> = Vec::new();
        let mut unhandled: Vec<PyResult<Py<PyAny>>> = Vec::new();
        let Inner {
            calls,
            on_loop,
            joined,
        } = &mut *inner;
        let plans = &self.plans;
        let mut on_event = |py: Python<'_>, event: Event<'_>| match event {
            Event::Ended {
                call,
                outcome,
                value,
            } => {
                let Some(entry) = calls.remove(&call) else {
                    return;
                };
                if matches!(entry.waiter, Waiter::Future(_)) {
                    *on_loop -= 1;
                }
                match entry.waiter {
                    Waiter::Turning => {
                        if Some(call) == mine {
                            taken = Some(ended(py, plans, entry.export, outcome, &value));
                        }
                    }
                    Waiter::Future(future) => {
                        // A cancelled caller stopped waiting: its result is
                        // freed unread.
                        if !is_done(py, &future) {
                            let result = ended(py, plans, entry.export, outcome, &value);
                            resolved.push((future, result));
                        }
                    }
                    Waiter::Abandoned => {}
                }
            }
            Event::Unhandled { error, .. } => {
                unhandled.push(thrown_error(py, plans, None, &error));
            }
        };
        let library = self.library;
        let next = if RELEASE_GIL.load(Ordering::Relaxed) {
            py.detach(|| {
                let session = library.session();
                session.turn(|event| Python::attach(|py| on_event(py, event)))
            })
        } else {
            let session = library.session();
            session.turn(|event| on_event(py, event))
        };
        // A loop timer only matters to calls that wait on the loop.
        if *on_loop > 0
            && let Some(joined) = joined
        {
            arm(py, joined, next);
        }
        drop(inner);
        for (future, result) in resolved {
            let future = future.bind(py);
            match result {
                Ok(value) => future.call_method1(intern!(py, "set_result"), (value,))?,
                Err(error) => {
                    future.call_method1(intern!(py, "set_exception"), (error.into_value(py),))?
                }
            };
        }
        for error in unhandled {
            let error = match error {
                Ok(error) => error,
                Err(error) => error.into_value(py).into_any(),
            };
            if let Err(error) = hooks(py)?.report_unhandled.bind(py).call1((error,)) {
                error.write_unraisable(py, None);
            }
        }
        Ok(Turned { mine: taken, next })
    }

    /// Start export `export` with `args` (`None` for a defaulted parameter
    /// left out). Its id.
    fn start(
        &'static self,
        py: Python<'_>,
        export: u32,
        args: &[Option<Py<PyAny>>],
    ) -> PyResult<u64> {
        let mut inner = self.inner()?;
        let function = &self.plans.interface().functions[export as usize];
        let session = self.library.session();
        let mut values: Vec<BamlValue<'_>> = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(&function.params) {
            values.push(match arg {
                Some(object) => self
                    .plans
                    .to_baml(py, &session, &param.ty, object.bind(py))?,
                None => session.omitted(),
            });
        }
        let id = session
            .start(export, &values)
            .map_err(|error| PyTypeError::new_err(error.to_string()))?;
        drop(values);
        drop(session);
        inner.calls.insert(
            id,
            Entry {
                export,
                waiter: Waiter::Turning,
            },
        );
        self.wake.owed.store(true, Ordering::Release);
        Ok(id)
    }

    /// Make the running loop the joined one, and a future on it for call
    /// `id`.
    fn wait_on_loop(
        &'static self,
        py: Python<'_>,
        id: u64,
        next: Option<Duration>,
    ) -> PyResult<Py<PyAny>> {
        static GET_RUNNING_LOOP: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
        let event_loop = GET_RUNNING_LOOP
            .import(py, "asyncio", "get_running_loop")?
            .call0()?;
        let mut inner = self.inner()?;
        let same = inner
            .joined
            .as_ref()
            .is_some_and(|joined| joined.event_loop.bind(py).is(&event_loop));
        if !same {
            if let Some(old) = inner.joined.take() {
                let old_loop = old.event_loop.bind(py);
                let closed = old_loop
                    .call_method0(intern!(py, "is_closed"))?
                    .is_truthy()?;
                let in_flight = inner.calls.values().any(|entry| match &entry.waiter {
                    Waiter::Future(future) => !is_done(py, future),
                    Waiter::Turning | Waiter::Abandoned => false,
                });
                if !closed && in_flight {
                    inner.joined = Some(old);
                    drop(inner);
                    self.abandon(py, id);
                    return Err(PyRuntimeError::new_err(
                        "the BAML library is joined to another event loop that has calls in \
                         flight; one loop at a time",
                    ));
                }
                if !closed {
                    let _ = old_loop.call_method1(
                        intern!(py, "remove_reader"),
                        (self.wake.reader.as_raw_fd(),),
                    );
                }
                if let Some((_, handle)) = old.timer {
                    let _ = handle.bind(py).call_method0(intern!(py, "cancel"));
                }
                // Calls waiting on the old loop's futures cannot be delivered.
                for entry in inner.calls.values_mut() {
                    if matches!(entry.waiter, Waiter::Future(_)) {
                        entry.waiter = Waiter::Abandoned;
                    }
                }
                inner.on_loop = 0;
            }
            watch(py, &event_loop, self.wake.reader.as_raw_fd())?;
            inner.joined = Some(Joined {
                event_loop: event_loop.clone().unbind(),
                timer: None,
            });
        }
        let future = event_loop
            .call_method0(intern!(py, "create_future"))?
            .unbind();
        if let Some(entry) = inner.calls.get_mut(&id) {
            entry.waiter = Waiter::Future(future.clone_ref(py));
            inner.on_loop += 1;
        }
        if let Some(joined) = &mut inner.joined {
            arm(py, joined, next);
        }
        drop(inner);
        // A wake that came before the reader was watched left its byte in
        // the socket, which the loop sees; a turn owed by a start is given
        // now.
        if self.wake.owed.load(Ordering::Acquire) {
            event_loop.call_method1(intern!(py, "call_soon"), (Py::new(py, Turn)?,))?;
        }
        Ok(future)
    }

    /// Forget call `id`: its result is freed when it arrives.
    fn abandon(&self, py: Python<'_>, id: u64) {
        if let Ok(mut inner) = self.inner.try_lock()
            && let Some(entry) = inner.calls.get_mut(&id)
        {
            let was_on_loop = if let Waiter::Future(future) = &entry.waiter {
                let _ = future.bind(py).call_method0(intern!(py, "cancel"));
                true
            } else {
                false
            };
            entry.waiter = Waiter::Abandoned;
            if was_on_loop {
                inner.on_loop -= 1;
            }
        }
    }

    /// A sync call: start it and turn until it ends.
    fn call_sync(
        &'static self,
        py: Python<'_>,
        export: u32,
        args: &[Option<Py<PyAny>>],
    ) -> PyResult<Py<PyAny>> {
        let id = self.start(py, export, args)?;
        loop {
            let turned = self.turn(py, Some(id))?;
            if let Some(result) = turned.mine {
                return result;
            }
            if !self.inner()?.calls.contains_key(&id) {
                return Err(PyRuntimeError::new_err("the call ended unseen"));
            }
            let deadline = turned.next.map(|next| Instant::now() + next);
            let wake = self.wake;
            py.detach(|| {
                let mut guard = wake.lock.lock().unwrap_or_else(PoisonError::into_inner);
                while !wake.owed.load(Ordering::Acquire) {
                    match deadline {
                        Some(deadline) => {
                            let now = Instant::now();
                            if now >= deadline {
                                break;
                            }
                            guard = wake
                                .changed
                                .wait_timeout(guard, deadline - now)
                                .unwrap_or_else(PoisonError::into_inner)
                                .0;
                        }
                        None => {
                            guard = wake
                                .changed
                                .wait(guard)
                                .unwrap_or_else(PoisonError::into_inner);
                        }
                    }
                }
            });
            self.wake.drain();
            py.check_signals()?;
        }
    }
}

fn is_done(py: Python<'_>, future: &Py<PyAny>) -> bool {
    future
        .bind(py)
        .call_method0(intern!(py, "done"))
        .and_then(|done| done.is_truthy())
        .unwrap_or(true)
}

/// Arm the joined loop's timer for the library's next timer, unless an
/// earlier one is armed. A loop that was closed takes no timer.
fn arm(py: Python<'_>, joined: &mut Joined, next: Option<Duration>) {
    let Some(next) = next else { return };
    let when = Instant::now() + next;
    if let Some((armed, handle)) = &joined.timer {
        if *armed <= when {
            return;
        }
        let _ = handle.bind(py).call_method0(intern!(py, "cancel"));
    }
    let handle = Py::new(py, TimerFired).and_then(|fired| {
        joined
            .event_loop
            .bind(py)
            .call_method1(intern!(py, "call_later"), (next.as_secs_f64(), fired))
    });
    joined.timer = handle.ok().map(|handle| (when, handle.unbind()));
}

/// Watch the wake socket from `event_loop`, in an empty context so the
/// first caller's context variables do not live as long as the loop.
fn watch(py: Python<'_>, event_loop: &Bound<'_, PyAny>, fd: i32) -> PyResult<()> {
    let context = py.import("contextvars")?.getattr("Context")?.call0()?;
    let add_reader = event_loop.getattr(intern!(py, "add_reader"))?;
    context.call_method1(intern!(py, "run"), (add_reader, fd, Py::new(py, Turn)?))?;
    Ok(())
}

/// The loop's callback for the wake socket: a turn.
#[pyclass(frozen)]
struct Turn;

#[pymethods]
impl Turn {
    #[expect(clippy::unused_self, reason = "a callable object")]
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = runtime(py)?;
        runtime.wake.drain();
        runtime.turn(py, None).map(drop)
    }
}

/// The loop timer for the library's next timer.
#[pyclass(frozen)]
struct TimerFired;

#[pymethods]
impl TimerFired {
    #[expect(clippy::unused_self, reason = "a callable object")]
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        let runtime = runtime(py)?;
        if let Some(joined) = &mut runtime.inner()?.joined {
            joined.timer = None;
        }
        runtime.turn(py, None).map(drop)
    }
}

// ---- how a call ends ----

/// The class name of a thrown or panicking value: `user.<name>` for an
/// exported class, else the head of its description.
fn class_name(plans: &Plans, value: &BamlValue<'_>, described: &str) -> Option<String> {
    if value.kind() != tag::INSTANCE {
        return None;
    }
    if let Some(number) = value.class_of() {
        return Some(plans.fqn(number));
    }
    described
        .split([' ', '{'])
        .next()
        .filter(|head| !head.is_empty())
        .map(str::to_owned)
}

/// The Python value of a thrown or panicking BAML value: typed when it is
/// of `ty`, a `str` when it is a string, else its description.
fn error_value(
    py: Python<'_>,
    plans: &Plans,
    ty: Option<&Type>,
    value: &BamlValue<'_>,
    described: &str,
) -> Py<PyAny> {
    if let Some(ty) = ty
        && let Ok(typed) = plans.to_py(py, ty, value)
    {
        return typed.unbind();
    }
    if value.is_string()
        && let Ok(text) = value.string()
    {
        return PyString::new(py, &text).into_any().unbind();
    }
    PyString::new(py, described).into_any().unbind()
}

/// The `BamlError` a throw becomes.
fn thrown_error(
    py: Python<'_>,
    plans: &Plans,
    ty: Option<&Type>,
    value: &BamlValue<'_>,
) -> PyResult<Py<PyAny>> {
    let hooks = hooks(py)?;
    let described = value.describe();
    let class = class_name(plans, value, &described);
    let python = error_value(py, plans, ty, value, &described);
    let kwargs = PyDict::new(py);
    kwargs.set_item(intern!(py, "class_name"), class)?;
    Ok(hooks
        .baml_error
        .bind(py)
        .call((python,), Some(&kwargs))?
        .unbind())
}

/// What a call ended with, as a result or an exception.
fn ended(
    py: Python<'_>,
    plans: &Plans,
    export: u32,
    outcome: Outcome,
    value: &BamlValue<'_>,
) -> PyResult<Py<PyAny>> {
    let function = &plans.interface().functions[export as usize];
    let hooks = hooks(py)?;
    match outcome {
        Outcome::Returned => match plans.to_py(py, &function.result, value) {
            Ok(result) => Ok(result.unbind()),
            Err(error) => {
                let message = format!("{}: {}", function.name, error.value(py));
                Err(PyErr::from_value(
                    hooks.sdk_panic.bind(py).call1((message,))?,
                ))
            }
        },
        Outcome::Threw => Err(PyErr::from_value(
            thrown_error(py, plans, function.throws.as_ref(), value)?.into_bound(py),
        )),
        Outcome::Panicked => {
            let described = value.describe();
            let class = class_name(plans, value, &described);
            let python = error_value(py, plans, None, value, &described);
            let kwargs = PyDict::new(py);
            kwargs.set_item(intern!(py, "class_name"), class)?;
            Err(PyErr::from_value(
                hooks.baml_panic.bind(py).call((python,), Some(&kwargs))?,
            ))
        }
        Outcome::Stuck => {
            let fields = PyDict::new(py);
            fields.set_item(
                "message",
                format!(
                    "{} can never finish: nothing can run, no timer is set and no I/O can \
                     complete",
                    function.name
                ),
            )?;
            let value = hooks
                .class_instance
                .bind(py)
                .call1(("baml.panics.Stuck", fields))?;
            let kwargs = PyDict::new(py);
            kwargs.set_item(intern!(py, "class_name"), "baml.panics.Stuck")?;
            Err(PyErr::from_value(
                hooks.baml_panic.bind(py).call((value,), Some(&kwargs))?,
            ))
        }
    }
}

// ---- functions and calls ----

/// One exported function, as `define_function` makes it.
#[pyclass(dict, frozen, module = "baml_native._native")]
struct Function {
    export: u32,
    sync: bool,
    /// Per required name, in order, its parameter's place.
    positional: Vec<usize>,
    /// Every name a caller may pass (wire names and their host aliases).
    by_name: HashMap<String, usize>,
}

impl Function {
    fn arguments(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Vec<Option<Py<PyAny>>>> {
        let runtime = runtime(py)?;
        let function = &runtime.plans.interface().functions[self.export as usize];
        let mut values: Vec<Option<Py<PyAny>>> = function.params.iter().map(|_| None).collect();
        if args.len() > self.positional.len() {
            return Err(PyTypeError::new_err(format!(
                "{} takes {} positional arguments but {} were given",
                function.name,
                self.positional.len(),
                args.len()
            )));
        }
        for (arg, place) in args.iter().zip(&self.positional) {
            values[*place] = Some(arg.unbind());
        }
        if let Some(kwargs) = kwargs {
            let unset = hooks(py)?.unset.bind(py);
            for (key, value) in kwargs.iter() {
                let name = key.cast::<PyString>()?.to_str()?;
                if name == "_baml" || name == "_types" {
                    if value.is_none() {
                        continue;
                    }
                    return Err(PyNotImplementedError::new_err(format!(
                        "`{name}=` is not supported by the native bridge"
                    )));
                }
                if value.is(unset) {
                    continue;
                }
                let place = *self.by_name.get(name).ok_or_else(|| {
                    PyTypeError::new_err(format!(
                        "{} got an unexpected keyword argument {name:?}",
                        function.name
                    ))
                })?;
                if values[place].is_some() {
                    return Err(PyTypeError::new_err(format!(
                        "multiple values for argument {name:?}"
                    )));
                }
                values[place] = Some(value.unbind());
            }
        }
        for (value, param) in values.iter().zip(&function.params) {
            if value.is_none() && !param.has_default {
                return Err(PyTypeError::new_err(format!(
                    "{} missing required argument {:?}",
                    function.name, param.name
                )));
            }
        }
        Ok(values)
    }
}

#[pymethods]
impl Function {
    #[pyo3(signature = (*args, **kwargs))]
    fn __call__(
        &self,
        py: Python<'_>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let values = self.arguments(py, args, kwargs)?;
        if self.sync {
            runtime(py)?.call_sync(py, self.export, &values)
        } else {
            Ok(Py::new(
                py,
                Call {
                    export: self.export,
                    state: State::Ready(values),
                },
            )?
            .into_any())
        }
    }
}

enum State {
    /// Not awaited yet: its arguments.
    Ready(Vec<Option<Py<PyAny>>>),
    /// Waiting on a future of the joined loop: the future and its
    /// `__await__` iterator.
    Waiting {
        id: u64,
        future: Py<PyAny>,
        iter: Py<PyAny>,
    },
    Done,
}

/// An async call: a coroutine object, so `await`, `asyncio.run`,
/// `create_task`, `gather` and `wait_for` take it as they take today's.
#[pyclass(module = "baml_native._native")]
struct Call {
    export: u32,
    state: State,
}

impl Call {
    /// The first step: start, turn, and either return or wait.
    fn begin(&mut self, py: Python<'_>, args: &[Option<Py<PyAny>>]) -> PyResult<Py<PyAny>> {
        let runtime = runtime(py)?;
        let id = runtime.start(py, self.export, args)?;
        let turned = runtime.turn(py, Some(id))?;
        if let Some(result) = turned.mine {
            self.state = State::Done;
            return Err(PyStopIteration::new_err((result?,)));
        }
        let future = runtime.wait_on_loop(py, id, turned.next)?;
        let iter = future.bind(py).call_method0(intern!(py, "__await__"))?;
        self.state = State::Waiting {
            id,
            future,
            iter: iter.clone().unbind(),
        };
        self.forward(py, |iter| {
            iter.call_method1(intern!(py, "send"), (py.None(),))
        })
    }

    /// Step the future's iterator; once it stops, the call is done.
    fn forward<'py>(
        &mut self,
        py: Python<'py>,
        step: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let State::Waiting { iter, .. } = &self.state else {
            return Err(PyRuntimeError::new_err(
                "cannot reuse already awaited coroutine",
            ));
        };
        let iter = iter.bind(py).clone();
        match step(&iter) {
            Ok(yielded) => Ok(yielded.unbind()),
            Err(error) => {
                self.state = State::Done;
                Err(error)
            }
        }
    }

    fn send_value(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<Py<PyAny>> {
        match std::mem::replace(&mut self.state, State::Done) {
            State::Ready(args) => self.begin(py, &args),
            waiting @ State::Waiting { .. } => {
                self.state = waiting;
                self.forward(py, |iter| iter.call_method1(intern!(py, "send"), (value,)))
            }
            State::Done => Err(PyRuntimeError::new_err(
                "cannot reuse already awaited coroutine",
            )),
        }
    }
}

#[pymethods]
impl Call {
    fn __await__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __iter__(slf: Py<Self>) -> Py<Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.send_value(py, py.None())
    }

    fn send(&mut self, py: Python<'_>, value: Py<PyAny>) -> PyResult<Py<PyAny>> {
        self.send_value(py, value)
    }

    #[pyo3(signature = (kind, value = None, traceback = None))]
    fn throw(
        &mut self,
        py: Python<'_>,
        kind: Py<PyAny>,
        value: Option<Py<PyAny>>,
        traceback: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match &self.state {
            State::Waiting { .. } => self.forward(py, |iter| {
                iter.call_method1(intern!(py, "throw"), (kind, value, traceback))
            }),
            _ => {
                self.state = State::Done;
                let kind = kind.into_bound(py);
                let error = if kind.is_instance_of::<pyo3::exceptions::PyBaseException>() {
                    kind
                } else {
                    match value {
                        Some(value) if !value.is_none(py) => kind.call1((value,))?,
                        _ => kind.call0()?,
                    }
                };
                Err(PyErr::from_value(error))
            }
        }
    }

    fn close(&mut self, py: Python<'_>) {
        if let State::Waiting { id, future, .. } = std::mem::replace(&mut self.state, State::Done) {
            let _ = future.bind(py).call_method0(intern!(py, "cancel"));
            if let Ok(runtime) = runtime(py) {
                runtime.abandon(py, id);
            }
        }
    }
}

// ---- the module ----

/// Today's SDK's error classes and the hook for unhandled failures.
#[pyfunction]
fn configure(
    py: Python<'_>,
    baml_error: Py<PyAny>,
    baml_panic: Py<PyAny>,
    sdk_panic: Py<PyAny>,
    class_instance: Py<PyAny>,
    unset: Py<PyAny>,
    report_unhandled: Py<PyAny>,
) {
    let _ = HOOKS.set(
        py,
        Hooks {
            baml_error,
            baml_panic,
            sdk_panic,
            class_instance,
            unset,
            report_unhandled,
        },
    );
}

/// Load the library at `path`, refusing one whose interface hash is not
/// `interface_hash`. Once per process; the thread that loads it is the one
/// that may call it.
#[pyfunction]
fn load(py: Python<'_>, path: &str, interface_hash: u64) -> PyResult<()> {
    if RUNTIME.get(py).is_some() {
        return Ok(());
    }
    let (reader, writer) =
        UnixStream::pair().map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    for end in [&reader, &writer] {
        end.set_nonblocking(true)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    }
    let wake: &'static Wake = Box::leak(Box::new(Wake {
        owed: AtomicBool::new(false),
        reader,
        writer,
        lock: Mutex::new(()),
        changed: Condvar::new(),
    }));
    let library = py
        .detach(|| Library::load(path, interface_hash, move || wake.wake()))
        .map_err(|error| -> PyErr {
            match hooks(py).and_then(|hooks| hooks.sdk_panic.bind(py).call1((error.to_string(),))) {
                Ok(panic) => PyErr::from_value(panic),
                Err(error) => error,
            }
        })?;
    let _ = RUNTIME.set(
        py,
        Runtime {
            library,
            plans: Plans::new(library.interface()),
            owner: std::thread::current().id(),
            wake,
            inner: Mutex::new(Inner::default()),
        },
    );
    Ok(())
}

/// The generated type map: how a class or an enum finds its Python class.
#[pyfunction]
fn set_type_map(py: Python<'_>, type_map: Py<PyAny>) -> PyResult<()> {
    runtime(py)?.plans.set_type_map(py, type_map);
    Ok(())
}

/// The export `name` as a callable, or `None` when the library does not
/// export it. `required` are the generated positional names, `aliases` the
/// host names of renamed parameters.
#[pyfunction]
fn function(
    py: Python<'_>,
    name: &str,
    sync: bool,
    required: Vec<String>,
    aliases: HashMap<String, String>,
) -> PyResult<Option<Function>> {
    let runtime = runtime(py)?;
    let Some(export) = runtime.library.find(name) else {
        return Ok(None);
    };
    let function = &runtime.plans.interface().functions[export as usize];
    let mut by_name: HashMap<String, usize> = function
        .params
        .iter()
        .enumerate()
        .map(|(place, param)| (param.name.clone(), place))
        .collect();
    for (host, wire) in aliases {
        if let Some(place) = by_name.get(&wire).copied() {
            by_name.insert(host, place);
        }
    }
    let positional = required
        .into_iter()
        .map(|name| {
            by_name.get(&name).copied().ok_or_else(|| {
                PyRuntimeError::new_err(format!("{} has no parameter {name:?}", function.name))
            })
        })
        .collect::<PyResult<_>>()?;
    Ok(Some(Function {
        export,
        sync,
        positional,
        by_name,
    }))
}

/// How results become models: "validate", "construct" or "native".
#[pyfunction]
fn set_model_mode(mode: &str) -> PyResult<()> {
    plan::set_model_mode(match mode {
        "validate" => ModelMode::Validate,
        "construct" => ModelMode::Construct,
        "native" => ModelMode::Native,
        other => return Err(PyTypeError::new_err(format!("no model mode {other:?}"))),
    });
    Ok(())
}

/// Whether a turn releases the GIL while BAML runs.
#[pyfunction]
fn set_release_gil(release: bool) {
    RELEASE_GIL.store(release, Ordering::Relaxed);
}

/// How many handles are live. For tests.
#[pyfunction]
fn handle_count(py: Python<'_>) -> PyResult<u64> {
    Ok(runtime(py)?.library.handle_count())
}

/// Collect now. For tests.
#[pyfunction]
fn collect(py: Python<'_>) -> PyResult<()> {
    runtime(py)?.library.collect();
    Ok(())
}

/// How many calls have not been reported ended. For tests.
#[pyfunction]
fn calls_in_flight(py: Python<'_>) -> PyResult<usize> {
    Ok(runtime(py)?.inner()?.calls.len())
}

/// Start export `name` with `nulls` null arguments, past the module's own
/// checks, so the library itself refuses: its reason, as the `TypeError` a
/// refusal raises. For tests.
#[pyfunction]
fn start_unchecked(py: Python<'_>, name: &str, nulls: usize) -> PyResult<u64> {
    let runtime = runtime(py)?;
    let export = runtime
        .library
        .find(name)
        .ok_or_else(|| PyRuntimeError::new_err(format!("no export {name}")))?;
    let _inner = runtime.inner()?;
    let session = runtime.library.session();
    let args: Vec<BamlValue<'_>> = (0..nulls).map(|_| session.null()).collect();
    session
        .start(export, &args)
        .map_err(|error| PyTypeError::new_err(error.to_string()))
}

/// The library's interface as JSON.
#[pyfunction]
fn interface_json(py: Python<'_>) -> PyResult<String> {
    Ok(runtime(py)?.library.interface_json().to_owned())
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(configure, module)?)?;
    module.add_function(wrap_pyfunction!(load, module)?)?;
    module.add_function(wrap_pyfunction!(set_type_map, module)?)?;
    module.add_function(wrap_pyfunction!(function, module)?)?;
    module.add_function(wrap_pyfunction!(set_model_mode, module)?)?;
    module.add_function(wrap_pyfunction!(set_release_gil, module)?)?;
    module.add_function(wrap_pyfunction!(handle_count, module)?)?;
    module.add_function(wrap_pyfunction!(collect, module)?)?;
    module.add_function(wrap_pyfunction!(calls_in_flight, module)?)?;
    module.add_function(wrap_pyfunction!(interface_json, module)?)?;
    module.add_function(wrap_pyfunction!(start_unchecked, module)?)?;
    module.add_class::<Function>()?;
    module.add_class::<Call>()?;
    Ok(())
}
