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
    errors::{VmBamlError, VmInternalError},
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

fn alloc_settings(vm: &mut BexVm, options: &TraceOptionsData) -> Value {
    let mode = options.mode.map_or(Value::NULL, |mode| {
        let name = match mode {
            InvocationMode::Hidden => "Hidden",
            InvocationMode::Timing => "Timing",
            InvocationMode::Span => "Span",
        };
        let enm = vm.lookup_type_by_fqn("trace.Mode").expect("trace.Mode");
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

    fn _usage_target(vm: &mut BexVm) -> i64 {
        vm.usage_target().map_or(0, |id| id.get().cast_signed())
    }

    #[allow(clippy::too_many_arguments, reason = "mirrors ai.events.Usage")]
    fn _record_usage(
        vm: &mut BexVm,
        target: i64,
        model: Option<&bex_str::BexStr>,
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: Option<i64>,
        cache_write_tokens: Option<i64>,
        reasoning_tokens: Option<i64>,
    ) -> bool {
        let tokens = |count: i64| u64::try_from(count).unwrap_or(0);
        vm.record_model_usage(
            target.cast_unsigned(),
            model.map(|model| Box::from(model.as_str())),
            tokens(input_tokens),
            tokens(output_tokens),
            cache_read_tokens.map(tokens),
            cache_write_tokens.map(tokens),
            reasoning_tokens.map(tokens),
        )
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
