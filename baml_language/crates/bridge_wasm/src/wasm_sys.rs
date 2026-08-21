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
/// into an `owned::sys::ShellOutput`.
fn unpack_shell_result(obj: &JsValue) -> Result<io::owned::sys::ShellOutput, VmBamlError> {
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

    Ok(io::owned::sys::ShellOutput {
        stdout,
        stderr,
        exit_code,
    })
}

/// Serialize `ProcessOptions` to a JSON string for the JS callback.
fn options_to_js(options: Option<&io::owned::sys::ProcessOptions>) -> JsValue {
    match options {
        None => JsValue::NULL,
        Some(opts) => {
            let obj = js_sys::Object::new();
            if let Some(ref cwd) = opts.cwd {
                let _ = Reflect::set(&obj, &"cwd".into(), &cwd.into());
            }
            if let Some(ref env) = opts.env {
                let env_obj = js_sys::Object::new();
                for (k, v) in env {
                    let _ = Reflect::set(&env_obj, &k.into(), &v.into());
                }
                let _ = Reflect::set(&obj, &"env".into(), &env_obj.into());
            }
            if let Some(ms) = opts.timeout_ms {
                #[allow(clippy::cast_precision_loss)]
                let _ = Reflect::set(&obj, &"timeout_ms".into(), &JsValue::from_f64(ms as f64));
            }
            if let Some(ref stdin) = opts.stdin {
                let _ = Reflect::set(&obj, &"stdin".into(), &stdin.into());
            }
            if let Some(BexExternalValue::Variant { variant_name, .. }) = opts.stderr.as_ref() {
                let _ = Reflect::set(&obj, &"stderr".into(), &variant_name.into());
            }
            if let Some(detached) = opts.detached {
                let _ = Reflect::set(&obj, &"detached".into(), &JsValue::from_bool(detached));
            }
            js_sys::JSON::stringify(&obj)
                .map(JsValue::from)
                .unwrap_or(JsValue::NULL)
        }
    }
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
        SysOpOutput::err(VmBamlError::Unsupported {
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
        SysOpOutput::err(VmBamlError::Unsupported {
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
        SysOpOutput::err(VmBamlError::Unsupported {
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
        SysOpOutput::err(VmBamlError::Unsupported {
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Live processes are not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassSysProcess for WasmSys {
    fn wait(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ProcessExit> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn kill(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::ok(())
    }
}

impl IoNamespaceSys for WasmSys {
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

    fn exec(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        program: String,
        args: Option<Vec<String>>,
        options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        let exec_fn = self.exec_fn.clone();
        SysOpOutput::async_op(SendFuture(async move {
            let program_js: JsValue = program.into();
            let args_js: JsValue = match args {
                Some(a) => {
                    let arr = js_sys::Array::new();
                    for s in a {
                        arr.push(&s.into());
                    }
                    arr.into()
                }
                None => JsValue::UNDEFINED,
            };
            let options_js = options_to_js(options.as_ref());

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

    fn start_process(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _program: String,
        _args: Option<Vec<String>>,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::Process> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Live processes are not supported on this platform".to_string(),
        })
    }

    fn shell(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        command: String,
        options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        let shell_fn = self.shell_fn.clone();
        SysOpOutput::async_op(SendFuture(async move {
            let command_js: JsValue = command.into();
            let options_js = options_to_js(options.as_ref());

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
