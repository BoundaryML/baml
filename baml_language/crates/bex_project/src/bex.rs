use ::bex_heap::HeapPermit as _;
use ::std::sync::Arc;
use async_trait::async_trait;
use bex_engine::{BexCallArg, BexEngine, CallRef, FunctionCallContext, UnhandledSpawnErrorHandler};
use bex_heap::{BexExternalValue, BexValue};
use sys_types::CallId;

use crate::{BexArgs, RuntimeError};

pub struct BexCallTraceResult {
    pub value: Result<BexExternalValue, RuntimeError>,
    pub entry_call_ref: CallRef,
}

/// A concrete caller selects a checked class member, not a function pointer.
/// Its receiver is owned while class slots and method evidence are checked.
pub struct ConcreteMethodCall {
    pub receiver: bex_external_types::Handle,
    pub class: baml_type::QualifiedTypeName,
    /// Some selects an implementation obligation; None selects an inherent
    /// receiver method of the checked class.
    pub interface_pattern: Option<bex_external_types::RuntimeTy>,
    pub member: String,
    pub type_args: Vec<bex_external_types::TypeArgument>,
}

/// Core runtime API: call functions and introspect parameters.
#[async_trait]
pub trait Bex: Send + Sync {
    async fn register_host_adapter(
        self: Arc<Self>,
        descriptor: crate::HostAdapterTypeDescriptor,
        type_args: Vec<crate::TypeArgument>,
    ) -> Result<crate::HostAdapterType, RuntimeError>;
    async fn create_host_adapter_instance(
        self: Arc<Self>,
        registration: Arc<crate::HostAdapterType>,
        receiver: Arc<crate::HostValueArc>,
        callbacks: Vec<Arc<crate::HostValueArc>>,
    ) -> Result<crate::Handle, RuntimeError>;
    async fn project_interface(
        self: Arc<Self>,
        value: BexExternalValue,
        expected: crate::TypeArgument,
    ) -> Result<Arc<crate::InterfaceValue>, RuntimeError>;

    async fn call_concrete_method(
        self: Arc<Self>,
        target: ConcreteMethodCall,
        args: indexmap::IndexMap<String, BexExternalValue>,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError>;

    /// Invoke a checked interface view. Parameter names and optional modes
    /// come from its retained declaration, not host-supplied type decoration.
    async fn call_interface_method(
        self: Arc<Self>,
        view: Arc<bex_external_types::InterfaceValue>,
        member: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
        args: indexmap::IndexMap<String, BexExternalValue>,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError>;

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

    /// Attach immutable host-dispatch issuer metadata before publication.
    fn install_host_bridge_context(&self, context: Arc<dyn std::any::Any + Send + Sync>) -> bool;

    /// Wait for current work without revoking admission or retained values.
    async fn wait_until_idle(self: Arc<Self>);

    async fn shutdown(self: Arc<Self>);

    /// Run-vocabulary alias for host-call cancellation. The parameter is still
    /// the adapter-owned `HostCallId` backing value, not a `RunId`.
    fn cancel_run(&self, host_call_id: CallId) -> Result<(), RuntimeError> {
        self.cancel_function_call(host_call_id)
    }
}

#[async_trait]
impl Bex for BexEngine {
    async fn register_host_adapter(
        self: Arc<Self>,
        descriptor: crate::HostAdapterTypeDescriptor,
        type_args: Vec<crate::TypeArgument>,
    ) -> Result<crate::HostAdapterType, RuntimeError> {
        self.register_host_adapter_with_type_arguments(descriptor, type_args)
            .await
            .map_err(RuntimeError::from)
    }
    async fn create_host_adapter_instance(
        self: Arc<Self>,
        registration: Arc<crate::HostAdapterType>,
        receiver: Arc<crate::HostValueArc>,
        callbacks: Vec<Arc<crate::HostValueArc>>,
    ) -> Result<crate::Handle, RuntimeError> {
        BexEngine::create_host_adapter_instance(&self, &registration, receiver, callbacks)
            .await
            .map_err(RuntimeError::from)
    }
    async fn project_interface(
        self: Arc<Self>,
        value: BexExternalValue,
        expected: crate::TypeArgument,
    ) -> Result<Arc<crate::InterfaceValue>, RuntimeError> {
        self.project_interface_with_type_argument(value, expected)
            .await
            .map_err(RuntimeError::from)
    }

    fn install_host_bridge_context(&self, context: Arc<dyn std::any::Any + Send + Sync>) -> bool {
        BexEngine::install_host_bridge_context(self, context)
    }

    async fn call_concrete_method(
        self: Arc<Self>,
        target: ConcreteMethodCall,
        args: indexmap::IndexMap<String, BexExternalValue>,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        if !call_ctx.type_args.is_empty() {
            return Err(bex_engine::EngineError::TypeMismatch {
                message: "method type arguments belong to the checked method target".into(),
            }
            .into());
        }
        let callable = if let Some(pattern) = target.interface_pattern {
            self.bind_concrete_class_interface_method(
                target.receiver,
                &target.class,
                pattern,
                &target.member,
                target.type_args,
            )
            .await
        } else {
            self.bind_concrete_class_inherent_method(
                target.receiver,
                &target.class,
                &target.member,
                target.type_args,
            )
            .await
        }
        .map_err(RuntimeError::from)?;
        self.call_callable_keywords(callable, args, call_ctx, true)
            .await
            .map_err(RuntimeError::from)
    }

    async fn call_interface_method(
        self: Arc<Self>,
        view: Arc<bex_external_types::InterfaceValue>,
        member: &str,
        type_args: Vec<bex_external_types::TypeArgument>,
        args: indexmap::IndexMap<String, BexExternalValue>,
        call_ctx: FunctionCallContext,
    ) -> Result<BexExternalValue, RuntimeError> {
        self.call_interface_named(view, member, type_args, args, call_ctx)
            .await
            .map_err(RuntimeError::from)
    }

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

    async fn wait_until_idle(self: Arc<Self>) {
        BexEngine::wait_until_idle(&self).await;
    }

    async fn shutdown(self: Arc<Self>) {
        BexEngine::shutdown(&self).await;
    }
}
