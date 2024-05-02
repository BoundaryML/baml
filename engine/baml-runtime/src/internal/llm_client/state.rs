pub(crate) struct LlmClientState {
    pub call_count: u64,
}

impl LlmClientState {
    pub fn new() -> Self {
        LlmClientState { call_count: 0 }
    }
}
