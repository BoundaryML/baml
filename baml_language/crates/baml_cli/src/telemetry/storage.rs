//! `Telemetry`: the singleton that owns the persistent config file
//! (`<baml_home>/telemetry.toml`), the first-run notice, the opt-out gate,
//! and the handle to the on-disk event queue (see [`super::queue`]).
//! Direct rewrite of Next.js's `packages/next/src/telemetry/storage.ts`
//! in Rust, with delivery upgraded from in-process HTTP to a
//! crash-safe disk queue drained by a detached child process.
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
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use console::style;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{TELEMETRY_URL, events::TelemetryEvent, post, queue};

/// Config schema version. Bumped on any breaking change to [`Config`].
const SCHEMA_VERSION: u32 = 1;

/// File name of the persistent config file under `<baml_home>`.
const CONFIG_FILE_NAME: &str = "telemetry.toml";

/// Legacy plain-text UUID file. Adopted as `anonymous_id` on first load,
/// then removed. See [`load_or_init_config`].
const LEGACY_ID_FILE_NAME: &str = "telemetry_id";

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
    /// Path of this process's current live queue file. Recorded events
    /// append here; the rotation timer swaps it out every
    /// [`queue::ROTATE_INTERVAL`] so long-running processes ship events
    /// without waiting for exit.
    live_path: Mutex<PathBuf>,
}

impl Telemetry {
    /// The process-wide singleton. First call loads the config and starts
    /// the rotation timer (relevant only for long-running commands like
    /// `baml lsp`; short commands exit long before the first tick).
    ///
    /// This is what makes `crate::telemetry::record(...)` a one-liner
    /// anywhere in the crate: no loading, no guard-passing.
    pub(crate) fn global() -> &'static Telemetry {
        static GLOBAL: OnceLock<Telemetry> = OnceLock::new();
        GLOBAL.get_or_init(|| {
            let telemetry = Telemetry::load();
            telemetry.spawn_rotation_timer();
            telemetry
        })
    }

    /// Load (and lazily create) the persistent config, migrating from the
    /// legacy `telemetry_id` file if present. Never panics; on any I/O
    /// failure we fall back to an in-memory config so downstream code
    /// still has a working `anonymous_id` / `salt` for the duration of
    /// this process.
    ///
    /// Prefer [`Telemetry::global`] outside of tests and the flush child —
    /// `load()` neither starts the rotation timer nor dedupes instances.
    pub(crate) fn load() -> Self {
        let config_path = config_path();
        let config = load_or_init_config(&config_path, &legacy_id_path());
        let session_id = random_hex_32();
        let debug = env_is_truthy("BAML_TELEMETRY_DEBUG");
        let disabled_by_env = env_disables();
        let live_path = queue::new_live_path_in(&queue::queue_dir());

        Self {
            inner: Arc::new(Inner {
                config: Mutex::new(config),
                config_path,
                session_id,
                debug,
                disabled_by_env,
                live_path: Mutex::new(live_path),
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

    /// Record an event. Serializes the complete request body and appends
    /// it to this process's live queue file — one atomic write syscall,
    /// ~10µs. No HTTP happens on the caller's thread, ever; delivery is
    /// the detached flush child's job (see [`queue`]). Because the event
    /// hits disk immediately, it survives panics, Ctrl-C, and SIGKILL.
    ///
    /// In debug mode (`BAML_TELEMETRY_DEBUG=1`) the payload is printed to
    /// stderr instead and nothing is written or sent — even when opted
    /// out, so users can always audit what *would* go out.
    pub(crate) fn record(&self, event: TelemetryEvent) {
        if self.debug_mode() {
            if let Some(body) = post::build_body(self, &event) {
                if let Ok(rendered) = serde_json::to_string_pretty(&body) {
                    eprintln!("[telemetry] {rendered}");
                }
            }
            return;
        }
        if !self.is_enabled() {
            return;
        }
        let Some(body) = post::build_body(self, &event) else {
            return;
        };
        let Ok(line) = serde_json::to_string(&body) else {
            return;
        };
        if let Ok(path) = self.inner.live_path.lock() {
            let _ = queue::append_line(&path, &line);
        }
    }

    /// Seal this process's live file and hand delivery to a detached
    /// child. Non-blocking: total cost is one rename + one process spawn
    /// (~1–2ms), after which the parent is free to exit. Also spawns a
    /// child when older sealed files are waiting, so any backlog from
    /// previous failed sends drains without this invocation having
    /// recorded anything itself.
    pub(crate) fn flush(&self) {
        if self.debug_mode() {
            return;
        }
        let sealed = self
            .inner
            .live_path
            .lock()
            .ok()
            .and_then(|path| queue::seal(&path))
            .is_some();
        if sealed || queue::has_sealed_work_in(&queue::queue_dir()) {
            queue::spawn_flush_child();
        }
    }

    /// Rotate + ship on a timer so long-running processes (LSP,
    /// playground) deliver events within [`queue::ROTATE_INTERVAL`]
    /// instead of hoarding them until exit. Short commands exit before
    /// the first tick; the extra sleeping thread costs nothing.
    ///
    /// Each tick first refreshes the config from disk, so a long-running
    /// process notices a `baml telemetry disable` issued in another
    /// terminal within one interval and stops recording (its already-queued
    /// events are then purged by the flush child — see [`queue::drain`]).
    fn spawn_rotation_timer(&self) {
        let this = self.clone();
        let _ = std::thread::Builder::new()
            .name("baml-telemetry-rotate".to_string())
            .spawn(move || {
                loop {
                    std::thread::sleep(queue::ROTATE_INTERVAL);
                    this.refresh_config();
                    this.rotate();
                }
            });
    }

    /// Re-read `telemetry.toml` and adopt its current `enabled` state.
    /// Lets a long-lived process pick up an opt-out (or opt-in) made after
    /// it started, rather than holding the snapshot taken at [`load`].
    /// Best-effort: an unreadable/corrupt file leaves the current state
    /// untouched.
    fn refresh_config(&self) {
        let Ok(contents) = std::fs::read_to_string(&self.inner.config_path) else {
            return;
        };
        let Ok(fresh) = toml::from_str::<Config>(&contents) else {
            return;
        };
        if let Ok(mut cfg) = self.inner.config.lock() {
            *cfg = fresh;
        }
    }

    /// Seal the current live file (if it has events), point new records
    /// at a fresh one, and spawn a child to ship what was sealed.
    fn rotate(&self) {
        let Ok(mut path) = self.inner.live_path.lock() else {
            return;
        };
        if queue::seal(&path).is_some() {
            *path = queue::new_live_path_in(&queue::queue_dir());
            queue::spawn_flush_child();
        }
    }
}

/// RAII guard: on drop, seals the live queue file and spawns the detached
/// flush child (see [`Telemetry::flush`] — non-blocking, ~1–2ms). Keep the
/// guard alive for the duration of the command.
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

    /// `record()` writes the full request body to the live queue file
    /// immediately — this is the crash-safety property: an event is on
    /// disk the moment it's recorded, not at process exit.
    #[test]
    fn record_appends_to_live_file_immediately() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());

        let t = Telemetry::load();
        t.record(TelemetryEvent::cli_invocation("fmt"));

        let live = t.inner.live_path.lock().unwrap().clone();
        let contents = std::fs::read_to_string(&live).expect("live file exists after record");
        let line = contents.lines().next().expect("one event line");
        let body: serde_json::Value = serde_json::from_str(line).expect("line is a full body");
        assert_eq!(body["event"], "cli_invocation");
        assert_eq!(body["properties"]["command"], "fmt");
    }

    /// `flush()` seals the live file (rename to `sealed_*`) so a flush
    /// child can claim it. The parent-side cost is just that rename —
    /// no joins, no network.
    #[test]
    fn flush_seals_live_file() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());

        let t = Telemetry::load();
        t.record(TelemetryEvent::cli_invocation("check"));
        let live = t.inner.live_path.lock().unwrap().clone();
        assert!(live.exists());

        // Note: flush() also spawns the detached child, which will try to
        // claim + send the sealed file. In tests the child is the test
        // binary, which has no `__flush-telemetry` command, so it exits
        // immediately without touching the queue.
        t.flush();
        assert!(!live.exists(), "live file renamed away");

        let queue_dir = queue::queue_dir();
        let sealed_exists = std::fs::read_dir(&queue_dir)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("sealed_"));
        assert!(sealed_exists, "sealed file waiting for the flush child");
    }

    /// When opted out via env, `record()` writes nothing at all — the
    /// event never touches disk.
    #[test]
    fn record_writes_nothing_when_disabled() {
        let _lock = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let _home = EnvGuard::set("BAML_HOME", dir.path().to_str().unwrap());
        let _disable = EnvGuard::set("BAML_TELEMETRY_DISABLED", "1");

        let t = Telemetry::load();
        t.record(TelemetryEvent::cli_invocation("fmt"));

        let live = t.inner.live_path.lock().unwrap().clone();
        assert!(!live.exists(), "no queue file when opted out");
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
