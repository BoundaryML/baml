use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use baml_runtime::RuntimeCliDefaults;
use baml_types::GeneratorOutputType;
use etcetera::AppStrategy;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::AuthCommands;
use crate::commands::Commands;

const TELEMETRY_DISABLE_ENV: &str = "BAML_CLI_DISABLE_TELEMETRY";
const POSTHOG_CAPTURE_URL: &str = "https://us.i.posthog.com/i/v0/e";
const POSTHOG_API_KEY: &str = "phc_732PWG6HFZ75S7h0TK2AuqRVkqZDiD4WePE9gXYJkOu";
const EVENT_NAME: &str = "baml.engine_cli.command.started";
const REQUEST_TIMEOUT_MS: u64 = 800;
const SCHEMA_VERSION: u32 = 1;
const MACHINE_ID_FILE: &str = "telemetry_machine_id";

#[derive(Debug, Clone, Serialize)]
struct TelemetryEvent {
    api_key: &'static str,
    event: &'static str,
    distinct_id: String,
    #[serde(rename = "$process_person_profile")]
    process_person_profile: bool,
    properties: TelemetryProperties,
}

#[derive(Debug, Clone, Serialize)]
struct TelemetryProperties {
    surface: &'static str,
    schema_version: u32,
    cli_version: &'static str,
    command: &'static str,
    subcommand: Option<&'static str>,
    caller_output_type: String,
    caller_runtime: &'static str,
    ci: bool,
    ci_provider: CiProvider,
    project_hash: String,
    project_hash_source: ProjectHashSource,
    machine_id: String,
    session_id: String,
    argv_len: usize,
    feature_flags_count: usize,
    os_platform: &'static str,
    os_arch: &'static str,
    stdout_is_tty: bool,
    stderr_is_tty: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProjectHashSource {
    FromArg,
    CwdBamlSrc,
    Cwd,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CiProvider {
    None,
    GithubActions,
    Gitlab,
    Circleci,
    Buildkite,
    Jenkins,
    AzurePipelines,
}

pub(crate) fn capture_command_started(
    argv: &[String],
    command: &Commands,
    caller_type: RuntimeCliDefaults,
) {
    capture_command_started_with_sender(argv, command, caller_type, spawn_send_event);
}

fn capture_command_started_with_sender<F>(
    argv: &[String],
    command: &Commands,
    caller_type: RuntimeCliDefaults,
    sender: F,
) where
    F: FnOnce(TelemetryEvent),
{
    if telemetry_disabled() || is_lsp_command(command) {
        return;
    }

    let machine_id = get_or_create_machine_id();
    let session_id = Uuid::new_v4().to_string();
    let payload = build_command_started_event(argv, command, caller_type, machine_id, session_id);

    sender(payload);
}

fn spawn_send_event(payload: TelemetryEvent) {
    let _ = std::thread::Builder::new()
        .name("baml-cli-telemetry".to_string())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };

            let _ = runtime.block_on(send_event(&payload));
        });
}

fn build_command_started_event(
    argv: &[String],
    command: &Commands,
    caller_type: RuntimeCliDefaults,
    machine_id: String,
    session_id: String,
) -> TelemetryEvent {
    let (command_name, subcommand_name) = map_command(command);
    let (project_hash, project_hash_source) = compute_project_hash(argv);
    let ci_provider = detect_ci_provider();

    TelemetryEvent {
        api_key: POSTHOG_API_KEY,
        event: EVENT_NAME,
        distinct_id: machine_id.clone(),
        process_person_profile: false,
        properties: TelemetryProperties {
            surface: "engine_cli",
            schema_version: SCHEMA_VERSION,
            cli_version: env!("CARGO_PKG_VERSION"),
            command: command_name,
            subcommand: subcommand_name,
            caller_output_type: caller_type.output_type.to_string(),
            caller_runtime: map_caller_runtime(caller_type),
            ci: env_truthy("CI"),
            ci_provider,
            project_hash,
            project_hash_source,
            machine_id,
            session_id,
            argv_len: argv.len(),
            feature_flags_count: count_feature_flags(argv),
            os_platform: std::env::consts::OS,
            os_arch: std::env::consts::ARCH,
            stdout_is_tty: std::io::stdout().is_terminal(),
            stderr_is_tty: std::io::stderr().is_terminal(),
        },
    }
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn telemetry_disabled() -> bool {
    env_truthy(TELEMETRY_DISABLE_ENV)
}

fn is_lsp_command(command: &Commands) -> bool {
    matches!(command, Commands::LanguageServer(_))
}

fn map_command(command: &Commands) -> (&'static str, Option<&'static str>) {
    match command {
        Commands::Init(_) => ("init", None),
        Commands::Generate(_) => ("generate", None),
        Commands::Check(_) => ("check", None),
        Commands::Serve(_) => ("serve", None),
        Commands::Dev(_) => ("dev", None),
        Commands::Auth(auth_command) => match auth_command {
            AuthCommands::Login(_) => ("auth", Some("login")),
            AuthCommands::Token(_) => ("auth", Some("token")),
        },
        Commands::Login(_) => ("login", None),
        Commands::Deploy(_) => ("deploy", None),
        Commands::Format(_) => ("fmt", None),
        Commands::Test(_) => ("test", None),
        Commands::DumpHIR(_) => ("dump_hir", None),
        Commands::DumpBytecode(_) => ("dump_bytecode", None),
        Commands::LanguageServer(_) => ("lsp", None),
        Commands::Repl(_) => ("repl", None),
        Commands::Optimize(_) => ("optimize", None),
    }
}

fn map_caller_runtime(caller_type: RuntimeCliDefaults) -> &'static str {
    match caller_type.output_type {
        GeneratorOutputType::OpenApi => "native",
        GeneratorOutputType::PythonPydantic | GeneratorOutputType::PythonPydanticV1 => "python",
        GeneratorOutputType::Typescript | GeneratorOutputType::TypescriptReact => "typescript",
        GeneratorOutputType::RubySorbet => "ruby",
        GeneratorOutputType::Go => "go",
        _ => "unknown",
    }
}

fn compute_project_hash(argv: &[String]) -> (String, ProjectHashSource) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (selected_path, source) = select_project_hash_path(argv, &cwd);
    let normalized_path = normalize_path_for_hash(&selected_path, &cwd);
    let mut hasher = Sha256::new();
    hasher.update(normalized_path.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    (hash.chars().take(8).collect(), source)
}

fn select_project_hash_path(argv: &[String], cwd: &Path) -> (PathBuf, ProjectHashSource) {
    if let Some(path) = from_arg_path(argv) {
        return (path, ProjectHashSource::FromArg);
    }

    let baml_src_path = cwd.join("baml_src");
    if baml_src_path.exists() {
        return (baml_src_path, ProjectHashSource::CwdBamlSrc);
    }

    (cwd.to_path_buf(), ProjectHashSource::Cwd)
}

fn from_arg_path(argv: &[String]) -> Option<PathBuf> {
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--from" {
            if let Some(path) = argv.get(index + 1) {
                return Some(PathBuf::from(path));
            }
        } else if let Some(value) = arg.strip_prefix("--from=") {
            if !value.trim().is_empty() {
                return Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    None
}

fn normalize_path_for_hash(path: &Path, cwd: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let canonical = dunce::canonicalize(&absolute).unwrap_or(absolute);
    dunce::simplified(&canonical)
        .to_string_lossy()
        .replace('\\', "/")
}

fn get_or_create_machine_id() -> String {
    if let Some(existing_machine_id) = read_machine_id() {
        return existing_machine_id;
    }

    let machine_id = format!("baml_machine_{}", Uuid::new_v4());
    let _ = write_machine_id(&machine_id);
    machine_id
}

fn read_machine_id() -> Option<String> {
    let path = machine_id_path().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let machine_id = content.trim();
    if machine_id.is_empty() {
        None
    } else {
        Some(machine_id.to_string())
    }
}

fn write_machine_id(machine_id: &str) -> anyhow::Result<()> {
    let path = machine_id_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, machine_id)?;
    Ok(())
}

fn machine_id_path() -> anyhow::Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = machine_id_path_override() {
        return Ok(path);
    }

    Ok(app_strategy()?.in_config_dir(MACHINE_ID_FILE))
}

fn app_strategy() -> anyhow::Result<impl AppStrategy> {
    Ok(etcetera::choose_app_strategy(etcetera::AppStrategyArgs {
        top_level_domain: "com".to_string(),
        author: "boundaryml".to_string(),
        app_name: "baml-cli".to_string(),
    })?)
}

fn env_is_set(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|value| !value.is_empty())
}

fn detect_ci_provider() -> CiProvider {
    if env_is_set("GITHUB_ACTIONS") {
        CiProvider::GithubActions
    } else if env_is_set("GITLAB_CI") {
        CiProvider::Gitlab
    } else if env_is_set("CIRCLECI") {
        CiProvider::Circleci
    } else if env_is_set("BUILDKITE") {
        CiProvider::Buildkite
    } else if env_is_set("JENKINS_URL") {
        CiProvider::Jenkins
    } else if env_is_set("TF_BUILD") {
        CiProvider::AzurePipelines
    } else {
        CiProvider::None
    }
}

fn count_feature_flags(argv: &[String]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < argv.len() {
        let arg = &argv[index];
        if arg == "--features" {
            if let Some(value) = argv.get(index + 1) {
                count += parse_feature_list(value);
                index += 1;
            }
        } else if let Some(value) = arg.strip_prefix("--features=") {
            count += parse_feature_list(value);
        }
        index += 1;
    }
    count
}

fn parse_feature_list(value: &str) -> usize {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .count()
}

async fn send_event(payload: &TelemetryEvent) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
        .build()?;
    let _ = client
        .post(POSTHOG_CAPTURE_URL)
        .json(payload)
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
use std::ffi::OsString;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
fn machine_id_path_override() -> Option<PathBuf> {
    machine_id_override_lock()
        .lock()
        .expect("machine id override lock should be available")
        .clone()
}

#[cfg(test)]
fn set_machine_id_path_override(path: Option<PathBuf>) {
    let mut guard = machine_id_override_lock()
        .lock()
        .expect("machine id override lock should be available");
    *guard = path;
}

#[cfg(test)]
fn machine_id_override_lock() -> &'static Mutex<Option<PathBuf>> {
    static MACHINE_ID_OVERRIDE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
    MACHINE_ID_OVERRIDE.get_or_init(|| Mutex::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("test lock should be available")
    }

    struct EnvVarGuard {
        key: String,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &str, value: Option<&str>) -> Self {
            let previous = std::env::var_os(key);
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
            Self {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(&self.key, previous);
            } else {
                std::env::remove_var(&self.key);
            }
        }
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().expect("cwd should be available");
            std::env::set_current_dir(path).expect("cwd should be set");
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.previous).expect("cwd should be restored");
        }
    }

    fn create_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baml-cli-telemetry-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        dir
    }

    fn parse_command(args: &[&str]) -> Commands {
        let mut argv = vec!["baml-cli".to_string()];
        argv.extend(args.iter().map(|value| (*value).to_string()));

        let _internal_guard = EnvVarGuard::set("BAML_INTERNAL", Some("1"));
        crate::commands::RuntimeCli::parse_from_smart(argv).command
    }

    fn clear_ci_envs() -> Vec<EnvVarGuard> {
        [
            "CI",
            "GITHUB_ACTIONS",
            "GITLAB_CI",
            "CIRCLECI",
            "BUILDKITE",
            "JENKINS_URL",
            "TF_BUILD",
        ]
        .into_iter()
        .map(|key| EnvVarGuard::set(key, None))
        .collect()
    }

    #[test]
    fn env_truthy_parser_supports_expected_values() {
        let _test_lock = test_lock();

        for value in ["1", "true", "TRUE", " yes ", "on"] {
            let _guard = EnvVarGuard::set("BAML_CLI_DISABLE_TELEMETRY", Some(value));
            assert!(env_truthy("BAML_CLI_DISABLE_TELEMETRY"));
        }

        for value in ["0", "false", "no", "off", ""] {
            let _guard = EnvVarGuard::set("BAML_CLI_DISABLE_TELEMETRY", Some(value));
            assert!(!env_truthy("BAML_CLI_DISABLE_TELEMETRY"));
        }

        let _guard = EnvVarGuard::set("BAML_CLI_DISABLE_TELEMETRY", None);
        assert!(!env_truthy("BAML_CLI_DISABLE_TELEMETRY"));
    }

    #[test]
    fn capture_is_suppressed_when_telemetry_is_disabled() {
        let _test_lock = test_lock();
        let _disable_guard = EnvVarGuard::set("BAML_CLI_DISABLE_TELEMETRY", Some("1"));

        let command = parse_command(&["generate"]);
        let argv = vec!["baml-cli".to_string(), "generate".to_string()];
        let mut sent = false;
        capture_command_started_with_sender(
            &argv,
            &command,
            RuntimeCliDefaults {
                output_type: GeneratorOutputType::Typescript,
            },
            |_| sent = true,
        );

        assert!(!sent);
    }

    #[test]
    fn capture_is_suppressed_for_lsp_command() {
        let _test_lock = test_lock();
        let _disable_guard = EnvVarGuard::set("BAML_CLI_DISABLE_TELEMETRY", None);

        let command = parse_command(&["lsp"]);
        let argv = vec!["baml-cli".to_string(), "lsp".to_string()];
        let mut sent = false;
        capture_command_started_with_sender(
            &argv,
            &command,
            RuntimeCliDefaults {
                output_type: GeneratorOutputType::OpenApi,
            },
            |_| sent = true,
        );

        assert!(!sent);
    }

    #[test]
    fn map_command_covers_all_runtime_commands() {
        let _test_lock = test_lock();

        let cases = [
            (&["init"][..], ("init", None)),
            (&["generate"][..], ("generate", None)),
            (&["check"][..], ("check", None)),
            (&["serve"][..], ("serve", None)),
            (&["dev"][..], ("dev", None)),
            (&["auth", "login"][..], ("auth", Some("login"))),
            (&["auth", "token"][..], ("auth", Some("token"))),
            (&["login"][..], ("login", None)),
            (&["deploy"][..], ("deploy", None)),
            (&["fmt"][..], ("fmt", None)),
            (&["test"][..], ("test", None)),
            (&["dump-hir", "--from", "."][..], ("dump_hir", None)),
            (
                &["dump-bytecode", "--from", "."][..],
                ("dump_bytecode", None),
            ),
            (&["lsp"][..], ("lsp", None)),
            (&["repl"][..], ("repl", None)),
            (&["optimize"][..], ("optimize", None)),
        ];

        for (argv, expected) in cases {
            let command = parse_command(argv);
            assert_eq!(map_command(&command), expected);
        }
    }

    #[test]
    fn project_hash_source_falls_back_in_expected_order() {
        let _test_lock = test_lock();

        let temp_root = create_temp_dir();
        let _cwd_guard = CwdGuard::set(&temp_root);

        let from_arg_path = temp_root.join("custom").join("baml_src");
        std::fs::create_dir_all(&from_arg_path).expect("from arg path should be created");
        let argv_with_from = vec![
            "baml-cli".to_string(),
            "generate".to_string(),
            "--from".to_string(),
            from_arg_path.to_string_lossy().to_string(),
        ];
        let (from_hash, from_source) = compute_project_hash(&argv_with_from);
        assert_eq!(from_source, ProjectHashSource::FromArg);
        assert_eq!(from_hash.len(), 8);

        let cwd_baml_src = temp_root.join("baml_src");
        std::fs::create_dir_all(&cwd_baml_src).expect("cwd baml_src path should be created");
        let argv_without_from = vec!["baml-cli".to_string(), "generate".to_string()];
        let (_, baml_src_source) = compute_project_hash(&argv_without_from);
        assert_eq!(baml_src_source, ProjectHashSource::CwdBamlSrc);

        std::fs::remove_dir_all(&cwd_baml_src).expect("cwd baml_src path should be removed");
        let (_, cwd_source) = compute_project_hash(&argv_without_from);
        assert_eq!(cwd_source, ProjectHashSource::Cwd);
    }

    #[test]
    fn ci_provider_mapping_matches_expected_order() {
        let _test_lock = test_lock();

        let _cleared = clear_ci_envs();
        assert_eq!(detect_ci_provider(), CiProvider::None);

        for (key, expected) in [
            ("GITHUB_ACTIONS", CiProvider::GithubActions),
            ("GITLAB_CI", CiProvider::Gitlab),
            ("CIRCLECI", CiProvider::Circleci),
            ("BUILDKITE", CiProvider::Buildkite),
            ("JENKINS_URL", CiProvider::Jenkins),
            ("TF_BUILD", CiProvider::AzurePipelines),
        ] {
            let _cleared = clear_ci_envs();
            let _provider_guard = EnvVarGuard::set(key, Some("1"));
            assert_eq!(detect_ci_provider(), expected);
        }

        let _cleared = clear_ci_envs();
        let _gitlab_guard = EnvVarGuard::set("GITLAB_CI", Some("1"));
        let _github_guard = EnvVarGuard::set("GITHUB_ACTIONS", Some("1"));
        assert_eq!(detect_ci_provider(), CiProvider::GithubActions);
    }

    #[test]
    fn payload_contains_expected_fields_and_excludes_raw_argv_and_path() {
        let _test_lock = test_lock();
        let _cleared = clear_ci_envs();

        let temp_root = create_temp_dir();
        let _cwd_guard = CwdGuard::set(&temp_root);

        let machine_path = temp_root.join("telemetry_machine_id");
        set_machine_id_path_override(Some(machine_path));

        let sensitive_path = temp_root.join("private").join("sensitive").join("baml_src");
        std::fs::create_dir_all(&sensitive_path).expect("sensitive path should be created");
        let sensitive_path_value = sensitive_path.to_string_lossy().to_string();

        let argv = vec![
            "baml-cli".to_string(),
            "generate".to_string(),
            "--from".to_string(),
            sensitive_path_value.clone(),
            "--features=beta,display_all_warnings".to_string(),
            "super-secret-argv-value".to_string(),
        ];
        let command = parse_command(&["generate"]);
        let payload = build_command_started_event(
            &argv,
            &command,
            RuntimeCliDefaults {
                output_type: GeneratorOutputType::Typescript,
            },
            "baml_machine_test".to_string(),
            "session-test".to_string(),
        );
        set_machine_id_path_override(None);

        let value = serde_json::to_value(&payload).expect("payload should serialize");
        assert_eq!(value["api_key"], POSTHOG_API_KEY);
        assert_eq!(value["event"], EVENT_NAME);
        assert_eq!(value["distinct_id"], "baml_machine_test");
        assert_eq!(value["$process_person_profile"], false);
        assert_eq!(value["properties"]["surface"], "engine_cli");
        assert_eq!(value["properties"]["schema_version"], 1);
        assert_eq!(value["properties"]["command"], "generate");
        assert_eq!(value["properties"]["subcommand"], serde_json::Value::Null);
        assert_eq!(value["properties"]["caller_output_type"], "typescript");
        assert_eq!(value["properties"]["caller_runtime"], "typescript");
        assert_eq!(value["properties"]["project_hash_source"], "from_arg");
        assert_eq!(value["properties"]["machine_id"], "baml_machine_test");
        assert_eq!(value["properties"]["session_id"], "session-test");
        assert_eq!(value["properties"]["argv_len"], 6);
        assert_eq!(value["properties"]["feature_flags_count"], 2);

        let serialized = serde_json::to_string(&value).expect("payload should serialize");
        assert!(!serialized.contains(&sensitive_path_value));
        assert!(!serialized.contains("super-secret-argv-value"));
        assert!(!serialized.contains("--from"));
    }
}
