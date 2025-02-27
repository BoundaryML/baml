use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use baml_types::tracing::events::FunctionId;
use uuid::Uuid;

use crate::tracingv2::storage::storage::BAML_TRACER;

use super::function_log_inner::FunctionLogInner;
use super::models::*;

///
/// Represents a single function call's log.
///
#[derive(Debug)]
pub struct FunctionLog {
    id: FunctionId,
    /// We store an optional Arc<Mutex<FunctionLogInner>> so that we only load it lazily.
    inner: Option<Arc<Mutex<FunctionLogInner>>>,
    instance_id: String,
}

impl Clone for FunctionLog {
    fn clone(&self) -> Self {
        // Creating a new FunctionLog will inc_ref again:
        Self::new(self.id.clone())
    }
}

impl FunctionLog {
    pub fn new(id: FunctionId) -> Self {
        // Manually increment the global reference count
        BAML_TRACER.lock().unwrap().inc_ref(&id);
        let instance_id = Uuid::new_v4().to_string();

        Self {
            id,
            inner: None,
            instance_id,
        }
    }

    // Private helper to get or build the inner reference
    fn get_inner(&mut self) -> &Arc<Mutex<FunctionLogInner>> {
        if self.inner.is_none() {
            // We attempt to build or retrieve from the global tracer
            let maybe_arc = {
                let tracer = BAML_TRACER.lock().unwrap();
                FunctionLogInner::get_or_create(&self.id)
                    .expect("Function log expected to be present (no FunctionStart event?). Did you forget to track_function()?")
            };
            self.inner = Some(maybe_arc);
        }
        self.inner.as_ref().unwrap()
    }

    pub fn id(&self) -> FunctionId {
        self.id.clone()
    }

    // The methods below clone from the underlying data (no references).
    pub fn function_name(&mut self) -> String {
        self.get_inner().lock().unwrap().function_name.clone()
    }

    pub fn log_type(&mut self) -> String {
        self.get_inner().lock().unwrap().r#type.clone()
    }

    pub fn timing(&mut self) -> Timing {
        self.get_inner().lock().unwrap().timing.clone()
    }

    pub fn usage(&mut self) -> Usage {
        self.get_inner().lock().unwrap().usage.clone()
    }

    pub fn calls(&mut self) -> Vec<LLMCallKind> {
        self.get_inner().lock().unwrap().calls.clone()
    }

    pub fn raw_llm_response(&mut self) -> Option<String> {
        self.get_inner().lock().unwrap().raw_llm_response.clone()
    }

    pub fn metadata(&mut self) -> HashMap<String, serde_json::Value> {
        self.get_inner().lock().unwrap().metadata.clone()
    }
}

impl Drop for FunctionLog {
    fn drop(&mut self) {
        // Manually decrement the global ref count
        BAML_TRACER.lock().unwrap().dec_ref(&self.id);
    }
}
