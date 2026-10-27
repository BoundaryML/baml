// Wrapper-owned PostHog pipeline. Event/config/queue schemas are shared only
// between the Rust and BAML wrapper implementations, independently of baml-cli.
class WrapperTelemetryConfig {
    anonymous_id: string,
    salt: string,
    enabled: bool,
    notified_at: int,
}

class WrapperTelemetry {
    config: WrapperTelemetryConfig,
    session_id: string,
    live_path: string,
    project_id: string,
    host: baml.sys.HostInfo,
    is_ci: bool,
    flushed: bool,
}

function telemetry_preference(path: string) -> bool {
    telemetry_preference_impl(path) catch (_) {
        _ => false
    }
}

function telemetry_preference_impl(path: string) -> bool {
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

function telemetry_opted_out() -> bool {
    let raw = baml.env.get("DO_NOT_TRACK") catch (_) {
        _ => "true"
    };
    (raw != null && !["false", "0", "no", "off"].includes((raw ?? "").trim().to_lower_case()))
        || !telemetry_preference(`${baml_home()}/wrapper-telemetry.toml`)
        || !telemetry_preference(`${baml_home()}/telemetry.toml`)
}

function telemetry_truthy(name: string) -> bool {
    let value = (baml.env.get(name) ?? "").trim().to_lower_case();
    value != "" && !["0", "false", "no", "off"].includes(value)
}

function telemetry_random(bytes: int) -> string {
    baml.random.SystemRandom.get().as<baml.random.Rng>.random(bytes).to_hex()
}

function telemetry_now() -> bigint {
    baml.time.Instant.now().to_timestamp_milliseconds()
}

function telemetry_config() -> WrapperTelemetryConfig {
    let path = `${baml_home()}/wrapper-telemetry.toml`;
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
            anonymous_id: identity ?? "",
            salt: salt ?? "",
            enabled: telemetry_preference(path),
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
        `enabled = true\nnotified_at = 0\nanonymous_id = ${baml.json.stringify(config.anonymous_id)}\nsalt = ${baml.json.stringify(config.salt)}\n`,
    );
    config
}

function telemetry_project_id(salt: string, directory: string) -> string {
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
    digest.update(salt.to_utf8());
    digest.update(project_root.to_utf8());
    digest.finish().to_hex()
}

function telemetry_command(args: string[]) -> string {
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

function telemetry_begin(args: string[], directory: string) -> WrapperTelemetry? {
    telemetry_begin_impl(args, directory) catch (_) {
        _ => null
    }
}

function telemetry_begin_impl(args: string[], directory: string) -> WrapperTelemetry? {
    if (telemetry_opted_out() || telemetry_api_key().trim() == "") {
        return null;
    }
    baml.fs.mkdir(baml_home(), baml.fs.MkdirOptions { recursive: true });
    let lock = baml.fs.try_lock(`${baml_home()}/.wrapper-telemetry.lock`);
    if let lock: baml.fs.File = lock {
        defer { lock.close(); }
        let config = telemetry_config();
        if (!config.enabled || config.anonymous_id == "" || config.salt == "") {
            return null;
        }
        let host = baml.sys.host_info();
        let ci = telemetry_truthy("CI");
        if (host.stderr_is_terminal && !ci && config.notified_at == 0) {
            baml.io.eprintln(
                "\nAttention: BAML collects anonymous wrapper usage telemetry.\nLearn more, including how to opt out, at:\nhttps://boundaryml.com/telemetry\n",
            );
            let path = `${baml_home()}/wrapper-telemetry.toml`;
            write_atomic(
                path,
                baml.toml.edit(read_text(path), ["notified_at"], telemetry_now().to_string(), []),
            );
        }
        let session = WrapperTelemetry {
            config: config,
            host: host,
            is_ci: ci,
            flushed: false,
            session_id: telemetry_random(32),
            live_path: `${baml_home()}/wrapper-telemetry/live_${baml.sys.pid()}_${telemetry_random(4)}.jsonl`,
            project_id: telemetry_project_id(config.salt, directory),
        };
        telemetry_event(session, "wrapper_invocation", fields = { "command": telemetry_command(args) });
        return session;
    }
    null
}

function telemetry_event(
    session: WrapperTelemetry?,
    event: string,
    fields: map<string, baml.json.json> = {},
) -> null {
    if let session: WrapperTelemetry = session {
        telemetry_event_impl(session, event, fields) catch (_) {
            _ => null
        };
    }
    null
}

function telemetry_event_impl(
    session: WrapperTelemetry,
    event: string,
    fields: map<string, baml.json.json>,
) -> null {
    let properties: map<string, baml.json.json> = {
        "schema_version": 1,
        "$process_person_profile": false,
        "$session_id": session.session_id,
        "project_id": session.project_id,
        "wrapper_version": wrapper_version(),
        "wrapper_implementation": "baml",
        "system_platform": session.host.platform,
        "system_architecture": session.host.architecture,
        "cpu_count": session.host.cpu_count,
        "is_ci": session.is_ci,
        "robot": if (session.is_ci) {
            1
        } else {
            0
        },
    };
    for (let key in fields.keys()) {
        properties[key] = fields[key];
    }
    let body = baml.json.stringify(
        {
            "api_key": telemetry_api_key(),
            "event": event,
            "distinct_id": session.config.anonymous_id,
            "properties": properties,
        },
    );
    baml.fs.mkdir(parent(session.live_path), baml.fs.MkdirOptions { recursive: true });
    let file = baml.fs.open(session.live_path, "a");
    defer { file.close(); }
    file.as<baml.io.Write>.write(`${body}\n`.to_utf8());
    null
}

function telemetry_kind(selector: string) -> string {
    if (channel(selector)) {
        "channel"
    } else if (is_path(selector)) {
        "path"
    } else {
        "version"
    }
}

function telemetry_finish(session: WrapperTelemetry?) -> null {
    telemetry_finish_impl(session) catch (_) {
        _ => null
    }
}

function telemetry_finish_impl(session: WrapperTelemetry?) -> null {
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
    let dir = `${baml_home()}/wrapper-telemetry`;
    if (baml.fs.exists(dir)) {
        for (let entry in baml.fs.read_dir(dir)) {
            if (entry.name.starts_with("sealed_")) {
                baml.sys.start_detached(baml.sys.executable(), ["__flush-wrapper-telemetry"]);
                break;
            }
        }
    }
    null
}

function telemetry_queue_name(name: string) -> bool {
    name.starts_with("live_") || name.starts_with("sealed_") || name.starts_with("sending_")
}

function telemetry_disabled_for_delivery() -> bool {
    if (telemetry_opted_out() || telemetry_api_key().trim() == "") {
        return true;
    }
    !baml.fs.exists(`${baml_home()}/wrapper-telemetry.toml`)
}

function telemetry_drain() -> null {
    telemetry_drain_impl() catch (_) {
        _ => null
    }
}

function telemetry_drain_impl() -> null {
    let dir = `${baml_home()}/wrapper-telemetry`;
    if (!baml.fs.exists(dir)) {
        return null;
    }
    let disabled = telemetry_disabled_for_delivery();
    let now = telemetry_now();
    for (let entry in baml.fs.read_dir(dir)) {
        if (!telemetry_queue_name(entry.name) || entry.is_dir) {
            continue;
        }
        let path = `${dir}/${entry.name}`;
        telemetry_sweep(path, entry.name, now, disabled) catch (_) {
            _ => null
        };
    }
    if (disabled) {
        return null;
    }
    for (let entry in baml.fs.read_dir(dir)) {
        if (entry.name.starts_with("sealed_") && !entry.is_dir) {
            telemetry_deliver(dir, entry.name) catch (_) {
                _ => null
            };
        }
    }
    null
}

function telemetry_sweep(path: string, name: string, now: bigint, disabled: bool) -> null {
    if (disabled) {
        baml.fs.remove(path);
        return null;
    }
    if let modified: int = baml.fs.stat(path).modified_at_millis {
        let age = now - bigint.parse(modified.to_string());
        if (age > 86400000n) {
            baml.fs.remove(path);
        } else if (age > 1800000n && name.starts_with("live_")) {
            baml.fs.rename(path, `${parent(path)}/sealed_${name.slice(5, name.length())}`);
        }
    }
    null
}

function telemetry_deliver(dir: string, name: string) -> null {
    let original = `${dir}/${name}`;
    let claimed = `${dir}/sending_${baml.sys.pid()}_${name}`;
    // Atomic claim: a competing helper that loses the rename skips this file.
    baml.fs.rename(original, claimed);
    let delivered = telemetry_send_file(claimed) catch (_) {
        _ => false
    };
    if (delivered) {
        baml.fs.remove(claimed);
    } else {
        baml.fs.rename(claimed, original);
    }
    null
}

function telemetry_send_file(path: string) -> bool {
    let host = baml.env.get("BAML_WRAPPER_TELEMETRY_HOST") ?? telemetry_default_host();
    ensure(secure_url(host), "telemetry host must use HTTPS or loopback HTTP");
    while (host.ends_with("/")) {
        host = host.slice(0, host.length() - 1);
    }
    for (let line in baml.fs.read(path).split("\n")) {
        if (line.trim() == "") {
            continue;
        }
        let valid = baml.json.decode<baml.json.json>(line) catch (_) {
            _ => null
        };
        if (valid == null) {
            continue;
        }
        // Re-read opt-out before every request, including backlog retries.
        if (telemetry_disabled_for_delivery()) {
            return true;
        }
        let request = baml.http.Request.from_url(`${host}/capture/`)
            .set_method("POST")
            .set_headers({ "content-type": "application/json" })
            .set_body(line)
            .set_timeout(baml.time.Duration.from_seconds(2));
        if (!baml.http.send(request).ok()) {
            return false;
        }
    }
    true
}
