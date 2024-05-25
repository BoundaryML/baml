use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_types::BamlValue;
use napi_derive::napi;

use super::runtime::BamlRuntimePy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;

crate::lang_wrapper!(BamlSpanPy,
  Option<baml_runtime::tracing::TracingSpan>,
  no_from,
  thread_safe,
  rt: std::sync::Arc<baml_runtime::BamlRuntime>
);

#[napi]
impl BamlSpanPy {
    #[napi(ts_return_type = "BamlSpanPy")]
    pub fn new(
        runtime: &BamlRuntimePy,
        function_name: String,
        args: serde_json::Value,
        ctx: &RuntimeContextManagerPy,
    ) -> napi::Result<Self> {
        let args: BamlValue = serde_json::from_value(args)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
        let Some(args_map) = args.as_map() else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid args",
            ));
        };

        let (span, _) = runtime
            .inner
            .start_span(&function_name, &args_map, &ctx.inner);
        Ok(Self {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(span)),
            rt: runtime.inner.clone(),
        })
    }

    // mthod to finish
    #[napi]
    pub async fn finish(
        &self,
        result: serde_json::Value,
        ctx: &RuntimeContextManagerPy,
    ) -> napi::Result<serde_json::Value> {
        let result: BamlValue = serde_json::from_value(result)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;

        let span = {
            self.inner.lock().await.take().ok_or_else(|| {
                napi::Error::new(napi::Status::GenericFailure, "Already used span")
            })?
        };

        let result = self
            .rt
            .finish_span(span, Some(result), &ctx.inner)
            .await
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
            .map(|u| u.map(|id| id.to_string()))?;

        Ok(serde_json::json!(result))
    }
}
