//! Owned I/O requests. Futures never capture rt handles or touch the heap.
use crate::{Error, R};
use baml_rt_tasks::{OwnedValue, Value};
use serde_json::Value as Owned;
use std::{future::Future, pin::Pin};

pub mod clock;
pub mod fs;

pub struct Failure {
    pub class: &'static str,
    pub message: String,
    pub resource: String,
}
pub enum Output {
    Ready(Result<Owned, Failure>),
    Async(Pin<Box<dyn Future<Output = Result<Owned, Failure>> + Send>>),
}
impl Failure {
    fn io(message: String) -> Self {
        Self {
            class: "Io",
            message,
            resource: String::new(),
        }
    }
}
fn failure(error: Failure) -> Box<Error> {
    match error.class {
        "ParseError" => crate::language::panic_value(11, "baml.errors.ParseError", error.message),
        "Io" => crate::language::panic_value(13, "baml.errors.Io", error.message),
        "HostUnavailable" => host_unavailable(error.resource, error.message),
        _ => Error::internal(error.message),
    }
}
fn host_unavailable(resource: String, message: String) -> Box<Error> {
    let mut cx = unsafe { crate::strings::Cx::current() };
    let result = (|| -> R<usize> {
        let resource = cx.string(&resource)?;
        let message = cx.string(&message)?;
        let object = unsafe { crate::try_alloc(12, 24) }?;
        unsafe {
            baml_rt_gc::write_ptr(object, object.add(8).cast(), resource.as_raw());
            baml_rt_gc::write_ptr(object, object.add(16).cast(), message.as_raw());
        }
        Ok(object as usize)
    })();
    match result {
        Ok(value) => Box::new(Error {
            value,
            class: "baml.panics.HostUnavailable",
            message: message.clone(),
            frames: vec![],
            telemetry_at: None,
            telemetry_raise: None,
            display_value: Some(format!(
                "baml.panics.HostUnavailable {{resource: {resource:?}, message: {message:?}}}"
            )),
        }),
        Err(error) => error,
    }
}

/// Run a heap-free sysop, then hydrate its owned result on the owner thread.
pub fn perform(output: Output) -> R<Owned> {
    let started = crate::telemetry::sysop_started();
    let result = match output {
        Output::Ready(result) => result.map_err(failure),
        Output::Async(future) => {
            crate::telemetry::before_yield();
            let result = unsafe { &mut *crate::TASK.get() }.io(move |_| async move {
                let value = match future.await {
                    Ok(value) => serde_json::json!({"value":value}),
                    Err(e) => serde_json::json!({"error":e.class,"message":e.message,"resource":e.resource}),
                };
                Ok(OwnedValue::Text(value.to_string()))
            });
            match result {
                Ok(Value::Owned(OwnedValue::Text(text))) => {
                    let mut value: Owned =
                        serde_json::from_str(&text).expect("owned I/O completion JSON");
                    if let Some(class) = value["error"].as_str() {
                        let class = match class {
                            "Io" => "Io",
                            "ParseError" => "ParseError",
                            "HostUnavailable" => "HostUnavailable",
                            _ => "InternalError",
                        };
                        Err(failure(Failure {
                            class,
                            message: value["message"].as_str().unwrap_or("").into(),
                            resource: value["resource"].as_str().unwrap_or("").into(),
                        }))
                    } else {
                        Ok(value["value"].take())
                    }
                }
                Err(baml_rt_tasks::Err::Cancelled) => Err(crate::language::from_panic(
                    bex_vm_types::errors::VmPanic::Cancelled,
                )),
                other => Err(Error::internal(format!(
                    "invalid I/O completion: {other:?}"
                ))),
            }
        }
    };
    crate::telemetry::sysop_finished(started);
    result
}

/// Typed, heap-free entry points used by generated I/O adapters.
pub fn env_get(key: String) -> Output {
    Output::Ready(
        match baml_env::os_var(&key).map(std::ffi::OsString::into_string) {
            Some(Ok(v)) => Ok(Owned::String(v)),
            None => Ok(Owned::Null),
            Some(Err(_)) => Err(Failure {
                class: "ParseError",
                message: format!("Environment variable '{key}' is not valid UTF-8"),
                resource: String::new(),
            }),
        },
    )
}
pub fn pid() -> Output {
    Output::Ready(Ok(Owned::from(std::process::id())))
}
pub fn platform() -> Output {
    Output::Ready(Ok(Owned::from(std::env::consts::OS)))
}
fn path_string(path: std::path::PathBuf) -> Result<Owned, Failure> {
    path.to_str()
        .map(|s| Owned::from(s.to_owned()))
        .ok_or_else(|| Failure::io("Path is not valid UTF-8".into()))
}
pub fn home_dir() -> Output {
    Output::Ready(
        baml_env::os_var("HOME")
            .or_else(|| baml_env::os_var("USERPROFILE"))
            .map(|p| path_string(p.into()))
            .transpose()
            .map(|v| v.unwrap_or(Owned::Null)),
    )
}
pub fn current_dir() -> Output {
    Output::Ready(
        std::env::current_dir()
            .map_err(|e| Failure::io(e.to_string()))
            .and_then(path_string),
    )
}
pub fn current_exe() -> Output {
    Output::Ready(
        std::env::current_exe()
            .map_err(|e| Failure::io(e.to_string()))
            .and_then(path_string),
    )
}
pub fn print(text: String) -> Output {
    write(text, false, false)
}
pub fn println(text: String) -> Output {
    write(text, false, true)
}
pub fn eprint(text: String) -> Output {
    write(text, true, false)
}
pub fn eprintln(text: String) -> Output {
    write(text, true, true)
}
fn write(mut text: String, stderr: bool, newline: bool) -> Output {
    if newline {
        text.push('\n');
    }
    Output::Async(Box::pin(async move {
        use tokio::io::AsyncWriteExt;
        let stream = if stderr { "stderr" } else { "stdout" };
        let write_error = |e| Failure {
            class: "HostUnavailable",
            resource: stream.into(),
            message: format!("failed to write {stream}: {e}"),
        };
        let flush_error = |e| Failure {
            class: "HostUnavailable",
            resource: stream.into(),
            message: format!("failed to flush {stream}: {e}"),
        };
        if stderr {
            let mut out = tokio::io::stderr();
            out.write_all(text.as_bytes()).await.map_err(write_error)?;
            out.flush().await.map_err(flush_error)?;
        } else {
            let mut out = tokio::io::stdout();
            out.write_all(text.as_bytes()).await.map_err(write_error)?;
            out.flush().await.map_err(flush_error)?;
        }
        Ok(Owned::Null)
    }))
}

pub fn host_target() -> Output {
    Output::Ready(Ok(Owned::from(env!("BAML_HOST_TARGET"))))
}
