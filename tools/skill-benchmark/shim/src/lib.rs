use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;

const REAL_BAML_ENV: &str = "SKILL_BENCH_REAL_BAML";
const BAML_AUDIT_DIR_ENV: &str = "SKILL_BENCH_BAML_AUDIT_DIR";
const RUN_ID_ENV: &str = "SKILL_BENCH_RUN_ID";
const REAL_CLAUDE_ENV: &str = "SKILL_BENCH_REAL_CLAUDE";
const CLAUDE_LOG_ENV: &str = "SKILL_BENCH_CLAUDE_LOG";
const CLAUDE_STDERR_LOG_ENV: &str = "SKILL_BENCH_CLAUDE_STDERR_LOG";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum AuditStatus {
    Running,
    Exited,
    SpawnFailed,
}

#[derive(Debug, Serialize)]
struct AuditMetadata {
    schema_version: u32,
    invocation_id: String,
    run_id: String,
    status: AuditStatus,
    shim_version: &'static str,
    shim_pid: u32,
    cwd: String,
    real_baml: String,
    argv: Vec<String>,
    subcommand: Option<String>,
    started_at_unix_ms: u64,
    finished_at_unix_ms: Option<u64>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    signal: Option<i32>,
    success: Option<bool>,
    error: Option<String>,
    stdout_path: String,
    stderr_path: String,
}

pub fn main_baml() {
    exit_with(run_baml());
}

pub fn main_claude() {
    exit_with(run_claude());
}

fn exit_with(result: Result<i32>) -> ! {
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("skill benchmark shim: {error:#}");
            std::process::exit(1);
        }
    }
}

fn run_baml() -> Result<i32> {
    let real = required_executable(REAL_BAML_ENV)?;
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let Some(audit_root) = env::var_os(BAML_AUDIT_DIR_ENV).filter(|value| !value.is_empty()) else {
        return run_direct(&real, &args);
    };

    let audit_root = absolute_path(PathBuf::from(audit_root))?;
    let started_at_unix_ms = unix_ms();
    let invocation_id = format!("{started_at_unix_ms}-{}", std::process::id());
    let invocation_dir = audit_root.join(&invocation_id);
    fs::create_dir_all(&invocation_dir)
        .with_context(|| format!("failed to create {}", invocation_dir.display()))?;

    let metadata_path = invocation_dir.join("metadata.json");
    let stdout_path = invocation_dir.join("stdout");
    let stderr_path = invocation_dir.join("stderr");
    let argv = display_args(&args);
    let mut metadata = AuditMetadata {
        schema_version: 1,
        invocation_id: invocation_id.clone(),
        run_id: env::var(RUN_ID_ENV).unwrap_or_default(),
        status: AuditStatus::Running,
        shim_version: env!("CARGO_PKG_VERSION"),
        shim_pid: std::process::id(),
        cwd: env::current_dir()?.display().to_string(),
        real_baml: real.display().to_string(),
        subcommand: classify_subcommand(&argv),
        argv,
        started_at_unix_ms,
        finished_at_unix_ms: None,
        duration_ms: None,
        exit_code: None,
        signal: None,
        success: None,
        error: None,
        stdout_path: stdout_path.display().to_string(),
        stderr_path: stderr_path.display().to_string(),
    };
    write_metadata(&metadata_path, &metadata)?;

    eprintln!("SKILL_BENCH_BAML_INVOCATION={invocation_id}");
    let started = Instant::now();
    let stdout_file = File::create(&stdout_path)?;
    let stderr_file = File::create(&stderr_path)?;
    let mut child = match Command::new(&real)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            finish_metadata(&mut metadata, started.elapsed());
            metadata.status = AuditStatus::SpawnFailed;
            metadata.error = Some(error.to_string());
            write_metadata(&metadata_path, &metadata)?;
            return Err(error).context("failed to execute real baml");
        }
    };

    let stdout = child
        .stdout
        .take()
        .context("real baml stdout was not captured")?;
    let stderr = child
        .stderr
        .take()
        .context("real baml stderr was not captured")?;
    let stdout_thread = thread::spawn(move || tee(stdout, io::stdout(), stdout_file));
    let stderr_thread = thread::spawn(move || tee(stderr, io::stderr(), stderr_file));
    let status = child.wait().context("failed to wait for real baml")?;
    let capture_result = join_tee(stdout_thread, "stdout").and(join_tee(stderr_thread, "stderr"));

    finish_metadata(&mut metadata, started.elapsed());
    metadata.status = AuditStatus::Exited;
    metadata.exit_code = status.code();
    metadata.signal = exit_signal(&status);
    metadata.success = Some(status.success());
    if let Err(error) = capture_result {
        metadata.error = Some(error.to_string());
    }
    write_metadata(&metadata_path, &metadata)?;
    Ok(normalized_exit_code(status))
}

fn run_claude() -> Result<i32> {
    let real = required_executable(REAL_CLAUDE_ENV)?;
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let Some(stdout_log) = env::var_os(CLAUDE_LOG_ENV).filter(|value| !value.is_empty()) else {
        return run_direct(&real, &args);
    };

    let stdout_log = absolute_path(PathBuf::from(stdout_log))?;
    let stderr_log = env::var_os(CLAUDE_STDERR_LOG_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(absolute_path)
        .transpose()?
        .unwrap_or_else(|| stdout_log.with_extension("stderr"));
    create_parent(&stdout_log)?;
    create_parent(&stderr_log)?;

    let stdout_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&stdout_log)?;
    let stderr_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(stderr_log)?;
    let mut child = Command::new(&real)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to execute real claude")?;
    let stdout = child
        .stdout
        .take()
        .context("claude stdout was not captured")?;
    let stderr = child
        .stderr
        .take()
        .context("claude stderr was not captured")?;
    let stdout_thread = thread::spawn(move || tee(stdout, io::stdout(), stdout_file));
    let stderr_thread = thread::spawn(move || tee(stderr, io::stderr(), stderr_file));
    let status = child.wait().context("failed to wait for claude")?;
    join_tee(stdout_thread, "stdout")?;
    join_tee(stderr_thread, "stderr")?;
    Ok(normalized_exit_code(status))
}

fn required_executable(name: &str) -> Result<PathBuf> {
    let path = env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("{name} must name the real executable"))?;
    let path = absolute_path(path)?;
    if !path.is_file() {
        bail!("executable not found at {}", path.display());
    }
    let current = fs::canonicalize(env::current_exe()?)?;
    let resolved = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if current == resolved {
        bail!("{name} points back to the shim");
    }
    Ok(path)
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()?.join(path))
    }
}

fn create_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn run_direct(real: &Path, args: &[OsString]) -> Result<i32> {
    let status = Command::new(real)
        .args(args)
        .status()
        .with_context(|| format!("failed to execute {}", real.display()))?;
    Ok(normalized_exit_code(status))
}

fn display_args(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

fn classify_subcommand(argv: &[String]) -> Option<String> {
    const COMMANDS: &[&str] = &[
        "agent",
        "auth",
        "check",
        "clean",
        "describe",
        "feedback",
        "fmt",
        "generate",
        "help",
        "ide",
        "init",
        "pack",
        "run",
        "serve",
        "test",
        "toolchain",
        "version",
    ];
    argv.iter()
        .map(String::as_str)
        .find(|arg| COMMANDS.contains(arg))
        .map(str::to_owned)
}

fn finish_metadata(metadata: &mut AuditMetadata, duration: Duration) {
    metadata.finished_at_unix_ms = Some(unix_ms());
    metadata.duration_ms = Some(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX));
}

fn tee<R: Read, W: Write>(mut input: R, mut visible: W, mut audit: File) -> io::Result<()> {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        audit.write_all(&buffer[..read])?;
        let _ = visible.write_all(&buffer[..read]);
        let _ = visible.flush();
    }
    audit.flush()
}

fn join_tee(handle: thread::JoinHandle<io::Result<()>>, stream: &str) -> Result<()> {
    handle
        .join()
        .map_err(|_| anyhow!("{stream} capture thread panicked"))?
        .with_context(|| format!("failed to capture {stream}"))
}

fn write_metadata(path: &Path, metadata: &AuditMetadata) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(metadata)?)?;
    fs::rename(&temporary, path)?;
    Ok(())
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(unix)]
fn exit_signal(status: &ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    status.signal()
}

#[cfg(not(unix))]
fn exit_signal(_status: &ExitStatus) -> Option<i32> {
    None
}

fn normalized_exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + exit_signal(&status).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_subcommand_after_global_options() {
        let argv = vec![
            "--color".to_owned(),
            "never".to_owned(),
            "describe".to_owned(),
            "Array".to_owned(),
        ];
        assert_eq!(classify_subcommand(&argv), Some("describe".to_owned()));
    }

    #[test]
    fn display_args_preserves_forwarded_values() {
        let args = vec![
            OsString::from("check"),
            OsString::from("--project"),
            OsString::from("a b"),
        ];
        assert_eq!(display_args(&args), ["check", "--project", "a b"]);
    }

    #[test]
    fn absolute_path_keeps_absolute_input() {
        let path = env::temp_dir().join("skill-benchmark-test");
        assert_eq!(absolute_path(path.clone()).unwrap(), path);
    }

    #[test]
    fn command_catalog_contains_observed_commands() {
        for command in ["check", "describe", "run", "test"] {
            assert_eq!(
                classify_subcommand(&[command.to_owned()]),
                Some(command.to_owned())
            );
        }
    }
}
