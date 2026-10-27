//! Wrapper-owned `PostHog` events. This pipeline is independent of baml-cli.
//! Events reach disk before handoff; only a detached helper performs HTTP.
#![allow(clippy::print_stderr)]

use std::{
    fs,
    io::{IsTerminal as _, Write as _},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

const POSTHOG_API_KEY: &str = "phc_zgLi9FbzjkLLX6vDsUUixBnDsW6GbN93ohcdboSXSpGy";
const POSTHOG_HOST: &str = "https://us.i.posthog.com";
const CONFIG: &str = "wrapper-telemetry.toml";
const QUEUE: &str = "wrapper-telemetry";
static CURRENT: OnceLock<Option<Telemetry>> = OnceLock::new();
static FLUSHED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default)]
    notified_at: u64,
    anonymous_id: String,
    salt: String,
}
fn yes() -> bool {
    true
}
struct Telemetry {
    config: Config,
    session: String,
    live: PathBuf,
    project: String,
    write: Mutex<()>,
}
pub(super) struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        flush();
    }
}

fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn home() -> PathBuf {
    baml_release::baml_home()
}
fn queue() -> PathBuf {
    home().join(QUEUE)
}
fn opted_out() -> bool {
    baml_env::os_var("DO_NOT_TRACK").is_some_and(|value| {
        value.to_str().is_none_or(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
    })
}
fn truthy(name: &str) -> bool {
    baml_env::raw_var(name).is_some_and(|s| {
        !matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    })
}
fn preference(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .is_some_and(|t| match t.get("enabled") {
            None => true,
            Some(v) => v.as_bool().unwrap_or(false),
        })
}
fn enabled_now() -> bool {
    !POSTHOG_API_KEY.trim().is_empty()
        && !opted_out()
        && preference(&home().join(CONFIG))
        && preference(&home().join("telemetry.toml"))
}
fn load(start: &Path) -> Option<Telemetry> {
    if !enabled_now() {
        return None;
    }
    fs::create_dir_all(home()).ok()?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home().join(".wrapper-telemetry.lock"))
        .ok()?;
    lock.try_lock().ok()?; // Never wait behind another wrapper on the command path.
    let path = home().join(CONFIG);
    let config = if path.exists() {
        toml::from_str::<Config>(&fs::read_to_string(&path).ok()?).ok()?
    } else {
        let config = Config {
            enabled: true,
            notified_at: 0,
            anonymous_id: random(),
            salt: random()[..32].to_string(),
        };
        super::write_text_atomic(&path, &toml::to_string(&config).ok()?).ok()?;
        config
    };
    if !config.enabled || config.anonymous_id.is_empty() || config.salt.is_empty() {
        return None;
    }
    let mut config = config;
    if config.notified_at == 0 && std::io::stderr().is_terminal() && !truthy("CI") {
        eprintln!(
            "\nAttention: BAML collects anonymous wrapper usage telemetry.\nLearn more, including how to opt out, at:\nhttps://boundaryml.com/telemetry\n"
        );
        config.notified_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_millis()
            .try_into()
            .ok()?;
        let _ = super::write_text_atomic(&path, &toml::to_string(&config).ok()?);
    }
    let start = dunce::simplified(start);
    let mut project = start;
    while !project.join(".git").exists() {
        match project.parent() {
            Some(parent) => project = parent,
            None => {
                project = start;
                break;
            }
        }
    }
    let mut digest = Sha256::new();
    digest.update(config.salt.as_bytes());
    digest.update(project.as_os_str().as_encoded_bytes());
    Some(Telemetry {
        config,
        session: random(),
        live: queue().join(format!(
            "live_{}_{}.jsonl",
            std::process::id(),
            &random()[..8]
        )),
        project: format!("{:x}", digest.finalize()),
        write: Mutex::new(()),
    })
}

pub(super) fn invocation(args: &[String], start: &Path) -> Guard {
    CURRENT.get_or_init(|| load(start));
    let command = match args.first().map(String::as_str) {
        Some(
            name @ ("toolchain" | "self-update" | "check" | "run" | "test" | "generate" | "fmt"
            | "lsp" | "playground" | "init" | "new" | "pack" | "describe" | "query" | "ide"
            | "agent" | "telemetry" | "clean" | "update" | "upgrade"),
        ) => name,
        Some("--version" | "-V") => "version",
        Some("--help" | "-h" | "help") => "help",
        _ => "other",
    };
    event("wrapper_invocation", json!({ "command": command }));
    Guard
}

pub(super) fn kind(selector: &str) -> &'static str {
    if super::is_channel(selector) {
        "channel"
    } else if super::is_path_selector(selector) {
        "path"
    } else {
        "version"
    }
}
pub(super) fn outcome<T>(result: &anyhow::Result<T>) -> &'static str {
    if result.is_ok() { "success" } else { "failure" }
}
pub(super) fn event(name: &str, fields: Value) {
    let Some(Some(t)) = CURRENT.get() else {
        return;
    };
    let Ok(_held) = t.write.lock() else {
        return;
    };
    let mut properties = json!({
        "schema_version": 1, "$process_person_profile": false, "$session_id": t.session,
        "project_id": t.project, "wrapper_version": env!("CARGO_PKG_VERSION"), "wrapper_implementation": "rust",
        "system_platform": std::env::consts::OS, "system_architecture": std::env::consts::ARCH,
        "cpu_count": std::thread::available_parallelism().map(std::num::NonZeroUsize::get).unwrap_or(0),
        "is_ci": truthy("CI"), "robot": u8::from(truthy("CI")),
    });
    if let (Some(props), Value::Object(fields)) = (properties.as_object_mut(), fields) {
        props.extend(fields);
    }
    let body = json!({ "api_key": POSTHOG_API_KEY, "event": name, "distinct_id": t.config.anonymous_id, "properties": properties });
    let _ = (|| -> std::io::Result<()> {
        fs::create_dir_all(queue())?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&t.live)?;
        file.write_all(format!("{body}\n").as_bytes())
    })();
}

pub(super) fn flush() {
    if FLUSHED.swap(true, Ordering::Relaxed) {
        return;
    }
    if let Some(Some(t)) = CURRENT.get() {
        if let Some(name) = t
            .live
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("live_"))
        {
            let _ = fs::rename(&t.live, queue().join(format!("sealed_{name}")));
        }
    }
    if !fs::read_dir(queue()).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("sealed_"))
    }) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = Command::new(exe);
    command
        .arg("__flush-wrapper-telemetry")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000);
    }
    let _ = command.spawn();
}

fn queue_name(name: &str) -> bool {
    name.starts_with("live_") || name.starts_with("sealed_") || name.starts_with("sending_")
}
pub(super) fn drain() {
    let dir = queue();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    let enabled = enabled_now() && dir.parent().is_some_and(|p| p.join(CONFIG).exists());
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !queue_name(&name) || !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let age = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| SystemTime::now().duration_since(m).ok());
        if !enabled || age.is_some_and(|age| age > Duration::from_hours(24)) {
            let _ = fs::remove_file(entry.path());
        } else if name.starts_with("live_") && age.is_some_and(|age| age > Duration::from_mins(30))
        {
            let _ = fs::rename(entry.path(), dir.join(name.replacen("live_", "sealed_", 1)));
        }
    }
    if !enabled {
        return;
    }
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return;
    };
    let host = baml_env::raw_var("BAML_WRAPPER_TELEMETRY_HOST")
        .unwrap_or_else(|| POSTHOG_HOST.to_string());
    if baml_release::validate_download_url(&host).is_err() {
        return;
    }
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("sealed_") {
            continue;
        }
        let claimed = dir.join(format!("sending_{}_{}", std::process::id(), name));
        if fs::rename(entry.path(), &claimed).is_err() {
            continue;
        }
        let mut unavailable = false;
        let delivered = fs::read_to_string(&claimed).map_or(true, |text| {
            text.lines().all(|line| {
                let Ok(body) = serde_json::from_str::<Value>(line) else {
                    return true;
                };
                if !enabled_now() {
                    return true;
                }
                match client
                    .post(format!("{}/capture/", host.trim_end_matches('/')))
                    .json(&body)
                    .send()
                {
                    Ok(response) => response.status().is_success(),
                    Err(_) => {
                        unavailable = true;
                        false
                    }
                }
            })
        });
        if delivered {
            let _ = fs::remove_file(&claimed);
        } else {
            let _ = fs::rename(&claimed, entry.path());
        }
        // HTTP rejection concerns this file. A transport failure or timeout
        // means later files would hit the same unavailable host.
        if unavailable {
            break;
        }
    }
}
