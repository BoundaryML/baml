class Candidate {
    label: string,
    command: string[],
    self_update: bool,
    version: string?,
    posthog: bool?,

    function from_environment() -> Self {
        baml.json.decode<Candidate>(baml.env.get_or_panic("WRAPPER_CANDIDATE"))
    }

    function supports(self, capabilities: Capability[]) -> bool {
        let reasons = capabilities.filter_map((capability) -> string? {
            self.missing_requirement(capability)
        });
        if (reasons) {
            baml.io.println(`SKIP ${self.label}: ${reasons.join("; ")}`);
            return false;
        }
        true
    }

    function missing_requirement(self, capability: Capability) -> string? {
        let windows = (baml.env.get("OS") ?? "") == "Windows_NT";
        match (capability) {
            Capability.Posthog => {
                if (!(self.posthog ?? false)) {
                    "candidate does not own PostHog telemetry"
                } else {
                    null
                }
            },
            Capability.Windows => {
                if (!windows) {
                    "Windows only"
                } else {
                    null
                }
            },
            Capability.Symlinks => {
                if (windows) {
                    "symlink creation requires Developer Mode or privilege"
                } else {
                    null
                }
            },
            Capability.SelfUpdate => {
                if (!self.self_update) {
                    "self-update disabled build"
                } else {
                    null
                }
            },
            Capability.NoSelfUpdate => {
                if (self.self_update) {
                    "self-update enabled build"
                } else {
                    null
                }
            },
            Capability.PosixPermissions => {
                if (windows) {
                    "POSIX permission fixture; Windows ACL coverage is separate"
                } else {
                    null
                }
            },
        }
    }
}

class Invocation {
    identity: string,
    args: string[],
    cwd: string,
    env: map<string, string?>,
}

// Data returned by the OS helper before the fixture's server is started.
class FixtureHost {
    cpu_count: int,
    platform: string,
    architecture: string,
}

class FixtureEnvironment {
    root: string,
    cwd: string,
    env: map<string, string>,
    suffix: string,
    windows: bool,
    host: FixtureHost,
}

class Fixture {
    root: string,
    cwd: string,
    env: map<string, string>,
    suffix: string,
    windows: bool,
    host: FixtureHost,
    server: baml.sys.Process,

    function start() -> Self {
        let result = helper(["init"]);
        require(result.ok(), result.stderr.to_string());
        let environment = baml.json.decode<FixtureEnvironment>(result.stdout.to_string());
        let server = baml.sys.start_process(
            baml.env.get_or_panic("WRAPPER_E2E_PYTHON"),
            [baml.env.get_or_panic("WRAPPER_E2E_HELPER"), "serve", environment.root],
            null,
        ) catch (error) {
            // Resource cleanup must also happen when process creation fails.
            _ => {
                helper(["remove", environment.root, "."]);
                throw error
            },
        };
        let fixture = Fixture {
            root: environment.root,
            cwd: environment.cwd,
            env: environment.env,
            suffix: environment.suffix,
            windows: environment.windows,
            host: environment.host,
            server: server,
        };
        fixture.env["BAML_MANIFEST_BASE_URL"]
            = match (server.stdout.lines().next()) {
                let line: string => line,
                baml.iter.Done => {
                    throw baml.errors.InvalidArgument { message: "fixture server did not start" }
                },
            } catch (error) {
                // A failed readiness handshake must not leak the child or its files.
                _ => {
                    fixture.close();
                    throw error
                },
            };
        fixture
    }

    function close(self) -> null {
        self.server.close();
        self.remove(".")
    }

    function write(self, path: string, content: string) -> null {
        baml.fs.write(`${self.root}/${path}`, content);
        null
    }

    function canonicalize(self, path: string) -> string {
        let result = helper(["canonicalize", self.root, path]);
        require(result.ok(), result.stderr.to_string());
        result.stdout.to_string().trim()
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
            root.USER_CONFIG_FILE,
            `
                [default]
                selector = ${baml.json.stringify(selector)}
                [update]
                auto_check = false\n
            `,
        )
    }

    function project(self, selector: string, key: string = "version") -> null {
        self.write(
            root.PROJECT_MANIFEST_FILE,
            `
                [package]
                name = "fixture"
                [toolchain]
                ${key} = ${baml.json.stringify(selector)}\n
            `,
        )
    }

    function absent(self, version: string) -> null {
        self.remove(`baml/toolchains/${version}`)
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
        self.read("invocations.jsonl")
            .split("\n")
            .filter((line) -> bool {
                line.trim() != ""
            })
            .map((line) -> Invocation {
                baml.json.decode<Invocation>(line)
            })
    }

    function requests(self) -> string[] {
        self.read("requests.jsonl")
            .split("\n")
            .filter((line) -> bool {
                line.trim() != ""
            })
            .map((line) -> string {
                baml.json.decode<string>(line)
            })
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
            `${result.stderr.to_string()}${result.stdout.to_string()}`.includes(diagnostic),
            `missing diagnostic ${diagnostic}: ${result.stderr.to_string()}`,
        );
        result
    }

    function channel_version(self, channel: string) -> string {
        let document = baml.toml.Table.parse(self.read(root.CHANNEL_STATE_FILE));
        baml.json.path<string>(document.to_json(), `.channels.${channel}.active_version`)
    }

    function failed_check(self, c: Candidate, diagnostic: string = "") -> null {
        let output = self.run(c, ["check"]);
        require(!output.ok(), "expected launch to fail");
        require(
            output.stderr.to_string().includes(diagnostic),
            `missing ${diagnostic}: ${output.stderr.to_string()}`,
        );
        require(self.calls().length() == 0, "failed selection invoked a toolchain");
        null
    }

    function captures(self) -> root.PosthogCapture[] {
        self.read("telemetry.jsonl")
            .split("\n")
            .filter((line) -> bool {
                line.trim() != ""
            })
            .map((line) -> root.PosthogCapture {
                baml.json.decode<root.PosthogCapture>(line)
            })
    }

    function telemetry_pending(self) -> string[] {
        let dir = `${self.root}/baml/wrapper-telemetry`;
        if (!baml.fs.exists(dir)) {
            return [];
        }
        baml.fs.read_dir(dir).map((entry) -> string {
            entry.name
        })
    }

    function await_telemetry(self, count: int, empty: bool = true) -> null {
        let start = baml.time.Instant.now();
        while (start.elapsed().to_milliseconds() < root.TELEMETRY_SETTLE_TIMEOUT.to_milliseconds()) {
            if (self.captures().length() >= count && (!empty || !self.telemetry_pending())) {
                return null;
            }
            baml.sys.sleep(root.TELEMETRY_POLL_INTERVAL);
        }
        require(
            false,
            `telemetry did not settle: ${self.read("telemetry.jsonl")}; pending ${baml.json.stringify(self.telemetry_pending())}`,
        );
        null
    }

    function seed_telemetry(self, enabled: bool = true) -> null {
        self.write(
            "baml/wrapper-telemetry.toml",
            `
                schema_version = 1
                enabled = ${if (enabled) { "true" } else { "false" }}
                notified_at = 0
                anonymous_id = "shared-identity"
                salt = "local-secret-salt"\n
            `,
        );
        null
    }

    function seed_queue(self, name: string) -> null {
        baml.fs.mkdir(
            `${self.root}/baml/wrapper-telemetry`,
            baml.fs.MkdirOptions { recursive: true },
        );
        self.write(
            `baml/wrapper-telemetry/${name}`,
            "{\"api_key\":\"local-test-key\",\"event\":\"fixture\",\"distinct_id\":\"shared-identity\",\"properties\":{}}\n",
        );
        null
    }

    function check_installed_selection(self, candidate: Candidate, identity: string) -> null {
        let config = self.read(root.USER_CONFIG_FILE);
        let project = self.read(root.PROJECT_MANIFEST_FILE);
        self.check(candidate, identity);
        require(
            self.read(root.USER_CONFIG_FILE) == config && self.read(root.PROJECT_MANIFEST_FILE) == project,
            "launch changed selectors",
        );
        require(self.requests().length() == 0, "installed selection contacted release server");
        null
    }

    function remove(self, path: string) -> null {
        self.os(["remove", path])
    }
    function release(
        self,
        version: string,
        mode: FixtureReleaseMode,
        identity: string = "",
    ) -> null {
        self.os(["release", version, mode, identity])
    }
    function channel(self, channel: FixtureChannel, version: string) -> null {
        self.os(["channel", channel, version])
    }
    function symlink(self, path: string, target: string) -> null {
        self.os(["symlink", path, target])
    }
    function copy(self, source: string, destination: string) -> null {
        self.os(["copy", source, destination])
    }
    function wrapper_archive(self, mode: "valid" | "checksum" = "valid") -> null {
        self.os(["wrapper", mode])
    }
    function chmod(self, path: string, mode: FixturePermissions) -> null {
        self.os(["chmod", path, mode])
    }
    function age(self, path: string, seconds: int) -> null {
        self.os(["age", path, seconds.to_string()])
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

// Unsupported tests are excluded while their testset is collected, before any
// fixture is created. Their bodies never appear as passed tests.
enum Capability {
    Posthog,
    Windows,
    Symlinks,
    SelfUpdate,
    NoSelfUpdate,
    PosixPermissions,
}

class ResolutionResult {
    schema_version: int,
    selector: string,
    version: string?,
    source: string,
    wrapper_path: string,
    toolchain_path: string,
    managed: bool,
    installed: bool,

    function expect_project_selection(
        self,
        version: string?,
        managed: bool,
        installed: bool,
    ) -> null {
        require(self.schema_version == 1 && self.version == version, "unexpected resolved version");
        require(self.managed == managed && self.installed == installed, "unexpected resolution state");
        require(self.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(self.wrapper_path), "wrapper path does not exist");
        null
    }
}

// Fixture corruption modes are wire values accepted by fixture.py.
type FixtureReleaseMode = "valid"
    | "missing-cli"
    | "layout"
    | "traversal"
    | "link"
    | "checksum"
    | "schema"
    | "target";

type FixtureChannel = "canary" | "nightly";

type FixturePermissions = "000" | "444" | "555" | "600" | "644" | "755";

// Pins a known contract gap at this PR layer. An unexpected pass is a failure,
// so the implementation fix must also remove the marker and assert the contract.
// Match the recorded failure message so unrelated fixture/runtime errors still fail.
function failure_intended(message: string) -> testing.TestRunner {
    (run_once) -> testing.TestReportThunk {
        () -> testing.TestReport {
            let report = run_once();
            // Error reports render quoted message fields; match both literal
            // assertions and their escaped representation without broadening the failure.
            let quoted = baml.json.stringify(message);
            let escaped = quoted.slice(1, quoted.length() - 1);
            let actual = if (report.runs.length() == 1) {
                report.runs[0].message ?? ""
            } else {
                ""
            };
            if (report.outcome == "pass") {
                testing.TestReport {
                    outcome: "fail",
                    runs: [
                        testing.RunReport {
                            outcome: "fail",
                            duration_ms: 0,
                            message: "Known failure now passes: remove failure_intended and assert expected behavior",
                        },
                    ],
                }
            } else if (report.runs.length() == 1 && (actual.includes(message) || actual.includes(escaped))) {
                baml.io.println(`XFAIL (intended): ${message}`);
                testing.TestReport {
                    outcome: "pass",
                    runs: [
                        testing.RunReport {
                            outcome: "pass",
                            duration_ms: report.runs[0].duration_ms,
                            message: `Failure intended: ${message}`,
                        },
                    ],
                }
            } else {
                report
            }
        }
    }
}
