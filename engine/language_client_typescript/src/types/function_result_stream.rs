use napi::bindgen_prelude::ObjectFinalize;
use napi::threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode};
use napi::Env;
use napi::{JsFunction, JsObject, JsUndefined};
use napi_derive::napi;

use super::function_results::FunctionResult;
use super::runtime_ctx_manager::RuntimeContextManager;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream,
    custom_finalize,
    no_from,
    thread_safe,
    callback: Option<napi::Ref<()>>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<napi::Ref<()>>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            callback: event,
            tb,
            cb,
        }
    }
}

#[napi]
impl FunctionResultStream {
    #[napi]
    pub fn on_event(
        &mut self,
        env: Env,
        #[napi(ts_arg_type = "(err: any, param: FunctionResult) => void")] func: JsFunction,
    ) -> napi::Result<JsUndefined> {
        let cb = env.create_reference(func)?;
        let prev = self.callback.take();
        if let Some(mut old_cb) = prev {
            old_cb.unref(env)?;
        }
        self.callback = Some(cb);
        env.get_undefined()
    }

    #[napi(ts_return_type = "Promise<FunctionResult>")]
    pub fn done(&self, env: Env, rctx: &RuntimeContextManager) -> napi::Result<JsObject> {
        let inner = self.inner.clone();

        let on_event = match &self.callback {
            Some(cb) => {
                let cb = env.get_reference_value::<JsFunction>(cb)?;
                let tsfn = env.create_threadsafe_function(
                    &cb,
                    0,
                    |ctx: ThreadSafeCallContext<baml_runtime::FunctionResult>| {
                        Ok(vec![FunctionResult::from(ctx.value)])
                    },
                )?;

                Some(move |event: baml_runtime::FunctionResult| {
                    let res = tsfn.call(Ok(event), ThreadsafeFunctionCallMode::Blocking);
                    if res != napi::Status::Ok {
                        log::error!("Error calling on_event callback: {:?}", res);
                    }
                })
            }
            None => None,
        };

        let ctx_mng = rctx.inner.clone();
        let tb = self.tb.as_ref().map(|tb| tb.clone());
        let cb = self.cb.as_ref().map(|cb| cb.clone());

        let fut = async move {
            let ctx_mng = ctx_mng;
            let res = inner
                .lock()
                .await
                .run(on_event, &ctx_mng, tb.as_ref(), cb.as_ref())
                .await;
            res.0
                .map(FunctionResult::from)
                .map_err(|e| napi::Error::new(napi::Status::GenericFailure, format!("{:?}", e)))
        };

        env.execute_tokio_future(fut, |&mut _, data| Ok(data))
    }
}

impl ObjectFinalize for FunctionResultStream {
    fn finalize(mut self, env: Env) -> napi::Result<()> {
        if let Some(mut cb) = self.callback.take() {
            cb.unref(env)?;
        }
        Ok(())
    }
}
