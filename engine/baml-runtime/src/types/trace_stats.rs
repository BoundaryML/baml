/// Stats about all the spans sent to the tracer.
///
/// A span has the following lifecycle and can fail at any of these points:
///
/// ```text
/// start -> finalize (ctx.exit) -> submit -> send
/// ```
///
/// TODO: return stats about the # of spans successfully sent
#[derive(Clone)]
pub struct TraceStats {
    /// # of spans that we called finish_span or finish_baml_span on
    /// but did not submit due to an error
    pub n_spans_failed_before_submit: u32,
}
