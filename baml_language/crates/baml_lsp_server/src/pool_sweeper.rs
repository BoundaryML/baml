use std::{sync::Once, time::Duration};

static START: Once = Once::new();

/// How often the type intern pool is swept for dead entries.
///
/// The pool no longer evicts on handle drop (that cost a global lock plus a
/// second full hash on every transient type, on the compiler's hottest
/// path), so a long-lived LSP process must reclaim periodically or pool
/// memory grows monotonically across compiles. Each sweep pass holds the
/// global pool mutex for a full scan — low single-digit milliseconds on an
/// LSP-sized pool — so the cadence stays coarse: often enough to bound idle
/// memory at roughly one compile's transient types, rare enough that the
/// stall never lands inside an interactive burst's tail.
const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

pub fn spawn() {
    START.call_once(|| {
        let spawn_result = std::thread::Builder::new()
            .name("baml-lsp-ty-pool-sweeper".to_string())
            .spawn(|| {
                loop {
                    std::thread::sleep(SWEEP_INTERVAL);
                    let reclaimed = baml_type::interned::Ty::sweep_pool();
                    if reclaimed > 0 {
                        tracing::debug!(reclaimed, "swept type intern pool");
                    }
                }
            });

        if let Err(err) = spawn_result {
            tracing::warn!("failed to spawn ty intern pool sweeper: {err}");
        }
    });
}
