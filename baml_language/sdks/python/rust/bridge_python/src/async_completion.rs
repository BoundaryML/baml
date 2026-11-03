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

use std::{
    collections::HashMap,
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    sync::{Arc, LazyLock, Mutex, PoisonError},
};

use futures::FutureExt;
use pyo3::{exceptions::PyNotImplementedError, intern, prelude::*, sync::PyOnceLock};

use crate::{
    callback_dispatch::DispatchScope,
    runtime::{RetainedStream, execute_python_invocation, result_with_stream},
};

/// A finished call: its asyncio future, whether it pulled a stream step, and
/// its result.
type Finished = (Py<PyAny>, bool, Vec<u8>, Option<RetainedStream>);

struct Queue {
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
        // A queue that already held a call has a wake-up on its way.
        if was_empty {
            let _ = (&self.writer).write(&[1]);
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

/// An event loop's queue of finished calls. The loop calls it when the queue's
/// socket is readable.
#[pyclass(frozen)]
pub(crate) struct LoopQueue {
    queue: Arc<Queue>,
}

#[pymethods]
impl LoopQueue {
    fn __call__(&self, py: Python<'_>) {
        for (future, stream, bytes, retained) in self.queue.take() {
            resolve(future.bind(py), stream, bytes, retained);
        }
    }
}

impl LoopQueue {
    /// Start the call on the shared tokio runtime and return the asyncio
    /// future that `event_loop` resolves when it finishes.
    pub(crate) fn start(
        &self,
        event_loop: &Bound<'_, PyAny>,
        request: Result<bridge_cffi::InvocationRequest, bridge_cffi::BridgeError>,
        stream: bool,
        environment: Option<DispatchScope>,
    ) -> PyResult<Py<PyAny>> {
        let py = event_loop.py();
        let future = event_loop.call_method0(intern!(py, "create_future"))?;
        let call = future.clone().unbind();
        let queue = Arc::clone(&self.queue);
        match bridge_cffi::shared_tokio_runtime() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    let _environment = environment;
                    let finished =
                        std::panic::AssertUnwindSafe(execute_python_invocation(request, stream))
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
}

fn resolve(
    future: &Bound<'_, PyAny>,
    stream: bool,
    bytes: Vec<u8>,
    retained: Option<RetainedStream>,
) {
    let py = future.py();
    let resolved = (|| -> PyResult<()> {
        // A cancelled caller has stopped waiting; its result is dropped.
        if future.call_method0(intern!(py, "done"))?.is_truthy()? {
            return Ok(());
        }
        match result_with_stream(py, bytes, retained, stream) {
            Ok(value) => future.call_method1(intern!(py, "set_result"), (value,))?,
            Err(error) => {
                future.call_method1(intern!(py, "set_exception"), (error.into_value(py),))?
            }
        };
        Ok(())
    })();
    // One call that cannot be resolved must not strand the rest of the batch.
    if let Err(error) = resolved {
        error.write_unraisable(py, Some(future));
    }
}

/// A weak reference to an event loop, and its queue (`None` for a loop
/// without `add_reader`).
type Registered = (Py<PyAny>, Option<Py<LoopQueue>>);

/// The event loops that ran BAML calls, by address. The weak reference tells
/// a loop from a later one at the same address.
///
/// Python code can switch threads, so it never runs while this lock is held:
/// a thread waiting here holds the GIL.
static LOOPS: LazyLock<Mutex<HashMap<usize, Registered>>> = LazyLock::new(Default::default);

/// The running event loop and its queue, or `None` when the loop has no
/// `add_reader`.
pub(crate) fn running_loop_queue(
    py: Python<'_>,
) -> PyResult<Option<(Bound<'_, PyAny>, Py<LoopQueue>)>> {
    static GET_RUNNING_LOOP: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let event_loop = GET_RUNNING_LOOP
        .import(py, "asyncio", "get_running_loop")?
        .call0()?;
    let key = event_loop.as_ptr() as usize;
    let known = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .map(|(loop_ref, queue)| {
            (
                loop_ref.clone_ref(py),
                queue.as_ref().map(|q| q.clone_ref(py)),
            )
        });
    let queue = match known {
        Some((loop_ref, queue)) if loop_ref.bind(py).call0()?.is(&event_loop) => queue,
        _ => register(&event_loop)?,
    };
    Ok(queue.map(|queue| (event_loop, queue)))
}

fn register(event_loop: &Bound<'_, PyAny>) -> PyResult<Option<Py<LoopQueue>>> {
    let py = event_loop.py();
    forget_closed_loops(py);

    let (reader, writer) = UnixStream::pair()?;
    reader.set_nonblocking(true)?;
    writer.set_nonblocking(true)?;
    let fd = reader.as_raw_fd();
    let queue = Py::new(
        py,
        LoopQueue {
            queue: Arc::new(Queue {
                finished: Mutex::new(Vec::new()),
                reader,
                writer,
            }),
        },
    )?;
    let queue = match event_loop.call_method1(intern!(py, "add_reader"), (fd, queue.clone_ref(py)))
    {
        Ok(_) => Some(queue),
        Err(error) if error.is_instance_of::<PyNotImplementedError>(py) => None,
        Err(error) => return Err(error),
    };

    static WEAKREF: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let loop_ref = WEAKREF
        .import(py, "weakref", "ref")?
        .call1((event_loop,))?
        .unbind();
    let key = event_loop.as_ptr() as usize;
    let replaced = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, (loop_ref, queue.as_ref().map(|q| q.clone_ref(py))));
    drop(replaced);
    Ok(queue)
}

/// Forget the loops that were closed or collected. A closed loop no longer
/// watches its queue's socket, so calls that finish after the close are
/// dropped with the queue.
fn forget_closed_loops(py: Python<'_>) {
    let registered: Vec<(usize, Py<PyAny>)> = LOOPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(key, (loop_ref, _))| (*key, loop_ref.clone_ref(py)))
        .collect();
    let closed: Vec<(usize, Py<PyAny>)> = registered
        .into_iter()
        .filter(|(_, loop_ref)| !is_open(loop_ref.bind(py)))
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
    // Dropping a queue can drop a loop's last reference, which runs Python.
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
