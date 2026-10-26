//! Logs `parking_lot` deadlocks: every 10 s, reports each cycle of blocked
//! threads with their backtraces.
//!
//! Off by default. To debug a hung LSP, build with
//! `cargo build --release -p baml_cli --features deadlock-watchdog`.

use std::{sync::Once, time::Duration};

static START: Once = Once::new();

pub(crate) fn spawn() {
    START.call_once(|| {
        let spawn_result = std::thread::Builder::new()
            .name("baml-lsp-deadlock-watchdog".to_string())
            .spawn(|| {
                loop {
                    std::thread::sleep(Duration::from_secs(10));

                    let deadlocks = parking_lot::deadlock::check_deadlock();
                    if deadlocks.is_empty() {
                        continue;
                    }

                    tracing::error!(
                        deadlock_count = deadlocks.len(),
                        "parking_lot deadlock detected"
                    );
                    for (cycle_index, threads) in deadlocks.iter().enumerate() {
                        tracing::error!(
                            cycle_index,
                            thread_count = threads.len(),
                            "parking_lot deadlock cycle"
                        );
                        for thread in threads {
                            tracing::error!(
                                thread_id = ?thread.thread_id(),
                                backtrace = ?thread.backtrace(),
                                "thread blocked in parking_lot deadlock"
                            );
                        }
                    }
                }
            });

        if let Err(err) = spawn_result {
            tracing::warn!("failed to spawn parking_lot deadlock watchdog: {err}");
        }
    });
}
