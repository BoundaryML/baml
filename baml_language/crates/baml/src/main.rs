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
const CHANNEL_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

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
    auto_check: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    channels: BTreeMap<String, ChannelState>,
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
        println!("baml {}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if args.first().map(String::as_str) == Some("toolchain") {
        args.remove(0);
        return toolchain(args).map(|()| 0);
    }
    if args.first().map(String::as_str) == Some("self-update") {
        return self_update().map(|()| 0);
    }
    pass_through(args)
}

fn baml_home() -> PathBuf {
    env::var_os("BAML_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
                .map(|home| home.join(".baml"))
        })
        .unwrap_or_else(|| PathBuf::from(".baml"))
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
        _ => Err(anyhow!(
            "usage: baml toolchain <install|use|update|list|uninstall> ..."
        )),
    }
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

    let mut command = Command::new(cli);
    command.args(args);
    command.env("BAML_WRAPPER_EXEC", "1");
    command.env("BAML_WRAPPER_RESOLVED_TOOLCHAIN", &version);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        Err(anyhow!("failed to exec baml-cli: {err}"))
    }
    #[cfg(not(unix))]
    {
        let status = command.status().context("failed to run baml-cli")?;
        Ok(status.code().unwrap_or(1))
    }
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
    if is_channel(&selector.selector) {
        let state = read_state();
        if let Some(channel) = state.channels.get(&selector.selector) {
            let current_base = baml_release::manifest_base_url();
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

fn manifest_base_url(override_url: Option<&str>) -> String {
    override_url
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(baml_release::manifest_base_url)
}

fn fetch_manifest(selector: &str, override_url: Option<&str>) -> Result<ToolchainManifest> {
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
    let use_cache = !is_channel(selector)
        || cache_path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age <= CHANNEL_CACHE_TTL);
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
            let client = http_client()?;
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

fn install_toolchain(
    selector: &str,
    activate_channel: bool,
    override_url: Option<&str>,
    force: bool,
) -> Result<()> {
    let manifest = fetch_manifest(selector, override_url)?;
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
            let manifest = fetch_manifest(selector, override_url)?;
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
    install_toolchain(&config.default.selector, true, override_url, false)
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
    reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
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
