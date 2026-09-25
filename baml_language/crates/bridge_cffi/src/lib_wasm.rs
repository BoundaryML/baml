//! Wasm runtime implementation for `bridge_cffi`.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    sync::Arc,
};

use bex_project::Bex;

use crate::{BridgeError, PreparedRuntime, error_to_outbound};

thread_local! {
    static RUNTIME: RefCell<RuntimeState> = const { RefCell::new(RuntimeState::Empty) };
    static WORKERD: Cell<bool> = const { Cell::new(false) };
}

enum RuntimeState {
    Empty,
    Pending(Box<PreparedRuntime>),
    Initializing,
    Ready(Arc<dyn Bex>),
    Failed(String),
}

/// Workerd forbids entropy and I/O during module evaluation. Compile and decode
/// eagerly, but construct the engine and execute `$init` on the first call.
pub fn configure_workerd_runtime() {
    WORKERD.set(true);
    bex_project::configure_workerd_runtime();
}

pub fn stage_runtime_from_bytecode_with_sys_ops(
    bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
    sys_ops: sys_ops::SysOps,
) -> Result<(), BridgeError> {
    stage_runtime(crate::prepare_runtime_from_bytecode(
        bytecode,
        embedded_baml_toml,
        sys_ops,
    )?)
}

pub fn stage_runtime_from_files_with_sys_ops(
    root_path: &str,
    files: HashMap<String, String>,
    sys_ops: sys_ops::SysOps,
) -> Result<(), BridgeError> {
    stage_runtime(crate::prepare_runtime_from_files(
        root_path, files, sys_ops,
    )?)
}

fn stage_runtime(runtime: PreparedRuntime) -> Result<(), BridgeError> {
    if WORKERD.get() {
        replace_state(RuntimeState::Pending(Box::new(runtime)))
    } else {
        replace_runtime(runtime.build()?)
    }
}

fn replace_state(state: RuntimeState) -> Result<(), BridgeError> {
    let previous = RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();
        if matches!(*slot, RuntimeState::Initializing) {
            return Err(initialization_in_progress());
        }
        Ok(std::mem::replace(&mut *slot, state))
    })?;
    if let RuntimeState::Ready(previous) = previous {
        wasm_bindgen_futures::spawn_local(previous.shutdown());
    }
    Ok(())
}

pub(crate) fn replace_runtime(runtime: Arc<dyn Bex>) -> Result<(), BridgeError> {
    replace_state(RuntimeState::Ready(runtime))
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    RUNTIME.with(|slot| {
        let mut slot = slot.borrow_mut();
        if matches!(*slot, RuntimeState::Initializing) {
            return Err(initialization_in_progress());
        }
        Ok(match std::mem::replace(&mut *slot, RuntimeState::Empty) {
            RuntimeState::Ready(runtime) => Some(runtime),
            _ => None,
        })
    })
}

fn initialization_in_progress() -> BridgeError {
    BridgeError::Startup("BAML runtime initialization is already in progress".to_string())
}

pub(crate) fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    RUNTIME.with(|slot| {
        let pending = {
            let mut state = slot.borrow_mut();
            match &*state {
                RuntimeState::Empty => return Err(BridgeError::NotInitialized),
                RuntimeState::Ready(runtime) => return Ok(Arc::clone(runtime)),
                RuntimeState::Failed(error) => return Err(BridgeError::Startup(error.clone())),
                RuntimeState::Initializing => return Err(initialization_in_progress()),
                RuntimeState::Pending(_) => {}
            }
            let RuntimeState::Pending(pending) =
                std::mem::replace(&mut *state, RuntimeState::Initializing)
            else {
                unreachable!()
            };
            pending
        };

        // Release the slot borrow before executing initializers. Reentrant calls
        // get an explicit error rather than a second engine or a RefCell panic.
        match pending.build() {
            Ok(runtime) => {
                *slot.borrow_mut() = RuntimeState::Ready(Arc::clone(&runtime));
                Ok(runtime)
            }
            Err(error) => {
                let message = error.to_string();
                *slot.borrow_mut() = RuntimeState::Failed(message.clone());
                Err(BridgeError::Startup(message))
            }
        }
    })
}

fn prepare_runtime_call(
    encoded_args: &[u8],
) -> Result<(Arc<dyn Bex>, crate::PreparedCall), BridgeError> {
    let call = crate::prepare_call(encoded_args)?;
    Ok((get_runtime()?, call))
}

pub async fn call_function_in_wasm(encoded_args: &[u8]) -> Vec<u8> {
    match prepare_runtime_call(encoded_args) {
        Ok((runtime, call)) => crate::invoke_prepared(runtime, call).await,
        Err(error) => error_to_outbound(error),
    }
}

pub fn call_function_in_wasm_sync(encoded_args: &[u8]) -> Vec<u8> {
    // Engine construction acquires its initial heap permits with block_on.
    // Complete it before entering the executor used for the synchronous call.
    match prepare_runtime_call(encoded_args) {
        Ok((runtime, call)) => futures::executor::block_on(crate::invoke_prepared(runtime, call)),
        Err(error) => error_to_outbound(error),
    }
}
