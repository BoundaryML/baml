//! Async calls resolved on their event loop in batches.
//!
//! `pyo3_async_runtimes::tokio::future_into_py` resolves each call on its own:
//! two tokio tasks, then a blocking-pool thread that takes the GIL and wakes
//! the event loop through `call_soon_threadsafe`. A BAML call returns bytes,
//! so nothing about its result needs Python until the event loop's thread
//! picks it up. Here a finished call goes into its event loop's queue without
//! the GIL, and wakes the loop through a socket the loop watches
//! (`add_reader`) only when the queue was empty. The loop then resolves every
//! call in the queue at once.
//!
//! A loop without `add_reader` (the Windows proactor loop is one) keeps using
//! `future_into_py`.
//!
//! The selector of the loop owns the queue's callback, so a loop that is closed
//! or collected takes its queue with it; the registry here holds weak
//! references only.

use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    panic::AssertUnwindSafe,
    sync::{Arc, LazyLock, Mutex, PoisonError, Weak},
};

use futures::FutureExt;
use pyo3::{
    PyTraverseError, PyVisit,
    exceptions::{PyException, PyRuntimeError},
    intern,
    prelude::*,
    sync::PyOnceLock,
};

use crate::{
    callback_dispatch::DispatchScope,
    runtime::{RetainedStream, execute_python_invocation, result_with_stream},
};

/// A finished call: its asyncio future, whether it pulled a stream step, and
/// its result.
type Finished = (Py<PyAny>, bool, Vec<u8>, Option<RetainedStream>);

pub(crate) struct Queue {
    finished: Mutex<Vec<Finished>>,
    /// Both ends live as long as the queue, so a call that finishes after its
    /// loop closed still writes to an open socket.
    reader: UnixStream,
    writer: UnixStream,
}

impl Queue {
    fn push(&self, call: Finished) {
        let was_empty = {
            let mut finished = self.finished.lock().unwrap_or_else(PoisonError::into_inner);
            finished.push(call);
            finished.len() == 1
        };
        // A queue that already held a call has a wake-up on its way. The
        // socket never holds more than a couple of unread bytes, so the write
        // does not block; `write_all` retries it when a signal interrupts it.
        if was_empty {
            let _ = (&self.writer).write_all(&[1]);
        }
    }

    fn take(&self) -> Vec<Finished> {
        // Read the wake-ups before taking the calls: a call pushed after the
        // take writes a new one.
        let mut wakeups = [0u8; 64];
        while matches!((&self.reader).read(&mut wakeups), Ok(n) if n > 0) {}
        std::mem::take(&mut *self.finished.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

/// The `add_reader` callback of an event loop: resolves the calls that
/// finished. The loop's selector owns it, so the queue lives as long as the
/// loop watches it.
#[pyclass(frozen)]
struct LoopQueue {
    queue: Arc<Queue>,
}

#[pymethods]
impl LoopQueue {
    fn __call__(&self, py: Python<'_>) -> PyResult<()> {
        let mut interrupted = None;
        for (future, stream, bytes, retained) in self.queue.take() {
            let future = future.bind(py);
            let resolved = std::panic::catch_unwind(AssertUnwindSafe(|| {
                resolve(future, stream, bytes, retained)
            }))
            .unwrap_or_else(|_| {
                fail(
                    future,
                    PyRuntimeError::new_err("BAML panicked while returning this call's result"),
                )
            });
            let Err(error) = resolved else {
                continue;
            };
            // One call that cannot be resolved must not strand the rest of the
            // batch. A KeyboardInterrupt or SystemExit still leaves the loop,
            // as it would from any callback, once the batch is resolved.
            if error.is_instance_of::<PyException>(py) {
                error.write_unraisable(py, Some(future));
            } else if interrupted.is_none() {
                interrupted = Some(error);
            }
        }
        interrupted.map_or(Ok(()), Err)
    }

    /// A finished call's future refers to its loop, and the loop to this
    /// callback; the garbage collector sees that cycle through here.
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        // Every pass must see the queued futures: one that missed them would
        // count them as garbage. Waiting is safe, since nothing holds this
        // lock while it waits for the GIL or runs Python.
        let finished = self
            .queue
            .finished
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for (future, ..) in finished.iter() {
            visit.call(future)?;
        }
        Ok(())
    }

    fn __clear__(&self) {
        let finished = std::mem::take(
            &mut *self
                .queue
                .finished
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        // Dropping the futures runs Python, which can collect again and
        // traverse this queue; the lock is released first.
        drop(finished);
    }
}

/// Start the call on the shared tokio runtime and return the asyncio future
/// that `event_loop` resolves when it finishes.
pub(crate) fn start(
    event_loop: &Bound<'_, PyAny>,
    queue: Arc<Queue>,
    request: Result<bridge_cffi::InvocationRequest, bridge_cffi::BridgeError>,
    stream: bool,
    environment: Option<DispatchScope>,
) -> PyResult<Py<PyAny>> {
    let py = event_loop.py();
    let future = event_loop.call_method0(intern!(py, "create_future"))?;
    if let Ok(request) = &request {
        // A caller that cancels this future stops the call, as dropping the
        // future did with `future_into_py`.
        let cancel = CancelOnCancelled {
            call_id: request.host_call_id(),
        };
        future.call_method1(intern!(py, "add_done_callback"), (cancel,))?;
    }
    let call = future.clone().unbind();
    match bridge_cffi::shared_tokio_runtime() {
        Ok(runtime) => {
            runtime.spawn(async move {
                let _environment = environment;
                let finished = AssertUnwindSafe(execute_python_invocation(request, stream))
                    .catch_unwind()
                    .await;
                let (bytes, retained) = finished.unwrap_or_else(|_| {
                    let panicked = bridge_cffi::BridgeError::Internal(
                        "BAML execution task panicked".to_owned(),
                    );
                    (bridge_cffi::error_to_outbound(panicked), None)
                });
                queue.push((call, stream, bytes, retained));
            });
        }
        Err(error) => queue.push((call, stream, bridge_cffi::error_to_outbound(error), None)),
    }
    Ok(future.unbind())
}

#[pyclass(frozen)]
struct CancelOnCancelled {
    call_id: u64,
}

#[pymethods]
impl CancelOnCancelled {
    fn __call__(&self, future: &Bound<'_, PyAny>) -> PyResult<()> {
        if future
            .call_method0(intern!(future.py(), "cancelled"))?
            .is_truthy()?
        {
            bridge_cffi::cancel_function_call_by_id(self.call_id);
        }
        Ok(())
    }
}

/// Resolve `future` with a finished call's result. A cancelled caller has
/// stopped waiting, so its result is dropped.
fn resolve(
    future: &Bound<'_, PyAny>,
    stream: bool,
    bytes: Vec<u8>,
    retained: Option<RetainedStream>,
) -> PyResult<()> {
    let py = future.py();
    if is_done(future)? {
        return Ok(());
    }
    let result = result_with_stream(py, bytes, retained, stream);
    // Building the result runs Python, which may have cancelled the future.
    if is_done(future)? {
        return Ok(());
    }
    match result {
        Ok(value) => future.call_method1(intern!(py, "set_result"), (value,))?,
        Err(error) => return fail(future, error),
    };
    Ok(())
}

fn fail(future: &Bound<'_, PyAny>, error: PyErr) -> PyResult<()> {
    let py = future.py();
    if !is_done(future)? {
        future.call_method1(intern!(py, "set_exception"), (error.into_value(py),))?;
    }
    Ok(())
}

fn is_done(future: &Bound<'_, PyAny>) -> PyResult<bool> {
    future
        .call_method0(intern!(future.py(), "done"))?
        .is_truthy()
}

/// A weak reference to an event loop, and its queue (`None` for a loop whose
/// `add_reader` failed). Both are weak: a loop dropped without `close()`
/// must not be kept alive from here.
type Registered = (Py<PyAny>, Option<Weak<Queue>>);

/// The event loops that ran BAML calls, by address. The weak reference tells
/// a loop from a later one at the same address.
///
/// Python code can switch threads, so it never runs while this lock is held:
/// a thread waiting here holds the GIL.
static LOOPS: LazyLock<Mutex<HashMap<usize, Registered>>> = LazyLock::new(Default::default);

/// The running event loop and its queue, or `None` when the loop cannot watch
/// a queue (no `add_reader`, or setting one up failed).
pub(crate) fn running_loop_queue(
    py: Python<'_>,
) -> PyResult<Option<(Bound<'_, PyAny>, Arc<Queue>)>> {
    static GET_RUNNING_LOOP: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let event_loop = GET_RUNNING_LOOP
        .import(py, "asyncio", "get_running_loop")?
        .call0()?;
    let key = event_loop.as_ptr() as usize;
    let known = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .map(|(loop_ref, queue)| (loop_ref.clone_ref(py), queue.clone()));
    let queue = match known {
        Some((loop_ref, queue)) if loop_ref.bind(py).call0()?.is(&event_loop) => match queue {
            None => None,
            // The loop stopped watching its queue (`remove_reader`).
            Some(queue) => match queue.upgrade() {
                Some(queue) => Some(queue),
                None => register(&event_loop),
            },
        },
        _ => register(&event_loop),
    };
    Ok(queue.map(|queue| (event_loop, queue)))
}

/// Give `event_loop` a queue to watch. A failure leaves calls on the
/// `future_into_py` path.
fn register(event_loop: &Bound<'_, PyAny>) -> Option<Arc<Queue>> {
    let py = event_loop.py();
    forget_closed_loops(py);

    static WEAKREF: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let loop_ref = WEAKREF
        .import(py, "weakref", "ref")
        .and_then(|weakref| weakref.call1((event_loop,)))
        .ok()?
        .unbind();
    let (reader, writer) = UnixStream::pair().ok()?;
    reader.set_nonblocking(true).ok()?;
    writer.set_nonblocking(true).ok()?;
    let fd = reader.as_raw_fd();
    let queue = Arc::new(Queue {
        finished: Mutex::new(Vec::new()),
        reader,
        writer,
    });
    let watched = watch(event_loop, fd, &queue).is_ok();
    let queue = watched.then_some(queue);

    let key = event_loop.as_ptr() as usize;
    let replaced = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, (loop_ref, queue.as_ref().map(Arc::downgrade)));
    drop(replaced);
    queue
}

fn watch(event_loop: &Bound<'_, PyAny>, fd: i32, queue: &Arc<Queue>) -> PyResult<()> {
    let py = event_loop.py();
    let callback = Py::new(
        py,
        LoopQueue {
            queue: Arc::clone(queue),
        },
    )?;
    // `add_reader` keeps the context it runs in for every later callback. An
    // empty one keeps the first caller's context variables from living as
    // long as the loop.
    static CONTEXT: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let context = CONTEXT.import(py, "contextvars", "Context")?.call0()?;
    let add_reader = event_loop.getattr(intern!(py, "add_reader"))?;
    context.call_method1(intern!(py, "run"), (add_reader, fd, callback))?;
    Ok(())
}

/// Forget the loops that were closed or collected, and the queues no loop
/// watches any more.
fn forget_closed_loops(py: Python<'_>) {
    let registered: Vec<(usize, Py<PyAny>, Option<Weak<Queue>>)> = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(key, (loop_ref, queue))| (*key, loop_ref.clone_ref(py), queue.clone()))
        .collect();
    let closed: Vec<(usize, Py<PyAny>)> = registered
        .into_iter()
        .filter(|(_, loop_ref, queue)| {
            queue
                .as_ref()
                .is_some_and(|queue| queue.strong_count() == 0)
                || !is_open(loop_ref.bind(py))
        })
        .map(|(key, loop_ref, _)| (key, loop_ref))
        .collect();
    if closed.is_empty() {
        return;
    }
    let forgotten: Vec<Registered> = {
        let mut loops = LOOPS.lock().unwrap_or_else(PoisonError::into_inner);
        closed
            .iter()
            .filter_map(|(key, loop_ref)| {
                // Another thread may have registered a new loop at this address.
                let same = loops.get(key).is_some_and(|(r, _)| r.is(loop_ref));
                same.then(|| loops.remove(key)).flatten()
            })
            .collect()
    };
    // Dropping a weak reference can run Python.
    drop(forgotten);
}

fn is_open(loop_ref: &Bound<'_, PyAny>) -> bool {
    let py = loop_ref.py();
    let Ok(event_loop) = loop_ref.call0() else {
        return false;
    };
    !event_loop.is_none()
        && event_loop
            .call_method0(intern!(py, "is_closed"))
            .and_then(|closed| closed.is_truthy())
            .is_ok_and(|closed| !closed)
}
