use pyo3::pymethods;

crate::lang_wrapper!(TraceStats, baml_runtime::TraceStats);

#[pymethods]
impl TraceStats {
    pub fn n_spans_failed_before_submit(&self) -> u32 {
        self.inner.n_spans_failed_before_submit
    }
}
