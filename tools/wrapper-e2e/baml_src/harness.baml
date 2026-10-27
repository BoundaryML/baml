class Candidate {
    label: string,
    command: string[],
    self_update: bool,
    version: string?,
}

class Invocation {
    identity: string,
    args: string[],
    cwd: string,
    env: map<string, string?>,
}

class Fixture {
    root: string,
    cwd: string,
    env: map<string, string>,
    suffix: string,
    windows: bool,

    function write(self, path: string, content: string) -> null {
        baml.fs.write(`${self.root}/${path}`, content);
        null
    }

    function read(self, path: string) -> string {
        baml.fs.read(`${self.root}/${path}`)
    }

    function os(self, args: string[]) -> null {
        let result = helper([args[0], self.root].concat(args.slice(1, args.length())));
        require(result.ok(), result.stderr.to_string());
        null
    }

    function config(self, selector: string) -> null {
        self.write(
            "baml/config.toml",
            `[default]\nselector = ${baml.json.stringify(selector)}\n[update]\nauto_check = false\n`,
        )
    }

    function project(self, selector: string, key: string = "version") -> null {
        self.write(
            "project/baml.toml",
            `[package]\nname = "fixture"\n[toolchain]\n${key} = ${baml.json.stringify(selector)}\n`,
        )
    }

    function absent(self, version: string) -> null {
        self.os(["remove", `baml/toolchains/${version}`])
    }

    function behavior(
        self,
        identity: string,
        stdout: string,
        code: int = 0,
        args: string[] = ["check"],
        stderr: string = "",
        echo_stdin: bool = false,
    ) -> null {
        self.write(
            `behaviors/${identity}.json`,
            baml.json.stringify(
                {
                    "args": [args],
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": code,
                    "echo_stdin": echo_stdin,
                },
            ),
        );
        null
    }

    function run(
        self,
        candidate: Candidate,
        args: string[],
        stdin: string? = null,
    ) -> baml.sys.ShellOutput {
        baml.sys.exec(
            candidate.command[0],
            candidate.command.slice(1, candidate.command.length()).concat(args),
            baml.sys.ProcessOptions {
                cwd: self.cwd,
                env: self.env,
                timeout_ms: 45000,
                stdin: stdin,
            },
        )
    }

    function calls(self) -> Invocation[] {
        let result: Invocation[] = [];
        for (let line in self.read("invocations.jsonl").split("\n")) {
            if (line.trim() != "") {
                result.push(baml.json.decode<Invocation>(line));
            }
        }
        result
    }

    function requests(self) -> string[] {
        let result: string[] = [];
        for (let line in self.read("requests.jsonl").split("\n")) {
            if (line.trim() != "") {
                result.push(baml.json.decode<string>(line));
            }
        }
        result
    }

    function check(
        self,
        candidate: Candidate,
        identity: string,
        args: string[] = ["check"],
        output: string? = null,
        code: int = 0,
        stdin: string? = null,
        expected_stderr: string? = null,
    ) -> null {
        let before = self.calls();
        let result = self.run(candidate, args, stdin = stdin);
        require(result.exit_code == code, `exit ${result.exit_code}: ${result.stderr.to_string()}`);
        require(
            result.stdout.to_string() == (output ?? `${identity}\n`),
            `stdout: ${result.stdout.to_string()}`,
        );
        if let expected_stderr: string = expected_stderr {
            require(result.stderr.to_string() == expected_stderr, "stderr changed");
        }
        let after = self.calls();
        require(
            after.length() == before.length() + 1,
            `expected exactly one new CLI call, got ${baml.json.stringify(after)}`,
        );
        let call = after[before.length()];
        require(call.identity == identity, `selected ${call.identity}, expected ${identity}`);
        require(
            baml.json.stringify(call.args) == baml.json.stringify(args),
            "forwarded argument array differs",
        );
        require(
            call.cwd.replace_all("\\", "/") == self.cwd.replace_all("\\", "/"),
            `cwd changed: ${call.cwd}`,
        );
        require(call.env["BAML_CLI_ALLOW_DIRECT"] == "1", "direct CLI guard was not set");
        null
    }

    function manage(
        self,
        candidate: Candidate,
        args: string[],
        success: bool = true,
        diagnostic: string = "",
    ) -> baml.sys.ShellOutput {
        let before = self.read("invocations.jsonl");
        let result = self.run(candidate, args);
        require(
            result.ok() == success,
            `management exit ${result.exit_code}: ${result.stderr.to_string()}`,
        );
        require(self.read("invocations.jsonl") == before, "management unexpectedly invoked baml-cli");
        require(
            !result.stdout.to_string().ends_with("null\n"),
            "management printed an entry-point return value",
        );
        require(
            (result.stderr.to_string() + result.stdout.to_string()).includes(diagnostic),
            `missing diagnostic ${diagnostic}: ${result.stderr.to_string()}`,
        );
        result
    }
}

function require(condition: bool, message: string) -> null throws baml.errors.InvalidArgument {
    if (!condition) {
        throw baml.errors.InvalidArgument { message: message };
    }
    null
}

function helper(args: string[]) -> baml.sys.ShellOutput {
    baml.sys.exec(
        baml.env.get_or_panic("WRAPPER_E2E_PYTHON"),
        [baml.env.get_or_panic("WRAPPER_E2E_HELPER")].concat(args),
        null,
    )
}

function fixture(name: string) -> Fixture {
    let result = helper(["init", name]);
    require(result.ok(), result.stderr.to_string());
    baml.json.decode<Fixture>(result.stdout.to_string())
}

function candidates() -> Candidate[] {
    baml.json.decode<Candidate[]>(baml.env.get_or_panic("WRAPPER_CANDIDATES"))
}

function run_case(candidate: Candidate, id: string) -> null {
    let f = fixture(id);
    defer { helper(["remove", f.root, "."]); }
    let server = baml.sys.start_process(
        baml.env.get_or_panic("WRAPPER_E2E_PYTHON"),
        [baml.env.get_or_panic("WRAPPER_E2E_HELPER"), "serve", f.root],
        null,
    );
    defer { server.close(); }
    let line = server.stdout.lines().next();
    match (line) {
        string => {
            f.env["BAML_MANIFEST_BASE_URL"] = line;
        },
        _ => {
            require(false, "fixture server did not start");
        },
    }
    scenario(f, candidate, id);
    null
}

function main() -> null {
    let failures: string[] = [];
    let filter = baml.env.get("WRAPPER_E2E_FILTER") ?? "";
    for (let candidate in candidates()) {
        for (let id in case_ids()) {
            if (!id.includes(filter)) {
                continue;
            }
            let exclusion = excluded(candidate, id);
            if (exclusion != "") {
                baml.io.println(`SKIP ${candidate.label} ${id}: ${exclusion}`);
                continue;
            }
            run_case(candidate, id) catch (error) {
                _ => {
                    let message = `${candidate.label} ${id}: ${baml.json.stringify(error)}`;
                    failures.push(message);
                    baml.io.eprintln(`FAIL ${message}`);
                    continue;
                },
            };
            baml.io.println(`PASS ${candidate.label} ${id}`);
        }
    }
    require(failures.length() == 0, `${failures.length()} wrapper scenarios failed`);
    null
}

function excluded(candidate: Candidate, id: string) -> string {
    if (id == "I20") {
        return "requires an unsupported-host build; the platform contract builds supported hosts";
    }
    let windows = (baml.env.get("OS") ?? "") == "Windows_NT";
    if ((id == "L14" || id == "L15" || id == "W04") && !windows) {
        return "Windows only";
    }
    if ((id == "L08" || id == "L06-symlink" || id == "I09-link") && windows) {
        return "symlink creation requires Developer Mode or privilege";
    }
    if (id == "W01" && candidate.self_update) {
        return "self-update enabled build";
    }
    if (
        (
            id == "W02"
                || id == "W03-download"
                || id == "W03-checksum"
                || id == "W03-replace"
                || id == "W04"
        )
            && !candidate.self_update
    ) {
        return "self-update disabled build";
    }
    if (id == "I16" && windows) {
        return "POSIX permission fixture; Windows ACL coverage is separate";
    }
    ""
}
