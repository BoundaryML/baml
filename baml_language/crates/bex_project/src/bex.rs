use ::bex_heap::HeapPermit as _;
use ::std::sync::Arc;
use async_trait::async_trait;
use bex_engine::{BexCallArg, BexEngine, CallRef, FunctionCallContext, UnhandledSpawnErrorHandler};
use bex_heap::{BexExternalValue, BexValue};
use sys_types::CallId;

use crate::{BexArgs, FunctionOperation, RuntimeError};

pub struct BexCallTraceResult {
    pub value: Result<BexExternalValue, RuntimeError>,
    pub entry_call_ref: CallRef,
}

fn ordered_function_args(
    engine: &BexEngine,
    function_name: &str,
    operation: FunctionOperation,
    BexArgs {
        mut required,
        mut optional,
    }: BexArgs,
) -> Result<Vec<BexCallArg>, RuntimeError> {
    let params = engine
        .function_operation_params(function_name, operation)
        .map_err(RuntimeError::from)?;
    let ordered = params
        .into_iter()
        .map(|(name, _ty, has_default)| {
            if let Some(value) = required
                .shift_remove(name)
                .or_else(|| optional.shift_remove(name))
            {
                Ok(BexCallArg::Provided(Box::new(value)))
            } else if has_default {
                Ok(BexCallArg::OmittedDefault)
            } else {
                Err(RuntimeError::InvalidArgument {
                    name: name.to_string(),
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

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
    Ok(ordered)
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

    /// Execute a semantic projection of an authored function. The authored FQN
    /// remains unchanged; `operation` selects its compiler-private entry.
    async fn call_function_operation(
        self: Arc<Self>,
        function_name: &str,
        operation: FunctionOperation,
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

    /// Execute a semantic projection of a live authored function value.
    async fn call_callable_operation(
        self: Arc<Self>,
        handle: bex_external_types::Handle,
        operation: FunctionOperation,
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

    async fn call_function_operation(
        self: Arc<Self>,
        function_name: &str,
        operation: FunctionOperation,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        if operation == FunctionOperation::Direct {
            return Bex::call_function(self, function_name, args, call_ctx).await;
        }

        let ordered_args = ordered_function_args(&self, function_name, operation, args)?;
        BexEngine::call_function_bound_args_operation(
            &self,
            function_name,
            operation,
            ordered_args,
            call_ctx,
            true,
        )
        .await
        .map_err(RuntimeError::from)
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

    async fn call_callable_operation(
        self: Arc<Self>,
        handle: bex_external_types::Handle,
        operation: FunctionOperation,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        if operation == FunctionOperation::Direct {
            return Bex::call_callable(self, handle, args, call_ctx).await;
        }

        let BexArgs { required, optional } = args;
        BexEngine::call_callable_operation_named(
            &self, handle, operation, required, optional, call_ctx, true,
        )
        .await
        .map_err(RuntimeError::from)
    }

    /// Resolve named `BexArgs` into the positional `Vec<BexExternalValue>` that
    /// `BexEngine::call_function` expects, using the engine's parameter metadata.
    async fn call_function_with_trace(
        self: Arc<Self>,
        function_name: &str,
        args: BexArgs,
        call_ctx: FunctionCallContext,
    ) -> Result<BexCallTraceResult, RuntimeError> {
        let ordered_args =
            ordered_function_args(&self, function_name, FunctionOperation::Direct, args)?;

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

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use bex_external_types::{BexExternalAdt, TaggedHeapHandleKind};
    use sys_native::SysOpsExt as _;

    const SOURCE: &str = r#"
        function Echo(value: string) -> string { value }

        function Ask(question: string) -> string {
            client: "openai/gpt-4o-mini"
            prompt: `${question}`
        }

        function AskValue() -> unknown { Ask }
    "#;

    fn engine() -> Arc<BexEngine> {
        let program = baml_db::testing::compile_source(SOURCE);
        Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new())
                .expect("test engine"),
        )
    }

    fn ctx() -> FunctionCallContext {
        bex_engine::FunctionCallContextBuilder::new(CallId::next()).build()
    }

    fn one_arg(name: &str, value: impl Into<BexExternalValue>) -> BexArgs {
        BexArgs {
            required: indexmap::IndexMap::from([(name.to_string(), value.into())]),
            optional: indexmap::IndexMap::new(),
        }
    }

    fn assert_function_spec(value: BexExternalValue) {
        assert!(
            matches!(
                value,
                BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                    kind: TaggedHeapHandleKind::FunctionSpec,
                    ..
                })
            ),
            "spec operation must return a rooted FunctionSpec handle"
        );
    }

    #[tokio::test]
    async fn semantic_operations_use_authored_names_and_callable_handles() {
        let engine = engine();
        assert!(!engine.function_exists("Ask$spec"));
        assert!(!engine.function_exists("Ask$stream"));

        let direct = Bex::call_function_operation(
            Arc::clone(&engine),
            "Echo",
            FunctionOperation::Direct,
            one_arg("value", "hello"),
            ctx(),
        )
        .await
        .expect("direct operation");
        assert_eq!(direct, BexExternalValue::String("hello".into()));

        let spec = Bex::call_function_operation(
            Arc::clone(&engine),
            "Ask",
            FunctionOperation::Spec,
            one_arg("question", "hello"),
            ctx(),
        )
        .await
        .expect("named spec operation");
        assert_function_spec(spec);

        let stream_params = engine
            .function_operation_params("Ask", FunctionOperation::Stream)
            .expect("stream operation params");
        assert_eq!(
            stream_params
                .iter()
                .map(|(name, _, _)| *name)
                .collect::<Vec<_>>(),
            ["question", "client", "on_event"]
        );

        let callable = Bex::call_function(
            Arc::clone(&engine),
            "AskValue",
            BexArgs {
                required: indexmap::IndexMap::new(),
                optional: indexmap::IndexMap::new(),
            },
            ctx(),
        )
        .await
        .expect("function value");
        let BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            kind: TaggedHeapHandleKind::Callable,
            heap_handle,
            ..
        }) = callable
        else {
            panic!("AskValue must return a callable handle");
        };
        let callable_spec = Bex::call_callable_operation(
            Arc::clone(&engine),
            heap_handle,
            FunctionOperation::Spec,
            one_arg("question", "hello"),
            ctx(),
        )
        .await
        .expect("callable spec operation");
        assert_function_spec(callable_spec);
    }

    #[tokio::test]
    async fn unsupported_spec_operation_is_rejected() {
        let engine = engine();
        let non_llm = Bex::call_function_operation(
            Arc::clone(&engine),
            "Echo",
            FunctionOperation::Spec,
            one_arg("value", "hello"),
            ctx(),
        )
        .await
        .expect_err("plain function has no spec operation")
        .to_string();
        assert!(non_llm.contains("does not support the `spec` operation"));

        let non_llm = Bex::call_function_operation(
            Arc::clone(&engine),
            "Echo",
            FunctionOperation::Stream,
            one_arg("value", "hello"),
            ctx(),
        )
        .await
        .expect_err("plain function has no stream operation")
        .to_string();
        assert!(non_llm.contains("does not support the `stream` operation"));
    }
}
