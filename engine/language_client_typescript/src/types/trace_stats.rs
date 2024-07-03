use napi_derive::napi;

crate::lang_wrapper!(TraceStats, baml_runtime::InnerTraceStats);

#[napi]
impl TraceStats {
    #[napi(getter)]
    pub fn get_failed(&self) -> u32 {
        self.inner.failed
    }

    #[napi(getter)]
    pub fn get_started(&self) -> u32 {
        self.inner.started
    }

    #[napi(getter)]
    pub fn get_finalized(&self) -> u32 {
        self.inner.finalized
    }

    #[napi(getter)]
    pub fn get_submitted(&self) -> u32 {
        self.inner.submitted
    }

    #[napi(getter)]
    pub fn get_sent(&self) -> u32 {
        self.inner.sent
    }

    #[napi(getter)]
    pub fn get_done(&self) -> u32 {
        self.inner.done
    }

    #[napi]
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "failed": self.inner.failed,
            "started": self.inner.started,
            "finalized": self.inner.finalized,
            "submitted": self.inner.submitted,
            "sent": self.inner.sent,
            "done": self.inner.done,
        })
        .to_string()
    }
}
