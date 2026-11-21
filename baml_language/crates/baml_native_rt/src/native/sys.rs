//! `baml.sys`: what the process knows of itself, and running a program.

use std::{
    io::IsTerminal as _,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use super::{
    Any, Arr, Big, Map, NativeBamlSys, Natives, Obj, ObjRef, Root, Scope, Str, Thrown,
    failure::Failure,
};
use crate::layout::{NAMED_SYS_HOST_INFO, NAMED_SYS_PROCESS_OUTPUT};

/// The fields of `baml.sys._ProcessOptions`, by position.
const OPTION_CWD: usize = 0;
const OPTION_ENV: usize = 1;
const OPTION_INHERIT_ENV: usize = 2;
const OPTION_TIMEOUT: usize = 3;
const OPTION_INPUT: usize = 4;
const OPTION_STDIN: usize = 5;
const OPTION_STDOUT: usize = 6;
const OPTION_STDERR: usize = 7;

/// How a program is to be run, read out of a `_ProcessOptions`.
struct Options {
    cwd: Option<String>,
    /// The variables to set, and with no value the ones to remove.
    env: Vec<(String, Option<String>)>,
    inherit_env: bool,
    timeout: Option<Duration>,
    input: Option<Vec<u8>>,
    /// How the child's input, output and errors are connected.
    streams: [String; 3],
}

impl Options {
    fn of(scope: &Scope<'_>, options: ObjRef<'_>) -> Result<Self, Failure> {
        // SAFETY: each field is read as its declaration types it.
        unsafe {
            let text = |field| {
                scope
                    .field_ref::<Str>(options, field)
                    .map(|text| text.as_str().to_owned())
            };
            let mut env = Vec::new();
            if let Some(map) = scope.field_ref::<Map>(options, OPTION_ENV) {
                map.each_str(|name, value| {
                    env.push((name.to_owned(), value.map(str::to_owned)));
                });
            }
            let timeout = scope
                .field_ref::<Obj>(options, OPTION_TIMEOUT)
                .and_then(|duration| scope.field_ref::<Big>(duration, 0))
                .map(|nanos| {
                    u64::try_from(nanos.value())
                        .map(Duration::from_nanos)
                        .map_err(|_| {
                            Failure::InvalidArgument(
                                "Process timeout must be a nonnegative duration representable \
                                 in nanoseconds"
                                    .to_owned(),
                            )
                        })
                })
                .transpose()?;
            let input = scope
                .field_ref::<Any>(options, OPTION_INPUT)
                .map(|input| match (input.as_str(), input.as_bytes()) {
                    (Some(text), _) => Ok(text.as_str().as_bytes().to_vec()),
                    (_, Some(bytes)) => Ok(bytes.to_vec()),
                    _ => Err(Failure::InvalidArgument(
                        "Process stdin must be text or bytes".to_owned(),
                    )),
                })
                .transpose()?;
            Ok(Self {
                cwd: text(OPTION_CWD),
                env,
                inherit_env: scope.field_bits(options, OPTION_INHERIT_ENV) == Some(1),
                timeout,
                input,
                streams: [OPTION_STDIN, OPTION_STDOUT, OPTION_STDERR]
                    .map(|field| text(field).unwrap_or_default()),
            })
        }
    }
}

fn stream(mode: &str) -> Result<Stdio, Failure> {
    match mode {
        "inherit" => Ok(Stdio::inherit()),
        "pipe" => Ok(Stdio::piped()),
        "ignore" => Ok(Stdio::null()),
        _ => Err(Failure::InvalidArgument(format!(
            "Invalid process stream mode '{mode}'"
        ))),
    }
}

/// The program a child runs, as a shell reads it: a name with a directory
/// in it is a path, from the directory the child is to run in, and a bare
/// name is left to the search of `PATH`.
fn program_path(program: &str, cwd: Option<&str>) -> std::io::Result<PathBuf> {
    let path = Path::new(program);
    let has_directory = path
        .parent()
        .is_some_and(|directory| !directory.as_os_str().is_empty());
    if !has_directory || path.is_absolute() {
        return Ok(path.to_owned());
    }
    let base = match cwd {
        Some(cwd) => std::path::absolute(cwd)?,
        None => std::env::current_dir()?,
    };
    std::path::absolute(base.join(path))
}

/// The shell `baml.sys.shell` runs a command with, and the arguments that
/// make it run one: the user's own when it is one of those known, and
/// otherwise the first the system has.
#[cfg(not(windows))]
fn shell() -> (PathBuf, &'static [&'static str]) {
    let known = |path: &Path| {
        matches!(
            path.file_stem().and_then(|stem| stem.to_str()),
            Some("zsh" | "bash" | "sh")
        )
    };
    let own = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.exists() && known(path));
    let candidates = if cfg!(target_os = "macos") {
        ["/bin/zsh", "/bin/bash"]
    } else {
        ["/bin/bash", "/bin/zsh"]
    };
    let found = own.or_else(|| {
        candidates
            .iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
    });
    (found.unwrap_or_else(|| PathBuf::from("/bin/sh")), &["-c"])
}

#[cfg(windows)]
fn shell() -> (PathBuf, &'static [&'static str]) {
    let found = [
        r"C:\Program Files\PowerShell\7\pwsh.exe",
        r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|path| path.exists());
    match found {
        Some(path) => (path, &["-NoProfile", "-Command"]),
        None => (PathBuf::from("cmd.exe"), &["/c"]),
    }
}

/// What a program that ran left: what it wrote, and how it ended.
struct Ran {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i64,
    signal: Option<i64>,
}

fn exit_of(status: ExitStatus) -> (i64, Option<i64>) {
    #[cfg(unix)]
    let signal = std::os::unix::process::ExitStatusExt::signal(&status);
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    (
        i64::from(status.code().unwrap_or(128 + signal.unwrap_or(0))),
        signal.map(i64::from),
    )
}

/// Everything `pipe` yields, read on a task of its own so that a child
/// that fills one pipe while another is read does not stall.
fn drain(
    pipe: Option<impl tokio::io::AsyncRead + Send + Unpin + 'static>,
) -> tokio::task::JoinHandle<std::io::Result<Vec<u8>>> {
    tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).await?;
        }
        Ok(bytes)
    })
}

/// Run `command`, which `label` names in an error, as `options` say, to
/// its end: its input written, and its output and errors read when they
/// are pipes. Past the limit the program is killed.
async fn run(
    mut command: tokio::process::Command,
    options: Options,
    label: &str,
) -> Result<Ran, Failure> {
    if let Some(cwd) = &options.cwd {
        command.current_dir(cwd);
    }
    if !options.inherit_env {
        command.env_clear();
    }
    for (name, value) in &options.env {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    let [stdin, stdout, stderr] = &options.streams;
    command
        .stdin(stream(stdin)?)
        .stdout(stream(stdout)?)
        .stderr(stream(stderr)?);
    if options.input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .spawn()
        .map_err(|error| Failure::io(format_args!("Failed to spawn '{label}'"), error))?;
    let stdin = child.stdin.take();
    let (stdout, stderr) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let input = options.input;
    let io = |error: std::io::Error| Failure::Io(error.to_string());
    let ended = async {
        if let (Some(mut stdin), Some(input)) = (stdin, input) {
            // A program that does not read its input to the end is no
            // failure of the one that ran it.
            match stdin.write_all(&input).await {
                Err(error)
                    if !matches!(
                        error.kind(),
                        std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                    ) =>
                {
                    return Err(io(error));
                }
                _ => {}
            }
        }
        let read = |joined: Result<std::io::Result<Vec<u8>>, tokio::task::JoinError>| {
            joined
                .map_err(|error| Failure::Io(error.to_string()))?
                .map_err(io)
        };
        let (stdout, stderr) = (read(stdout.await)?, read(stderr.await)?);
        let status = child.wait().await.map_err(io)?;
        Ok((stdout, stderr, status))
    };
    let ended = match options.timeout {
        None => ended.await,
        Some(limit) => match tokio::time::timeout(limit, ended).await {
            Ok(ended) => ended,
            Err(_) => {
                // The wait above is over, so the child is free to kill.
                let _ = child.kill().await;
                return Err(Failure::Timeout {
                    message: format!("Command '{label}' timed out after {}ms", limit.as_millis()),
                    limit: Some(limit),
                    which: "timeout",
                });
            }
        },
    };
    let (stdout, stderr, status) = ended?;
    let (exit_code, signal) = exit_of(status);
    Ok(Ran {
        stdout,
        stderr,
        exit_code,
        signal,
    })
}

/// A `ProcessOutput` of what a program left, or the error it failed with,
/// thrown.
fn output<'s>(
    scope: &mut Scope<'s>,
    ran: Result<Ran, Failure>,
) -> Result<Root<'s, Obj>, Thrown<'s>> {
    let ran = ran.map_err(|failure| failure.throw(scope))?;
    let stdout = scope.alloc_bytes(&ran.stdout);
    let stderr = scope.alloc_bytes(&ran.stderr);
    let mut built = scope.build(scope.named_class(NAMED_SYS_PROCESS_OUTPUT));
    built.set(scope, 0, stdout);
    built.set(scope, 1, stderr);
    built.set(scope, 2, ran.exit_code);
    built.set(scope, 3, ran.signal);
    Ok(built.finish())
}

/// A path as a string, or the error of one that is not UTF-8.
fn path_text<'s>(
    scope: &mut Scope<'s>,
    path: std::io::Result<PathBuf>,
) -> Result<Root<'s, Str>, Thrown<'s>> {
    let text = path.map_err(|error| error.to_string()).and_then(|path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| "Path is not valid UTF-8".to_owned())
    });
    match text {
        Ok(text) => Ok(scope.alloc_str(&text)),
        Err(message) => Err(scope.io_error(&message)),
    }
}

impl NativeBamlSys for Natives {
    fn argv<'s>(scope: &mut Scope<'s>) -> Root<'s, Arr<Str>> {
        // What follows the program's own name.
        let arguments: Vec<String> = std::env::args_os()
            .skip(1)
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        let array = scope.alloc_array::<Str>(arguments.len());
        for (index, argument) in arguments.iter().enumerate() {
            scope.scoped(|scope, _| {
                let argument = scope.alloc_str(argument);
                array.set(scope, index, Some(argument));
            });
        }
        array
    }

    fn pid() -> i64 {
        i64::from(std::process::id())
    }

    fn current_dir<'s>(scope: &mut Scope<'s>) -> Result<Root<'s, Str>, Thrown<'s>> {
        path_text(scope, std::env::current_dir())
    }

    fn current_exe<'s>(scope: &mut Scope<'s>) -> Result<Root<'s, Str>, Thrown<'s>> {
        path_text(scope, std::env::current_exe())
    }

    fn home_dir<'s>(scope: &mut Scope<'s>) -> Result<Option<Root<'s, Str>>, Thrown<'s>> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        home.map(|home| path_text(scope, Ok(PathBuf::from(home))))
            .transpose()
    }

    fn platform<'s>(scope: &mut Scope<'s>) -> Root<'s, Str> {
        scope.alloc_str(std::env::consts::OS)
    }

    fn host_target<'s>(scope: &mut Scope<'s>) -> Result<Root<'s, Str>, Thrown<'s>> {
        Ok(scope.alloc_str(env!("BAML_NATIVE_RT_TARGET")))
    }

    fn host_info<'s>(scope: &mut Scope<'s>) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let platform = scope.alloc_str(std::env::consts::OS);
        let architecture = scope.alloc_str(std::env::consts::ARCH);
        let cpu_count = std::thread::available_parallelism()
            .ok()
            .and_then(|count| i64::try_from(count.get()).ok())
            .unwrap_or(0);
        let mut built = scope.build(scope.named_class(NAMED_SYS_HOST_INFO));
        built.set(scope, 0, platform);
        built.set(scope, 1, architecture);
        built.set(scope, 2, cpu_count);
        built.set(scope, 3, std::io::stderr().is_terminal());
        Ok(built.finish())
    }

    fn collect_garbage(scope: &mut Scope<'_>) {
        scope.collect_now();
    }

    async fn _run<'s>(
        scope: &mut Scope<'s>,
        program: Root<'s, Str>,
        args: Root<'s, Arr<Str>>,
        options: Root<'s, Obj>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let program = program.get(scope).as_str().to_owned();
        let args: Vec<String> = args
            .get(scope)
            .iter()
            .map(|argument| argument.as_str().to_owned())
            .collect();
        let options = Options::of(scope, options.get(scope));
        let ran = scope
            .io(async move {
                let options = options?;
                let executable =
                    program_path(&program, options.cwd.as_deref()).map_err(|error| {
                        Failure::io(format_args!("Failed to resolve '{program}'"), error)
                    })?;
                let mut command = tokio::process::Command::new(executable);
                command.args(&args);
                run(command, options, &program).await
            })
            .await;
        output(scope, ran)
    }

    async fn _shell<'s>(
        scope: &mut Scope<'s>,
        command: Root<'s, Str>,
        options: Root<'s, Obj>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let line = command.get(scope).as_str().to_owned();
        let options = Options::of(scope, options.get(scope));
        let ran = scope
            .io(async move {
                let (shell, arguments) = shell();
                let mut command = tokio::process::Command::new(shell);
                command.args(arguments).arg(&line);
                run(command, options?, &line).await
            })
            .await;
        output(scope, ran)
    }
}
