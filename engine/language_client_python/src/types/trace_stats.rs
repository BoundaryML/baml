use pyo3::pymethods;

crate::lang_wrapper!(TraceStats, baml_runtime::InnerTraceStats);

#[pymethods]
impl TraceStats {
    #[getter]
    pub fn failed(&self) -> u32 {
        self.inner.failed
    }

    #[getter]
    pub fn started(&self) -> u32 {
        self.inner.started
    }

    #[getter]
    pub fn finalized(&self) -> u32 {
        self.inner.finalized
    }

    #[getter]
    pub fn submitted(&self) -> u32 {
        self.inner.submitted
    }

    #[getter]
    pub fn sent(&self) -> u32 {
        self.inner.sent
    }

    #[getter]
    pub fn done(&self) -> u32 {
        self.inner.done
    }

    pub fn __repr__(&self) -> String {
        format!(
            "TraceStats(failed={}, started={}, finalized={}, submitted={}, sent={}, done={})",
            self.failed(),
            self.started(),
            self.finalized(),
            self.submitted(),
            self.sent(),
            self.done()
        )
    }
}
