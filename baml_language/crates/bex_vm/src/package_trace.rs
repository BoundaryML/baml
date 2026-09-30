//! Immutable tracing builders and detached inspection snapshots.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    trace::{ReservedSpanData, SpanId, TraceOptionsData},
    types::{Instance, Object, Type, Value},
};
use btel_types::InvocationMode;

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
    *vm.as_rust_data::<TraceOptionsData>(&instance.load_field(0))
        .expect("trace.Options handle")
}

fn reservation_data(vm: &BexVm, value: Value) -> &ReservedSpanData {
    let instance = vm.as_instance(&value).expect("trace.ReservedSpan receiver");
    vm.as_rust_data::<ReservedSpanData>(&instance.load_field(0))
        .expect("trace.ReservedSpan handle")
}

fn span_id_data(vm: &BexVm, value: Value) -> SpanId {
    let instance = vm.as_instance(&value).expect("trace.SpanId value");
    *vm.as_rust_data::<SpanId>(&instance.load_field(0))
        .expect("trace.SpanId handle")
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

fn alloc_settings(vm: &mut BexVm, options: TraceOptionsData) -> Value {
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
    copy::Settings { mode, capture }.to_value(vm)
}

impl BamlPackageTrace for PackageTraceImpl {
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
            },
        )
    }

    fn current_span_id(vm: &mut BexVm) -> Option<Value> {
        vm.current_span_id().map(|id| alloc_id(vm, id))
    }

    fn _usage_target(vm: &mut BexVm) -> i64 {
        vm.usage_target().map_or(0, |id| id.get().cast_signed())
    }

    fn _usage_target_of(vm: &mut BexVm, span: &Value) -> i64 {
        span_id_data(vm, *span)
            .local_in(vm.trace_scope)
            .map_or(0, |id| id.get().cast_signed())
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
    fn mode(vm: &mut BexVm, options: &Value, mode: Option<&Value>) -> Value {
        let mut data = options_data(vm, *options);
        data.mode = mode_from_value(vm, mode);
        alloc_options(vm, data)
    }

    fn _span(
        vm: &mut BexVm,
        _options: &Value,
        inputs: Option<bool>,
        output: Option<bool>,
        error: Option<bool>,
    ) -> Value {
        <Self as BamlPackageTrace>::_span(vm, inputs, output, error)
    }

    fn inspect(vm: &mut BexVm, options: &Value) -> Value {
        let data = options_data(vm, *options);
        alloc_settings(vm, data)
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
        let options = reservation_data(vm, *reservedspan).options;
        alloc_settings(vm, options)
    }
}
