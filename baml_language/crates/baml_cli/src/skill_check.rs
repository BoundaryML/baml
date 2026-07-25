//! Passive agent-skill warning, printed on the core authoring commands
//! (`init`, `run`, `generate`, `pack`).
//!
//! Living in the toolchain (rather than the `baml` wrapper) means the warning
//! ships with every nightly instead of waiting on a wrapper release, and fires
//! even when the toolchain binary is invoked directly. The check itself never
//! blocks the command on the network: the warning is decided from the local
//! caches, and a TTL-throttled refresh of the latest-commit cache runs in the
//! background while the command executes, so a newly stale skill is reported
//! by the *next* invocation.

use std::time::{Duration, Instant};

use baml_release::skills;

/// Bound on how long [`SkillCheck::drop`] waits for the background refresh
/// after the command finishes, anchored at refresh start (a command that ran
/// 3s only waits up to 2s more). Matches the wrapper's auto-check budget.
const AUTO_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Guard returned by [`SkillCheck::start`]. Dropping it gives the background
/// cache refresh whatever remains of its time budget; cache writes are
/// atomic, so abandoning a refresh mid-write on timeout is safe.
pub(crate) struct SkillCheck {
    refresh: Option<(std::sync::mpsc::Receiver<()>, Instant)>,
}

impl SkillCheck {
    /// Print the applicable skill warning (if any) from the local caches and
    /// kick off the TTL-throttled background refresh of the latest-commit
    /// cache. `[update] auto_check = false` in `~/.baml/config.toml` disables
    /// the network refresh but not the cache-based warning.
    pub(crate) fn start() -> Self {
        let latest =
            skills::read_cached_latest_skill_commit(&skills::latest_skill_commit_cache_path());
        let state = skills::read_skills_state(&skills::state_path());
        if let Some(message) = skills::skill_warning_message(
            skills::project_has_baml_skills(),
            state.as_ref(),
            latest.as_deref(),
        ) {
            crate::reporter::print_warning(format_args!("{message}"));
        }

        let refresh = (skills::update_auto_check_enabled()
            && skills::should_attempt_latest_commit_refresh())
        .then(|| {
            let deadline = Instant::now() + AUTO_CHECK_TIMEOUT;
            let (sender, done) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                if let Ok(sha) = skills::fetch_latest_skill_commit(AUTO_CHECK_TIMEOUT) {
                    let _ = skills::write_cached_latest_skill_commit(
                        &skills::latest_skill_commit_cache_path(),
                        &sha,
                    );
                }
                let _ = sender.send(());
            });
            (done, deadline)
        });
        Self { refresh }
    }

    /// An inert guard for every command outside the init/run/generate/pack
    /// whitelist (machine-facing commands, utilities, and `baml agent …`
    /// itself, whose whole purpose is acting on skills).
    pub(crate) fn skipped() -> Self {
        Self { refresh: None }
    }
}

impl Drop for SkillCheck {
    fn drop(&mut self) {
        if let Some((done, deadline)) = self.refresh.take() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = done.recv_timeout(remaining);
        }
    }
}
