//! Wasm runtime implementation for `bridge_cffi`.

use std::{cell::RefCell, sync::Arc};

use bex_project::Bex;

use crate::{BridgeError, error_to_outbound};

thread_local! {
    static RUNTIME: RefCell<Option<(Arc<dyn Bex>, bridge_ctypes::TransferSession<'static>)>> = RefCell::new(None);
}

pub(crate) fn replace_runtime(runtime: Arc<dyn Bex>) -> Result<(), BridgeError> {
    let transfers = bridge_ctypes::TransferSession::new(&bridge_ctypes::HANDLE_TABLE);
    crate::RuntimeIssuer::install(&runtime, transfers.clone())?;
    RUNTIME.with(|slot| {
        let previous = slot.borrow_mut().replace((runtime, transfers));
        if let Some((previous, transfers)) = previous {
            transfers.close();
            wasm_bindgen_futures::spawn_local(previous.shutdown());
        }
    });
    Ok(())
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    let previous = RUNTIME.with(|slot| slot.borrow_mut().take());
    Ok(previous.map(|(runtime, transfers)| {
        transfers.close();
        runtime
    }))
}

pub(crate) fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    get_runtime_with_transfers().map(|(runtime, _)| runtime)
}

pub(crate) fn get_runtime_with_transfers()
-> Result<(Arc<dyn Bex>, bridge_ctypes::TransferSession<'static>), BridgeError> {
    RUNTIME.with(|slot| slot.borrow().clone().ok_or(BridgeError::NotInitialized))
}

pub async fn call_function_in_wasm(encoded_args: &[u8]) -> Vec<u8> {
    let prepared =
        crate::prepare_call(encoded_args).and_then(|call| Ok((crate::get_runtime()?, call)));
    match prepared {
        Ok((runtime, call)) => crate::invoke_prepared(runtime, call).await,
        Err(error) => error_to_outbound(error),
    }
}

pub fn call_function_in_wasm_sync(encoded_args: &[u8]) -> Vec<u8> {
    futures::executor::block_on(call_function_in_wasm(encoded_args))
}
