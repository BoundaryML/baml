//! Native provider calls exchange owned values; futures contain no rt pointers.
use crate::{Error, R};
use baml_rt_tasks::{OwnedValue, Value};
pub use baml_type;
pub use bex_external_types::{AsBexExternalValue, BexExternalValue, WireTy};
pub use indexmap;
pub use sys_native::io::{AccessError, owned};
pub use sys_types::{CallId, OpError, OpErrorPayload, SysOp, SysOpContext, SysOpResult};

pub fn native() -> sys_native::NativeSysOps {
    sys_native::NativeSysOps::new(sys_native::WorkingDir::Process)
}
pub fn context() -> SysOpContext {
    SysOpContext::empty()
}
pub fn perform(
    make: impl FnOnce() -> SysOpResult,
    on_error: impl FnOnce(OpError, bool) -> Box<Error>,
) -> R<BexExternalValue> {
    let started = crate::telemetry::sysop_started();
    let mut asynchronous = false;
    let result: Result<Result<BexExternalValue, OpError>, Box<Error>> = match make() {
        SysOpResult::Ready(result) => Ok(result),
        SysOpResult::Async(future) => {
            asynchronous = true;
            let (sender, mut receiver) = tokio::sync::oneshot::channel();
            crate::telemetry::before_yield();
            let outcome = unsafe { &mut *crate::TASK.get() }.io(move |_| async move {
                let _ = sender.send(future.await);
                Ok(OwnedValue::Unit)
            });
            match outcome {
                Ok(Value::Owned(OwnedValue::Unit)) => receiver.try_recv().map_err(|e| {
                    Error::internal(format!("native IO completion lost its owned result: {e}"))
                }),
                Err(baml_rt_tasks::Err::Cancelled) => Err(crate::language::from_panic(
                    bex_vm_types::errors::VmPanic::Cancelled,
                )),
                other => Err(Error::internal(format!(
                    "invalid native IO completion: {other:?}"
                ))),
            }
        }
    };
    crate::telemetry::sysop_finished(started);
    result?.map_err(|error| on_error(error, asynchronous))
}
fn string(s: String) -> BexExternalValue {
    BexExternalValue::String(s.into())
}
fn instance(
    name: &str,
    fields: impl IntoIterator<Item = (&'static str, BexExternalValue)>,
) -> BexExternalValue {
    BexExternalValue::Instance {
        class_name: name.into(),
        type_args: Vec::new(),
        fields: fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }
}
fn display(value: &BexExternalValue) -> String {
    use BexExternalValue as V;
    match value {
        V::Null => "null".into(),
        V::Int(v) => v.to_string(),
        V::Bigint(v) => v.to_string(),
        V::Bool(v) => v.to_string(),
        V::Float(v) => v.to_string(),
        V::String(v) => format!("{:?}", v.as_str()),
        V::Variant {
            enum_name,
            variant_name,
        } => format!("{enum_name}.{variant_name}"),
        V::Instance {
            class_name, fields, ..
        } => format!(
            "{class_name} {{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", display(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        V::Array { items, .. } => format!(
            "[{}]",
            items.iter().map(display).collect::<Vec<_>>().join(", ")
        ),
        V::Union { value, .. } => display(value),
        _ => value.type_name().into(),
    }
}
pub fn error(
    error: OpError,
    schemas: &[(&'static str, &'static crate::external::Type)],
) -> Box<Error> {
    use bex_vm_types::errors::{VmBamlError as E, VmRustFnError as V};
    let (class, message, owned) = match error.payload {
        OpErrorPayload::Vm(V::Panic(p)) => return crate::language::from_panic(p),
        OpErrorPayload::Vm(V::BamlError(e)) => {
            let (class, message, fields) = match e {
                E::InvalidArgument { message } => {
                    ("baml.errors.InvalidArgument", message, Vec::new())
                }
                E::ParseError { message } => ("baml.errors.ParseError", message, Vec::new()),
                E::Io { message } => ("baml.errors.Io", message, Vec::new()),
                E::Unsupported { message } => ("baml.errors.Unsupported", message, Vec::new()),
                E::AccessError { message } => ("baml.errors.AccessError", message, Vec::new()),
                E::RenderPrompt { message } => ("baml.errors.RenderPrompt", message, Vec::new()),
                E::LlmClient { message } => ("baml.errors.LlmClient", message, Vec::new()),
                E::Timeout {
                    message,
                    duration,
                    timeout_type,
                } => {
                    let duration = duration.map_or(BexExternalValue::Null, |d| {
                        instance(
                            "baml.time.Duration",
                            [(
                                "_nanoseconds",
                                BexExternalValue::Bigint(d.as_nanos().into()),
                            )],
                        )
                    });
                    (
                        "baml.errors.Timeout",
                        message,
                        vec![
                            ("duration", duration),
                            ("timeout_type", string(timeout_type)),
                        ],
                    )
                }
                e => return Error::internal(format!("unsupported native host exception: {e}")),
            };
            let owned = instance(
                class,
                std::iter::once(("message", string(message.clone()))).chain(fields),
            );
            (class, message, owned)
        }
        OpErrorPayload::Vm(error) => return Error::internal(error.to_string()),
        payload => return Error::internal(format!("unsupported native completion: {payload:?}")),
    };
    let Some((_, schema)) = schemas.iter().find(|(name, _)| *name == class) else {
        return Error::internal(format!("missing native exception schema {class}"));
    };
    let value = match unsafe { schema.hydrate(&owned, 0) } {
        Ok(value) => value,
        Err(error) => return error,
    };
    Box::new(Error {
        telemetry_history: Default::default(),
        thrown_value: None,
        exit_code: None,
        context: 0,
        cause: 0,
        trace: None,
        diagnostic_value: None,
        telemetry_caught: None,
        value: value.payload as usize,
        class,
        message,
        frames: Vec::new(),
        telemetry_at: None,
        telemetry_raise: None,
        display_value: Some(display(&owned)),
    })
}
