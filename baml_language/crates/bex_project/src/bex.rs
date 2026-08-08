use ::bex_heap::HeapPermit as _;
use ::std::sync::Arc;
use async_trait::async_trait;
use bex_engine::{BexCallArg, BexEngine, CallRef, FunctionCallContext, UnhandledSpawnErrorHandler};
use bex_heap::{BexExternalValue, BexValue};
use sys_types::CallId;

use crate::{BexArgs, RuntimeError, project::BexProject};

pub struct BexCallTraceResult {
    pub value: Result<BexExternalValue, RuntimeError>,
    pub entry_call_ref: CallRef,
}

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

    /// Execute an engine-owned callable by heap handle.
    async fn call_callable(
        self: Arc<Self>,
        handle: bex_external_types::Handle,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError>;

    /// Execute a function by name and surface the BEX entry trace identity once
    /// the VM has actually started. Pre-entry failures return `Err`; runtime
    /// success/failure returns `Ok` with the traced outcome.
    async fn call_function_with_trace(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError>;

    /// Run-vocabulary alias for the traced function entry path. `call_ctx`
    /// carries adapter-owned host plumbing; durable run identity stays in
    /// `RunStore`.
    async fn start_run(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError> {
        self.call_function_with_trace(function_name, args, call_ctx)
            .await
    }

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError>;

    fn set_unhandled_spawn_error_handler(&self, handler: Option<UnhandledSpawnErrorHandler>);

    async fn shutdown(self: Arc<Self>);

    /// Run-vocabulary alias for host-call cancellation. The parameter is still
    /// the adapter-owned `HostCallId` backing value, not a `RunId`.
    fn cancel_run(&self, host_call_id: CallId) -> Result<(), RuntimeError> {
        self.cancel_function_call(host_call_id)
    }
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

    async fn call_function_with_trace(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError> {
        let bex = self.get_bex()?;
        Bex::call_function_with_trace(bex, function_name, args, call_ctx).await
    }

    async fn call_callable(
        self: Arc<Self>,
        handle: bex_external_types::Handle,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        let bex = self.get_bex()?;
        Bex::call_callable(bex, handle, args, call_ctx).await
    }

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError> {
        let bex = self.get_bex()?;
        bex.cancel_function_call(call_id)
            .map_err(RuntimeError::from)
    }

    fn set_unhandled_spawn_error_handler(&self, handler: Option<UnhandledSpawnErrorHandler>) {
        if let Ok(bex) = self.get_bex() {
            bex.set_unhandled_spawn_error_handler(handler);
        }
    }

    async fn shutdown(self: Arc<Self>) {
        if let Ok(bex) = self.get_bex() {
            bex.shutdown().await;
        }
    }
}

#[async_trait]
impl Bex for BexEngine {
    /// Resolve named `BexArgs` into the positional `Vec<BexExternalValue>` that
    /// `BexEngine::call_function` expects, using the engine's parameter metadata.
    async fn call_function(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        let result = Bex::call_function_with_trace(self, function_name, args, call_ctx).await?;
        result.value
    }

    async fn call_callable(
        self: Arc<Self>,
        handle: bex_external_types::Handle,
        BexArgs { required, optional }: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        BexEngine::call_callable_named(&self, handle, required, optional, call_ctx, true)
            .await
            .map_err(RuntimeError::from)
    }

    /// Resolve named `BexArgs` into the positional `Vec<BexExternalValue>` that
    /// `BexEngine::call_function` expects, using the engine's parameter metadata.
    async fn call_function_with_trace(
        self: Arc<Self>,
        function_name: &str,
        BexArgs {
            mut required,
            mut optional,
        }: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError> {
        let params = self
            .function_params(function_name)
            .map_err(RuntimeError::from)?;

        let ordered_args: Vec<BexCallArg> = params
            .into_iter()
            .map(|(name, _ty, has_default)| {
                if let Some(value) = required
                    .shift_remove(name)
                    .or_else(|| optional.shift_remove(name))
                {
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

        if !required.is_empty() || !optional.is_empty() {
            let extra_args = required
                .keys()
                .chain(optional.keys())
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RuntimeError::InvalidArgument {
                name: format!("extra arguments: {extra_args}"),
            });
        }

        let result = BexEngine::call_function_bound_args_with_trace(
            &self,
            function_name,
            ordered_args,
            call_ctx,
            true,
        )
        .await?;

        let value = match result.value {
            Ok(result) => {
                let permit = self
                    .heap_permit_manager()
                    .new_permit(())
                    .await
                    .acquire()
                    .await;
                let owned_result = BexValue::from(&result)
                    .as_owned_with_package_handles(self.heap(), permit.proof())?;
                Ok(owned_result)
            }
            Err(err) => Err(RuntimeError::from(err)),
        };

        Ok(BexCallTraceResult {
            value,
            entry_call_ref: result.entry_call_ref,
        })
    }

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError> {
        BexEngine::cancel_function_call(self, call_id).map_err(RuntimeError::from)
    }

    fn set_unhandled_spawn_error_handler(&self, handler: Option<UnhandledSpawnErrorHandler>) {
        BexEngine::set_unhandled_spawn_error_handler(self, handler);
    }

    async fn shutdown(self: Arc<Self>) {
        BexEngine::shutdown(&self).await;
    }
}
