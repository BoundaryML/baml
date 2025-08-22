use std::cell::RefCell;
use std::collections::HashMap;
use stream_cancel::{Trigger, Tripwire};
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;
use web_sys::AbortSignal;

thread_local! {
    static ABORT_CLOSURES: RefCell<HashMap<u32, Closure<dyn Fn()>>> = RefCell::new(HashMap::new());
    static OPERATION_TRIGGERS: RefCell<HashMap<u32, Trigger>> = RefCell::new(HashMap::new());
    static OPERATION_ID_COUNTER: RefCell<u32> = RefCell::new(0);
}

pub fn js_abort_signal_to_tripwire(
    signal: Option<js_sys::Object>,
) -> Result<(u32, Option<Tripwire>), JsError> {
    let Some(signal) = signal else {
        log::info!("BAML Abort: No abort signal provided");
        return Ok((0, None));
    };
    
    log::warn!("BAML Abort: Received abort signal object");
    
    let abort_signal: AbortSignal = signal.dyn_into()
        .map_err(|_| JsError::new("Expected AbortSignal"))?;
    
    let operation_id = OPERATION_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    });
    
    log::warn!("BAML Abort: Created operation ID {}", operation_id);
    
    let (trigger, tripwire) = Tripwire::new();
    
    // Early abort check
    if abort_signal.aborted() {
        log::warn!("BAML Abort: Signal already aborted on arrival for operation {}", operation_id);
        trigger.cancel();
        return Ok((operation_id, Some(tripwire)));
    }
    
    // Store the trigger for later cancellation
    OPERATION_TRIGGERS.with(|triggers| {
        let count = triggers.borrow().len();
        triggers.borrow_mut().insert(operation_id, trigger);
        log::info!("BAML Abort: Stored trigger for operation {} (total triggers: {})", operation_id, count + 1);
    });
    
    // Create closure for abort event
    let op_id = operation_id;
    let closure = Closure::wrap(Box::new(move || {
        log::warn!("BAML Abort: Abort event fired for operation {}!", op_id);
        // Cancel the operation when abort is triggered
        OPERATION_TRIGGERS.with(|triggers| {
            if let Some(trigger) = triggers.borrow_mut().remove(&op_id) {
                log::warn!("BAML Abort: Cancelling trigger for operation {}", op_id);
                trigger.cancel();
            } else {
                log::error!("BAML Abort: No trigger found for operation {} to cancel", op_id);
            }
        });
        // Self-cleanup after firing
        ABORT_CLOSURES.with(|closures| {
            closures.borrow_mut().remove(&op_id);
            log::info!("BAML Abort: Cleaned up closure for operation {}", op_id);
        });
    }) as Box<dyn Fn()>);
    
    // Set up event listener
    abort_signal.set_onabort(Some(closure.as_ref().unchecked_ref()));
    log::warn!("BAML Abort: Registered abort event listener for operation {}", operation_id);
    
    // Store closure to prevent deallocation
    ABORT_CLOSURES.with(|closures| {
        closures.borrow_mut().insert(operation_id, closure);
    });
    
    Ok((operation_id, Some(tripwire)))
}

pub fn cleanup_operation(operation_id: u32) {
    log::info!("BAML Abort: Cleaning up operation {}", operation_id);
    ABORT_CLOSURES.with(|closures| {
        let removed = closures.borrow_mut().remove(&operation_id);
        if removed.is_some() {
            log::info!("BAML Abort: Removed closure for operation {}", operation_id);
        }
    });
    OPERATION_TRIGGERS.with(|triggers| {
        let removed = triggers.borrow_mut().remove(&operation_id);
        if removed.is_some() {
            log::info!("BAML Abort: Removed trigger for operation {}", operation_id);
        }
    });
}