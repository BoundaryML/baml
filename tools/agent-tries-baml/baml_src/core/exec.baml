// Subprocess capture that survives leaked grandchildren. Ports the hard-won
// reaping logic of claude_proxy/runner.py (spawn_claude / run_command) to BAML's
// baml.sys.exec.
//
// Two BAML-runtime facts shape this (both verified by spiking):
//   1. baml.sys.exec waits for the captured stdout/stderr PIPE to hit EOF — a
//      grandchild that inherits the pipe (e.g. a `baml test` stuck on a
//      no-timeout TcpStream read) keeps it open and wedges the call until the
//      timeout. Mitigation: redirect the command's stdout/stderr to FILES inside
//      a shell wrapper, so the captured pipe closes when the direct child exits,
//      and read the files afterward. (Spiked: returns in ~0.7s with a bg
//      grandchild instead of wedging to the timeout.)
//   2. ProcessOptions.env REPLACES the environment (no merge, and there's no
//      enumerate-all-env API). So we inherit the full parent env (env: null) and
//      adjust PATH / scrub the API key *inside* the wrapper script instead.
//
// timeout_ms still bounds wall-clock and reaps the direct child (exit_code -9 on
// timeout, matching the Python convention).

class ExecResult {
    stdout: string,
    stderr: string,
    exit_code: int,
    timed_out: bool,
}

// Per-run environment sandbox for agent subprocesses. The agent's shell tools
// inherit the proxy's env, so without this a run can read AND mutate machine
// state: a cold-start agent once ran the quickstart installer with the image's
// BAML_HOME=/opt/baml inherited, overwriting the SERVICE's own wrapper +
// default-toolchain selection and appending to the real /root/.bashrc.
class SandboxOpts {
    // Fresh per-run HOME (under the staging dir, so cleanup_staging wipes it).
    // Installer/profile writes land here instead of the machine's /root.
    home: string,
    // The real Claude auth/session store. Exported as CLAUDE_CONFIG_DIR so the
    // CLI keeps its OAuth login and still writes the session jsonl where
    // session_log_path looks (verified live: sessions land under
    // $CLAUDE_CONFIG_DIR/projects even with HOME overridden).
    claude_config_dir: string,
    // Cold start: also strip the image's baml wrapper (PATH reset to the
    // system default) and unset every BAML_* var, so "BAML is NOT installed"
    // is actually true and installers can't be redirected into /opt/baml.
    hide_baml: bool,
}

// The env-sandbox prelude of the wrapper script ("" when no sandbox). Pure,
// unit-tested; paths are our own controlled staging/config dirs.
function sandbox_script(sandbox: SandboxOpts?) -> string {
    return match (sandbox) {
        null => "",
        let s: SandboxOpts => {
            let base = "export HOME=\""
                + s.home
                + "\"; mkdir -p \"$HOME\"; export CLAUDE_CONFIG_DIR=\""
                + s.claude_config_dir
                + "\"; ";
            if (s.hide_baml) {
                base
                    + "unset BAML_HOME BAML_VERSION BAML_CACHE_DIR BAML_WRAPPER_EXEC BAML_WRAPPER_RESOLVED_TOOLCHAIN; "
                    + "export PATH=\"/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\"; "
            } else {
                base
            }
        },
    };
}

// Per-run agent sandbox: isolate HOME (installer/profile writes) and, on cold
// start, strip the image's preinstalled baml wrapper from the agent's PATH.
function agent_sandbox_opts(staging: string, cold: bool) -> SandboxOpts {
    return SandboxOpts {
        home: staging + "/.home",
        claude_config_dir: claude_config_dir(),
        hide_baml: cold,
    };
}

// Run `program` + `args` (or `sh -c <command>` — pass program="sh",
// args=["-c", command]) in `cwd`, with `baml_bin_dir` prepended to PATH and the
// API key optionally scrubbed (OAuth mode). stdout/stderr are captured via files
// under cwd so leaked grandchildren can't wedge the pipe.
//
// `baml_version` (when non-empty) is exported as BAML_VERSION inside the wrapper:
// the `baml` wrapper honors it to select the toolchain version the subprocess
// uses, so ONLY the spawned agent picks up a freshly-installed nightly while the
// service's own baml (the image PATH) is untouched.
function run_capture(
    program: string,
    args: string[],
    cwd: string,
    baml_bin_dir: string?,
    timeout_ms: int,
    stdin: string?,
    scrub_api_key: bool,
    baml_version: string?,
    sandbox: SandboxOpts?,
) -> ExecResult {
    let out_file = cwd + "/.atb_stdout";
    let err_file = cwd + "/.atb_stderr";

    // Build the wrapper script. cwd/baml_bin_dir are our own controlled paths
    // (cache + staging dirs), so embedding them in the script is safe.
    let path_line = match (baml_bin_dir) {
        let d: string => "export PATH=\"" + d + ":$PATH\"; ",
        null => "",
    };
    // Pin the agent's baml toolchain via BAML_VERSION (the wrapper resolves it).
    let version_line = match (baml_version) {
        let v: string => {
            if (v.trim() == "") {
                ""
            } else {
                "export BAML_VERSION=\"" + v + "\"; "
            }
        },
        null => "",
    };
    let scrub_line = if (scrub_api_key) {
        "unset ANTHROPIC_API_KEY; "
    } else {
        ""
    };
    // Sandbox first (it may reset PATH/HOME), then version/path adjustments on
    // top so an explicit baml_bin_dir still wins for sandboxed warm runs.
    let script = scrub_line
        + sandbox_script(sandbox)
        + version_line
        + path_line
        + "exec \"$@\" >\""
        + out_file
        + "\" 2>\""
        + err_file
        + "\"";

    // sh -c '<script>' atb <program> <args...>  ($0=atb, "$@"=program+args)
    let argv = ["-c", script, "atb", program].concat(args);

    let timed = false;
    let res = baml
        .sys
        .exec(
            "sh",
            argv,
            baml.sys.ProcessOptions { cwd: cwd, env: null, timeout_ms: timeout_ms, stdin: stdin },
        ) catch (e) {
        baml.errors.Timeout => {
            timed = true;
            null
        },
        baml.errors.Io => null,
    };

    let out = baml.fs.read(out_file) catch (_) {
        _ => ""
    };
    let err = baml.fs.read(err_file) catch (_) {
        _ => ""
    };

    return match (res) {
        let r: baml.sys.ShellOutput => {
            ExecResult { stdout: out, stderr: err, exit_code: r.exit_code, timed_out: false }
        },
        null => ExecResult { stdout: out, stderr: err, exit_code: -9, timed_out: timed },
    };
}
