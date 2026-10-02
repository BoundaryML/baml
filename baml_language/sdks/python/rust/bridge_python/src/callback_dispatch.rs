//! Per-SDK-entry callback execution. No callable registration owns a loop or
//! context. Routes are removed with their entry; dispatched Python tasks keep
//! their own arguments and Context until their bodies actually finish.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex, PoisonError, mpsc},
};

use pyo3::{
    Py, PyAny, PyResult, Python,
    exceptions::PyRuntimeError,
    types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods, PyModule},
};

use crate::host_value;

pub(crate) struct Dispatch {
    pub callable: Py<PyAny>,
    pub call_id: u32,
    pub args: Vec<u8>,
    pub execution: host_value::HostExecutionLease,
}

pub(crate) enum SyncMessage {
    Dispatch(Dispatch),
    Finished(Vec<u8>),
}

enum Environment {
    Sync(mpsc::Sender<SyncMessage>),
    Async {
        event_loop: Py<PyAny>,
        context: Py<PyAny>,
    },
}

static ENVIRONMENTS: LazyLock<Mutex<HashMap<u64, Arc<Environment>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct DispatchScope {
    call_id: u64,
    environment: Arc<Environment>,
}

impl DispatchScope {
    fn register(call_id: u64, environment: Environment) -> PyResult<Self> {
        let environment = Arc::new(environment);
        let mut routes = ENVIRONMENTS.lock().unwrap_or_else(PoisonError::into_inner);
        if routes.contains_key(&call_id) {
            return Err(PyRuntimeError::new_err(
                "SDK call ID already has an active callback route",
            ));
        }
        routes.insert(call_id, Arc::clone(&environment));
        Ok(Self {
            call_id,
            environment,
        })
    }

    pub(crate) fn synchronous(call_id: u64, sender: mpsc::Sender<SyncMessage>) -> PyResult<Self> {
        Self::register(call_id, Environment::Sync(sender))
    }

    pub(crate) fn asynchronous(py: Python<'_>, call_id: u64) -> PyResult<Self> {
        let event_loop = PyModule::import(py, "asyncio")?
            .getattr("get_running_loop")?
            .call0()?
            .unbind();
        let context = PyModule::import(py, "contextvars")?
            .getattr("copy_context")?
            .call0()?
            .unbind();
        Self::register(
            call_id,
            Environment::Async {
                event_loop,
                context,
            },
        )
    }
}

impl Drop for DispatchScope {
    fn drop(&mut self) {
        let removed = {
            let mut routes = ENVIRONMENTS.lock().unwrap_or_else(PoisonError::into_inner);
            if routes
                .get(&self.call_id)
                .is_some_and(|env| Arc::ptr_eq(env, &self.environment))
            {
                routes.remove(&self.call_id)
            } else {
                None
            }
        };
        // Do not drop Python owners while holding the routing lock.
        drop(removed);
    }
}

pub(crate) fn route(dispatch: Dispatch) {
    let Some(origin) = sys_native::host_dispatch::origin_call_id(dispatch.call_id) else {
        // Cancellation already evicted this dispatch. Do not execute stale work.
        return;
    };
    let environment = ENVIRONMENTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&origin.0)
        .cloned();
    let Some(environment) = environment else {
        host_value::send_dispatch_bridge_failure(
            dispatch.call_id,
            "originating Python SDK call has no active callback environment".to_owned(),
        );
        return;
    };

    match &*environment {
        Environment::Sync(sender) => {
            let call_id = dispatch.call_id;
            if let Err(mpsc::SendError(SyncMessage::Dispatch(dispatch))) =
                sender.send(SyncMessage::Dispatch(dispatch))
            {
                host_value::send_dispatch_bridge_failure(
                    call_id,
                    "originating Python caller stopped servicing callbacks".to_owned(),
                );
                drop(dispatch);
            }
        }
        Environment::Async {
            event_loop,
            context,
        } => {
            let result = Python::attach(|py| -> PyResult<()> {
                let start =
                    PyModule::import(py, "baml_bridge._dispatch")?.getattr("_start_dispatch")?;
                let kwargs = PyDict::new(py);
                // Contexts may not be entered concurrently, and callback writes
                // must not contaminate a sibling dispatch of this same entry.
                kwargs.set_item("context", context.bind(py).call_method0("copy")?)?;
                event_loop.bind(py).call_method(
                    "call_soon_threadsafe",
                    (
                        start,
                        dispatch.callable.bind(py),
                        dispatch.call_id,
                        PyBytes::new(py, &dispatch.args),
                        Py::new(py, dispatch.execution)?,
                    ),
                    Some(&kwargs),
                )?;
                Ok(())
            });
            if let Err(error) = result {
                host_value::send_dispatch_bridge_failure(
                    dispatch.call_id,
                    format!("could not schedule Python callback on its originating loop: {error}"),
                );
            }
        }
    }
}
