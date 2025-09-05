use std::collections::HashMap;

use napi::{
    bindgen_prelude::{ObjectFinalize, Function, FunctionRef, Undefined},
    threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode},
    Env, JsObject,
};
use napi_derive::napi;

use super::{function_results::FunctionResult, runtime_ctx_manager::RuntimeContextManager};
use crate::errors::from_anyhow_error;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream,
    custom_finalize,
    no_from,
    thread_safe,
    callback: Option<FunctionRef<FunctionResult, ()>>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<FunctionRef<FunctionResult, ()>>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
    ) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(inner)),
            callback: event,
            tb,
            cb,
            env_vars: HashMap::new(),
        }
    }
}

#[napi]
impl FunctionResultStream {
    #[napi]
    pub fn on_event(
        &mut self,
        env: Env,
        #[napi(ts_arg_type = "((err: any, param: FunctionResult) => void) | undefined")]
        func: Option<Function<FunctionResult, ()>>,
    ) -> napi::Result<Undefined> {
        if let Some(func) = func {
            let new_ref = func.create_ref()?;
            self.callback = Some(new_ref);
        } else {
            self.callback = None;
        }

        Ok(())
    }

    #[napi(ts_return_type = "Promise<FunctionResult>")]
    pub fn done(&self, env: Env, rctx: &RuntimeContextManager) -> napi::Result<JsObject> {
        let inner = self.inner.clone();

        let on_event = match &self.callback {
            Some(cb_ref) => {
                let cb = cb_ref.borrow_back(&env)?;
                let thread_safe_fn = cb
                    .build_threadsafe_function()
                    .build_callback(|ctx: ThreadSafeCallContext<baml_runtime::FunctionResult>| {
                        Ok(FunctionResult::from(ctx.value))
                    })?;

                Some(move |event: baml_runtime::FunctionResult| {
                    let status = thread_safe_fn.call(event, ThreadsafeFunctionCallMode::Blocking);
                    if status != napi::Status::Ok {
                        log::error!("Error calling on_event callback: {status:?}");
                    }
                })
            }
            None => None,
        };

        let ctx_mng = rctx.inner.clone();
        let tb = self.tb.clone();
        let cb = self.cb.clone();
        let env_vars = self.env_vars.clone();

        let fut = async move {
            let ctx_mng = ctx_mng;
            let res = inner
                .lock()
                .await
                .run(
                    None::<fn()>,
                    on_event,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    env_vars,
                )
                .await;
            res.0.map(FunctionResult::from).map_err(from_anyhow_error)
        };

        env.execute_tokio_future(fut, |&mut _, data| Ok(data))
    }
}

// TODO: Probably no longer needed because of the FunctionRef Drop impl.
impl ObjectFinalize for FunctionResultStream {
    fn finalize(self, env: Env) -> napi::Result<()> {
        // dropping the FunctionRef automatically unrefs Node callback
        Ok(())
    }
}
