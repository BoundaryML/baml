/// Filesystem and process orchestration for the native BAML service.
class Job {
    feedback: Feedback,
    status: "queued" | "running" | "completed" | "failed" | "interrupted",
    error: string?,
}

class RunInput {
    feedback: Feedback,
    workspace: Workspace,
}

function data_dir() -> string {
    (baml.env.get("MINIATB_DATA") ?? "/data/miniatb") + "/" + dataset()
}

function app_dir() -> string {
    baml.env.get("MINIATB_APP") ?? "/app"
}

function controller_cli() -> string {
    baml.env.get("MINIATB_CLI") ?? "/opt/baml/bin/baml-cli"
}

function toolchain_dir() -> string {
    data_dir() + "/toolchain"
}

/// ProcessOptions.env replaces the environment, so forward only controller inputs.
function controller_env(id: string) -> map<string, string> {
    let result = {
        "PATH": "/opt/baml/bin:/usr/local/bin:/usr/bin:/bin",
        "HOME": "/tmp",
        "MINIATB_RUN": id,
        "MINIATB_DATA": baml.env.get("MINIATB_DATA") ?? "/data/miniatb",
        "MINIATB_APP": app_dir(),
        "MINIATB_CLI": controller_cli(),
        "BAML_TELEMETRY_DISABLED": "1",
    };
    for (
        let key in [
            "TYPESAFE_API_KEY",
            "FEEDBACK_SUPABASE_URL",
            "FEEDBACK_SUPABASE_KEY",
            "ATB_SLACK_BOT_TOKEN",
            "ATB_SLACK_FIX_CHANNEL",
            "ATB2_SLACK_CHANNEL",
            "MINIATB_CLAUDE_HOME",
            "ATB2_SHEPHERDS",
            "ATB2_UI_URL",
            "MINIATB_SLACK_NOTIFY",
            "MINIATB_DATASET",
            "MINIATB_ALLOW_PUSH",
            "MINIATB_PUBLISH_REPO",
            "BAMMY_GITHUB_APP_CLIENT_ID",
            "BAMMY_GITHUB_APP_PRIVATE_KEY",
        ]
    ) {
        if let value: string = baml.env.get(key) {
            result.set(key, value);
        }
    }
    result
}

function require(condition: bool, message: string) -> void {
    if (!condition) {
        throw baml.errors.InvalidArgument { message: message };
    }
}

function mkdir(path: string) -> void {
    baml.fs.mkdir(path, baml.fs.MkdirOptions { recursive: true });
}

function checked(
    program: string,
    args: string[],
    options: baml.sys.ProcessOptions? = null,
) -> string {
    let result = baml.sys.exec(program, args, options);
    require(result.exit_code == 0, program + " failed: " + result.stderr.to_string().slice(0, 2000));
    result.stdout.to_string().trim()
}

function safe_args(program: string, args: string[]) -> string[] {
    [
        "-i",
        "PATH=/usr/local/bin:/usr/bin:/bin",
        "HOME=/tmp",
        "GIT_CONFIG_NOSYSTEM=1",
        "GIT_CONFIG_GLOBAL=/dev/null",
        "GIT_TERMINAL_PROMPT=0",
        program,
    ]
        .concat(args)
}

function safe_exec(program: string, args: string[]) -> string {
    checked(
        "/usr/bin/env",
        safe_args(program, args),
        options = baml.sys.ProcessOptions { timeout_ms: 900000 },
    )
}

function read_json<T>(path: string) -> T {
    baml.json.from_string<T>(baml.fs.read(path))
}

/// Rename on the same filesystem publishes a complete document atomically.
function write_json(path: string, value: json) -> void {
    let temporary = path + "." + baml.id.new() + ".tmp";
    baml.fs.write(temporary, baml.json.stringify(value));
    defer { if (baml.fs.exists(temporary)) { baml.fs.remove(temporary); } }
    safe_exec("mv", ["-f", "--", temporary, path]);
}

function feedback_id(feedback: Feedback) -> string {
    let hash = baml.crypto.Sha256.new();
    hash.update(feedback.id.to_utf8());
    hash.finish().to_hex().slice(0, 24)
}

function valid_run_id(id: string) -> bool {
    id.length() == 24
        && id.chars()
            .filter((c) -> {
                !"0123456789abcdef".includes(c)
            })
            .length()
            == 0
}

function run_path(id: string) -> string {
    require(valid_run_id(id), "Invalid run ID");
    data_dir() + "/runs/" + id
}

function run_dir() -> string {
    run_path(baml.env.get("MINIATB_RUN") ?? "")
}

/// Atomic hard-link creation makes retries keep the original feedback and status.
function enqueue(feedback: Feedback, directory: string? = null) -> void {
    let queue = (directory ?? data_dir()) + "/queue";
    mkdir(queue);
    let path = queue + "/" + feedback_id(feedback) + ".json";
    let temporary = queue + "/" + baml.id.new() + ".tmp";
    baml.fs.write(
        temporary,
        baml.json.to_string(Job { feedback: feedback, status: "queued", error: null }),
    );
    defer { baml.fs.remove(temporary); }
    let result = baml.sys.exec("/usr/bin/env", safe_args("ln", [temporary, path]), null);
    require(result.exit_code == 0 || baml.fs.exists(path), "Could not persist feedback");
}

function record(stage: string, status: string, detail: json) -> null {
    let entry = {
        "at": baml.time.Instant.now().to_string(),
        "run": baml.env.get("MINIATB_RUN"),
        "stage": stage,
        "status": status,
        "detail": detail,
    }
        .to_json();
    let stream = baml.fs.open(run_dir() + "/timeline.jsonl", "a");
    defer { stream.close(); }
    stream.write(baml.json.stringify(entry) + "\n");
    if (status != "event") {
        store_stage(stage, status, detail);
    };
    log.info({ "stage": stage, "status": status });
    null
}

function fetch_json(url: string) -> json {
    let response = baml.http.send(
        baml.http.Request {
            method: "GET",
            url: url,
            headers: { "User-Agent": "MiniATB", "Accept": "application/json" },
            body: "",
        },
        timeout = baml.time.Duration.from_seconds(120),
    );
    require(response.ok(), "Download failed (" + response.status_code.to_string() + "): " + url);
    baml.json.parse(response.text())
}

/// A single worker owns toolchain updates. The controller executable stays fixed.
function current_toolchain() -> string {
    let manifest = fetch_json("https://pkg.boundaryml.com/manifest/v1/nightly.json");
    let version = baml.json.path<string>(manifest, ".version");
    let release = fetch_json("https://api.github.com/repos/BoundaryML/baml/releases/tags/baml-language-" + version);
    let revision = baml.json.path<string>(release, ".target_commitish");
    require(
        revision.length() == 40
            && revision.chars()
                .filter((c) -> {
                    !"0123456789abcdef".includes(c)
                })
                .length()
                == 0,
        "Release must pin a source revision",
    );
    let installed = toolchain_dir();
    if (baml.fs.exists(installed + "/version") && baml.fs.read(installed + "/version") == version) {
        return revision;
    }
    require(safe_exec("uname", ["-s"]) == "Linux", "Toolchain installation requires Linux");
    let arch = safe_exec("uname", ["-m"]);
    let target = (
        if (arch == "aarch64" || arch == "arm64") {
            "aarch64"
        } else {
            "x86_64"
        }
    )
        + "-unknown-linux-gnu";
    let artifact = baml.json.path<map<string, json>>(manifest, ".artifacts").get(target)
        ?? baml.sys.panic("Missing toolchain artifact");
    let url = baml.json.path<string>(artifact, ".url");
    require(
        url.starts_with("https://github.com/BoundaryML/baml/releases/download/"),
        "Unexpected artifact host",
    );
    let temp = safe_exec("mktemp", ["-d", data_dir() + "/toolchain.XXXXXX"]);
    defer { baml.fs.remove_dir_all(temp); }
    safe_exec("curl", ["-fsSL", "--max-time", "300", url, "-o", temp + "/archive"]);
    let hash = baml.crypto.Sha256.new();
    let archive = baml.fs.open(temp + "/archive", "r");
    while let chunk: uint8array = archive.read(65536) {
        hash.update(chunk);
    }
    archive.close();
    require(
        hash.finish().to_hex() == baml.json.path<string>(artifact, ".sha256"),
        "Toolchain checksum mismatch",
    );
    let members = safe_exec("tar", ["-tzf", temp + "/archive"]).split("\n");
    mkdir(temp + "/new/bin");
    for (let binary in ["baml-cli", "baml-pack-host"]) {
        let member = members.find((name) -> {
            name == "bin/" + binary || name.ends_with("/bin/" + binary)
        })
            ?? baml.sys.panic("Missing toolchain binary");
        require(!member.starts_with("/") && !member.split("/").includes(".."), "Unsafe archive member");
        safe_exec("tar", ["-xzf", temp + "/archive", "-C", temp, "--", member]);
        safe_exec("mv", ["--", temp + "/" + member, temp + "/new/bin/" + binary]);
        baml.fs.chmod(temp + "/new/bin/" + binary, 493);
    }
    baml.fs.symlink("baml-cli", temp + "/new/bin/baml");
    baml.fs.write(temp + "/new/version", version);
    if (baml.fs.exists(installed)) {
        baml.fs.remove_dir_all(installed);
    }
    safe_exec("mv", ["--", temp + "/new", installed]);
    revision
}

function repo_git(args: string[]) -> string {
    safe_exec("git", ["-c", "core.hooksPath=/dev/null", "-C", data_dir() + "/repo"].concat(args))
}

function prepare_workspace(feedback: Feedback) -> Workspace {
    let id = feedback_id(feedback);
    require(run_dir() == run_path(id), "Feedback does not match run");
    mkdir(run_dir());
    let revision = current_toolchain();
    if (!baml.fs.exists(data_dir() + "/repo")) {
        safe_exec("git", ["clone", "--bare", "https://github.com/BoundaryML/baml.git", data_dir() + "/repo"]);
    }
    repo_git(["fetch", "origin", revision]);
    require(repo_git(["rev-parse", "FETCH_HEAD"]) == revision, "Source revision mismatch");
    let slug = feedback.description.to_lower_case()
        .chars()
        .map((c) -> {
            if ("abcdefghijklmnopqrstuvwxyz0123456789".includes(c)) {
                c
            } else {
                "-"
            }
        })
        .join("")
        .slice(0, 40);
    let branch = "bammy/" + id + "-" + slug;
    repo_git(["worktree", "add", "-b", branch, run_dir() + "/worktree", revision]);
    mkdir(run_dir() + "/scratch");
    baml.fs.chmod(run_dir() + "/scratch", 511);
    let workspace = Workspace {
        id: id,
        revision: revision,
        branch: branch,
        version: baml.fs.read(toolchain_dir() + "/version"),
    };
    write_json(run_dir() + "/input.json", RunInput { feedback: feedback, workspace: workspace }.to_json());
    workspace
}

/// Internal worker target, invoked under the OS worker lock.
function execute_job(id: string) -> void {
    require(run_dir() == run_path(id), "Run environment mismatch");
    let path = data_dir() + "/queue/" + id + ".json";
    let job = read_json<Job>(path);
    if (job.status != "queued") {
        return;
    }
    job.status = "running";
    write_json(path, job.to_json());
    pipeline(job.feedback) catch_all (error) {
        _ => {
            job.status = "failed";
            job.error = "Pipeline failed; see controller.log";
            write_json(path, job.to_json());
            throw error;
        }
    };
    job.status = "completed";
    write_json(path, job.to_json());
}

function drain_log(pipe: baml.sys.ReadPipe, path: string) -> void {
    let stream = baml.fs.open(path, "a");
    defer { stream.close(); }
    let written = 0;
    while let chunk: uint8array = pipe.read(65536) {
        if (written < 67108864) {
            let part = chunk.slice(0, (67108864 - written).min(chunk.length()));
            stream.write(part);
            written += part.length();
        }
    }
}

function worker_loop() -> never {
    mkdir(data_dir() + "/queue");
    while (true) {
        for (let entry in baml.fs.read_dir(data_dir() + "/queue")) {
            if (!entry.is_file || !entry.name.ends_with(".json")) {
                continue;
            }
            let path = data_dir() + "/queue/" + entry.name;
            let job = read_json<Job>(path);
            if (job.status == "running") {
                job.status = "interrupted";
                write_json(path, job.to_json());
            }
            if (job.status != "queued") {
                continue;
            }
            let id = entry.name.slice(0, entry.name.length() - 5);
            let folder = run_path(id);
            mkdir(folder);
            let child = baml.sys.start_process(
                "timeout",
                [
                    "--kill-after=10s",
                    "7200s",
                    "flock",
                    "--close",
                    data_dir() + "/worker.lock",
                    controller_cli(),
                    "run",
                    "--directory",
                    app_dir(),
                    "--agent-skill-check",
                    "off",
                    "execute_job",
                    "--",
                    "--id",
                    id,
                ],
                baml.sys.ProcessOptions {
                    env: controller_env(id),
                    stderr: baml.sys.StderrMode.Pipe,
                },
            );
            defer { child.close(); }
            child.stdin.close();
            let out = spawn { drain_log(child.stdout, folder + "/controller.log") };
            let err = spawn {
                drain_log(
                    child.stderr ?? baml.sys.panic("Missing stderr"),
                    folder + "/controller.stderr.log",
                )
            };
            let result = child.wait();
            await out;
            await err;
            if (result.exit_code != 0) {
                let failed = read_json<Job>(path);
                failed.status = "failed";
                failed.error = "Worker process failed; see controller logs";
                write_json(path, failed.to_json());
            }
        }
        baml.sys.sleep(baml.time.Duration.from_seconds(2));
    }
}
