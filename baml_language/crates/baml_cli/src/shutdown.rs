use std::sync::Arc;

use bex_engine::BexEngine;

use crate::reporter::Reporter;

pub(crate) fn shutdown_engine(
    rt: &tokio::runtime::Runtime,
    engine: &Arc<BexEngine>,
    reporter: &Reporter,
) {
    rt.block_on(engine.shutdown_with_progress(|count| {
        reporter.status("Waiting", wait_message(count));
    }));
}

fn wait_message(count: usize) -> String {
    let threads = if count == 1 { "thread" } else { "threads" };
    format!("for {count} remaining BAML {threads} to finish; press Ctrl+C to cancel")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_message_pluralizes_thread_count() {
        assert_eq!(
            wait_message(1),
            "for 1 remaining BAML thread to finish; press Ctrl+C to cancel"
        );
        assert_eq!(
            wait_message(2),
            "for 2 remaining BAML threads to finish; press Ctrl+C to cancel"
        );
    }
}
