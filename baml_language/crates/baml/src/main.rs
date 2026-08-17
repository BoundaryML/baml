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
#[cfg(all(feature = "self-update", not(feature = "no-self-update")))]
use baml_release::WrapperManifest;
use baml_release::{Artifact, Product, ReleaseSpec, ToolchainManifest};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Key, Table, Value};

const CONFIG_FILE: &str = "config.toml";
const STATE_FILE: &str = "state.toml";
const CHANNEL_CACHE_TTL: Duration = Duration::from_hours(24);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Short timeout for the passive background freshness checks that run before
/// normal commands, so an unreachable network can't stall the actual work.
const AUTO_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `baml --version` waits for a local toolchain to report its own
/// version before giving up and printing just the path.
const LOCAL_VERSION_TIMEOUT: Duration = Duration::from_secs(3);

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
    path: Option<String>,
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
  use <canary|nightly|version|path>  Install if needed and select as default
  pin <canary|nightly|version|path>  Select in the nearest baml.toml
  install <canary|nightly|version>   Download without selecting
  update                             Advance the active channel
  status                             Check latest remote version without installing
  list                               Show installed toolchains, local only
  uninstall <version>                Remove an installed concrete version

Local toolchains:
  A selector containing a path separator is a baml-cli binary the wrapper does
  not manage, for running a local build. install, update, and uninstall do not
  apply to it, and `baml ide install` needs a managed toolchain.

    baml toolchain use ~/repos/baml/target/debug/baml-cli
    baml toolchain pin ./target/debug/baml-cli
    BAML_VERSION=./target/debug/baml-cli baml check

Network behavior:
  list is local-only.
  status checks remote metadata but does not install or change selection.
  status is local-only for a path toolchain, which has nothing remote to check.
  use, pin, install, and update may download toolchains or change local state.

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
    #[cfg(all(windows, feature = "self-update", not(feature = "no-self-update")))]
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
    if let Some(cli) = path_selector(&selector.selector) {
        // The failure branch needs the origin most: a broken local toolchain is
        // useless to diagnose without knowing which setting chose it.
        match verify_path_toolchain(cli, &path_selector_origin(&selector.source)) {
            Ok(()) => {
                let version =
                    local_toolchain_version(cli).unwrap_or_else(|| "version unknown".to_string());
                println!("baml toolchain {version} (local: {})", cli.display());
                println!("  {}", path_source_label(&selector.source));
            }
            Err(err) => {
                println!("baml toolchain not usable");
                println!("{err:#}");
            }
        }
        return;
    }
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
            let selector = args.get(1).ok_or_else(|| {
                anyhow!("usage: baml toolchain use <canary|nightly|version|path>")
            })?;
            use_toolchain(selector, manifest_base_url.as_deref())
        }
        Some("pin") => {
            let selector = args.get(1).ok_or_else(|| {
                anyhow!("usage: baml toolchain pin <canary|nightly|version|path>")
            })?;
            if args.len() > 2 {
                return Err(anyhow!(
                    "usage: baml toolchain pin <canary|nightly|version|path>\nunexpected arguments: {}",
                    args[2..].join(" ")
                ));
            }
            pin_toolchain(selector, manifest_base_url.as_deref())
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
    if let Some(cli) = path_selector(&selector.selector) {
        return exec_path_toolchain(cli, &selector, args);
    }
    ensure_toolchain_for_ide_install(&selector, &args)?;
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

/// `baml ide install` needs a released toolchain because the matching VSIX is
/// shipped inside the toolchain archive. Make that command usable on a fresh
/// wrapper install by preparing its selected managed toolchain before passing
/// control to `baml-cli`.
///
/// Local path selectors are deliberately left alone: replacing an explicit
/// local override with a managed release would violate the user's selection,
/// and the toolchain binary already explains that released IDE assets require
/// a managed toolchain.
fn ensure_toolchain_for_ide_install(selector: &ResolvedSelector, args: &[String]) -> Result<()> {
    if !is_ide_install(args) || is_path_selector(&selector.selector) {
        return Ok(());
    }

    let resolved_version = match concrete_version_for_selector(selector) {
        Ok(version) => Some(version),
        Err(_) if channel_needs_initial_setup(&selector.selector, &read_state()) => None,
        Err(error) => return Err(error),
    };
    let toolchain_is_installed = resolved_version.as_deref().is_some_and(toolchain_is_usable);
    if toolchain_is_installed {
        return Ok(());
    }

    install_toolchain(
        &selector.selector,
        is_channel(&selector.selector),
        None,
        false,
    )
    .context("failed to set up the BAML toolchain required by `baml ide install`")
}

fn channel_needs_initial_setup(selector: &str, state: &State) -> bool {
    is_channel(selector) && !state.channels.contains_key(selector)
}

fn is_ide_install(args: &[String]) -> bool {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return false;
    }

    let Some((command, rest)) = next_cli_command(args) else {
        return false;
    };
    if command != "ide" {
        return false;
    }
    next_cli_command(rest).is_some_and(|(subcommand, _)| subcommand == "install")
}

/// Skip the toolchain CLI's global options and return its next positional
/// command. Keeping this small parser here avoids downloading a toolchain for
/// an unrelated invocation whose arguments happen to contain `ide install`.
fn next_cli_command(args: &[String]) -> Option<(&str, &[String])> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(String::as_str) {
        if matches!(
            arg,
            "--directory"
                | "--project"
                | "--output-preset"
                | "--color"
                | "--hyperlinks"
                | "--diagnostic-format"
        ) {
            index += 2;
            continue;
        }
        if arg.starts_with("--directory=")
            || arg.starts_with("--project=")
            || arg.starts_with("--output-preset=")
            || arg.starts_with("--color=")
            || arg.starts_with("--hyperlinks=")
            || arg.starts_with("--diagnostic-format=")
            || arg == "--quiet"
            || arg == "--verbose"
            || arg == "--no-progress"
            || (arg.starts_with('-')
                && arg.len() > 1
                && arg[1..].chars().all(|flag| matches!(flag, 'q' | 'v')))
        {
            index += 1;
            continue;
        }
        if arg.starts_with('-') {
            return None;
        }
        return Some((arg, &args[index + 1..]));
    }
    None
}

fn toolchain_is_usable(version: &str) -> bool {
    let root = toolchains_dir().join(version);
    installed_toolchain_is_usable(&root, version)
}

fn installed_toolchain_is_usable(root: &Path, version: &str) -> bool {
    verify_path_toolchain(&root.join("bin").join(cli_exe_name()), "").is_ok()
        && fs::read_to_string(root.join("VERSION"))
            .is_ok_and(|installed| installed.trim() == version)
}

/// Run a path toolchain. None of the version bookkeeping applies to a binary
/// the wrapper did not install: no VERSION file, no channel freshness warning,
/// and no background manifest refresh. That also keeps the exec fast path
/// unconditional, so a local build costs nothing extra to launch.
fn exec_path_toolchain(cli: &Path, selector: &ResolvedSelector, args: Vec<String>) -> Result<i32> {
    let origin = path_selector_origin(&selector.source);
    verify_path_toolchain(cli, &origin)?;
    reject_self_exec(cli, &origin)?;

    let mut command = Command::new(cli);
    command.args(args);
    command.env("BAML_WRAPPER_EXEC", "1");
    // Deliberately not BAML_WRAPPER_RESOLVED_TOOLCHAIN: that carries a version,
    // and a local build has none. A separate variable also lets the toolchain
    // binary tell the two situations apart, which `baml ide install` needs.
    command.env("BAML_WRAPPER_LOCAL_TOOLCHAIN", cli);

    // Anything verify_path_toolchain could not rule out (wrong architecture,
    // a noexec mount, a missing interpreter) surfaces here, so this message
    // carries the attribution too.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        Err(anyhow!("failed to exec {}: {err}{origin}", cli.display()))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|err| anyhow!("failed to run {}: {err}{origin}", cli.display()))?;
        Ok(status.code().unwrap_or(1))
    }
}

fn active_selector() -> Result<ResolvedSelector> {
    if let Ok(value) = env::var("BAML_VERSION") {
        if !value.trim().is_empty() {
            return Ok(ResolvedSelector {
                selector: normalize_selector(value.trim(), &env::current_dir()?),
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
    let selector = config.default.selector.trim();
    if !selector.is_empty() {
        // A machine-global default resolved against cwd would run a different
        // binary from each directory, so it has to stand on its own.
        if is_path_selector(selector) && !is_cwd_independent(selector) {
            return Err(anyhow!(
                "{} sets default.selector to a relative path ({selector}), which would depend on the current directory.\nUse an absolute path, or run: baml toolchain use <path>",
                config_path().display()
            ));
        }
        return Ok(ResolvedSelector {
            selector: normalize_selector(selector, &env::current_dir()?),
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
                if let Some(path) = toolchain.path {
                    let base = candidate.parent().unwrap_or_else(|| Path::new("."));
                    let selector = normalize_selector(&path, base);
                    return Ok(Some((candidate, selector)));
                }
            }
        }
        if home.as_ref().is_some_and(|home| dir == *home) || !dir.pop() {
            break;
        }
    }
    Ok(None)
}

fn find_project_manifest(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    let home = env::var_os("HOME").map(PathBuf::from);
    loop {
        let candidate = dir.join("baml.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if home.as_ref().is_some_and(|home| dir == *home) || !dir.pop() {
            break;
        }
    }
    None
}

fn concrete_version_for_selector(selector: &ResolvedSelector) -> Result<String> {
    concrete_version_for_selector_with_base(selector, &baml_release::manifest_base_url())
}

fn concrete_version_for_selector_with_base(
    selector: &ResolvedSelector,
    current_base: &str,
) -> Result<String> {
    if is_path_selector(&selector.selector) {
        return Err(anyhow!(
            "active toolchain is a local path and has no version: {}",
            selector.selector
        ));
    }
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

fn cli_exe_name() -> &'static str {
    if cfg!(windows) {
        "baml-cli.exe"
    } else {
        "baml-cli"
    }
}

fn toolchain_cli_path(version: &str) -> PathBuf {
    toolchains_dir()
        .join(version)
        .join("bin")
        .join(cli_exe_name())
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

/// Whether a selector names a local `baml-cli` binary rather than a channel or
/// a version. Channels and versions never contain a path separator, so a bare
/// path is unambiguous and needs no prefix or flag to mark it.
fn is_path_selector(selector: &str) -> bool {
    is_path_selector_on(selector, cfg!(windows))
}

/// Split out so the Windows rule is exercised by tests on every platform,
/// rather than only where `cfg(windows)` holds.
fn is_path_selector_on(selector: &str, windows: bool) -> bool {
    selector.starts_with('~')
        || selector.starts_with('.')
        || selector.contains('/')
        || (windows && selector.contains('\\'))
}

/// The binary a selector points at, if it is a path selector.
fn path_selector(selector: &str) -> Option<&Path> {
    is_path_selector(selector).then(|| Path::new(selector))
}

/// Expand a leading `~` and absolutize against `base`, the directory the path
/// was written in.
fn resolve_selector_path(raw: &str, base: &Path) -> PathBuf {
    let home = home_dir();
    resolve_selector_path_with_home(raw, base, home.as_deref())
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Whether a path selector stands on its own, independent of the directory it
/// is read from. Only absolute paths and a `~` that can actually be expanded
/// qualify: with no home directory `~/x` falls back to being joined onto the
/// current one, and `~user` is not a form we expand at all, so both would leave
/// a global default meaning something different in every directory.
fn is_cwd_independent(selector: &str) -> bool {
    is_cwd_independent_with_home(selector, home_dir().as_deref())
}

fn is_cwd_independent_with_home(selector: &str, home: Option<&Path>) -> bool {
    if Path::new(selector).is_absolute() {
        return true;
    }
    (selector == "~" || selector.starts_with("~/")) && home.is_some()
}

fn resolve_selector_path_with_home(raw: &str, base: &Path, home: Option<&Path>) -> PathBuf {
    let joined = join_selector_path(raw, base, home);
    let normalized = lexically_normalize(&joined);
    // Lexical `..` removal disagrees with the filesystem only when a symlinked
    // directory is followed by `..`. Keep the joined form in that case, so
    // tidying a path can never turn a working one into a broken one.
    let tidy = if normalized.exists() || !joined.exists() {
        normalized
    } else {
        joined
    };
    with_exe_suffix(tidy)
}

/// Collapse `.` and `..` without consulting the filesystem, so the path reads
/// cleanly wherever it is echoed back while any symlink in it survives.
/// Canonicalizing instead would resolve a `current -> release-N` symlink down
/// to its physical target, freezing the selector on whatever it pointed at the
/// day it was set, and on Windows would hand back a `\\?\C:\...` verbatim path.
fn lexically_normalize(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                // `..` above the root is the root.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Windows developers type `baml-cli`; the file on disk is `baml-cli.exe`.
#[cfg(windows)]
fn with_exe_suffix(path: PathBuf) -> PathBuf {
    if path.extension().is_some() || path.exists() {
        return path;
    }
    let candidate = path.with_extension("exe");
    if candidate.is_file() { candidate } else { path }
}

#[cfg(not(windows))]
fn with_exe_suffix(path: PathBuf) -> PathBuf {
    path
}

fn join_selector_path(raw: &str, base: &Path, home: Option<&Path>) -> PathBuf {
    let raw = raw.trim();
    if raw == "~" || raw.starts_with("~/") {
        if let Some(home) = home {
            return home.join(raw.trim_start_matches('~').trim_start_matches('/'));
        }
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

/// Absolutize a path selector, so every later consumer (exec, error text,
/// status output) sees the same path regardless of cwd. Channels and versions
/// pass through untouched. Absolutizing is idempotent: the result still reads
/// as a path selector.
fn normalize_selector(selector: &str, base: &Path) -> String {
    if is_path_selector(selector) {
        return resolve_selector_path(selector, base).display().to_string();
    }
    selector.to_string()
}

/// Where a path toolchain came from. Unlike [`selector_origin`], this always
/// answers, including for the global config: with a local toolchain the whole
/// question is which forgotten setting picked this binary.
fn path_source_label(source: &SelectorSource) -> String {
    match source {
        SelectorSource::Env => "set by $BAML_VERSION".to_string(),
        SelectorSource::Project(path) => format!("set by {}", path.display()),
        SelectorSource::Config | SelectorSource::Fallback => {
            format!("set by default.selector in {}", config_path().display())
        }
    }
}

/// The `set by ...` line appended to path-toolchain errors, so a stale
/// override is traceable to whatever set it.
fn path_selector_origin(source: &SelectorSource) -> String {
    format!("\n  {}", path_source_label(source))
}

/// Ask a local toolchain what version it is. There is no manifest to consult
/// for a binary the wrapper did not install, and reporting only a path would
/// leave `baml --version` with no version at all for anything reading it.
/// Returns `None` if the binary fails, answers with nothing, or does not answer
/// promptly, in which case the caller falls back to reporting just the path.
fn local_toolchain_version(cli: &Path) -> Option<String> {
    use std::process::Stdio;

    let mut child = Command::new(cli)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("BAML_WRAPPER_EXEC", "1")
        .env("BAML_WRAPPER_LOCAL_TOOLCHAIN", cli)
        .spawn()
        .ok()?;

    let deadline = std::time::Instant::now() + LOCAL_VERSION_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            // A binary that will not answer promptly is not worth blocking
            // `--version` on, and a hung child must not outlive us.
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Refuse to exec the wrapper itself. `baml` and `baml-cli` are built into the
/// same directory, so pointing at the wrapper is a one-character slip; without
/// this it re-resolves the same selector and execs itself forever, silently on
/// unix and as an unbounded process tree everywhere else.
fn reject_self_exec(cli: &Path, origin: &str) -> Result<()> {
    let Ok(current) = env::current_exe() else {
        return Ok(());
    };
    let same = match (current.canonicalize(), cli.canonicalize()) {
        (Ok(current), Ok(cli)) => current == cli,
        _ => current == cli,
    };
    if same {
        return Err(anyhow!(
            "toolchain path points at the baml wrapper itself: {}{origin}\nPoint at the {} binary instead, which is built alongside it.",
            cli.display(),
            cli_exe_name()
        ));
    }
    Ok(())
}

/// Check that a path selector points at something runnable. `origin` is a
/// pre-formatted `set by ...` line, or empty when the path came straight from
/// the command line and needs no attribution.
fn verify_path_toolchain(cli: &Path, origin: &str) -> Result<()> {
    if cli.is_dir() {
        return Err(anyhow!(
            "toolchain path is a directory: {}{origin}\nPoint at the {} binary, e.g. {}",
            cli.display(),
            cli_exe_name(),
            cli.join("bin").join(cli_exe_name()).display()
        ));
    }
    if !cli.exists() {
        return Err(anyhow!(
            "toolchain binary not found: {}{origin}",
            cli.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(cli)
            .with_context(|| format!("failed to read {}", cli.display()))?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err(anyhow!(
                "toolchain binary is not executable: {}{origin}",
                cli.display()
            ));
        }
    }
    Ok(())
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
/// [`should_attempt_refresh`] throttles retries). Returns `None` when
/// nothing is due or `[update] auto_check = false` — the common case, which
/// costs only a few mtime checks and lets the caller keep the exec fast path.
fn start_lazy_refresh(selector: &ResolvedSelector) -> Option<LazyRefresh> {
    if !read_config().update.auto_check_enabled() {
        return None;
    }

    let manifest_due = is_channel(&selector.selector)
        && should_attempt_refresh(
            &manifest_cache_dir(&baml_release::manifest_base_url())
                .join(format!("{}.json", selector.selector)),
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

/// A cache file is due for a refresh attempt when both the file itself and
/// its attempt marker are older than the TTL. The marker is touched before
/// every attempt (success or failure) so an unreachable network is retried at
/// most once per TTL window instead of on every command.
fn should_attempt_refresh(cache_path: &Path) -> bool {
    let marker = refresh_marker_path(cache_path);
    if !file_older_than(cache_path, CHANNEL_CACHE_TTL)
        || !file_older_than(&marker, CHANNEL_CACHE_TTL)
    {
        return false;
    }
    if let Some(parent) = marker.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&marker, "").is_ok()
}

fn refresh_marker_path(cache_path: &Path) -> PathBuf {
    let mut path = cache_path.as_os_str().to_owned();
    path.push(".last-check");
    PathBuf::from(path)
}

/// True when the file is missing or its mtime is older than `ttl`.
fn file_older_than(path: &Path, ttl: Duration) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_none_or(|age| age > ttl)
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
    if is_path_selector(selector) {
        return Err(anyhow!(
            "{selector} is a local path; there is nothing to install.\nRun: baml toolchain use {selector}"
        ));
    }
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

fn prepare_toolchain_selector(
    selector: &str,
    base: &Path,
    override_url: Option<&str>,
) -> Result<String> {
    let selector = normalize_selector(selector, base);
    if is_path_selector(&selector) {
        verify_path_toolchain(Path::new(&selector), "")?;
        return Ok(selector);
    }
    if is_channel(&selector) {
        install_toolchain(&selector, true, override_url, false)?;
    } else {
        let target = baml_release::release_host_target_triple()?;
        if !toolchain_cli_path(&selector).exists() {
            let manifest = fetch_manifest(&selector, override_url, FetchPolicy::CacheAllowed)?;
            let artifact = manifest.artifact_for_target(target)?.clone();
            install_manifest_artifact(&manifest.version, target, artifact, false)?;
        }
    }
    Ok(selector)
}

fn use_toolchain(selector: &str, override_url: Option<&str>) -> Result<()> {
    let selector = prepare_toolchain_selector(selector, &env::current_dir()?, override_url)?;

    let mut config = read_config();
    config.default.selector.clone_from(&selector);
    write_config(&config)?;
    println!("selected BAML toolchain {selector}");
    Ok(())
}

fn pin_toolchain(selector: &str, override_url: Option<&str>) -> Result<()> {
    let cwd = env::current_dir()?;
    let manifest_path = find_project_manifest(&cwd).ok_or_else(|| {
        anyhow!(
            "no baml.toml found from {} up to the home directory\nRun this command inside a BAML project.",
            cwd.display()
        )
    })?;
    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let normalized_selector = normalize_selector(selector, &cwd);
    let selector_key = if is_channel(&normalized_selector) {
        "channel"
    } else if is_path_selector(&normalized_selector) {
        "path"
    } else {
        "version"
    };
    let updated = pin_selector_in_manifest(&content, selector_key, &normalized_selector)
        .with_context(|| format!("failed to update {}", manifest_path.display()))?;

    let selector = prepare_toolchain_selector(&normalized_selector, &cwd, override_url)?;

    write_text_atomic(&manifest_path, &updated)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    println!(
        "pinned BAML toolchain {selector} in {}",
        manifest_path.display()
    );
    Ok(())
}

fn pin_selector_in_manifest(content: &str, selector_key: &str, selector: &str) -> Result<String> {
    let mut document = content
        .parse::<DocumentMut>()
        .context("invalid baml.toml")?;
    if document.get("toolchain").is_none() {
        document.insert("toolchain", Item::Table(Table::new()));
    }
    let toolchain = document["toolchain"]
        .as_table_like_mut()
        .context("`toolchain` must be a TOML table or inline table")?;
    let decorated_selector = ["version", "channel", "path"]
        .into_iter()
        .find_map(|selector| {
            let key = toolchain.key(selector)?.clone();
            let item = toolchain.remove(selector)?;
            Some((key, item))
        });
    toolchain.remove("version");
    toolchain.remove("channel");
    toolchain.remove("path");

    let mut key = Key::new(selector_key);
    let mut selector_value = Value::from(selector);
    if let Some((old_key, old_item)) = decorated_selector {
        *key.leaf_decor_mut() = old_key.leaf_decor().clone();
        if let Some(old_value) = old_item.as_value() {
            *selector_value.decor_mut() = old_value.decor().clone();
        }
    }
    toolchain
        .entry_format(&key)
        .or_insert(Item::Value(selector_value));
    Ok(document.to_string())
}

fn update_toolchain(override_url: Option<&str>) -> Result<()> {
    let config = read_config();
    if is_path_selector(&config.default.selector) {
        println!(
            "active selector {} is a local path and is not managed by the wrapper.\nRun: baml toolchain use canary",
            config.default.selector
        );
        return Ok(());
    }
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

    if let Some(cli) = path_selector(&selector.selector) {
        println!("source: {}", path_source_label(&selector.source));
        match verify_path_toolchain(cli, "") {
            Ok(()) => {
                let version =
                    local_toolchain_version(cli).unwrap_or_else(|| "version unknown".to_string());
                println!("reported version: {version}");
                println!("status: local toolchain binary, not managed by the wrapper");
            }
            Err(err) => println!("status: local toolchain binary is unusable\n{err:#}"),
        }
        println!("Remote versions were not checked.");
        println!("Run: baml toolchain use canary to go back to a managed toolchain");
        return Ok(());
    }

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
    // `list` is what a user runs to work out why nothing runs, so a broken
    // baml.toml or config has to show up here rather than be swallowed.
    match active_selector() {
        Ok(active) if active.selector != config.default.selector => println!(
            "active selector: {}{}",
            active.selector,
            selector_annotation(&active)
        ),
        Ok(_) => {}
        Err(err) => {
            println!("active selector: (unresolved)");
            println!("{err:#}");
        }
    }
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
    if is_path_selector(version) {
        return Err(anyhow!(
            "{version} is a local path and is not managed by the wrapper, so there is nothing to uninstall.\nTo stop using it, select a managed toolchain instead.\nRun: baml toolchain use canary\nOr:  baml toolchain use nightly"
        ));
    }
    let dir = toolchains_dir().join(version);
    if !dir.exists() {
        return Err(anyhow!("BAML toolchain {version} is not installed"));
    }
    fs::remove_dir_all(dir)?;
    println!("uninstalled BAML toolchain {version}");
    Ok(())
}

#[cfg(any(not(feature = "self-update"), feature = "no-self-update"))]
fn self_update() -> Result<()> {
    Err(anyhow!(
        "self-update is disabled in this build.\nUpdate BAML with your package manager."
    ))
}

#[cfg(all(feature = "self-update", not(feature = "no-self-update")))]
fn self_update() -> Result<()> {
    let current = env::current_exe()?;
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

#[cfg(all(feature = "self-update", not(feature = "no-self-update")))]
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

#[cfg(all(feature = "self-update", not(feature = "no-self-update")))]
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

#[cfg(all(windows, feature = "self-update", not(feature = "no-self-update")))]
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

    #[test]
    fn ide_install_is_detected_with_editor_and_global_options() {
        for args in [
            &["ide", "install", "--code"][..],
            &["ide", "install", "--cursor"][..],
            &["--quiet", "ide", "install", "--output-dir", "out"][..],
            &["ide", "install", "--output-dir", "help"][..],
        ] {
            let args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
            assert!(is_ide_install(&args), "{args:?}");
        }
    }

    #[test]
    fn ide_install_help_and_other_commands_do_not_trigger_setup() {
        for args in [
            &["ide", "install", "--help"][..],
            &["help", "ide", "install"][..],
            &["ide", "status"][..],
            &["run", "ide", "install"][..],
        ] {
            let args = args.iter().map(ToString::to_string).collect::<Vec<_>>();
            assert!(!is_ide_install(&args), "{args:?}");
        }
    }

    #[test]
    fn only_channels_without_active_state_need_initial_setup() {
        let mut state = State::default();
        assert!(channel_needs_initial_setup("canary", &state));
        assert!(!channel_needs_initial_setup("0.16.0", &state));

        state.channels.insert(
            "canary".to_string(),
            ChannelState {
                active_version: "0.16.0".to_string(),
                resolved_at: "x".to_string(),
                manifest_path: "y".to_string(),
                manifest_base_url: Some("https://old.example.test".to_string()),
            },
        );
        assert!(!channel_needs_initial_setup("canary", &state));
    }

    #[cfg(unix)]
    #[test]
    fn managed_toolchain_without_executable_permission_is_unusable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("0.16.0");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(root.join("VERSION"), "0.16.0\n").unwrap();
        let cli = bin.join(cli_exe_name());
        fs::write(&cli, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!installed_toolchain_is_usable(&root, "0.16.0"));

        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(installed_toolchain_is_usable(&root, "0.16.0"));
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

    #[test]
    fn channels_and_versions_are_not_path_selectors() {
        for selector in ["canary", "nightly", "0.412.0", "5.0.0-pre.20210317.1"] {
            assert!(!is_path_selector(selector), "{selector}");
        }
    }

    #[test]
    fn anything_with_a_path_shape_is_a_path_selector() {
        for selector in [
            "~/repos/baml/target/debug/baml-cli",
            "./target/debug/baml-cli",
            "../baml/target/debug/baml-cli",
            "/usr/local/bin/baml-cli",
            "target/debug/baml-cli",
            // Forward slashes are a path on Windows too, so a drive-letter
            // path in this form is detected on every platform.
            "C:/repos/baml/target/debug/baml-cli.exe",
        ] {
            assert!(is_path_selector(selector), "{selector}");
        }
    }

    /// Backslash counts as a separator only on Windows, where a channel or
    /// version can never contain one.
    #[test]
    fn windows_backslash_paths_are_path_selectors() {
        for selector in [
            r"C:\repos\baml\target\debug\baml-cli.exe",
            r".\target\debug\baml-cli.exe",
            r"..\baml\target\debug\baml-cli.exe",
            r"\\server\share\baml-cli.exe",
        ] {
            assert!(is_path_selector_on(selector, true), "{selector}");
        }
    }

    #[test]
    fn backslash_alone_is_not_a_separator_off_windows() {
        assert!(!is_path_selector_on(r"weird\name", false));
    }

    /// Channels and versions stay non-paths under the Windows rule too.
    #[test]
    fn windows_rule_still_admits_channels_and_versions() {
        for selector in ["canary", "nightly", "0.412.0"] {
            assert!(!is_path_selector_on(selector, true), "{selector}");
        }
    }

    // Absoluteness is platform-defined (`/opt` has a root but no prefix on
    // Windows, so it is not absolute there), so the resolution tests build
    // their paths from platform-appropriate roots.
    #[cfg(not(windows))]
    const TEST_BASE: &str = "/work/demo";
    #[cfg(not(windows))]
    const TEST_ABSOLUTE: &str = "/opt/baml-cli";
    #[cfg(not(windows))]
    const TEST_HOME: &str = "/home/tester";

    #[cfg(windows)]
    const TEST_BASE: &str = r"C:\work\demo";
    #[cfg(windows)]
    const TEST_ABSOLUTE: &str = r"C:\opt\baml-cli.exe";
    #[cfg(windows)]
    const TEST_HOME: &str = r"C:\Users\tester";

    #[test]
    fn relative_path_resolves_against_the_declaring_directory() {
        let base = Path::new(TEST_BASE);
        assert_eq!(
            resolve_selector_path("../baml/target/debug/baml-cli", base),
            base.parent().unwrap().join("baml/target/debug/baml-cli")
        );
    }

    #[test]
    fn absolute_path_ignores_the_declaring_directory() {
        assert_eq!(
            resolve_selector_path(TEST_ABSOLUTE, Path::new(TEST_BASE)),
            PathBuf::from(TEST_ABSOLUTE)
        );
    }

    #[test]
    fn tilde_expands_to_home() {
        assert_eq!(
            resolve_selector_path_with_home(
                "~/builds/baml-cli",
                Path::new(TEST_BASE),
                Some(Path::new(TEST_HOME))
            ),
            Path::new(TEST_HOME).join("builds/baml-cli")
        );
    }

    /// `..` is collapsed so the path reads cleanly in `--version`, `status`,
    /// and the config file it gets written to.
    #[test]
    fn resolved_path_loses_its_parent_segments() {
        let tmp = tempfile::tempdir().unwrap();
        let build = tmp.path().join("build");
        let proj = tmp.path().join("proj");
        fs::create_dir_all(&build).unwrap();
        fs::create_dir_all(&proj).unwrap();
        let cli = build.join(cli_exe_name());
        fs::write(&cli, "").unwrap();

        let resolved = resolve_selector_path(&format!("../build/{}", cli_exe_name()), &proj);
        assert_eq!(resolved, cli);
        assert!(!resolved.to_string_lossy().contains(".."), "{resolved:?}");
    }

    /// A path that does not exist yet is still tidied, so the error names a
    /// readable path rather than one full of `..`.
    #[test]
    fn missing_path_is_still_tidied() {
        let tmp = tempfile::tempdir().unwrap();
        let resolved = resolve_selector_path("../build/baml-cli", tmp.path());
        assert_eq!(
            resolved,
            tmp.path().parent().unwrap().join("build/baml-cli")
        );
    }

    /// Tidying must not resolve symlinks. A `current -> release-N` indirection
    /// has to keep following repoints instead of being frozen to whatever it
    /// pointed at when the selector was set.
    #[test]
    #[cfg(unix)]
    fn symlinked_path_is_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let release = tmp.path().join("release-1");
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("baml-cli"), "").unwrap();
        let current = tmp.path().join("current");
        std::os::unix::fs::symlink(&release, &current).unwrap();

        let resolved = resolve_selector_path("current/baml-cli", tmp.path());
        assert_eq!(resolved, current.join("baml-cli"));
        assert!(
            !resolved.starts_with(&release),
            "symlink was resolved away: {resolved:?}"
        );
    }

    #[test]
    fn parent_segments_above_the_root_are_dropped() {
        let root = Path::new(TEST_BASE)
            .components()
            .next()
            .unwrap()
            .as_os_str();
        let mut deep = PathBuf::from(root);
        deep.push("a");
        deep.push("..");
        deep.push("..");
        deep.push("b");
        assert_eq!(lexically_normalize(&deep), Path::new(root).join("b"));
    }

    /// Without a home directory the `~` stays literal rather than silently
    /// resolving somewhere unintended.
    #[test]
    fn tilde_without_home_is_left_relative() {
        assert_eq!(
            resolve_selector_path_with_home("~/builds/baml-cli", Path::new(TEST_BASE), None),
            Path::new(TEST_BASE).join("~/builds/baml-cli")
        );
    }

    #[test]
    fn normalizing_an_absolute_path_selector_is_idempotent() {
        let once = normalize_selector("./target/debug/baml-cli", Path::new(TEST_BASE));
        assert!(Path::new(&once).is_absolute(), "{once}");
        assert!(is_path_selector(&once), "{once}");
        assert_eq!(normalize_selector(&once, Path::new("/elsewhere")), once);
    }

    #[test]
    fn normalizing_leaves_channels_and_versions_alone() {
        assert_eq!(normalize_selector("canary", Path::new(TEST_BASE)), "canary");
        assert_eq!(
            normalize_selector("0.412.0", Path::new(TEST_BASE)),
            "0.412.0"
        );
    }

    #[test]
    fn pin_version_adds_toolchain_table_without_reformatting_manifest() {
        let input = "# project comment\n[package]\nname = \"demo\"\n";
        let output =
            pin_selector_in_manifest(input, "version", "0.15.1-nightly.20260807.a").unwrap();
        assert!(output.starts_with(input), "{output}");
        assert!(
            output.contains("[toolchain]\nversion = \"0.15.1-nightly.20260807.a\""),
            "{output}"
        );
    }

    #[test]
    fn pin_version_replaces_conflicting_selector_and_preserves_comments() {
        let input = "[package]\nname = \"demo\"\n\n[toolchain]\n# keep this comment\nchannel = \"nightly\"\n";
        let output = pin_selector_in_manifest(input, "version", "0.16.0").unwrap();
        assert!(output.contains("# keep this comment"), "{output}");
        assert!(output.contains("version = \"0.16.0\""), "{output}");
        assert!(!output.contains("channel ="), "{output}");
    }

    #[test]
    fn pin_version_rejects_non_table_toolchain() {
        let error = pin_selector_in_manifest("toolchain = \"nightly\"\n", "version", "0.16.0")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("must be a TOML table or inline table"),
            "{error}"
        );
    }

    #[test]
    fn pin_selector_preserves_inline_toolchain_table() {
        let output = pin_selector_in_manifest(
            "toolchain = { version = \"0.15.0\" }\n\n[package]\nname = \"demo\"\n",
            "channel",
            "nightly",
        )
        .unwrap();
        assert!(
            output.contains("toolchain = { channel = \"nightly\" }"),
            "{output}"
        );
    }

    #[test]
    fn pin_selector_writes_each_supported_selector_kind() {
        for (key, selector) in [
            ("channel", "canary"),
            ("channel", "nightly"),
            ("version", "0.16.0"),
            ("path", "/work/baml-cli"),
        ] {
            let output =
                pin_selector_in_manifest("[toolchain]\nversion = \"0.15.0\"\n", key, selector)
                    .unwrap();
            assert!(
                output.contains(&format!("{key} = \"{selector}\"")),
                "{output}"
            );
            for conflicting_key in ["version", "channel", "path"] {
                if conflicting_key != key {
                    assert!(
                        !output.contains(&format!("{conflicting_key} =")),
                        "{output}"
                    );
                }
            }
        }
    }

    #[test]
    fn path_toolchain_pointing_at_a_directory_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let err = verify_path_toolchain(dir.path(), "")
            .unwrap_err()
            .to_string();
        assert!(err.contains("is a directory"), "{err}");
        assert!(err.contains("baml-cli"), "{err}");
    }

    #[test]
    fn missing_path_toolchain_reports_its_origin() {
        let missing = Path::new(TEST_BASE).join("missing").join(cli_exe_name());
        let origin = path_selector_origin(&SelectorSource::Env);
        let err = verify_path_toolchain(&missing, &origin)
            .unwrap_err()
            .to_string();
        assert!(err.contains("toolchain binary not found"), "{err}");
        assert!(err.contains("set by $BAML_VERSION"), "{err}");
    }

    #[test]
    fn absolute_selectors_stand_on_their_own() {
        assert!(is_cwd_independent_with_home(TEST_ABSOLUTE, None));
        assert!(is_cwd_independent_with_home(
            TEST_ABSOLUTE,
            Some(Path::new(TEST_HOME))
        ));
    }

    /// `~/x` only stands on its own if there is a home directory to expand it
    /// to; otherwise it falls back to being joined onto the current directory.
    #[test]
    fn tilde_stands_on_its_own_only_with_a_home() {
        assert!(is_cwd_independent_with_home(
            "~/builds/baml-cli",
            Some(Path::new(TEST_HOME))
        ));
        assert!(!is_cwd_independent_with_home("~/builds/baml-cli", None));
    }

    /// `~user` is not a form we expand, so it must not be waved through by a
    /// bare `~` prefix check.
    #[test]
    fn other_users_tilde_does_not_stand_on_its_own() {
        assert!(!is_cwd_independent_with_home(
            "~alice/builds/baml-cli",
            Some(Path::new(TEST_HOME))
        ));
    }

    #[test]
    fn relative_selectors_never_stand_on_their_own() {
        for selector in ["./target/debug/baml-cli", "../build/baml-cli"] {
            assert!(
                !is_cwd_independent_with_home(selector, Some(Path::new(TEST_HOME))),
                "{selector}"
            );
        }
    }

    /// The global config is the source most likely to be forgotten, and the one
    /// `selector_origin` stays quiet about, so a path toolchain must still name
    /// it.
    #[test]
    fn config_sourced_path_still_names_its_origin() {
        let label = path_source_label(&SelectorSource::Config);
        assert!(label.contains("default.selector"), "{label}");
        assert!(
            label.contains(&config_path().display().to_string()),
            "{label}"
        );
    }

    #[test]
    fn path_selector_has_no_version_to_resolve() {
        let selector = resolved(TEST_ABSOLUTE, SelectorSource::Config);
        let err = concrete_version_for_selector_with_base(&selector, "https://example.test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("has no version"), "{err}");
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
    fn missing_file_counts_as_older_than_ttl() {
        let missing = Path::new("/tmp/definitely-missing-baml-freshness-file");
        assert!(file_older_than(missing, CHANNEL_CACHE_TTL));
    }

    #[test]
    fn fresh_file_is_not_older_than_ttl() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        assert!(!file_older_than(tmp.path(), CHANNEL_CACHE_TTL));
    }

    #[test]
    fn refresh_attempt_is_throttled_by_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("latest-commit.json");
        // Cache missing and no marker: attempt allowed, marker gets created.
        assert!(should_attempt_refresh(&cache));
        assert!(refresh_marker_path(&cache).exists());
        // Fresh marker: no retry within the TTL window.
        assert!(!should_attempt_refresh(&cache));
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
