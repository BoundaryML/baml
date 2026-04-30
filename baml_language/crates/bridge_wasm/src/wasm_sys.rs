use std::sync::Arc;

use js_sys::{Function, Promise, Reflect, Uint8Array};
use sys_ops::io::{self, IoNamespaceSys};
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};
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
fn unpack_shell_result(obj: &JsValue) -> Result<io::owned::sys::ShellOutput, OpErrorKind> {
    let exit_code_f64 = Reflect::get(obj, &"exit_code".into())
        .map_err(|e| OpErrorKind::Io {
            message: format!("missing exit_code: {e:?}"),
        })?
        .as_f64()
        .unwrap_or(-1.0);
    #[allow(clippy::cast_possible_truncation)]
    let exit_code = exit_code_f64 as i64;

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
            js_sys::JSON::stringify(&obj)
                .map(JsValue::from)
                .unwrap_or(JsValue::NULL)
        }
    }
}

impl IoNamespaceSys for WasmSys {
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
                .map_err(|e| OpErrorKind::Io {
                    message: format!("exec callback failed: {e:?}"),
                })?;

            let promise: Promise = result.dyn_into().map_err(|_| OpErrorKind::Io {
                message: "exec callback did not return a Promise".into(),
            })?;
            let obj = JsFuture::from(promise).await.map_err(|e| OpErrorKind::Io {
                message: format!("exec callback rejected: {e:?}"),
            })?;

            unpack_shell_result(&obj)
        }))
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
                .map_err(|e| OpErrorKind::Io {
                    message: format!("shell callback failed: {e:?}"),
                })?;

            let promise: Promise = result.dyn_into().map_err(|_| OpErrorKind::Io {
                message: "shell callback did not return a Promise".into(),
            })?;
            let obj = JsFuture::from(promise).await.map_err(|e| OpErrorKind::Io {
                message: format!("shell callback rejected: {e:?}"),
            })?;

            unpack_shell_result(&obj)
        }))
    }

    fn sleep(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        delay_ms: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        let millis = i32::try_from(delay_ms.clamp(0, i64::from(i32::MAX))).unwrap_or(i32::MAX);
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            set_timeout(&resolve, millis);
        });
        SysOpOutput::async_op(SendFuture(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            Ok(())
        }))
    }
}
