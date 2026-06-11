use ::bex_heap::HeapPermit as _;
use ::std::sync::Arc;
use async_trait::async_trait;
use bex_engine::{BexCallArg, BexEngine, FunctionCallContext};
use bex_heap::{BexExternalValue, BexValue};
use sys_types::CallId;

use crate::{BexArgs, RuntimeError, project::BexProject};

/// Core runtime API: call functions and introspect parameters.
#[async_trait]
pub trait Bex: Send + Sync {
    /// Execute a function by name. Returns a fully owned value (no Handle variants).
    async fn call_function(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError>;

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError>;
}

#[async_trait]
impl Bex for BexProject {
    async fn call_function(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        let bex = self.get_bex()?;
        Bex::call_function(bex, function_name, args, call_ctx).await
    }

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError> {
        let bex = self.get_bex()?;
        bex.cancel_function_call(call_id)
            .map_err(RuntimeError::from)
    }
}

#[async_trait]
impl Bex for BexEngine {
    /// Resolve named `BexArgs` into the positional `Vec<BexExternalValue>` that
    /// `BexEngine::call_function` expects, using the engine's parameter metadata.
    async fn call_function(
        self: Arc<Self>,
        function_name: &str,
        BexArgs(mut args): BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        let params = self
            .function_params(function_name)
            .map_err(RuntimeError::from)?;

        let ordered_args: Vec<BexCallArg> = params
            .into_iter()
            .map(|(name, _ty, has_default)| {
                if let Some(value) = args.remove(name) {
                    // Type-directed coercion (class-name rewriting,
                    // int↔bigint widening, optional/union recursion) now
                    // happens inside `call_function_bound_args` for all
                    // entry paths; we just deliver the raw provided value.
                    Ok(BexCallArg::Provided(Box::new(value)))
                } else if has_default {
                    Ok(BexCallArg::OmittedDefault)
                } else {
                    Err(RuntimeError::InvalidArgument {
                        name: name.to_string(),
                    })
                }
            })
            .collect::<Result<_, _>>()?;

        if !args.is_empty() {
            let extra_args = args.keys().cloned().collect::<Vec<_>>().join(", ");
            return Err(RuntimeError::InvalidArgument {
                name: format!("extra arguments: {extra_args}"),
            });
        }

        let result =
            BexEngine::call_function_bound_args(&self, function_name, ordered_args, call_ctx, true)
                .await?;

        let permit = self
            .heap_permit_manager()
            .new_permit(())
            .await
            .acquire()
            .await;
        let owned_result =
            BexValue::from(&result).as_owned_but_very_slow(self.heap(), permit.proof())?;

        Ok(owned_result)
    }

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError> {
        BexEngine::cancel_function_call(self, call_id).map_err(RuntimeError::from)
    }
}
