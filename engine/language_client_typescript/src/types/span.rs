use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use napi_derive::napi;

use super::runtime_ctx_manager::RuntimeContextManager;
use crate::BamlRuntime;

crate::lang_wrapper!(BamlSpan,
  Option<Option<baml_runtime::tracing::TracingSpan>>,
  no_from,
  rt: std::sync::Arc<baml_runtime::BamlRuntime>
);

#[napi]
impl BamlSpan {
    #[napi(ts_return_type = "BamlSpan")]
    pub fn new(
        runtime: &BamlRuntime,
        function_name: String,
        args: serde_json::Value,
        ctx: &RuntimeContextManager,
    ) -> napi::Result<Self> {
        let args: BamlValue = serde_json::from_value(args)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
        let Some(args_map) = args.as_map() else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid span args",
            ));
        };

        let (span, _) = runtime
            .inner
            .start_span(&function_name, &args_map, &ctx.inner);
        log::trace!("Starting span: {:#?} for {:?}\n", span, function_name);
        Ok(Self {
            inner: span.into(),
            rt: runtime.inner.clone(),
        })
    }

    // mthod to finish
    #[napi]
    pub fn finish(
        &mut self,
        result: serde_json::Value,
        ctx: &RuntimeContextManager,
    ) -> napi::Result<serde_json::Value> {
        log::info!("Finishing span: {:?}", self.inner);
        let result: BamlValue = serde_json::from_value(result)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
        // log::info!("Finishing span: {:#?}\n", self.inner.lock().await);

        let span = self
            .inner
            .take()
            .ok_or_else(|| napi::Error::new(napi::Status::GenericFailure, "Already used span"))?;

        self.rt
            .finish_span(span, Some(result), &ctx.inner)
            .map(|u| u.map(|id| id.to_string()))
            .map(|u| serde_json::json!(u))
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
    }
}
