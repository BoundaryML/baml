use std::collections::HashMap;

use napi::{
    bindgen_prelude::{
        Error, FnArgs, Function, FunctionRef, Object, ObjectFinalize, PromiseRaw, Undefined,
    },
    threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode},
    Env,
};
use napi_derive::napi;

use super::{function_results::FunctionResult, runtime_ctx_manager::RuntimeContextManager};
use crate::errors::from_anyhow_error;

crate::lang_wrapper!(
    FunctionResultStream,
    baml_runtime::FunctionResultStream,
    custom_finalize,
    no_from,
    optional,
    callback: Option<FunctionRef<FnArgs<(Error, FunctionResult)>, ()>>,
    on_tick: Option<FunctionRef<(), ()>>,
    tb: Option<baml_runtime::type_builder::TypeBuilder>,
    cb: Option<baml_runtime::client_registry::ClientRegistry>,
    env_vars: HashMap<String, String>
);

impl FunctionResultStream {
    pub(crate) fn new(
        inner: baml_runtime::FunctionResultStream,
        event: Option<FunctionRef<FnArgs<(Error, FunctionResult)>, ()>>,
        on_tick: Option<FunctionRef<(), ()>>,
        tb: Option<baml_runtime::type_builder::TypeBuilder>,
        cb: Option<baml_runtime::client_registry::ClientRegistry>,
    ) -> Self {
        Self {
            inner: Some(inner),
            callback: event,
            on_tick,
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
        func: Option<Function<FnArgs<(Error, FunctionResult)>, ()>>,
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
    pub fn done<'e>(
        &mut self,
        env: &'e Env,
        rctx: &RuntimeContextManager,
    ) -> napi::Result<PromiseRaw<'e, FunctionResult>> {
        let Some(inner) = self.inner.take() else {
            return Err(napi::Error::from_reason("Stream already finished"));
        };

        let on_event = match &self.callback {
            Some(cb_ref) => {
                let cb = cb_ref.borrow_back(env)?;
                // Prepare for some confusing control flow here:
                // This thing is Rust wrapper over a JS function that essentially
                // maps Rust objects to JS objects before the JS function is called.
                let thread_safe_fn = cb.build_threadsafe_function().build_callback(
                    |ctx: ThreadSafeCallContext<
                        FnArgs<(Option<Error>, Option<baml_runtime::FunctionResult>)>,
                    >| {
                        // TODO: These parameters are annoying, figure out if we can do Result<FunctionResult>.
                        match ctx.value.data {
                            (None, Some(event)) => {
                                Ok(FnArgs::from((None, Some(FunctionResult::from(event)))))
                            }
                            (Some(error), None) => Ok(FnArgs::from((Some(error), None))),
                            (Some(error), Some(event)) => Ok(FnArgs::from((
                                Some(error),
                                Some(FunctionResult::from(event)),
                            ))),
                            (None, None) => Ok(FnArgs::from((None, None))),
                        }
                    },
                )?;

                // Now this thing is the actual call to the Rust wrapper. We call
                // this with objects coming from pure Rust. For some reason we
                // have to match the JS expected types so that's why I'm using
                // FnArgs.
                Some(move |event: baml_runtime::FunctionResult| {
                    let status = thread_safe_fn.call(
                        FnArgs::from((None, Some(event))),
                        ThreadsafeFunctionCallMode::Blocking,
                    );
                    if status != napi::Status::Ok {
                        log::error!("Error calling on_event callback: {status:?}");
                    }
                })
            }
            None => None,
        };

        let on_tick_callback = match &self.on_tick {
            Some(tick_cb_ref) => {
                let tick_cb = tick_cb_ref.borrow_back(env)?;
                let thread_safe_fn = tick_cb
                    .build_threadsafe_function()
                    .build_callback(|_ctx: ThreadSafeCallContext<()>| Ok(()))?;

                Some(move || {
                    let res = thread_safe_fn.call((), ThreadsafeFunctionCallMode::Blocking);
                    if res != napi::Status::Ok {
                        log::error!("Error calling on_tick callback: {res:?}");
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
            let mut inner = inner;
            let res = inner
                .run(
                    on_tick_callback,
                    on_event,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    env_vars,
                )
                .await;
            res.0.map(FunctionResult::from).map_err(from_anyhow_error)
        };

        env.spawn_future(fut)
    }
}

// TODO: Probably no longer needed because of the FunctionRef Drop impl.
impl ObjectFinalize for FunctionResultStream {
    fn finalize(self, env: Env) -> napi::Result<()> {
        // dropping the FunctionRef automatically unrefs Node callback
        Ok(())
    }
}
