// Wrapper-owned PostHog pipeline. Event/config/queue schemas are shared only
// between the Rust and BAML wrapper implementations, independently of baml-cli.
// Telemetry boundaries discard every error so instrumentation cannot fail a command.
class WrapperTelemetryConfig {
    anonymous_id: string,
    salt: string,
    enabled: bool,
    notified_at: int,

    function load() -> WrapperTelemetryConfig {
        let path = `${baml_home()}/${TELEMETRY_CONFIG_FILE}`;
        let table = baml.toml.Table.parse(read_text(path));
        let identity = string_at(table, "anonymous_id", path);
        let salt = string_at(table, "salt", path);
        if (identity != null && salt != null) {
            let notified = match (table.items.get("notified_at")) {
                null => 0,
                let value: int => value,
                _ => fail("invalid wrapper telemetry notice timestamp"),
            };
            return WrapperTelemetryConfig {
                anonymous_id: identity ?? fail("wrapper telemetry identity is missing"),
                salt: salt ?? fail("wrapper telemetry salt is missing"),
                enabled: WrapperTelemetryConfig.preference(path),
                notified_at: notified,
            };
        }
        let config = WrapperTelemetryConfig {
            anonymous_id: telemetry_random(32),
            salt: telemetry_random(16),
            enabled: true,
            notified_at: 0,
        };
        write_atomic(
            path,
            `
                enabled = true
                notified_at = 0
                anonymous_id = ${baml.json.stringify(config.anonymous_id)}
                salt = ${baml.json.stringify(config.salt)}\n
            `,
        );
        config
    }

    function preference(path: string) -> bool {
        WrapperTelemetryConfig.preference_impl(path) catch (error) {
            baml.errors.Io | baml.toml.ParseError => false
        }
    }

    function preference_impl(path: string) -> bool {
        if (!baml.fs.exists(path)) {
            return true;
        }
        let table = baml.toml.Table.parse(baml.fs.read(path));
        match (table.items.get("enabled")) {
            null => true,
            let value: bool => value,
            _ => false,
        }
    }

    function opted_out() -> bool {
        let raw = baml.env.get("DO_NOT_TRACK") catch (error) {
            baml.errors.Io | baml.errors.ParseError => "true"
        };
        (
            match (raw) {
                null => false,
                let value: string => {
                    !TELEMETRY_DISABLED_VALUES.includes(value.trim().to_lower_case())
                },
            }
        )
            || !WrapperTelemetryConfig.preference(`${baml_home()}/${TELEMETRY_CONFIG_FILE}`)
            || !WrapperTelemetryConfig.preference(`${baml_home()}/telemetry.toml`)
    }

    function disabled_for_delivery() -> bool {
        if (WrapperTelemetryConfig.opted_out() || !telemetry_api_key().trim()) {
            return true;
        }
        !baml.fs.exists(`${baml_home()}/${TELEMETRY_CONFIG_FILE}`)
    }

    function project_id(self, directory: string) -> string {
        let start = if (windows()) {
            directory.replace_all("/", "\\")
        } else {
            directory
        };
        let project_root = start;
        let dir = start;
        while (true) {
            if (baml.fs.exists(`${dir}/.git`)) {
                project_root = dir;
                break;
            }
            let next = parent(dir);
            if (windows()) {
                next = next.replace_all("/", "\\");
            }
            if (next == dir) {
                break;
            }
            dir = next;
        }
        let digest = baml.crypto.Sha256.new();
        digest.update(self.salt.to_utf8());
        digest.update(project_root.to_utf8());
        digest.finish().to_hex()
    }
}

class WrapperTelemetry {
    config: WrapperTelemetryConfig,
    session_id: string,
    live_path: string,
    project_id: string,
    host: baml.sys.HostInfo,
    is_ci: bool,
    flushed: bool,

    function record(self, event: string, fields: map<string, baml.json.json>) -> null {
        let properties: map<string, baml.json.json> = {
            "schema_version": 1,
            "$process_person_profile": false,
            "$session_id": self.session_id,
            "project_id": self.project_id,
            "wrapper_version": wrapper_version(),
            "wrapper_implementation": "baml",
            "system_platform": self.host.platform,
            "system_architecture": self.host.architecture,
            "cpu_count": self.host.cpu_count,
            "is_ci": self.is_ci,
            "robot": if (self.is_ci) {
                1
            } else {
                0
            },
        };
        // Gap: maps do not yet provide a merge operation.
        for (let key in fields.keys()) {
            properties[key] = fields[key];
        }
        let body = baml.json.stringify(
            {
                "api_key": telemetry_api_key(),
                "event": event,
                "distinct_id": self.config.anonymous_id,
                "properties": properties,
            },
        );
        baml.fs.mkdir(parent(self.live_path), baml.fs.MkdirOptions { recursive: true });
        let file = baml.fs.open(self.live_path, "a");
        defer { file.close(); }
        file.as<baml.io.Write>.write(`${body}\n`.to_utf8());
        null
    }

    function begin(args: string[], directory: string) -> WrapperTelemetry? {
        WrapperTelemetry.begin_impl(args, directory) catch (error) {
            _ => null
        }
    }

    function begin_impl(args: string[], directory: string) -> WrapperTelemetry? {
        if (WrapperTelemetryConfig.opted_out() || !telemetry_api_key().trim()) {
            return null;
        }
        baml.fs.mkdir(baml_home(), baml.fs.MkdirOptions { recursive: true });
        let lock = try_lock(`${baml_home()}/.wrapper-telemetry.lock`);
        if let lock: baml.fs.Lock = lock {
            defer { lock.close(); }
            let config = WrapperTelemetryConfig.load();
            if (!config.enabled || !config.anonymous_id || !config.salt) {
                return null;
            }
            let host = baml.sys.host_info();
            let ci = telemetry_truthy("CI");
            if (host.stderr_is_terminal && !ci && config.notified_at == 0) {
                baml.io.eprintln(
                    `
                        \nAttention: BAML collects anonymous wrapper usage telemetry.
                        Learn more, including how to opt out, at:
                        https://boundaryml.com/telemetry\n
                    `,
                );
                let path = `${baml_home()}/${TELEMETRY_CONFIG_FILE}`;
                write_atomic(path, edit_config(read_text(path), ["notified_at"], int.parse(telemetry_now().to_string())));
            }
            let session = WrapperTelemetry {
                config: config,
                host: host,
                is_ci: ci,
                flushed: false,
                session_id: telemetry_random(32),
                live_path: `${baml_home()}/wrapper-telemetry/live_${baml.sys.pid()}_${telemetry_random(4)}.jsonl`,
                project_id: config.project_id(directory),
            };
            telemetry_event(session, "wrapper_invocation", fields = { "command": WrapperTelemetry.command(args) });
            return session;
        }
        null
    }

    function finish(session: WrapperTelemetry?) -> null {
        WrapperTelemetry.finish_impl(session) catch (error) {
            _ => null
        }
    }

    function finish_impl(session: WrapperTelemetry?) -> null {
        if let session: WrapperTelemetry = session {
            if (session.flushed) {
                return null;
            }
            session.flushed = true;
            let parts = session.live_path.split("/");
            let name = parts[parts.length() - 1];
            if (baml.fs.exists(session.live_path)) {
                baml.fs.rename(
                    session.live_path,
                    `${baml_home()}/wrapper-telemetry/sealed_${name.slice(5, name.length())}`,
                );
            }
        }
        let dir = `${baml_home()}/${TELEMETRY_QUEUE_DIRECTORY}`;
        if (baml.fs.exists(dir)) {
            for (let entry in baml.fs.read_dir(dir)) {
                if (entry.name.starts_with("sealed_")) {
                    start_detached(baml.sys.current_exe(), [FLUSH_TELEMETRY_COMMAND]);
                    break;
                }
            }
        }
        null
    }

    function command(args: string[]) -> string {
        let command = args.at(0) ?? "";
        if (
            [
                "toolchain",
                "self-update",
                "check",
                "run",
                "test",
                "generate",
                "fmt",
                "lsp",
                "playground",
                "init",
                "new",
                "pack",
                "describe",
                "query",
                "ide",
                "agent",
                "telemetry",
                "clean",
                "update",
                "upgrade",
            ]
                .includes(command)
        ) {
            return command;
        }
        if (["--version", "-V"].includes(command)) {
            return "version";
        }
        if (["--help", "-h", "help"].includes(command)) {
            return "help";
        }
        "other"
    }
}

function telemetry_truthy(name: string) -> bool {
    let value = (baml.env.get(name) ?? "").trim().to_lower_case();
    value && !TELEMETRY_DISABLED_VALUES.includes(value)
}

function telemetry_random(bytes: int) -> string {
    baml.random.SystemRandom.get().as<baml.random.Rng>.random(bytes).to_hex()
}

function telemetry_now() -> bigint {
    baml.time.Instant.now().to_timestamp_milliseconds()
}

function telemetry_event(
    session: WrapperTelemetry?,
    event: string,
    fields: map<string, baml.json.json> = {},
) -> null {
    if let session: WrapperTelemetry = session {
        session.record(event, fields) catch (error) {
            _ => null
        };
    }
    null
}

function telemetry_kind(selector: string) -> ToolchainKind {
    match (selector) {
        "canary" | "nightly" => "channel",
        _ => {
            if (is_path(selector)) {
                "path"
            } else {
                "version"
            }
        },
    }
}

function telemetry_queue_name(name: string) -> bool {
    name.starts_with("live_") || name.starts_with("sealed_") || name.starts_with("sending_")
}
