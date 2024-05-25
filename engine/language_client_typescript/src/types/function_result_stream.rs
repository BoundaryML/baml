use napi::threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode};
use napi::Env;
use napi::{JsFunction, JsObject, JsUndefined};
use napi_derive::napi;

use super::function_results::FunctionResultPy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;

crate::lang_wrapper!(
    FunctionResultStreamPy,
    baml_runtime::FunctionResultStream,
    no_from,
    thread_safe,
    cb: Option<napi::Ref<()>>
);

impl FunctionResultStreamPy {
    pub(super) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<napi::Ref<()>>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            cb: event,
        }
    }
}

#[napi]
impl FunctionResultStreamPy {
    #[napi]
    pub fn on_event(
        &mut self,
        env: Env,
        #[napi(ts_arg_type = "(FunctionResultPy) => void")] func: JsFunction,
    ) -> napi::Result<JsUndefined> {
        let cb = env.create_reference(func)?;
        self.cb = Some(cb);
        env.get_undefined()
    }

    #[napi(ts_return_type = "Promise<FunctionResultPy>")]
    pub fn done(&self, env: Env, rctx: &RuntimeContextManagerPy) -> napi::Result<JsObject> {
        let inner = self.inner.clone();

        let on_event = match &self.cb {
            Some(cb) => {
                let cb = env.get_reference_value::<JsFunction>(cb)?;
                let tsfn = env.create_threadsafe_function(
                    &cb,
                    0,
                    |ctx: ThreadSafeCallContext<baml_runtime::FunctionResult>| {
                        Ok(vec![FunctionResultPy::from(ctx.value)])
                    },
                )?;

                Some(move |event| {
                    log::info!("Calling on_event callback");
                    let res = tsfn.call(Ok(event), ThreadsafeFunctionCallMode::Blocking);
                    if res != napi::Status::Ok {
                        log::error!("Error calling on_event callback: {:?}", res);
                    }
                })
            }
            None => None,
        };

        let ctx_mng = rctx.inner.clone();
        let fut = async move {
            let ctx_mng = ctx_mng;
            let res = inner.lock().await.run(on_event, &ctx_mng).await;
            res.0
                .map(FunctionResultPy::from)
                .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
        };

        env.execute_tokio_future(fut, |&mut _, data| Ok(data))
    }
}
