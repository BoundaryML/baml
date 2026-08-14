//! Host-facing traceback formatting for the outbound wire envelope.

/// Render a traceback as one string per frame, most-recent-call-last, for
/// carrying structured onto the wire (`repeated string trace` in
/// [`BamlOutboundPanic`](crate::baml_bridge::cffi::BamlOutboundPanic)). Each line
/// uses the per-frame format `File "<file>", line N, in <function_name>` —
/// without a `Traceback (most recent call last):` header and without
/// `\n`-joining, so the host can map each frame 1:1 onto a synthesized Python
/// traceback frame (see 31g-phase6). Returns an empty `Vec` when `frames` is
/// empty.
///
/// The per-frame format intentionally mirrors `bex_vm`'s `format_traceback`
/// (the header form used for internal VM error display); keep the two in sync.
pub fn format_traceback_lines<'a>(
    frames: impl Iterator<Item = (&'a str, usize, &'a str)>,
) -> Vec<String> {
    frames
        .map(|(file, line, function_name)| {
            format!("File \"{file}\", line {line}, in {function_name}")
        })
        .collect()
}
