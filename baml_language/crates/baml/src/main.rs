#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, anyhow};
use baml_release::{Artifact, Product, ReleaseSpec, ToolchainManifest, WrapperManifest};
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.toml";
const STATE_FILE: &str = "state.toml";
const CHANNEL_CACHE_TTL: Duration = Duration::from_hours(24);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Short timeout for the passive background freshness checks that run before
/// normal commands, so an unreachable network can't stall the actual work.
const AUTO_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    default: DefaultConfig,
    #[serde(default)]
    update: UpdateConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct DefaultConfig {
    selector: String,
}

impl Default for DefaultConfig {
    fn default() -> Self {
        Self {
            selector: "canary".to_string(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateConfig {
    /// Whether normal commands may refresh the channel-manifest freshness
    /// cache over the network once per TTL window. Defaults to on; set
    /// `[update] auto_check = false` to opt out. The same setting governs the
    /// toolchain binary's agent-skill freshness check.
    auto_check: Option<bool>,
}

impl UpdateConfig {
    fn auto_check_enabled(&self) -> bool {
        self.auto_check.unwrap_or(true)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    channels: BTreeMap<String, ChannelState>,
    /// Sections owned by other writers (e.g. `[skills]`, written by
    /// `baml agent install`), preserved verbatim across wrapper writes.
    #[serde(flatten)]
    rest: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChannelState {
    active_version: String,
    resolved_at: String,
    manifest_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectConfig {
    toolchain: Option<ProjectToolchain>,
}

#[derive(Debug, Deserialize)]
struct ProjectToolchain {
    version: Option<String>,
    channel: Option<String>,
}

enum SelectorSource {
    Env,
    Project(PathBuf),
    Config,
    Fallback,
}

struct ResolvedSelector {
    selector: String,
    source: SelectorSource,
}

#[derive(Clone, Copy)]
enum FetchPolicy {
    CacheAllowed,
    ForceRemote,
}

const TOOLCHAIN_HELP: &str = r#"BAML toolchain management

Usage:
  baml toolchain <command>

Commands:
  use <canary|nightly|version>       Install if needed and select as default
  install <canary|nightly|version>   Download without selecting
  update                             Advance the active channel
  status                             Check latest remote version without installing
  list                               Show installed toolchains, local only
  uninstall <version>                Remove an installed concrete version

Network behavior:
  list is local-only.
  status checks remote metadata but does not install or change selection.
  use, install, and update may download toolchains or change local state.

Wrapper updates:
  baml self-update                   Update curl-installed wrapper only
"#;

const SELF_UPDATE_HELP: &str = r#"BAML wrapper self-update

Usage:
  baml self-update

Updates the wrapper binary only. It never installs or changes the active
language toolchain. Package-manager-managed wrappers refuse self-update and
print the package-manager upgrade command instead.
"#;

fn main() {
    let exit = match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(exit);
}

fn run() -> Result<i32> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    #[cfg(windows)]
    if args.first().map(String::as_str) == Some("--replace") {
        args.remove(0);
        return replace_running_exe(args).map(|()| 0);
    }
    if matches!(args.first().map(String::as_str), Some("--version" | "-V")) {
        print_version();
        return Ok(0);
    }
    if args.first().map(String::as_str) == Some("toolchain") {
        args.remove(0);
        return toolchain(args).map(|()| 0);
    }
    if args.first().map(String::as_str) == Some("self-update") {
        if args.get(1).is_some_and(|arg| is_help_arg(arg)) {
            print!("{SELF_UPDATE_HELP}");
            return Ok(0);
        }
        if args.len() > 1 {
            return Err(anyhow!(
                "usage: baml self-update\nunexpected arguments: {}",
                args[1..].join(" ")
            ));
        }
        return self_update().map(|()| 0);
    }
    pass_through(args)
}

fn print_version() {
    println!("baml wrapper {}", env!("CARGO_PKG_VERSION"));
    let selector = match active_selector() {
        Ok(selector) => selector,
        Err(err) => {
            println!("baml toolchain not resolved");
            println!("{err:#}");
            return;
        }
    };
    let version = match concrete_version_for_selector(&selector) {
        Ok(version) => version,
        Err(_) if is_channel(&selector.selector) => {
            println!(
                "baml toolchain not installed{}",
                selector_annotation(&selector)
            );
            println!("Run: baml toolchain use {}", selector.selector);
            return;
        }
        Err(err) => {
            println!("baml toolchain not resolved");
            println!("{err:#}");
            return;
        }
    };
    if !toolchain_cli_path(&version).exists() {
        println!(
            "baml toolchain not installed ({version}){}",
            selector_annotation(&selector)
        );
        if is_channel(&selector.selector) {
            println!("Run: baml toolchain use {}", selector.selector);
        } else {
            println!("Run: baml toolchain install {version}");
        }
        return;
    }
    match verify_toolchain_version_file(&version) {
        Ok(()) => println!("baml toolchain {version}{}", selector_annotation(&selector)),
        Err(_) => {
            println!("baml toolchain corrupt ({version})");
            println!("Run: baml toolchain install {version} --force");
        }
    }
}

/// Where the active selector was resolved from, as a trailing parenthetical
/// suffix for the `baml toolchain <version>` line (rustup-style, e.g. cargo's
/// "(overridden by ...)"). Returns `None` for the global default and the
/// built-in fallback, which need no annotation.
fn selector_origin(source: &SelectorSource) -> Option<String> {
    match source {
        SelectorSource::Env => Some("from $BAML_VERSION".to_string()),
        SelectorSource::Project(path) => Some(format!("from {}", path.display())),
        SelectorSource::Config | SelectorSource::Fallback => None,
    }
}

/// Build the `(channel, from <source>)` suffix shown after a resolved toolchain
/// version. The channel name is included only for channel selectors (canary /
/// nightly), since an exact-version selector already equals the printed version.
fn selector_annotation(selector: &ResolvedSelector) -> String {
    let channel = is_channel(&selector.selector).then_some(selector.selector.as_str());
    match (channel, selector_origin(&selector.source)) {
        (Some(channel), Some(origin)) => format!(" ({channel}, {origin})"),
        (Some(channel), None) => format!(" ({channel})"),
        (None, Some(origin)) => format!(" ({origin})"),
        (None, None) => String::new(),
    }
}

fn baml_home() -> PathBuf {
    baml_release::baml_home()
}

fn config_path() -> PathBuf {
    baml_home().join(CONFIG_FILE)
}

fn state_path() -> PathBuf {
    baml_home().join(STATE_FILE)
}

fn toolchains_dir() -> PathBuf {
    baml_home().join("toolchains")
}

fn manifest_cache_dir(base_url: &str) -> PathBuf {
    if base_url == baml_release::DEFAULT_MANIFEST_BASE_URL {
        return baml_home().join("manifest-cache").join("prod");
    }
    let mut hasher = DefaultHasher::new();
    base_url.hash(&mut hasher);
    baml_home()
        .join("manifest-cache")
        .join("override")
        .join(format!("{:016x}", hasher.finish()))
}

fn read_config() -> Config {
    fs::read_to_string(config_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_config(config: &Config) -> Result<()> {
    write_toml_atomic(&config_path(), config)
}

fn read_state() -> State {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_state(state: &State) -> Result<()> {
    write_toml_atomic(&state_path(), state)
}

fn write_toml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, toml::to_string_pretty(value)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn toolchain(args: Vec<String>) -> Result<()> {
    let (args, manifest_base_url) = parse_manifest_base_url(args)?;
    match args.first().map(String::as_str) {
        Some("--help" | "-h" | "help") | None => {
            print!("{TOOLCHAIN_HELP}");
            Ok(())
        }
        Some("install") => {
            let selector = args
                .get(1)
                .ok_or_else(|| anyhow!("usage: baml toolchain install <canary|nightly|version>"))?;
            let force = args.iter().any(|arg| arg == "--force");
            install_toolchain(selector, false, manifest_base_url.as_deref(), force)
        }
        Some("use") => {
            let selector = args
                .get(1)
                .ok_or_else(|| anyhow!("usage: baml toolchain use <canary|nightly|version>"))?;
            use_toolchain(selector, manifest_base_url.as_deref())
        }
        Some("update") => update_toolchain(manifest_base_url.as_deref()),
        Some("status") => status_toolchain(manifest_base_url.as_deref()),
        Some("list") => {
            list_toolchains();
            Ok(())
        }
        Some("uninstall") => {
            let version = args
                .get(1)
                .ok_or_else(|| anyhow!("usage: baml toolchain uninstall <version>"))?;
            uninstall_toolchain(version)
        }
        Some(other) => Err(anyhow!(
            "unknown toolchain command {other:?}\n\n{TOOLCHAIN_HELP}"
        )),
    }
}

fn is_help_arg(arg: &str) -> bool {
    matches!(arg, "--help" | "-h" | "help")
}

fn parse_manifest_base_url(mut args: Vec<String>) -> Result<(Vec<String>, Option<String>)> {
    let mut manifest_base_url = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--manifest-base-url" {
            let value = args
                .get(i + 1)
                .ok_or_else(|| anyhow!("--manifest-base-url requires a value"))?
                .trim_end_matches('/')
                .to_string();
            args.drain(i..=i + 1);
            manifest_base_url = Some(value);
        } else {
            i += 1;
        }
    }
    Ok((args, manifest_base_url))
}

fn pass_through(args: Vec<String>) -> Result<i32> {
    let selector = active_selector()?;
    let version = concrete_version_for_selector(&selector)?;
    let cli = toolchain_cli_path(&version);
    if !cli.exists() {
        return Err(missing_toolchain_error(&selector, &version));
    }
    verify_toolchain_version_file(&version)?;

    // Warnings from the existing caches print immediately (no network). The
    // agent-skill warning is NOT printed here: it lives in the toolchain
    // binary (which ships nightly, unlike the wrapper), so printing it here
    // too would double it up.
    let channel_warned = warn_if_channel_outdated(&selector, &version);
    // A cache refresh (due at most once per TTL window) runs in the
    // background while the command itself runs, instead of stalling it.
    let refresh = start_lazy_refresh(&selector);

    let mut command = Command::new(cli);
    command.args(args);
    command.env("BAML_WRAPPER_EXEC", "1");
    command.env("BAML_WRAPPER_RESOLVED_TOOLCHAIN", &version);

    let Some(refresh) = refresh else {
        // Common case: nothing to refresh, so the wrapper can hand the
        // process over entirely.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let err = command.exec();
            return Err(anyhow!("failed to exec baml-cli: {err}"));
        }
        #[cfg(not(unix))]
        {
            let status = command.status().context("failed to run baml-cli")?;
            return Ok(status.code().unwrap_or(1));
        }
    };

    // Refresh in flight: run the command as a child (exec would kill the
    // refresh thread), then give the refresh whatever remains of its budget
    // and surface any warnings the fresh caches newly justify.
    let status = command.status().context("failed to run baml-cli")?;
    refresh.wait();
    if !channel_warned {
        warn_if_channel_outdated(&selector, &version);
    }
    Ok(status.code().unwrap_or(1))
}

fn active_selector() -> Result<ResolvedSelector> {
    if let Ok(value) = env::var("BAML_VERSION") {
        if !value.trim().is_empty() {
            return Ok(ResolvedSelector {
                selector: value,
                source: SelectorSource::Env,
            });
        }
    }
    if let Some((path, selector)) = project_toolchain_selector()? {
        return Ok(ResolvedSelector {
            selector,
            source: SelectorSource::Project(path),
        });
    }
    let config = read_config();
    if !config.default.selector.trim().is_empty() {
        return Ok(ResolvedSelector {
            selector: config.default.selector,
            source: SelectorSource::Config,
        });
    }
    Ok(ResolvedSelector {
        selector: "canary".to_string(),
        source: SelectorSource::Fallback,
    })
}

fn project_toolchain_selector() -> Result<Option<(PathBuf, String)>> {
    let mut dir = env::current_dir()?;
    let home = env::var_os("HOME").map(PathBuf::from);
    loop {
        let candidate = dir.join("baml.toml");
        if candidate.exists() {
            let text = fs::read_to_string(&candidate)?;
            let config = toml::from_str::<ProjectConfig>(&text)
                .with_context(|| format!("failed to parse {}", candidate.display()))?;
            if let Some(toolchain) = config.toolchain {
                if let Some(version) = toolchain.version {
                    return Ok(Some((candidate, version)));
                }
                if let Some(channel) = toolchain.channel {
                    return Ok(Some((candidate, channel)));
                }
            }
        }
        if home.as_ref().is_some_and(|home| dir == *home) || !dir.pop() {
            break;
        }
    }
    Ok(None)
}

fn concrete_version_for_selector(selector: &ResolvedSelector) -> Result<String> {
    concrete_version_for_selector_with_base(selector, &baml_release::manifest_base_url())
}

fn concrete_version_for_selector_with_base(
    selector: &ResolvedSelector,
    current_base: &str,
) -> Result<String> {
    if is_channel(&selector.selector) {
        let state = read_state();
        if let Some(channel) = state.channels.get(&selector.selector) {
            if channel
                .manifest_base_url
                .as_deref()
                .is_some_and(|base| base.trim_end_matches('/') != current_base)
            {
                return Err(anyhow!(
                    "channel {} was selected from a different manifest source.\nRun: baml toolchain use {} under the current manifest source.",
                    selector.selector,
                    selector.selector
                ));
            }
            return Ok(channel.active_version.clone());
        }
        return match &selector.source {
            SelectorSource::Project(path) => Err(anyhow!(
                "baml.toml [toolchain] selects channel {}, but no active concrete version is recorded locally.\nRun: baml toolchain use {}",
                selector.selector,
                selector.selector
            )
            .context(format!("project config: {}", path.display()))),
            _ => Err(anyhow!(
                "error: no BAML toolchain is installed.\nRun: baml toolchain use canary\nOr:  baml toolchain use nightly"
            )),
        };
    }
    Ok(selector.selector.clone())
}

fn missing_toolchain_error(selector: &ResolvedSelector, version: &str) -> anyhow::Error {
    let installed = installed_toolchains().join(", ");
    match &selector.source {
        SelectorSource::Project(path) => anyhow!(
            "{} [toolchain] pins version {version}, but it isn't installed.\nInstalled toolchains: {}\nRun: baml toolchain install {version}",
            path.display(),
            if installed.is_empty() {
                "(none)"
            } else {
                &installed
            }
        ),
        _ => anyhow!(
            "BAML toolchain {version} is not installed.\nInstalled toolchains: {}\nRun: baml toolchain install {version}",
            if installed.is_empty() {
                "(none)"
            } else {
                &installed
            }
        ),
    }
}

fn toolchain_cli_path(version: &str) -> PathBuf {
    let exe = if cfg!(windows) {
        "baml-cli.exe"
    } else {
        "baml-cli"
    };
    toolchains_dir().join(version).join("bin").join(exe)
}

fn verify_toolchain_version_file(version: &str) -> Result<()> {
    let path = toolchains_dir().join(version).join("VERSION");
    let actual = fs::read_to_string(&path).with_context(|| {
        format!(
            "failed to read toolchain VERSION file at {}",
            path.display()
        )
    })?;
    if actual.trim() != version {
        anyhow::bail!(
            "BAML toolchain {version} is corrupt: VERSION contains {}.\nRun: baml toolchain install {version} --force",
            actual.trim()
        );
    }
    Ok(())
}

fn is_channel(selector: &str) -> bool {
    selector == "canary" || selector == "nightly"
}

/// Bold-yellow lowercase `warning` prefix, matching the styled diagnostics the
/// toolchain CLI emits (see `baml_exec::diag_print`). Color is dropped
/// automatically when stderr is not a TTY.
fn warning_prefix() -> impl std::fmt::Display {
    console::Style::new()
        .yellow()
        .bold()
        .for_stderr()
        .apply_to("warning")
}

/// An in-flight background refresh of the channel-manifest freshness cache.
/// Started before the main command runs and joined after it finishes, so the
/// network latency hides behind the command's own runtime instead of stalling
/// it up front. (The agent-skill freshness cache is refreshed by the
/// toolchain binary, which runs its own equivalent of this.)
struct LazyRefresh {
    done: std::sync::mpsc::Receiver<()>,
    deadline: std::time::Instant,
}

impl LazyRefresh {
    /// Wait for the refresh to finish, but never past the shared
    /// [`AUTO_CHECK_TIMEOUT`] deadline (anchored at refresh start, so a
    /// command that ran 3s only waits up to 2s more). On timeout the thread
    /// is abandoned; cache writes are atomic, so dying mid-write is safe.
    fn wait(self) {
        let remaining = self
            .deadline
            .saturating_duration_since(std::time::Instant::now());
        let _ = self.done.recv_timeout(remaining);
    }
}

/// Kick off a background refresh of any freshness cache older than the TTL,
/// at most one attempt per TTL window (failures are silent; the marker in
/// [`baml_release::skills::should_attempt_refresh`] throttles retries). Returns `None` when
/// nothing is due or `[update] auto_check = false` — the common case, which
/// costs only a few mtime checks and lets the caller keep the exec fast path.
fn start_lazy_refresh(selector: &ResolvedSelector) -> Option<LazyRefresh> {
    if !read_config().update.auto_check_enabled() {
        return None;
    }

    let manifest_due = is_channel(&selector.selector)
        && baml_release::skills::should_attempt_refresh(
            &manifest_cache_dir(&baml_release::manifest_base_url())
                .join(format!("{}.json", selector.selector)),
            CHANNEL_CACHE_TTL,
        );
    if !manifest_due {
        return None;
    }

    let deadline = std::time::Instant::now() + AUTO_CHECK_TIMEOUT;
    let channel = selector.selector.clone();
    let (sender, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = fetch_manifest_with_timeout(
            &channel,
            None,
            FetchPolicy::ForceRemote,
            AUTO_CHECK_TIMEOUT,
        );
        let _ = sender.send(());
    });
    Some(LazyRefresh { done, deadline })
}

/// Passive freshness check for channel selectors, run on every pass-through
/// invocation. Reads only the locally cached channel manifest (written by
/// explicit toolchain commands and the background auto-refresh); it never
/// touches the network, and stays silent if no cache exists yet. Returns
/// whether it printed, so a post-refresh re-check can avoid duplicating the
/// warning within one invocation.
fn warn_if_channel_outdated(selector: &ResolvedSelector, active_version: &str) -> bool {
    if !is_channel(&selector.selector) {
        return false;
    }
    let cache_path = manifest_cache_dir(&baml_release::manifest_base_url())
        .join(format!("{}.json", selector.selector));
    if !cached_manifest_is_newer(&cache_path, active_version) {
        return false;
    }
    eprintln!(
        "{}: Your version of baml for toolchain: {} is outdated. Update it with baml toolchain update.",
        warning_prefix(),
        selector.selector
    );
    true
}

fn cached_manifest_is_newer(cache_path: &Path, active_version: &str) -> bool {
    let Ok(text) = fs::read_to_string(cache_path) else {
        return false;
    };
    let Ok(manifest) = toml_or_json::<ToolchainManifest>(&text) else {
        return false;
    };
    manifest.version != active_version
}

fn manifest_base_url(override_url: Option<&str>) -> String {
    override_url
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(baml_release::manifest_base_url)
}

fn fetch_manifest(
    selector: &str,
    override_url: Option<&str>,
    policy: FetchPolicy,
) -> Result<ToolchainManifest> {
    fetch_manifest_with_timeout(selector, override_url, policy, HTTP_TIMEOUT)
}

fn fetch_manifest_with_timeout(
    selector: &str,
    override_url: Option<&str>,
    policy: FetchPolicy,
    timeout: Duration,
) -> Result<ToolchainManifest> {
    let base = manifest_base_url(override_url);
    let url = if is_channel(selector) {
        format!("{base}/{selector}.json")
    } else {
        format!("{base}/version/{selector}.json")
    };
    let cache_path = if is_channel(selector) {
        manifest_cache_dir(&base).join(format!("{selector}.json"))
    } else {
        manifest_cache_dir(&base)
            .join("version")
            .join(format!("{selector}.json"))
    };
    let use_cache = should_use_manifest_cache(selector, &cache_path, policy);
    let mut fetched_remote = false;
    let text = if use_cache {
        fs::read_to_string(&cache_path).ok()
    } else {
        None
    };
    let text = match text {
        Some(text) => text,
        None => {
            fetched_remote = true;
            let client = http_client_with_timeout(timeout)?;
            client
                .get(&url)
                .send()
                .with_context(|| format!("failed to fetch {url}"))?
                .error_for_status()
                .with_context(|| format!("failed to fetch {url}"))?
                .text()
                .with_context(|| format!("failed to read {url}"))?
        }
    };
    let manifest: ToolchainManifest = toml_or_json(&text)?;
    manifest.validate()?;
    if fetched_remote {
        write_text_atomic(&cache_path, &text)?;
    }
    let version_cache_path = manifest_cache_dir(&base)
        .join("version")
        .join(format!("{}.json", manifest.version));
    if !version_cache_path.exists() {
        write_text_atomic(&version_cache_path, &text)?;
    }
    Ok(manifest)
}

fn toml_or_json<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T> {
    serde_json::from_str(text).context("invalid JSON manifest")
}

fn should_use_manifest_cache(selector: &str, cache_path: &Path, policy: FetchPolicy) -> bool {
    match policy {
        FetchPolicy::ForceRemote => false,
        FetchPolicy::CacheAllowed => {
            !is_channel(selector)
                || cache_path
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age <= CHANNEL_CACHE_TTL)
        }
    }
}

fn install_toolchain(
    selector: &str,
    activate_channel: bool,
    override_url: Option<&str>,
    force: bool,
) -> Result<()> {
    install_toolchain_with_policy(
        selector,
        activate_channel,
        override_url,
        force,
        FetchPolicy::CacheAllowed,
    )
}

fn install_toolchain_with_policy(
    selector: &str,
    activate_channel: bool,
    override_url: Option<&str>,
    force: bool,
    policy: FetchPolicy,
) -> Result<()> {
    let manifest = fetch_manifest(selector, override_url, policy)?;
    let target = baml_release::release_host_target_triple()?;
    let artifact = manifest.artifact_for_target(target)?.clone();
    install_manifest_artifact(&manifest.version, target, artifact, force)?;
    if activate_channel && is_channel(selector) {
        let mut state = read_state();
        state.channels.insert(
            selector.to_string(),
            ChannelState {
                active_version: manifest.version.clone(),
                resolved_at: manifest.released_at.clone(),
                manifest_path: manifest_cache_dir(&manifest_base_url(override_url))
                    .join("version")
                    .join(format!("{}.json", manifest.version))
                    .display()
                    .to_string(),
                manifest_base_url: Some(manifest_base_url(override_url)),
            },
        );
        write_state(&state)?;
    }
    println!("installed BAML toolchain {}", manifest.version);
    Ok(())
}

fn install_manifest_artifact(
    version: &str,
    target: &str,
    artifact: Artifact,
    force: bool,
) -> Result<()> {
    fs::create_dir_all(toolchains_dir())?;
    if force {
        let dir = toolchains_dir().join(version);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
    }
    let fetcher = baml_release::Fetcher::from_artifact(
        ReleaseSpec {
            version: version.to_string(),
            target: target.to_string(),
        },
        Product::Toolchain,
        artifact,
    );
    fetcher
        .install_to_toolchain_root(&toolchains_dir())
        .map(|_| ())
        .map_err(|err| anyhow!("{err}"))
}

fn use_toolchain(selector: &str, override_url: Option<&str>) -> Result<()> {
    if is_channel(selector) {
        install_toolchain(selector, true, override_url, false)?;
    } else {
        let target = baml_release::release_host_target_triple()?;
        if !toolchain_cli_path(selector).exists() {
            let manifest = fetch_manifest(selector, override_url, FetchPolicy::CacheAllowed)?;
            let artifact = manifest.artifact_for_target(target)?.clone();
            install_manifest_artifact(&manifest.version, target, artifact, false)?;
        }
    }

    let mut config = read_config();
    config.default.selector = selector.to_string();
    write_config(&config)?;
    println!("selected BAML toolchain {selector}");
    Ok(())
}

fn update_toolchain(override_url: Option<&str>) -> Result<()> {
    let config = read_config();
    if !is_channel(&config.default.selector) {
        println!(
            "active selector {} is an exact version and does not advance automatically.\nRun: baml toolchain use canary\nOr:  baml toolchain use nightly",
            config.default.selector
        );
        return Ok(());
    }
    install_toolchain_with_policy(
        &config.default.selector,
        true,
        override_url,
        false,
        FetchPolicy::ForceRemote,
    )
    .with_context(|| {
        format!(
            "failed to refresh {} from the remote manifest; the active installed toolchain was left unchanged",
            config.default.selector
        )
    })?;
    Ok(())
}

fn status_toolchain(override_url: Option<&str>) -> Result<()> {
    let selector = active_selector()?;
    let base = manifest_base_url(override_url);
    println!("active selector: {}", selector.selector);

    if is_channel(&selector.selector) {
        match concrete_version_for_selector_with_base(&selector, &base) {
            Ok(version) => println!("active version: {version}"),
            Err(_) => println!("active version: (none recorded locally)"),
        }

        let manifest = fetch_manifest(&selector.selector, override_url, FetchPolicy::ForceRemote)
            .with_context(|| {
                format!(
                    "failed to check latest {} from the remote manifest; local toolchain state was left unchanged",
                    selector.selector
                )
            })?;
        println!("latest {}: {}", selector.selector, manifest.version);

        match concrete_version_for_selector_with_base(&selector, &base).ok() {
            Some(active) if active == manifest.version => {
                println!("status: up to date");
            }
            Some(_) => {
                println!(
                    "status: a newer {} toolchain is available",
                    selector.selector
                );
                println!("Run: baml toolchain update");
            }
            None => {
                println!(
                    "status: no active {} toolchain is recorded locally",
                    selector.selector
                );
                println!("Run: baml toolchain use {}", selector.selector);
            }
        }
        return Ok(());
    }

    if toolchain_cli_path(&selector.selector).exists() {
        println!("selected version: {}", selector.selector);
    } else {
        println!("selected version: {} (not installed)", selector.selector);
        println!("Run: baml toolchain install {}", selector.selector);
    }

    let canary = fetch_manifest("canary", override_url, FetchPolicy::ForceRemote)
        .context("failed to check latest canary from the remote manifest")?;
    let nightly = fetch_manifest("nightly", override_url, FetchPolicy::ForceRemote)
        .context("failed to check latest nightly from the remote manifest")?;
    println!("latest canary: {}", canary.version);
    println!("latest nightly: {}", nightly.version);
    println!("status: exact versions do not advance automatically");
    println!("Run: baml toolchain use canary");
    println!("Or:  baml toolchain use nightly");
    Ok(())
}

fn installed_toolchains() -> Vec<String> {
    let mut versions = fs::read_dir(toolchains_dir())
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(std::result::Result::ok))
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(std::fs::FileType::is_dir)
                .and_then(|_| entry.file_name().into_string().ok())
        })
        .filter(|name| !name.starts_with('.'))
        .collect::<Vec<_>>();
    versions.sort();
    versions
}

fn list_toolchains() {
    let config = read_config();
    let state = read_state();
    println!("default selector: {}", config.default.selector);
    for (channel, state) in state.channels {
        println!("{channel}: {}", state.active_version);
    }
    let installed = installed_toolchains();
    if installed.is_empty() {
        println!("installed toolchains: (none)");
    } else {
        println!("installed toolchains:");
        for version in installed {
            println!("  {version}");
        }
    }
    println!();
    println!("Remote versions were not checked.");
    println!("Run: baml toolchain status");
}

fn uninstall_toolchain(version: &str) -> Result<()> {
    let dir = toolchains_dir().join(version);
    if !dir.exists() {
        return Err(anyhow!("BAML toolchain {version} is not installed"));
    }
    fs::remove_dir_all(dir)?;
    println!("uninstalled BAML toolchain {version}");
    Ok(())
}

fn self_update() -> Result<()> {
    let current = env::current_exe()?;
    if is_managed_install(&current) {
        return Err(anyhow!(
            "this BAML wrapper appears to be managed by a package manager.\nRun: brew upgrade baml or your system package-manager upgrade command."
        ));
    }
    let manifest = fetch_wrapper_manifest()?;
    let target = baml_release::release_host_target_triple()?;
    let artifact = manifest.artifact_for_target(target)?.clone();
    let binary = if cfg!(windows) { "baml.exe" } else { "baml" };
    let fetcher = baml_release::Fetcher::from_artifact(
        ReleaseSpec {
            version: manifest.version.clone(),
            target: target.to_string(),
        },
        Product::Wrapper,
        artifact,
    );
    let bytes = fetcher
        .fetch_binary(binary)
        .map_err(|err| anyhow!("{err}"))?;
    let tmp = current.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(windows)]
    {
        std::process::Command::new(&current)
            .arg("--replace")
            .arg(&tmp)
            .arg(&current)
            .spawn()
            .with_context(|| format!("failed to spawn updater {}", current.display()))?;
        println!("updating BAML wrapper to {}", manifest.version);
        return Ok(());
    }
    fs::rename(tmp, current)?;
    println!("updated BAML wrapper to {}", manifest.version);
    Ok(())
}

fn fetch_wrapper_manifest() -> Result<WrapperManifest> {
    let base = baml_release::manifest_base_url();
    let url = format!("{base}/wrapper.json");
    let cache_path = manifest_cache_dir(&base).join("wrapper.json");
    let client = http_client()?;
    let text = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to fetch {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to fetch {url}"))?
        .text()
        .with_context(|| format!("failed to read {url}"))?;
    write_text_atomic(&cache_path, &text)?;
    let manifest: WrapperManifest =
        serde_json::from_str(&text).context("invalid wrapper manifest")?;
    manifest.validate()?;
    Ok(manifest)
}

fn http_client() -> Result<reqwest::blocking::Client> {
    http_client_with_timeout(HTTP_TIMEOUT)
}

fn http_client_with_timeout(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(timeout.min(Duration::from_secs(10)))
        .timeout(timeout)
        .build()
        .context("failed to build HTTP client")
}

#[cfg(windows)]
fn replace_running_exe(args: Vec<String>) -> Result<()> {
    if args.len() != 2 {
        anyhow::bail!("usage: baml --replace <tmp> <current>");
    }
    let tmp = PathBuf::from(&args[0]);
    let current = PathBuf::from(&args[1]);
    let mut last_error = None;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(250));
        match fs::remove_file(&current) {
            Ok(()) => match fs::rename(&tmp, &current) {
                Ok(()) => return Ok(()),
                Err(err) => last_error = Some(err),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                match fs::rename(&tmp, &current) {
                    Ok(()) => return Ok(()),
                    Err(err) => last_error = Some(err),
                }
            }
            Err(err) => last_error = Some(err),
        }
    }
    Err(anyhow!(
        "failed to replace {} with {}: {}",
        current.display(),
        tmp.display(),
        last_error
            .map(|err| err.to_string())
            .unwrap_or_else(|| "timed out".to_string())
    ))
}

fn write_text_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn is_managed_install(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/opt/homebrew/")
        || text.contains("/usr/local/Cellar/")
        || text.starts_with("/usr/bin/")
        || text.starts_with("/opt/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_allowed_uses_fresh_channel_cache() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(should_use_manifest_cache(
            "canary",
            tmp.path(),
            FetchPolicy::CacheAllowed
        ));
    }

    #[test]
    fn force_remote_ignores_fresh_channel_cache() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(!should_use_manifest_cache(
            "nightly",
            tmp.path(),
            FetchPolicy::ForceRemote
        ));
    }

    #[test]
    fn exact_version_cache_is_immutable_and_allowed() {
        let missing = Path::new("/tmp/definitely-missing-baml-manifest.json");
        assert!(should_use_manifest_cache(
            "0.11.0",
            missing,
            FetchPolicy::CacheAllowed
        ));
    }

    fn resolved(selector: &str, source: SelectorSource) -> ResolvedSelector {
        ResolvedSelector {
            selector: selector.to_string(),
            source,
        }
    }

    #[test]
    fn annotation_shows_channel_and_project_source() {
        let path = PathBuf::from("/work/demo/baml.toml");
        assert_eq!(
            selector_annotation(&resolved("canary", SelectorSource::Project(path))),
            " (canary, from /work/demo/baml.toml)"
        );
    }

    #[test]
    fn annotation_shows_channel_for_default_source_without_origin() {
        assert_eq!(
            selector_annotation(&resolved("canary", SelectorSource::Config)),
            " (canary)"
        );
        assert_eq!(
            selector_annotation(&resolved("nightly", SelectorSource::Fallback)),
            " (nightly)"
        );
    }

    #[test]
    fn annotation_omits_channel_for_exact_version() {
        assert_eq!(
            selector_annotation(&resolved("0.11.0", SelectorSource::Config)),
            ""
        );
        assert_eq!(
            selector_annotation(&resolved(
                "0.11.0",
                SelectorSource::Project(PathBuf::from("/work/demo/baml.toml"))
            )),
            " (from /work/demo/baml.toml)"
        );
    }

    #[test]
    fn annotation_reports_env_override() {
        assert_eq!(
            selector_annotation(&resolved("nightly", SelectorSource::Env)),
            " (nightly, from $BAML_VERSION)"
        );
    }

    fn write_cached_manifest(version: &str) -> tempfile::NamedTempFile {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(
            tmp.path(),
            format!(
                r#"{{"schema":1,"version":"{version}","channel":"canary","released_at":"2026-07-10T00:00:00Z","artifacts":{{}}}}"#
            ),
        )
        .unwrap();
        tmp
    }

    #[test]
    fn cached_manifest_newer_than_active_version_is_detected() {
        let tmp = write_cached_manifest("0.12.0");
        assert!(cached_manifest_is_newer(tmp.path(), "0.11.0"));
    }

    #[test]
    fn cached_manifest_matching_active_version_is_not_outdated() {
        let tmp = write_cached_manifest("0.11.0");
        assert!(!cached_manifest_is_newer(tmp.path(), "0.11.0"));
    }

    #[test]
    fn missing_manifest_cache_stays_silent() {
        let missing = Path::new("/tmp/definitely-missing-baml-manifest.json");
        assert!(!cached_manifest_is_newer(missing, "0.11.0"));
    }

    #[test]
    fn unparseable_manifest_cache_stays_silent() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        fs::write(tmp.path(), "not json").unwrap();
        assert!(!cached_manifest_is_newer(tmp.path(), "0.11.0"));
    }

    #[test]
    fn refresh_attempt_is_throttled_by_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("latest-commit.json");
        // Cache missing and no marker: attempt allowed, marker gets created.
        assert!(baml_release::skills::should_attempt_refresh(
            &cache,
            CHANNEL_CACHE_TTL
        ));
        // Fresh marker: no retry within the TTL window.
        assert!(!baml_release::skills::should_attempt_refresh(
            &cache,
            CHANNEL_CACHE_TTL
        ));
    }

    #[test]
    fn state_write_preserves_foreign_sections() {
        let text = "[channels.canary]\nactive_version = \"0.11.0\"\nresolved_at = \"x\"\nmanifest_path = \"y\"\n\n[skills]\ninstalled_commit = \"abc\"\ninstalled_at = \"2026-07-10T00:00:00Z\"\n";
        let state: State = toml::from_str(text).unwrap();
        let out = toml::to_string_pretty(&state).unwrap();
        assert!(out.contains("[skills]"), "{out}");
        assert!(out.contains("installed_commit = \"abc\""), "{out}");
        assert!(out.contains("[channels.canary]"), "{out}");
    }

    #[test]
    fn auto_check_defaults_on_and_respects_optout() {
        assert!(UpdateConfig::default().auto_check_enabled());
        let config: Config = toml::from_str("[update]\nauto_check = false\n").unwrap();
        assert!(!config.update.auto_check_enabled());
        let config: Config = toml::from_str("[update]\nauto_check = true\n").unwrap();
        assert!(config.update.auto_check_enabled());
    }
}
