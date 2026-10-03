use std::sync::Arc;

use js_sys::{Function, Promise, Reflect, Uint8Array};
use sys_ops::io::{self, IoNamespaceSys};
use sys_types::{
    BexExternalValue, BexHeap, CallId, SysOpContext, SysOpOutput, VmBamlError, VmPanic,
    VmRustFnError,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::JsFuture;

use crate::send_wrapper::{SendFuture, SendWrapper};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "setTimeout")]
    fn set_timeout(closure: &js_sys::Function, millis: i32) -> i32;
}

pub(crate) struct WasmSys {
    exec_fn: SendWrapper<Function>,
    shell_fn: SendWrapper<Function>,
}

impl WasmSys {
    pub(crate) fn new(exec_fn: Function, shell_fn: Function) -> Self {
        Self {
            exec_fn: SendWrapper::new(exec_fn),
            shell_fn: SendWrapper::new(shell_fn),
        }
    }
}

/// Unpack a JS result object `{ exit_code, stdout_bytes, stderr_bytes }`
/// into an `owned::sys::ProcessOutput`.
fn unpack_shell_result(obj: &JsValue) -> Result<io::owned::sys::ProcessOutput, VmBamlError> {
    let exit_code_f64 = Reflect::get(obj, &"exit_code".into())
        .map_err(|e| VmBamlError::Io {
            message: format!("missing exit_code: {e:?}"),
        })?
        .as_f64()
        .unwrap_or(-1.0);
    // `as i64` for f64 is saturating: NaN → 0, ±inf → i64 extremes,
    // fractionals → truncated toward zero. A NaN exit code would
    // silently become 0 (success). `FromPrimitive::from_f64` returns
    // `None` exactly when the value is non-finite, out of `i64` range,
    // or non-integer — for those, fall back to `-1` (the same sentinel
    // the `unwrap_or` above uses when `exit_code` is missing entirely).
    let exit_code = <i64 as num_traits::FromPrimitive>::from_f64(exit_code_f64).unwrap_or(-1);

    let stdout = Reflect::get(obj, &"stdout_bytes".into())
        .ok()
        .and_then(|v| v.dyn_into::<Uint8Array>().ok())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    let stderr = Reflect::get(obj, &"stderr_bytes".into())
        .ok()
        .and_then(|v| v.dyn_into::<Uint8Array>().ok())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    Ok(io::owned::sys::ProcessOutput {
        stdout,
        stderr,
        exit_code,
        signal: None,
    })
}

/// Serialize `_ProcessOptions` to a JSON string for the JS callback.
fn options_to_js(options: &io::owned::sys::ProcessOptions) -> Result<JsValue, VmRustFnError> {
    let obj = js_sys::Object::new();
    if let Some(ref cwd) = options.cwd {
        let _ = Reflect::set(&obj, &"cwd".into(), &cwd.into());
    }
    let _ = Reflect::set(
        &obj,
        &"inherit_env".into(),
        &JsValue::from_bool(options.inherit_env),
    );
    if let Some(ref env) = options.env {
        let env_obj = js_sys::Object::new();
        for (name, value) in env {
            let value = value.as_ref().map_or(JsValue::NULL, Into::into);
            let _ = Reflect::set(&env_obj, &name.into(), &value);
        }
        let _ = Reflect::set(&obj, &"env".into(), &env_obj.into());
    }
    if let Some(timeout) = &options.timeout {
        let _ = Reflect::set(
            &obj,
            &"timeout_ms".into(),
            &JsValue::from_f64(process_timeout_millis(timeout)?),
        );
    }
    if let Some(input) = &options.input {
        let _ = Reflect::set(&obj, &"input".into(), &process_input_js(input));
    }
    for (name, value) in [
        ("stdin", &options.stdin),
        ("stdout", &options.stdout),
        ("stderr", &options.stderr),
    ] {
        fn mode(value: &BexExternalValue) -> &str {
            match value {
                BexExternalValue::String(value) => value.as_str(),
                BexExternalValue::Union { value, .. } => mode(value),
                _ => "inherit",
            }
        }
        let _ = Reflect::set(&obj, &name.into(), &mode(value).into());
    }
    Ok(js_sys::JSON::stringify(&obj)
        .map(JsValue::from)
        .unwrap_or(JsValue::NULL))
}

impl io::IoClassSysReadPipe for WasmSys {
    fn read(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _readpipe: io::owned::sys::ReadPipe,
        _limit: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<Vec<u8>>> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _readpipe: io::owned::sys::ReadPipe,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassSysWritePipe for WasmSys {
    fn write_some(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _writepipe: io::owned::sys::WritePipe,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn flush(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _writepipe: io::owned::sys::WritePipe,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _writepipe: io::owned::sys::WritePipe,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassSysSubprocess for WasmSys {
    fn wait(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Subprocess,
        _timeout: Option<BexExternalValue>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ProcessExit> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn kill(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Subprocess,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Subprocess,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::ok(())
    }
}

impl IoNamespaceSys for WasmSys {
    fn current_dir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "native-host".into(),
            message: "Native host information is not supported by this host".into(),
        })
    }
    fn current_exe(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "native-host".into(),
            message: "Native host information is not supported by this host".into(),
        })
    }
    fn home_dir(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "native-host".into(),
            message: "Native host information is not supported by this host".into(),
        })
    }
    fn platform(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "native-host".into(),
            message: "Native host information is not supported by this host".into(),
        })
    }
    fn host_target(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "native-host".into(),
            message: "Native host information is not supported by this host".into(),
        })
    }

    fn collect_garbage(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // BexEngine intercepts this operation before ordinary sys-op dispatch
        // so it can release the calling VM's heap permit and coordinate a
        // stop-the-world collection. This fallback keeps the generated IO
        // contract complete for alternate dispatchers.
        SysOpOutput::ok(())
    }

    fn _run(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        program: String,
        args: Vec<String>,
        options: io::owned::sys::ProcessOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ProcessOutput> {
        let exec_fn = self.exec_fn.clone();
        SysOpOutput::async_op(SendFuture(async move {
            let program_js: JsValue = program.into();
            let arr = js_sys::Array::new();
            for arg in args {
                arr.push(&arg.into());
            }
            let args_js: JsValue = arr.into();
            let options_js = options_to_js(&options)?;

            let result = exec_fn
                .call3(&JsValue::NULL, &program_js, &args_js, &options_js)
                .map_err(|e| VmBamlError::Io {
                    message: format!("exec callback failed: {e:?}"),
                })?;

            let promise: Promise = result.dyn_into().map_err(|_| VmBamlError::Io {
                message: "exec callback did not return a Promise".into(),
            })?;
            let obj = JsFuture::from(promise).await.map_err(|e| VmBamlError::Io {
                message: format!("exec callback rejected: {e:?}"),
            })?;

            unpack_shell_result(&obj).map_err(VmRustFnError::from)
        }))
    }

    fn _subprocess(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _program: String,
        _args: Vec<String>,
        _options: io::owned::sys::ProcessOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::Subprocess> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process".to_string(),
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn _shell(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        command: String,
        options: io::owned::sys::ProcessOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ProcessOutput> {
        let shell_fn = self.shell_fn.clone();
        SysOpOutput::async_op(SendFuture(async move {
            let command_js: JsValue = command.into();
            let options_js = options_to_js(&options)?;

            let result = shell_fn
                .call2(&JsValue::NULL, &command_js, &options_js)
                .map_err(|e| VmBamlError::Io {
                    message: format!("shell callback failed: {e:?}"),
                })?;

            let promise: Promise = result.dyn_into().map_err(|_| VmBamlError::Io {
                message: "shell callback did not return a Promise".into(),
            })?;
            let obj = JsFuture::from(promise).await.map_err(|e| VmBamlError::Io {
                message: format!("shell callback rejected: {e:?}"),
            })?;

            unpack_shell_result(&obj).map_err(VmRustFnError::from)
        }))
    }

    fn sleep(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        delay: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let millis = match sleep_millis_from_delay(delay) {
            Ok(millis) => millis,
            Err(err) => return SysOpOutput::err(err),
        };
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            set_timeout(&resolve, millis);
        });
        SysOpOutput::async_op(SendFuture(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            Ok(())
        }))
    }

    fn pid(&self, _heap: &Arc<BexHeap>, _call_id: CallId, _ctx: &SysOpContext) -> SysOpOutput<i64> {
        // Read `globalThis.process.pid` directly rather than through an
        // injected callback — the same feature-detection route `WasmTime`
        // takes to `globalThis.Temporal`. Node and Node-compatible runtimes
        // provide it; a browser does not, and neither do the `process` shims
        // bundlers inject, which is why a non-positive or non-integral value
        // is treated as absent (no live process ever has PID 0).
        match host_process_pid() {
            Some(pid) => SysOpOutput::ok(pid),
            // `baml.sys.pid` declares `throws never`, so an environment
            // without process IDs panics rather than throwing.
            None => SysOpOutput::err(VmPanic::HostUnavailable {
                resource: "process-id".to_string(),
                message: "the host JavaScript environment does not provide process.pid".to_string(),
            }),
        }
    }
}

/// `globalThis.process.pid`, or `None` when the host does not expose a usable
/// process ID.
fn host_process_pid() -> Option<i64> {
    let process = Reflect::get(&js_sys::global(), &"process".into()).ok()?;
    let pid = Reflect::get(&process, &"pid".into()).ok()?.as_f64()?;
    // `from_f64` rejects non-finite, out-of-range, and fractional values, so
    // only a genuine integral PID survives.
    <i64 as num_traits::FromPrimitive>::from_f64(pid).filter(|pid| *pid > 0)
}

fn sleep_millis_from_delay(delay: BexExternalValue) -> Result<i32, VmRustFnError> {
    match delay {
        BexExternalValue::Instance {
            class_name,
            mut fields,
            ..
        } if class_name == "baml.time.Duration" => {
            let Some(nanos) = fields.swap_remove("_nanoseconds") else {
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: "sleep delay is missing Duration._nanoseconds".to_string(),
                }));
            };
            let BexExternalValue::Bigint(nanos) = nanos else {
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: "sleep delay Duration._nanoseconds is not a bigint".to_string(),
                }));
            };
            if nanos.sign() == num_bigint::Sign::Plus {
                let nanos = u64::try_from(&nanos).unwrap_or(u64::MAX);
                let rounded_millis = nanos.saturating_add(999_999) / 1_000_000;
                Ok(i32::try_from(rounded_millis).unwrap_or(i32::MAX))
            } else {
                Ok(0)
            }
        }
        BexExternalValue::Union { value, .. } => sleep_millis_from_delay(*value),
        other => Err(VmRustFnError::from(VmBamlError::Io {
            message: format!(
                "sleep delay must be baml.time.Duration, got {}",
                other.type_name()
            ),
        })),
    }
}

fn process_input_js(input: &BexExternalValue) -> JsValue {
    match input {
        BexExternalValue::String(text) => text.as_str().into(),
        BexExternalValue::Uint8Array(bytes) => {
            let array = js_sys::Array::new();
            for byte in bytes {
                array.push(&JsValue::from_f64(f64::from(*byte)));
            }
            array.into()
        }
        BexExternalValue::Union { value, .. } => process_input_js(value),
        _ => JsValue::NULL,
    }
}

fn process_timeout_millis(value: &BexExternalValue) -> Result<f64, VmRustFnError> {
    if let BexExternalValue::Union { value, .. } = value {
        return process_timeout_millis(value);
    }
    if let BexExternalValue::Instance { fields, .. } = value {
        if let Some(BexExternalValue::Bigint(nanos)) = fields.get("_nanoseconds") {
            let nanos = u64::try_from(nanos).map_err(|_| VmBamlError::InvalidArgument {
                message: "Process timeout must be nonnegative and fit in 64-bit nanoseconds".into(),
            })?;
            // u64 nanoseconds produce at most 2^45 milliseconds: exactly representable in JS.
            return Ok(num_traits::ToPrimitive::to_f64(&nanos.div_ceil(1_000_000))
                .expect("milliseconds fit in f64"));
        }
    }
    Err(VmBamlError::InvalidArgument {
        message: "Expected a Duration process timeout".into(),
    }
    .into())
}
