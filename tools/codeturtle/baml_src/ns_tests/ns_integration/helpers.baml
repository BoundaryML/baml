// Integration assertions live in BAML; subprocesses exercise the actual CLI and Git.
function exec(
    program: string,
    args: string[],
    cwd: string? = null,
    env: map<string, string>? = null,
) -> baml.sys.ShellOutput {
    let command = program;
    let arguments = args;
    if (env != null) {
        // ProcessOptions.env replaces the whole environment. env(1) changes only
        // these test overrides, preserving the installed BAML toolchain setup.
        let overrides: string[] = [];
        for (let key in env.keys()) {
            overrides.push(key + "=" + env[key]);
        }
        command = "/usr/bin/env";
        arguments = overrides.concat([program]).concat(args);
    }
    baml.sys.exec(
        command,
        arguments,
        baml.sys.ProcessOptions {
            cwd: cwd,
            env: null,
            timeout_ms: 60000,
            stdin: null,
            stderr: baml.sys.StderrMode.Pipe,
        },
    )
}

function success(result: baml.sys.ShellOutput) -> string {
    if (result.exit_code != 0) {
        baml.sys.panic("Command failed: " + result.stderr.to_string() + result.stdout.to_string());
    }
    result.stdout.to_string().trim()
}

function project_dir() -> string {
    success(exec("pwd", [])) + "/" + root.tests.fixture_dir() + "/.."
}

function python_path() -> string {
    success(exec("which", ["python3"]))
}

function cli(
    args: string[],
    cwd: string? = null,
    env: map<string, string>? = null,
) -> baml.sys.ShellOutput {
    exec(python_path(), [project_dir() + "/git_input.py"].concat(args), cwd = cwd, env = env)
}

function expect_exit(result: baml.sys.ShellOutput, expected: int) -> void {
    if (result.exit_code != expected) {
        baml.sys.panic(
            "Expected exit "
                + expected.to_string()
                + ", got "
                + result.exit_code.to_string()
                + ": "
                + result.stdout.to_string()
                + result.stderr.to_string(),
        );
    }
}

function report(result: baml.sys.ShellOutput, expected: int = 0) -> root.Report {
    expect_exit(result, expected);
    baml.json.from_string<root.Report>(result.stdout.to_string())
}

function with_temp<T, E>(
    body: (dir: string) -> T throws E,
) -> T throws E | baml.errors.Io | baml.errors.Timeout {
    let dir = success(exec("mktemp", ["-d", "/tmp/codeturtle-integration.XXXXXX"]));
    defer { baml.fs.remove_dir_all(dir); }
    body(dir)
}

function mkdir(path: string) -> void {
    baml.fs.mkdir(path, baml.fs.MkdirOptions { recursive: true });
}

function write(dir: string, path: string, content: string) -> void {
    let parts = path.split("/");
    if (parts.length() > 1) {
        mkdir(dir + "/" + parts.slice(0, parts.length() - 1).join("/"));
    }
    baml.fs.write(dir + "/" + path, content);
}

function copy_dir(source: string, target: string) -> void {
    mkdir(target);
    for (let entry in baml.fs.read_dir(source)) {
        if (entry.is_dir) {
            copy_dir(source + "/" + entry.name, target + "/" + entry.name);
        } else if (entry.is_file) {
            baml.fs.write(target + "/" + entry.name, baml.fs.read(source + "/" + entry.name));
        }
    }
}

function with_fixture<T, E>(body: (dir: string) -> T throws E) -> T {
    with_temp((dir) -> {
        copy_dir(root.tests.fixture_dir(), dir);
        body(dir)
    })
}

function fixture_cli(
    dir: string,
    mode: string? = null,
    extra: string[] = [],
) -> baml.sys.ShellOutput {
    let args = ["check", "--repo", dir, "--changes-path", dir + "/changes.json"];
    if (mode != null) {
        args.push("--mode=" + mode);
    }
    cli(args.concat(extra))
}

function git(dir: string, args: string[]) -> string {
    success(
        exec(
            "git",
            ["-C", dir, "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"].concat(args),
            env = {
                "GIT_AUTHOR_NAME": "CodeTurtle Test",
                "GIT_AUTHOR_EMAIL": "codeturtle@example.test",
                "GIT_COMMITTER_NAME": "CodeTurtle Test",
                "GIT_COMMITTER_EMAIL": "codeturtle@example.test",
                "GIT_CONFIG_NOSYSTEM": "1",
            },
        ),
    )
}

function commit(dir: string, title: string) -> string {
    git(dir, ["add", "."]);
    git(dir, ["commit", "-m", title]);
    git(dir, ["rev-parse", "HEAD"])
}

function with_git<T, E>(body: (dir: string, base: string) -> T throws E) -> T {
    with_temp((dir) -> {
        git(dir, ["init", "-b", "main"]);
        copy_dir(root.tests.fixture_dir() + "/.github", dir + "/.github");
        let owners = baml.fs.read(dir + "/.github/codeturtle/OWNERS");
        write(
            dir,
            ".github/codeturtle/OWNERS",
            owners + "\n/protected/ @original-owner\n* @snapshot # codeturtle: when=inspect_file\n",
        );
        write(
            dir,
            ".github/codeturtle/snapshot.baml",
            `
                function inspect_file(change: codeturtle.FileChange) -> codeturtle.Match {
                    codeturtle.Match { path: change.path, verdict: codeturtle.Verdict.Applies,
                        reason: baml.json.to_string(change), evidence: [] }
                }
            `,
        );
        write(dir, "src/timeout.rs", "let greeting = \"hello\";\n");
        write(dir, "src/api/client.rs", "pub fn original() {}\n");
        write(dir, "src/deleted.rs", "let old = true;\n");
        let lines: string[] = [];
        let i = 0;
        while (i < 30) {
            lines.push("// line " + i.to_string());
            i += 1;
        }
        write(dir, "protected/old.rs", lines.join("\n") + "\n");
        mkdir(dir + "/assets");
        baml.fs.write_bytes(dir + "/assets/data.bin", b"\x00old\x01");
        let base = commit(dir, "Baseline");
        body(dir, base)
    })
}

function change_code(dir: string) -> void {
    write(dir, "src/timeout.rs", "let timeout = std::env::var(\"BAML_REQUEST_TIMEOUT\");\n");
    write(dir, "src/api/client.rs", "pub fn original() {}\npub fn with_timeout() {}\n");
}

function git_check(
    dir: string,
    args: string[],
    expected: int = 0,
    env: map<string, string>? = null,
) -> root.Report {
    report(cli(["check", "--repo", dir, "--mode=json"].concat(args), env = env), expected = expected)
}

// The test policy records exactly what the adapter supplied, through the public extension API.
function snapshot(report: root.Report, path: string) -> root.FileChange {
    for (let requirement in report.requirements) {
        if (requirement.owners.includes("@snapshot") && requirement.path == path) {
            return baml.json.from_string<root.FileChange>(requirement.evaluation.result.reason);
        }
    }
    throw "No adapter snapshot for " + path;
}
