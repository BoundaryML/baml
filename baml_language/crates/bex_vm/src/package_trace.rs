//! Native storage and identity operations for immutable trace builders.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::types::{Instance, Object, Type, Value};
use btel_types::InvocationMode;

use crate::{
    BexVm,
    errors::VmInternalError,
    package_baml::{NativeCallResult, NativeFunction, NativeFunctionResult},
    trace::{SpanId, TraceCall, TraceOptions},
};

#[allow(
    unused_variables,
    unsafe_code,
    non_camel_case_types,
    non_snake_case,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
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

fn alloc_handle<T: Send + Sync + 'static>(vm: &mut BexVm, name: &str, data: T) -> Value {
    let class = vm.resolve_class(name);
    let handle = Value::object(vm.alloc_rust_data(Arc::new(data)));
    Value::object(vm.alloc_instance(class, vec![handle]))
}

fn data<T: 'static>(vm: &BexVm, value: Value) -> &T {
    let instance = vm
        .as_instance(&value)
        .expect("trace receiver must be an instance");
    vm.as_rust_data(&instance.load_field(0))
        .expect("trace receiver must hold its native data")
}

fn decode_mode(vm: &BexVm, value: Option<&Value>) -> Option<InvocationMode> {
    value.map(|value| {
        let ptr = value.as_object_ptr().expect("trace.Mode must be a variant");
        let Object::Variant(variant) = vm.get_object(ptr) else {
            panic!("trace.Mode must be a variant");
        };
        match variant.index {
            0 => InvocationMode::Hidden,
            1 => InvocationMode::Timing,
            2 => InvocationMode::Span,
            _ => panic!("unknown trace.Mode variant"),
        }
    })
}

fn inspect(vm: &mut BexVm, options: TraceOptions) -> Value {
    let mode = options.mode.map_or(Value::NULL, |mode| {
        let index = match mode {
            InvocationMode::Hidden => 0,
            InvocationMode::Timing => 1,
            InvocationMode::Span => 2,
        };
        let enm = vm.resolve_class("trace.Mode");
        Value::object(vm.alloc_variant(enm, index))
    });
    let capture_class = vm.resolve_class("trace.Capture");
    let capture = Value::object(vm.alloc_instance(
        capture_class,
        vec![
            options.inputs.map_or(Value::NULL, Value::bool),
            options.output.map_or(Value::NULL, Value::bool),
            options.error.map_or(Value::NULL, Value::bool),
        ],
    ));
    let settings_class = vm.resolve_class("trace.Settings");
    Value::object(vm.alloc_instance(settings_class, vec![mode, capture]))
}

impl BamlPackageTrace for PackageTraceImpl {
    fn _options(vm: &mut BexVm, mode: Option<&Value>) -> Value {
        let options = TraceOptions {
            mode: decode_mode(vm, mode),
            ..TraceOptions::default()
        };
        alloc_handle(vm, "trace.Options", options)
    }

    fn current_span_id(vm: &mut BexVm) -> Option<Value> {
        vm.current_span_id()
            .map(|id| alloc_handle(vm, "trace.SpanId", SpanId(id)))
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassOptions for PackageTraceImpl {
    fn mode(vm: &mut BexVm, options: &Value, mode: Option<&Value>) -> Value {
        let mut options = *data::<TraceOptions>(vm, *options);
        options.mode = decode_mode(vm, mode);
        alloc_handle(vm, "trace.Options", options)
    }

    fn _span(
        vm: &mut BexVm,
        _options: &Value,
        inputs: Option<bool>,
        output: Option<bool>,
        error: Option<bool>,
    ) -> Value {
        alloc_handle(
            vm,
            "trace.Options",
            TraceOptions {
                mode: Some(InvocationMode::Span),
                inputs,
                output,
                error,
            },
        )
    }

    fn inspect(vm: &mut BexVm, options: &Value) -> Value {
        let options = *data::<TraceOptions>(vm, *options);
        inspect(vm, options)
    }

    fn reserve(vm: &mut BexVm, options: &Value) -> Value {
        let call = TraceCall::reserve(*data::<TraceOptions>(vm, *options), &vm.heap);
        alloc_handle(vm, "trace.ReservedSpan", call)
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassReservedSpan for PackageTraceImpl {
    fn id(vm: &mut BexVm, reservedspan: &Value) -> Value {
        let id = data::<TraceCall>(vm, *reservedspan)
            .id()
            .expect("ReservedSpan must have a reservation");
        alloc_handle(vm, "trace.SpanId", SpanId(id))
    }

    fn inspect(vm: &mut BexVm, reservedspan: &Value) -> Value {
        let options = data::<TraceCall>(vm, *reservedspan).options;
        inspect(vm, options)
    }
}

#[allow(clippy::used_underscore_items)]
impl BamlClassSpanId for PackageTraceImpl {
    fn _equals(vm: &BexVm, spanid: &view::SpanId<'_>, other: &Value) -> bool {
        let other = vm.as_instance(other).expect("SpanId expected");
        let other = vm
            .as_rust_data::<SpanId>(&other.load_field(0))
            .expect("SpanId must hold an identity");
        spanid._handle::<SpanId>(vm) == other
    }
}
