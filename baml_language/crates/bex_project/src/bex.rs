use ::bex_heap::HeapPermit as _;
use ::std::sync::Arc;
use async_trait::async_trait;
use baml_type::Ty;
use bex_engine::{BexCallArg, BexEngine, FunctionCallContext};
use bex_heap::{BexExternalValue, BexValue};
use sys_types::CallId;

use crate::{BexArgs, RuntimeError, project::BexProject};

/// Coerce a raw external value to match the shape the VM expects for a
/// declared parameter type.
///
/// Host bridges (Python/Node) that encode via protobuf don't always have
/// per-call type metadata, so a plain dict comes across as
/// `BexExternalValue::Map` even when the function declares a class
/// parameter. The VM treats `Map` and `Instance` as distinct object kinds
/// (see `bex_vm::vm::as_instance`) and refuses the mismatch, which surfaces
/// as "type error: expected instance, got map" the first time the function
/// body reads a field.
///
/// The class / enum name on an incoming `Instance` / `Variant` is also
/// diagnostic metadata from the host encoder (Python codegen 09d §2: the
/// name on `InboundClassValue` / `InboundEnumValue` is informational —
/// Rust rederives the expected name from the function signature). Override
/// it here so `resolved_class_names` / `resolved_enum_names` lookups use
/// the engine-registered FQN instead of whatever the host side derived
/// (e.g. a `root.lorem.MyLorem` vs. engine `user.lorem.MyLorem` mismatch).
///
/// Coercing at the project boundary — rather than inside each bridge —
/// keeps host encoders simple and fixes this uniformly across Python, Node,
/// and future bridges. Optional/list/map wrappers and unions are not
/// walked here; host-side schema-aware encoders (codegen phase 4+) remain
/// responsible for nested class shaping.
fn coerce_arg_to_declared_type(value: BexExternalValue, ty: &Ty) -> BexExternalValue {
    match (value, ty) {
        (BexExternalValue::Map { entries, .. }, Ty::Class(type_name, _, _)) => {
            BexExternalValue::Instance {
                class_name: type_name.to_string(),
                fields: entries,
            }
        }
        (BexExternalValue::Instance { fields, .. }, Ty::Class(type_name, _, _)) => {
            BexExternalValue::Instance {
                class_name: type_name.to_string(),
                fields,
            }
        }
        (BexExternalValue::Variant { variant_name, .. }, Ty::Enum(type_name, _)) => {
            BexExternalValue::Variant {
                enum_name: type_name.to_string(),
                variant_name,
            }
        }
        (v, _) => v,
    }
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

    fn cancel_function_call(&self, call_id: CallId) -> Result<(), RuntimeError>;

    /// Event sink for this runtime (if any). Used by bridges for flush and `HostSpanManager`.
    fn event_sink(&self) -> Option<std::sync::Arc<dyn bex_events::EventSink>>;
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

    fn event_sink(&self) -> Option<std::sync::Arc<dyn bex_events::EventSink>> {
        self.event_sink()
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
            .map(|(name, ty, has_default)| {
                if let Some(value) = args.remove(name) {
                    Ok(BexCallArg::Provided(Box::new(coerce_arg_to_declared_type(
                        value, ty,
                    ))))
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

    fn event_sink(&self) -> Option<std::sync::Arc<dyn bex_events::EventSink>> {
        BexEngine::event_sink(self)
    }
}
