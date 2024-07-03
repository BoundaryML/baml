/// Stats about all the spans sent to the tracer.
///
/// A span has the following lifecycle and can fail at any of these points:
///
/// ```text
/// start -> finalize (ctx.exit) -> submit -> send
/// ```
///
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct InnerTraceStats {
    // Happen on the main runtime thread.
    pub started: u32,
    pub finalized: u32,
    pub submitted: u32,

    // Happen on the tracer thread.
    pub sent: u32,
    pub done: u32,
    // All errors are counted here.
    pub failed: u32,
}

#[derive(Clone, Default)]
pub struct TraceStats {
    inner: Arc<Mutex<InnerTraceStats>>,
}

impl TraceStats {
    pub fn drain(&self) -> InnerTraceStats {
        let mut inner = self.inner.lock().unwrap();
        let result = inner.clone();
        *inner = InnerTraceStats::default();
        result
    }

    pub fn guard(&self) -> SpanGuard {
        SpanGuard::new(self.clone())
    }

    // Add methods to access and modify the inner fields if needed
    fn inc_started(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.started += 1;
    }

    fn inc_finalized(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.finalized += 1;
    }

    fn inc_submitted(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.submitted += 1;
    }

    fn inc_sent(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.sent += 1;
    }

    fn inc_done(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.done += 1;
    }

    fn inc_failed(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.failed += 1;
    }
}

pub struct SpanGuard {
    stats: TraceStats,
    used: bool,
}

impl SpanGuard {
    pub fn new(stats: TraceStats) -> Self {
        Self { stats, used: false }
    }

    pub fn start(mut self) {
        self.stats.inc_started();
        self.used = true;
    }

    pub fn finalize(mut self) {
        self.stats.inc_finalized();
        self.used = true;
    }

    pub fn submit(mut self) {
        self.stats.inc_submitted();
        self.used = true;
    }

    pub fn send(mut self) {
        self.stats.inc_sent();
        self.used = true;
    }

    pub fn done(mut self) {
        self.stats.inc_done();
        self.used = true;
    }
}

impl Drop for SpanGuard {
    fn drop(&mut self) {
        if !self.used {
            self.stats.inc_failed();
        }
    }
}
