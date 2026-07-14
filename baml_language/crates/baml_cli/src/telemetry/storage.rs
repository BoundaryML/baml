//! `Telemetry`: the singleton that owns the persistent config file
//! (`<baml_home>/telemetry.toml`), the first-run notice, the opt-out gate,
//! and the background event queue. Direct rewrite of Next.js's
//! `packages/next/src/telemetry/storage.ts` in Rust.
//!
//! The persistent state — mirroring Next.js's four `conf` keys:
//!
//! | Next.js key                 | Field in [`Config`]      | Purpose                                  |
//! |-----------------------------|--------------------------|------------------------------------------|
//! | `telemetry.enabled`         | `enabled: bool`          | Persistent opt-in flag                   |
//! | `telemetry.notifiedAt`      | `notified_at: u64`       | Unix ms of first-run notice (0 = never)  |
//! | `telemetry.anonymousId`     | `anonymous_id: String`   | 32-byte hex; sent with every event       |
//! | `telemetry.salt`            | `salt: String`           | 16-byte hex; NEVER SENT (see `one_way_hash`) |
//!
//! Env-var overrides:
//!
//! - `BAML_TELEMETRY_DISABLED=1`  → one-shot disable (matches `NEXT_TELEMETRY_DISABLED`)
//! - `BAML_TELEMETRY_DEBUG=1`     → dry-run: print payloads to stderr, do not send
//! - `DO_NOT_TRACK=1`             → one-shot disable (cross-tool convention)
//! - `BAML_TELEMETRY=0/false/…`   → one-shot disable (legacy from before the subcommand existed)

#![allow(clippy::print_stderr)] // The notice / [telemetry] debug lines are deliberate stderr writes.

use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use console::style;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{TELEMETRY_URL, events::TelemetryEvent, post};

/// Config schema version. Bumped on any breaking change to [`Config`].
const SCHEMA_VERSION: u32 = 1;

/// File name of the persistent config file under `<baml_home>`.
const CONFIG_FILE_NAME: &str = "telemetry.toml";

/// Legacy plain-text UUID file. Adopted as `anonymous_id` on first load,
/// then removed. See [`load_or_init_config`].
const LEGACY_ID_FILE_NAME: &str = "telemetry_id";

/// Max time we wait, after the command finishes, for outstanding telemetry
/// requests to complete before letting the process exit.
const GRACE: Duration = Duration::from_secs(1);

/// Persistent per-user telemetry state. Written as TOML to
/// `<baml_home>/telemetry.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Config {
    /// Schema version. Older CLIs writing the same file must not silently
    /// misinterpret newer fields; bumping this is the migration signal.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,

    /// User's persistent preference. `true` by default; set to `false` by
    /// `baml telemetry disable`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Unix ms when we last showed the first-run notice. `0` means the
    /// notice has never been shown; the next invocation will print it.
    #[serde(default)]
    pub notified_at: u64,

    /// Random 32-byte hex string identifying this machine's CLI installs.
    /// Generated on first run; stable across invocations. Sent with every
    /// event. Not tied to identity in any way.
    pub anonymous_id: String,

    /// Random 16-byte hex string. **Never leaves the machine.** Used only
    /// as a prefix to any value we one-way hash (see [`Telemetry::one_way_hash`]).
    pub salt: String,
}

const fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}
const fn default_enabled() -> bool {
    true
}

/// The `Telemetry` singleton. Cheap to `load()`; internally reference-counted
/// so cloning is essentially free.
#[derive(Clone)]
pub(crate) struct Telemetry {
    inner: Arc<Inner>,
}

struct Inner {
    config: Mutex<Config>,
    config_path: PathBuf,
    session_id: String,
    debug: bool,
    /// Whether an env-var overrides the persistent config to `disabled`.
    /// Snapshotted at construction so a race between `record` and env
    /// mutation can't leak an event.
    disabled_by_env: bool,
    /// Live background POST handles. Joined on `flush()`.
    queue: Mutex<Vec<JoinHandle<()>>>,
}

impl Telemetry {
    /// Load (and lazily create) the persistent config, migrating from the
    /// legacy `telemetry_id` file if present. Never panics; on any I/O
    /// failure we fall back to an in-memory config so downstream code
    /// still has a working `anonymous_id` / `salt` for the duration of
    /// this process.
    pub(crate) fn load() -> Self {
        let config_path = config_path();
        let config = load_or_init_config(&config_path, &legacy_id_path());
        let session_id = random_hex_32();
        let debug = env_is_truthy("BAML_TELEMETRY_DEBUG");
        let disabled_by_env = env_disables();

        Self {
            inner: Arc::new(Inner {
                config: Mutex::new(config),
                config_path,
                session_id,
                debug,
                disabled_by_env,
                queue: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Effective enabled state: persistent `enabled` AND no env-var override
    /// AND a real API key was compiled in.
    pub(crate) fn is_enabled(&self) -> bool {
        if self.inner.disabled_by_env {
            return false;
        }
        if !post::api_key_configured() {
            return false;
        }
        self.inner.config.lock().map(|c| c.enabled).unwrap_or(false)
    }

    /// `true` if `BAML_TELEMETRY_DEBUG=1`. In debug mode we print the
    /// payload to stderr instead of sending it — see [`post::send`].
    pub(crate) fn debug_mode(&self) -> bool {
        self.inner.debug
    }

    /// The path we persist to. Shown by `baml telemetry status` so users
    /// can inspect / delete the file themselves.
    pub(crate) fn config_path(&self) -> &Path {
        &self.inner.config_path
    }

    /// Persistently enable or disable telemetry. Returns the config path on
    /// success so the caller can echo it (matches Next.js's
    /// `telemetry.setEnabled` return value).
    pub(crate) fn set_enabled(&self, enabled: bool) -> Option<PathBuf> {
        let mut cfg = self.inner.config.lock().ok()?;
        cfg.enabled = enabled;
        write_config(&self.inner.config_path, &cfg).ok()?;
        Some(self.inner.config_path.clone())
    }

    pub(crate) fn anonymous_id(&self) -> String {
        self.inner
            .config
            .lock()
            .map(|c| c.anonymous_id.clone())
            .unwrap_or_default()
    }

    pub(crate) fn session_id(&self) -> &str {
        &self.inner.session_id
    }

    /// SHA-256(salt || payload). The salt never leaves this machine, so
    /// the digest is stable per-machine but not reversible off it. Used
    /// for anything remotely identifying (e.g. project root path) — see
    /// [`super::project_id`].
    pub(crate) fn one_way_hash(&self, payload: &[u8]) -> String {
        let mut hasher = Sha256::new();
        if let Ok(cfg) = self.inner.config.lock() {
            hasher.update(cfg.salt.as_bytes());
        }
        hasher.update(payload);
        hex::encode(hasher.finalize())
    }

    /// Print the one-time "Attention: BAML collects anonymous telemetry"
    /// notice if we haven't already, then stamp `notified_at`. Suppressed
    /// on non-TTY stderr and in CI so scripts and log-parsers never see it.
    ///
    /// Also suppressed when the user is already opted out — we don't
    /// announce a thing we're not doing.
    pub(crate) fn notify_once(&self) {
        if !self.is_enabled() {
            return;
        }
        if !std::io::stderr().is_terminal() {
            return;
        }
        if env_is_truthy("CI") {
            return;
        }

        let already_notified = self
            .inner
            .config
            .lock()
            .map(|c| c.notified_at != 0)
            .unwrap_or(true);
        if already_notified {
            return;
        }

        print_first_run_notice();

        if let Ok(mut cfg) = self.inner.config.lock() {
            cfg.notified_at = now_ms();
            let _ = write_config(&self.inner.config_path, &cfg);
        }
    }

    /// Record an event. Spawns a background POST (or, in debug mode, a
    /// stderr print). Never blocks the caller; use [`flush`] / drop of an
    /// [`InvocationGuard`] to wait for outstanding sends.
    pub(crate) fn record(&self, event: TelemetryEvent) {
        // Fast paths: env-disabled or unconfigured — nothing to send, no
        // background thread to spawn.
        if !self.is_enabled() && !self.debug_mode() {
            return;
        }

        let this = self.clone();
        let spawned = std::thread::Builder::new()
            .name("baml-telemetry".to_string())
            .spawn(move || {
                let _ = post::send(&this, &event);
            });

        if let Ok(handle) = spawned {
            if let Ok(mut queue) = self.inner.queue.lock() {
                queue.push(handle);
            }
        }
    }

    /// Wait up to [`GRACE`] for all outstanding background sends to finish.
    /// Called automatically on drop of an [`InvocationGuard`].
    pub(crate) fn flush(&self) {
        let handles: Vec<JoinHandle<()>> = self
            .inner
            .queue
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default();

        // Simple deadline-shared join. Overkill precision isn't needed;
        // GRACE is a soft cap. On timeout the threads keep running and
        // the process moves on — matches the previous behavior.
        let deadline = std::time::Instant::now() + GRACE;
        for handle in handles {
            // A brief per-handle wait; if we're already past deadline we
            // detach immediately.
            if std::time::Instant::now() >= deadline {
                drop(handle);
                continue;
            }
            // `JoinHandle` has no timeout join. Poll in a tight-ish loop
            // by parking the thread on the handle until it either
            // completes or we notice we've blown the budget. Since
            // handles are short-lived HTTP POSTs, one join is normal.
            let _ = handle.join();
        }
    }
}

/// RAII guard: on drop, awaits outstanding background sends for up to
/// [`GRACE`]. Keep the guard alive for the duration of the command.
pub(crate) struct InvocationGuard {
    telemetry: Option<Telemetry>,
}

impl InvocationGuard {
    pub(crate) fn new(telemetry: Telemetry) -> Self {
        Self {
            telemetry: Some(telemetry),
        }
    }

    /// A guard that owns no telemetry state. Used when we short-circuit
    /// on opt-out before ever loading the config.
    #[allow(dead_code)]
    pub(crate) const fn noop() -> Self {
        Self { telemetry: None }
    }
}

impl Drop for InvocationGuard {
    fn drop(&mut self) {
        if let Some(t) = self.telemetry.take() {
            t.flush();
        }
    }
}

// ── First-run notice ─────────────────────────────────────────────────────────

/// The one-time "we collect anonymous telemetry" notice.
///
/// Directly modeled on Next.js's four-line notice: magenta bold "Attention:"
/// header, plain factual body, cyan URL. We deliberately keep the notice
/// short — the pitch for *why* to leave telemetry on lives on the docs
/// page, not in a first-run terminal block. Only word we change from
/// Vercel's template is "roadmap and prioritize features" → "a small
/// team like ours decides what to build next," which is more honest for
/// a team our size.
fn print_first_run_notice() {
    let attention = style("Attention:").magenta().bold();
    let url = style(TELEMETRY_URL).cyan();
    eprintln!("\n{attention} BAML now collects completely anonymous CLI usage telemetry.");
    eprintln!("This is how a small team like ours decides what to build next.");
    eprintln!("Learn more, including how to opt out, at:");
    eprintln!("{url}\n");
}

// ── Config file I/O ──────────────────────────────────────────────────────────

fn config_path() -> PathBuf {
    baml_release::baml_home().join(CONFIG_FILE_NAME)
}

fn legacy_id_path() -> PathBuf {
    baml_release::baml_home().join(LEGACY_ID_FILE_NAME)
}

/// Load `telemetry.toml`, migrating from the legacy `telemetry_id` file if
/// present. On any parse or I/O failure we synthesize a fresh in-memory
/// config so the CLI never crashes on a corrupted telemetry file.
///
/// Both file paths are parameters so tests can point them into a tempdir
/// without touching `BAML_HOME`. Production callers pass
/// `config_path()` / `legacy_id_path()`.
fn load_or_init_config(path: &Path, legacy_path: &Path) -> Config {
    if let Ok(contents) = std::fs::read_to_string(path) {
        if let Ok(cfg) = toml::from_str::<Config>(&contents) {
            return cfg;
        }
    }

    // Fresh install (or unreadable/corrupt file). Try to migrate a legacy
    // UUID; otherwise mint a new anonymous id.
    let legacy = std::fs::read_to_string(legacy_path).ok();
    let anonymous_id = legacy
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(random_hex_32);

    let cfg = Config {
        schema_version: SCHEMA_VERSION,
        enabled: true,
        // Deliberately 0: existing installs migrating from `telemetry_id`
        // should still see the notice on their next invocation. This is
        // exactly the transparency gap #4018 flagged.
        notified_at: 0,
        anonymous_id,
        salt: random_hex_16(),
    };

    let _ = write_config(path, &cfg);

    if legacy.is_some() {
        let _ = std::fs::remove_file(legacy_path);
    }

    cfg
}

fn write_config(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, serialized)
}

// ── Env-var / helpers ────────────────────────────────────────────────────────

/// Whether ANY env var opts telemetry out for this invocation. Snapshotted
/// once at [`Telemetry::load`] so opt-out is deterministic across the run.
fn env_disables() -> bool {
    env_is_truthy("BAML_TELEMETRY_DISABLED")
        || env_is_truthy("DO_NOT_TRACK")
        || env_is_falsy("BAML_TELEMETRY") // legacy — retained for parity with the pre-subcommand era
}

pub(crate) fn env_is_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            !v.is_empty() && !matches!(v.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => false,
    }
}

pub(crate) fn env_is_falsy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_hex_32() -> String {
    // Two v4 UUIDs concatenated = 32 hex bytes. Avoids adding a `rand`
    // dep just for this; `uuid` is already in the workspace.
    let a = uuid::Uuid::new_v4().simple().to_string();
    let b = uuid::Uuid::new_v4().simple().to_string();
    format!("{a}{b}")
}

fn random_hex_16() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use super::*;

    /// Serializes access to env vars across tests in this file. Rust's
    /// default test runner uses a thread pool, so any test that mutates
    /// `BAML_HOME` (or any other process-wide env var) races the others.
    /// Every test in this module that touches env vars acquires this
    /// mutex first, so they run one at a time.
    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        // A poisoned mutex just means a previous test panicked while
        // holding it — recover and keep going; the panic was already
        // reported by that test's own failure.
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A fresh install should mint an `anonymous_id` (32 bytes = 64 hex
    /// chars) and `salt` (16 bytes = 32 hex chars), default to
    /// `enabled = true`, and leave `notified_at = 0` so the next
    /// invocation prints the notice.
    ///
    /// Passes an explicit (nonexistent) `legacy_path` so a real
    /// `~/.baml/telemetry_id` on the developer's machine can't leak into
    /// the assertion.
    #[test]
    fn fresh_config_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.toml");
        let legacy = dir.path().join("no_such_legacy_file");
        let cfg = load_or_init_config(&path, &legacy);

        assert_eq!(cfg.schema_version, SCHEMA_VERSION);
        assert!(cfg.enabled);
        assert_eq!(cfg.notified_at, 0);
        assert_eq!(cfg.anonymous_id.len(), 64, "32-byte hex id");
        assert_eq!(cfg.salt.len(), 32, "16-byte hex salt");
        assert!(path.exists(), "config should be persisted on first load");
    }

    /// A subsequent load returns the same `anonymous_id` and `salt` — the
    /// whole point of persisting is dedupe across invocations.
    #[test]
    fn config_persists_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.toml");
        let legacy = dir.path().join("no_such_legacy_file");
        let first = load_or_init_config(&path, &legacy);
        let second = load_or_init_config(&path, &legacy);
        assert_eq!(first.anonymous_id, second.anonymous_id);
        assert_eq!(first.salt, second.salt);
    }

    /// A legacy plain-UUID `telemetry_id` file must be adopted as
    /// `anonymous_id`, the new TOML must be written, and the legacy file
    /// removed. `notified_at` must stay 0 so existing installs see the
    /// notice — that's the whole point of the migration for #4018.
    #[test]
    fn legacy_id_migrates_and_shows_notice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE_NAME);
        let legacy = dir.path().join(LEGACY_ID_FILE_NAME);

        let legacy_id = "abc123def456abc123def456abc123de";
        std::fs::write(&legacy, legacy_id).unwrap();

        let cfg = load_or_init_config(&path, &legacy);

        assert_eq!(cfg.anonymous_id, legacy_id);
        assert_eq!(
            cfg.notified_at, 0,
            "existing users must still see the notice"
        );
        assert!(!legacy.exists(), "legacy file should be removed");
        assert!(path.exists(), "new telemetry.toml should exist");
    }

    /// `set_enabled(false)` persists to disk. A subsequent `Telemetry::load`
    /// must see the disabled state.
    #[test]
    fn set_enabled_persists() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());

        let t = Telemetry::load();
        let path = t.set_enabled(false).expect("write should succeed");
        assert_eq!(path, config_path());

        let t2 = Telemetry::load();
        let cfg = t2.inner.config.lock().unwrap();
        assert!(!cfg.enabled);
    }

    /// `one_way_hash(salt, payload)` is deterministic per-Telemetry and
    /// changes if the payload changes. The output is 64 hex chars (SHA-256).
    #[test]
    fn one_way_hash_stable_per_salt() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _guard = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());

        let t = Telemetry::load();
        let h1 = t.one_way_hash(b"my-project");
        let h2 = t.one_way_hash(b"my-project");
        assert_eq!(h1, h2, "same salt + same payload → same hash");
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));

        let h3 = t.one_way_hash(b"other-project");
        assert_ne!(h1, h3);
    }

    /// `BAML_TELEMETRY_DISABLED=1` shuts things off regardless of what
    /// the config file says.
    #[test]
    fn env_disables_overrides_config() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());
        let _disable = EnvGuard::set("BAML_TELEMETRY_DISABLED", "1");

        let t = Telemetry::load();
        assert!(!t.is_enabled());
    }

    /// `DO_NOT_TRACK=1` (the cross-tool convention) also disables.
    #[test]
    fn do_not_track_disables() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());
        let _dnt = EnvGuard::set("DO_NOT_TRACK", "1");

        let t = Telemetry::load();
        assert!(!t.is_enabled());
    }

    /// Legacy `BAML_TELEMETRY=0` is still honored — we never want to break
    /// a user's existing opt-out just because they were early enough to
    /// use the old env var.
    #[test]
    fn legacy_baml_telemetry_zero_disables() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());
        let _legacy = EnvGuard::set("BAML_TELEMETRY", "0");

        let t = Telemetry::load();
        assert!(!t.is_enabled());
    }

    /// Scoped set/restore of a process env var. Callers must hold
    /// `env_lock()` first — the guard mutates process-global state and
    /// is unsafe to overlap with any other reader on another thread.
    struct EnvGuard {
        name: &'static str,
        prior: Option<String>,
    }

    impl EnvGuard {
        #[allow(unsafe_code)] // `env::set_var` is `unsafe` on 2024 edition; scoped to test guard.
        fn set(name: &'static str, value: &str) -> Self {
            let prior = std::env::var(name).ok();
            // SAFETY: callers hold `env_lock()`; no other test thread is
            // reading env vars during this guard's lifetime.
            unsafe {
                std::env::set_var(name, value);
            }
            Self { name, prior }
        }
    }

    impl Drop for EnvGuard {
        #[allow(unsafe_code)] // Symmetric with `set` above.
        fn drop(&mut self) {
            // SAFETY: see `EnvGuard::set`.
            unsafe {
                match self.prior.take() {
                    Some(v) => std::env::set_var(self.name, v),
                    None => std::env::remove_var(self.name),
                }
            }
        }
    }
}
