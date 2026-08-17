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
    let futures = if count == 1 { "future" } else { "futures" };
    format!("for {count} remaining BAML {futures} to finish (press Ctrl+C to cancel now)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_message_pluralizes_future_count() {
        assert_eq!(
            wait_message(1),
            "for 1 remaining BAML future to finish (press Ctrl+C to cancel now)"
        );
        assert_eq!(
            wait_message(2),
            "for 2 remaining BAML futures to finish (press Ctrl+C to cancel now)"
        );
    }
}
