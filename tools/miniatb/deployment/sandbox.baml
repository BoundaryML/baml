function sandbox_args(
    command: string,
    source: string,
    scratch: string,
    writable: bool,
    cli: string? = null,
) -> string[] {
    let args = [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--uid",
        "65534",
        "--gid",
        "65534",
        "--clearenv",
    ];
    for (let path in ["/usr", "/bin", "/lib", "/lib64", "/etc/alternatives"]) {
        if (baml.fs.exists(path)) {
            args = args.concat(["--ro-bind", path, path]);
        }
    }
    args
        = args.concat(
            [
                "--ro-bind",
                cli ?? toolchain_dir(),
                "/opt/baml",
                "--proc",
                "/proc",
                "--dev",
                "/dev",
                "--size",
                "134217728",
                "--tmpfs",
                "/tmp",
                "--tmpfs",
                "/home/agent",
                if (writable) {
                    "--bind"
                } else {
                    "--ro-bind"
                },
                source,
                "/workspace",
                "--bind",
                scratch,
                "/scratch",
            ],
        );
    if (baml.fs.exists("/opt/rustup") && baml.fs.exists("/opt/cargo/registry")) {
        args
            = args.concat(
                [
                    "--ro-bind",
                    "/opt/rustup",
                    "/opt/rustup",
                    "--ro-bind",
                    "/opt/cargo",
                    "/opt/cargo",
                    "--ro-bind",
                    "/opt/cargo/registry",
                    "/scratch/cargo/registry",
                    "--ro-bind",
                    "/opt/cargo/git",
                    "/scratch/cargo/git",
                ],
            );
    };
    if (baml.fs.exists(source + "/.git")) {
        args = args.concat(["--ro-bind", "/dev/null", "/workspace/.git"]);
    }
    args.concat(
        [
            "--setenv",
            "HOME",
            "/home/agent",
            "--setenv",
            "PATH",
            "/opt/cargo/bin:/opt/baml/bin:/usr/bin:/bin",
            "--setenv",
            "RUSTUP_HOME",
            "/opt/rustup",
            "--setenv",
            "CARGO_HOME",
            "/scratch/cargo",
            "--setenv",
            "CARGO_TARGET_DIR",
            "/scratch/target",
            "--setenv",
            "CARGO_NET_OFFLINE",
            "true",
            "--setenv",
            "CARGO_PROFILE_DEV_DEBUG",
            "0",
            "--setenv",
            "CARGO_INCREMENTAL",
            "0",
            "--setenv",
            "CARGO_BUILD_JOBS",
            "2",
            "--setenv",
            "BAML_TELEMETRY_DISABLED",
            "1",
            "--chdir",
            "/workspace",
            "--",
            "/bin/bash",
            "-c",
            command,
        ],
    )
}

/// Drain continuously with bounded memory, retaining only the first 64 KiB.
function bounded_output(pipe: baml.sys.ReadPipe) -> string {
    let kept = uint8array.from_array([]);
    let truncated = false;
    while let chunk: uint8array = pipe.read(65536) {
        let room = 65536 - kept.length();
        if (chunk.length() > room) {
            truncated = true;
        }
        kept = kept.concat(chunk.slice(0, room.min(chunk.length())));
    }
    kept.to_string()
        + if (truncated) {
            "\n[output truncated]"
        } else {
            ""
        }
}

function sandbox(
    command: string,
    source: string,
    scratch: string,
    writable: bool = false,
    persist_source: bool = true,
    cli: string? = null,
    seconds: int = 300,
) -> Execution {
    require(safe_exec("uname", ["-s"]) == "Linux", "Linux bubblewrap is required; no unsandboxed fallback");
    mkdir(scratch + "/cargo");
    baml.fs.chmod(scratch + "/cargo", 511);
    mkdir(scratch + "/cargo/registry");
    mkdir(scratch + "/cargo/git");
    let args = [
        "--mount",
        "--propagation",
        "private",
        "node",
        app_dir() + "/deployment/sandbox-run.mjs",
        source,
        scratch,
        if (writable) {
            if (persist_source) {
                "1"
            } else {
                "2"
            }
        } else {
            "0"
        },
        seconds.to_string(),
    ]
        .concat(sandbox_args(command, source, scratch, writable, cli = cli));
    let child = baml.sys.start_process(
        "/usr/bin/env",
        safe_args("unshare", args),
        baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
    );
    defer { child.close(); }
    child.stdin.close();
    let out = spawn { bounded_output(child.stdout) };
    let err = spawn { bounded_output(child.stderr ?? baml.sys.panic("Missing stderr")) };
    let status = child.wait();
    let result = Execution { exit_code: status.exit_code, stdout: await out, stderr: await err };
    require(
        result.exit_code >= 0 && result.exit_code < 124 && !result.stderr.starts_with("bwrap:"),
        "Sandbox could not complete: " + result.stderr.slice(0, 500),
    );
    result
}

/// Executes a command inside this run's isolated filesystem.
function shell(command: string) -> Execution {
    sandbox(
        command,
        run_dir() + "/worktree",
        run_dir() + "/scratch",
        writable = baml.env.get("MINIATB_FIX") == "1",
    )
}

function validate_repro(files: map<string, string>) -> void {
    require(files.keys().length() > 0 && files.keys().length() <= 30, "Repro files missing or too many");
    let size = 0;
    for (let name in files.keys()) {
        require(
            !name.starts_with("/")
                && !name.includes("\\")
                && name.split("/")
                    .filter((part) -> {
                        part.starts_with(".") || part == ""
                    })
                    .length()
                    == 0,
            "Unsafe repro path",
        );
        require(name.ends_with(".baml") || name.ends_with(".toml"), "Repros support only BAML projects");
        size += (files.get(name) ?? "").length();
    }
    require(size <= 200000 && files.has("baml.toml"), "Repro too large or missing baml.toml");
}

function replay(repro: Repro, cli: string? = null) -> Execution {
    validate_repro(repro.files);
    let folder = safe_exec("mktemp", ["-d", run_dir() + "/replay.XXXXXX"]);
    defer { baml.fs.remove_dir_all(folder); }
    baml.fs.chmod(folder, 511);
    for (let name in repro.files.keys()) {
        let parts = name.split("/");
        if (parts.length() > 1) {
            mkdir(folder + "/" + parts.slice(0, parts.length() - 1).join("/"));
        }
        baml.fs.write(folder + "/" + name, repro.files.get(name) ?? "");
    }
    // The isolated uid must be able to create compiler outputs and edit repros.
    safe_exec("chmod", ["-R", "a+rwX", folder]);
    sandbox(
        "baml " + repro.command + " --agent-skill-check off",
        folder,
        run_dir() + "/scratch",
        writable = true,
        cli = cli,
    )
}
