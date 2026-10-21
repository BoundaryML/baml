#![allow(unsafe_code)]
//! Immutable tracing builders and detached inspection snapshots.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    MapReadGuard,
    trace::{ReservedSpanData, SpanId, TraceOptionsData},
    types::{Instance, Object, Type, Value},
};
use btel_types::{
    InvocationMode,
    context::{Context, ContextPatch, ContextValue},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
    package_baml::{NativeCallResult, NativeFunction, NativeFunctionResult},
};

#[allow(
    unused_imports,
    unused_variables,
    unsafe_code,
    non_camel_case_types,
    non_snake_case,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::too_many_arguments,
    clippy::used_underscore_binding,
    clippy::elidable_lifetime_names,
    clippy::get_first,
    clippy::needless_lifetimes,
    clippy::redundant_closure_call
)]
mod generated {
    use super::*;
    include!(concat!(env!("OUT_DIR"), "/tracefunctions_generated.rs"));
}
pub use generated::*;

pub struct PackageTraceImpl;

fn options_data(vm: &BexVm, value: Value) -> TraceOptionsData {
    let instance = vm.as_instance(&value).expect("trace.Options receiver");
    vm.as_rust_data::<TraceOptionsData>(&instance.load_field(0))
        .expect("trace.Options handle")
        .clone()
}

fn reservation_data(vm: &BexVm, value: Value) -> &ReservedSpanData {
    let instance = vm.as_instance(&value).expect("trace.ReservedSpan receiver");
    vm.as_rust_data::<ReservedSpanData>(&instance.load_field(0))
        .expect("trace.ReservedSpan handle")
}

fn mode_from_value(vm: &BexVm, value: Option<&Value>) -> Option<InvocationMode> {
    let value = value?;
    let Object::Variant(variant) =
        vm.get_object(value.as_object_ptr().expect("trace.Mode variant"))
    else {
        unreachable!("trace.Mode must be an enum variant");
    };
    let Object::Enum(enm) = vm.get_object(variant.enm) else {
        unreachable!("trace.Mode must be an enum");
    };
    Some(match enm.variants[variant.index].name.as_str() {
        "Hidden" => InvocationMode::Hidden,
        "Timing" => InvocationMode::Timing,
        "Span" => InvocationMode::Span,
        _ => unreachable!("unknown trace.Mode"),
    })
}

fn alloc_options(vm: &mut BexVm, options: TraceOptionsData) -> Value {
    copy::Options {
        _handle: Arc::new(options),
    }
    .to_value(vm)
}

fn alloc_id(vm: &mut BexVm, id: SpanId) -> Value {
    copy::SpanId {
        _handle: Arc::new(id),
    }
    .to_value(vm)
}

pub(crate) fn alloc_settings(vm: &mut BexVm, options: &TraceOptionsData) -> Value {
    let mode = options.mode.map_or(Value::NULL, |mode| {
        let name = match mode {
            InvocationMode::Hidden => "Hidden",
            InvocationMode::Timing => "Timing",
            InvocationMode::Span => "Span",
        };
        let enm = vm.lang_object(crate::lang_items::TRACE_MODE);
        let Object::Enum(def) = vm.get_object(enm) else {
            unreachable!("trace.Mode must be an enum");
        };
        let index = def
            .variants
            .iter()
            .position(|variant| variant.name == name)
            .expect("trace.Mode variant");
        Value::object(vm.alloc_variant(enm, index))
    });
    let capture = copy::Capture {
        inputs: options.inputs.map_or(Value::NULL, Value::bool),
        output: options.output.map_or(Value::NULL, Value::bool),
        error: options.error.map_or(Value::NULL, Value::bool),
    }
    .to_value(vm);
    let context = alloc_patch(vm, options.context.as_deref());
    copy::Settings {
        mode,
        capture,
        context,
    }
    .to_value(vm)
}

fn patch_from_values(
    vm: &BexVm,
    metadata: &IndexMap<bex_str::BexStr, Value>,
    distinct_id: Option<&bex_str::BexStr>,
) -> ContextPatch {
    ContextPatch {
        metadata: metadata
            .iter()
            .map(|(key, value)| {
                let value = if value.is_null() {
                    None
                } else if let Some(value) = value.as_bool() {
                    Some(ContextValue::Bool(value))
                } else if let Some(value) = value.as_int() {
                    Some(ContextValue::Int(value))
                } else {
                    Some(
                        match vm.get_object(value.as_object_ptr().expect("context primitive")) {
                            Object::String(value) => ContextValue::String(value.to_string()),
                            Object::Float(value) => ContextValue::Float(*value),
                            _ => unreachable!("context metadata must be primitive"),
                        },
                    )
                };
                (key.to_string(), value)
            })
            .collect(),
        distinct_id: distinct_id.map(ToString::to_string),
    }
}

fn alloc_metadata<'a>(
    vm: &mut BexVm,
    entries: impl Iterator<Item = (&'a String, Option<&'a ContextValue>)>,
    nullable: bool,
) -> Value {
    use bex_vm_types::RealizedTy;
    let entries = entries
        .map(|(key, value)| {
            let value = match value {
                None => Value::NULL,
                Some(ContextValue::String(value)) => Value::object(vm.alloc_string(value.as_str())),
                Some(ContextValue::Int(value)) => Value::int(*value),
                Some(ContextValue::Float(value)) => Value::object(vm.alloc_float(*value)),
                Some(ContextValue::Bool(value)) => Value::bool(*value),
            };
            (key.as_str().into(), value)
        })
        .collect();
    let mut members = vec![
        RealizedTy::String,
        RealizedTy::Int,
        RealizedTy::Float,
        RealizedTy::Bool,
    ];
    if nullable {
        members.push(RealizedTy::Null);
    }
    Value::object(vm.alloc_map(
        RealizedTy::String,
        RealizedTy::Union(members.into()),
        entries,
    ))
}

fn alloc_identity(vm: &mut BexVm, identity: Option<&str>) -> Value {
    identity.map_or(Value::NULL, |identity| {
        Value::object(vm.alloc_string(identity))
    })
}

fn alloc_patch(vm: &mut BexVm, patch: Option<&ContextPatch>) -> Value {
    let empty = ContextPatch::default();
    let patch = patch.unwrap_or(&empty);
    let metadata = alloc_metadata(
        vm,
        patch
            .metadata
            .iter()
            .map(|(key, value)| (key, value.as_ref())),
        true,
    );
    let distinct_id = alloc_identity(vm, patch.distinct_id.as_deref());
    copy::ContextOptions {
        distinct_id,
        metadata,
    }
    .to_value(vm)
}

fn alloc_context(vm: &mut BexVm, context: &Context) -> Value {
    let metadata = alloc_metadata(
        vm,
        context
            .metadata()
            .iter()
            .map(|(key, value)| (key, Some(value))),
        false,
    );
    let distinct_id = alloc_identity(vm, context.distinct_id());
    copy::Context {
        distinct_id,
        metadata,
    }
    .to_value(vm)
}

fn compose_context(mut options: TraceOptionsData, patch: ContextPatch) -> TraceOptionsData {
    let mut combined = options.context.as_deref().cloned().unwrap_or_default();
    combined.compose(patch);
    options.context = (!combined.is_empty()).then(|| Arc::new(combined));
    options
}

impl BamlPackageTrace for PackageTraceImpl {
    fn start_span(
        vm: &mut BexVm,
        target: &Value,
        inputs: Option<&IndexMap<bex_str::BexStr, Value>>,
        parent: Option<&Value>,
        metadata: &IndexMap<bex_str::BexStr, Value>,
        distinct_id: Option<&bex_str::BexStr>,
    ) -> Result<Value, VmRustFnError> {
        start_span_impl(vm, *target, inputs, parent, metadata, distinct_id, None)
    }

    fn _passthrough(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
        let trace = vm.passthrough_trace().map_err(|message| {
            VmRustFnError::Panic(crate::errors::VmPanic::UserPanic {
                message: message.to_owned(),
            })
        })?;
        let options = TraceOptionsData {
            mode: trace.telemetry.mode,
            inputs: trace.telemetry.inputs,
            output: trace.telemetry.output,
            error: trace.telemetry.error,
            context: trace.context,
        };
        Ok(match trace.reservation {
            Some(reservation) => copy::ReservedSpan {
                _handle: Arc::new(reservation.claimed_view(options)),
            }
            .to_value(vm),
            None => alloc_options(vm, options),
        })
    }

    fn _create_span(
        vm: &mut BexVm,
        target: &Value,
        inputs: &IndexMap<bex_str::BexStr, Value>,
        config: &Value,
    ) -> Result<Value, VmRustFnError> {
        let trace = vm
            .span_creation_trace(*config)
            .map_err(|message| span_error(vm, message))?;
        start_span_impl(
            vm,
            *target,
            Some(inputs),
            None,
            &IndexMap::new(),
            None,
            Some(&trace),
        )
    }

    fn context(
        vm: &mut BexVm,
        metadata: &IndexMap<bex_str::BexStr, Value>,
        distinct_id: Option<&bex_str::BexStr>,
    ) -> Value {
        let patch = patch_from_values(vm, metadata, distinct_id);
        alloc_options(vm, compose_context(TraceOptionsData::default(), patch))
    }

    fn current_context(vm: &mut BexVm) -> Value {
        let context = vm.current_context().clone();
        alloc_context(vm, &context)
    }

    fn _options(vm: &mut BexVm, mode: Option<&Value>) -> Value {
        let mode = mode_from_value(vm, mode);
        alloc_options(
            vm,
            TraceOptionsData {
                mode,
                ..Default::default()
            },
        )
    }

    fn hidden(vm: &mut BexVm) -> Value {
        alloc_options(
            vm,
            TraceOptionsData {
                mode: Some(InvocationMode::Hidden),
                ..Default::default()
            },
        )
    }

    fn timing(vm: &mut BexVm) -> Value {
        alloc_options(
            vm,
            TraceOptionsData {
                mode: Some(InvocationMode::Timing),
                ..Default::default()
            },
        )
    }

    fn _span(
        vm: &mut BexVm,
        inputs: Option<bool>,
        output: Option<bool>,
        error: Option<bool>,
    ) -> Value {
        alloc_options(
            vm,
            TraceOptionsData {
                mode: Some(InvocationMode::Span),
                inputs,
                output,
                error,
                context: None,
            },
        )
    }

    fn current_span_id(vm: &mut BexVm) -> Option<Value> {
        vm.current_span_id().map(|id| alloc_id(vm, id))
    }
}

#[allow(
    clippy::used_underscore_items,
    reason = "the native _span method implements the BAML internal builder API"
)]
impl BamlClassOptions for PackageTraceImpl {
    fn context(
        vm: &mut BexVm,
        options: &Value,
        metadata: &IndexMap<bex_str::BexStr, Value>,
        distinct_id: Option<&bex_str::BexStr>,
    ) -> Value {
        let options = options_data(vm, *options);
        let patch = patch_from_values(vm, metadata, distinct_id);
        alloc_options(vm, compose_context(options, patch))
    }

    fn mode(vm: &mut BexVm, options: &Value, mode: Option<&Value>) -> Value {
        let mut data = options_data(vm, *options);
        data.mode = mode_from_value(vm, mode);
        alloc_options(vm, data)
    }

    fn _span(
        vm: &mut BexVm,
        options: &Value,
        inputs: Option<bool>,
        output: Option<bool>,
        error: Option<bool>,
    ) -> Value {
        let mut options = options_data(vm, *options);
        options.mode = Some(InvocationMode::Span);
        options.inputs = inputs;
        options.output = output;
        options.error = error;
        alloc_options(vm, options)
    }

    fn inspect(vm: &mut BexVm, options: &Value) -> Value {
        let data = options_data(vm, *options);
        alloc_settings(vm, &data)
    }

    fn reserve(vm: &mut BexVm, options: &Value) -> Value {
        let data = ReservedSpanData::new(vm.trace_scope, options_data(vm, *options));
        copy::ReservedSpan {
            _handle: Arc::new(data),
        }
        .to_value(vm)
    }
}

impl BamlClassReservedSpan for PackageTraceImpl {
    fn id(vm: &mut BexVm, reservedspan: &Value) -> Value {
        let id = reservation_data(vm, *reservedspan).id;
        alloc_id(vm, id)
    }

    fn inspect(vm: &mut BexVm, reservedspan: &Value) -> Value {
        let options = reservation_data(vm, *reservedspan).options.clone();
        alloc_settings(vm, &options)
    }
}

fn span_data(vm: &BexVm, value: Value) -> Arc<crate::telemetry::manual::ManualSpan> {
    use bex_vm_types::RustDataArc as _;
    let instance = vm.as_instance(&value).expect("trace.Span receiver");
    let handle = instance.load_field(0).as_object_ptr().expect("span handle");
    let Object::RustData(data) = vm.get_object(handle) else {
        unreachable!("span handle")
    };
    data.clone().downcast_payload().expect("manual span")
}

pub(crate) fn span_error(vm: &mut BexVm, message: &str) -> VmRustFnError {
    let message = Value::object(vm.alloc_string(message));
    VmRustFnError::thrown_fresh(copy::SpanError { message }.to_value(vm))
}

impl BamlClassSpan for PackageTraceImpl {
    fn id(vm: &mut BexVm, span: &Value) -> Option<Value> {
        span_data(vm, *span).id.map(|id| alloc_id(vm, id))
    }
    fn is_closed(vm: &mut BexVm, span: &Value) -> bool {
        span_data(vm, *span).is_closed()
    }
    fn finish(vm: &mut BexVm, span: &Value, value: &Value) {
        // SAFETY: native entry holds the heap permit and roots the argument.
        unsafe {
            span_data(vm, *span).finish(Some(btel_types::InvocationOutcome::Ok), Some(*value));
        }
    }
    fn fail(vm: &mut BexVm, span: &Value, error: &Value) {
        // SAFETY: native entry holds the heap permit and roots the argument.
        unsafe {
            span_data(vm, *span).finish(Some(btel_types::InvocationOutcome::Errored), Some(*error));
        }
    }
    fn fail_with_context(
        vm: &mut BexVm,
        span: &Value,
        error: &Value,
        context: &Value,
    ) -> Result<(), VmRustFnError> {
        if !vm.is_error_context(*context) {
            return Err(VmRustFnError::Panic(crate::errors::VmPanic::UserPanic {
                message: "Span.fail_with_context requires an existing baml.errors.Context".into(),
            }));
        }
        let original = vm.as_instance(context).expect("validated error Context");
        let stack = original.load_field(1);
        let cause = original.load_field(2);
        // Preserve the supplied stack/cause while recording the supplied error,
        // even when it differs from the Context's original error value.
        let recorded = vm.alloc_error_value(
            bex_vm_types::types::ErrorClass::Context,
            vec![*error, stack, cause],
        );
        let outcome = vm.telemetry_outcome_for_exception(*error);
        unsafe {
            span_data(vm, *span).finish(Some(outcome), Some(recorded));
        }
        Ok(())
    }
    fn close(vm: &mut BexVm, span: &Value) {
        // SAFETY: no value is traversed.
        unsafe {
            span_data(vm, *span).finish(None, None);
        }
    }
    fn _activate(vm: &mut BexVm, span: &Value) -> Result<(), VmRustFnError> {
        let span = span_data(vm, *span);
        vm.activate_manual_span(&span)
            .map_err(|message| span_error(vm, message))
    }
}

#[allow(clippy::too_many_arguments)]
fn start_span_impl(
    vm: &mut BexVm,
    target: Value,
    inputs: Option<&IndexMap<bex_str::BexStr, Value>>,
    parent: Option<&Value>,
    metadata: &IndexMap<bex_str::BexStr, Value>,
    distinct_id: Option<&bex_str::BexStr>,
    trace: Option<&crate::vm::CallTrace>,
) -> Result<Value, VmRustFnError> {
    (|| -> Result<Value, String> {
        let invalid = |message: &str| message.to_owned();
        let mut options = crate::telemetry::manual::OperationOptions::default();
        let mut captured_type_args = None;
        let definition = if let Ok(name) = vm.as_string(&target) {
            if name.is_empty() {
                return Err(invalid("span name must not be empty"));
            }
            btel_types::FunctionMetadata {
                function_id: vm
                    .heap
                    .named_span_function_id(name)
                    .map_err(|_| invalid("function identities exhausted"))?,
                definition_key: Some(btel_types::DefinitionKey(format!("manual:{name}"))),
                fqn: name.to_string(),
                display_name: name.to_string(),
                source_file: None,
                source_span: None,
                kind: btel_types::RuntimeFunctionKind::Native,
                origin: btel_types::RuntimeFunctionOrigin::UserDefined,
                owner_type: None,
                parent_function: None,
                lambda_path: None,
                package_name: None,
                namespace: vec![],
                argument_layout: None,
                source_map: None,
                model_invoke: false,
            }
        } else {
            let function = target
                .as_object_ptr()
                .and_then(|ptr| vm.callable_function_identity(ptr))
                .ok_or_else(|| invalid("span target must be a name or a BAML function"))?;
            // SAFETY: the callable argument roots its definition under the native heap permit.
            let Object::Function(function) = (unsafe { function.get() }) else {
                return Err(invalid("invalid function definition"));
            };
            if let Some(state) = &vm.telemetry {
                options = state.operation_options(
                    function,
                    trace.as_ref().map(|trace| trace.telemetry).unwrap_or(
                        crate::telemetry::TraceConfig {
                            mode: Some(btel_types::InvocationMode::Span),
                            inputs: Some(true),
                            output: Some(true),
                            error: Some(true),
                            reserved_id: None,
                        },
                    ),
                );
            }
            if options.mode == btel_types::InvocationMode::Span && options.inputs {
                let type_args = vm
                    .callable_function_and_type_args(target)
                    .map(|(_, args)| args.to_vec())
                    .unwrap_or_default();
                if !type_args.is_empty() {
                    captured_type_args = vm.telemetry.as_mut().and_then(|state| {
                        state.capture_type_args(
                            function,
                            crate::telemetry::CallTypeArgs {
                                carried: &type_args,
                                passed: &[],
                            },
                        )
                    });
                }
            }
            function
                .runtime_metadata()
                .ok_or_else(|| invalid("function has no tracing definition"))?
        };
        let parent = parent.map(|value| span_data(vm, *value));
        if parent
            .as_ref()
            .is_some_and(|parent| parent.scope != vm.trace_scope || parent.is_closed())
        {
            return Err(invalid(
                "parent span is closed or belongs to another runtime",
            ));
        }
        let context = parent
            .as_ref()
            .map_or_else(
                || vm.current_context().clone(),
                |parent| parent.context.clone(),
            )
            .with_patch(&patch_from_values(vm, metadata, distinct_id));
        let context = trace
            .as_ref()
            .and_then(|trace| trace.context.as_deref())
            .map_or_else(|| context.clone(), |patch| context.with_patch(patch));
        let inputs = inputs.map(|inputs| {
            Value::object(vm.alloc_map(
                bex_vm_types::RealizedTy::String,
                bex_vm_types::RealizedTy::Unknown,
                inputs.clone(),
            ))
        });
        // SAFETY: native builtins hold the VM heap permit; inputs remain rooted.
        let span = unsafe {
            crate::telemetry::manual::ManualSpan::start_with_options(
                vm.telemetry.as_mut(),
                vm.trace_scope,
                context,
                &definition,
                inputs,
                parent.as_deref(),
                options,
                captured_type_args,
            )
        };
        Ok(copy::Span {
            _handle: span.into_owner(),
        }
        .to_value(vm))
    })()
    .map_err(|message| span_error(vm, &message))
}
