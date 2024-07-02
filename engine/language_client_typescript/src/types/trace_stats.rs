use napi_derive::napi;

crate::lang_wrapper!(TraceStats, baml_runtime::TraceStats);

#[napi]
impl TraceStats {
    #[napi]
    pub fn n_spans_failed_before_submit(&self) -> u32 {
        self.inner.n_spans_failed_before_submit
    }
}
