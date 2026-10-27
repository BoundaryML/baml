class PosthogCapture {
    api_key: string,
    event: string,
    distinct_id: string,
    properties: map<string, baml.json.json>,
}

function captures(f: Fixture) -> PosthogCapture[] {
    let result: PosthogCapture[] = [];
    for (let line in f.read("telemetry.jsonl").split("\n")) {
        if (line.trim() != "") {
            result.push(baml.json.decode<PosthogCapture>(line));
        }
    }
    result
}

function telemetry_pending(f: Fixture) -> string[] {
    let dir = `${f.root}/baml/wrapper-telemetry`;
    if (!baml.fs.exists(dir)) {
        return [];
    }
    baml.fs.read_dir(dir).map((entry) -> string {
        entry.name
    })
}

function await_telemetry(f: Fixture, count: int, empty: bool = true) -> null {
    let attempts = 0;
    while (attempts < 1000) {
        if (captures(f).length() >= count && (!empty || telemetry_pending(f).length() == 0)) {
            return null;
        }
        baml.sys.sleep(baml.time.Duration.from_milliseconds(10));
        attempts += 1;
    }
    require(
        false,
        `telemetry did not settle: ${f.read("telemetry.jsonl")}; pending ${baml.json.stringify(telemetry_pending(f))}`,
    );
    null
}

function capture_string(event: PosthogCapture, key: string) -> string {
    match (event.properties.get(key)) {
        let value: string => value,
        _ => baml.sys.panic(`missing telemetry property ${key}`),
    }
}

function seed_telemetry(f: Fixture, enabled: bool = true) -> null {
    f.write(
        "baml/wrapper-telemetry.toml",
        `schema_version = 1\nenabled = ${if (enabled) { "true" } else { "false" }}\nnotified_at = 0\nanonymous_id = "shared-identity"\nsalt = "local-secret-salt"\n`,
    );
    null
}

function seed_queue(f: Fixture, name: string) -> null {
    baml.fs.mkdir(`${f.root}/baml/wrapper-telemetry`, baml.fs.MkdirOptions { recursive: true });
    f.write(
        `baml/wrapper-telemetry/${name}`,
        "{\"api_key\":\"local-test-key\",\"event\":\"fixture\",\"distinct_id\":\"shared-identity\",\"properties\":{}}\n",
    );
    null
}

function telemetry_scenario(f: Fixture, c: Candidate, id: string) -> null {
    f.env["DO_NOT_TRACK"] = "0";
    f.env["BAML_WRAPPER_TELEMETRY_HOST"] = f.env["BAML_MANIFEST_BASE_URL"];
    f.env["CI"] = "true";
    if (id == "T01-payload") {
        baml.fs.mkdir(`${f.root}/project/.git`, baml.fs.MkdirOptions { recursive: true });
        f.cwd = `${f.root}/project/nested/deep`;
        f.manage(c, ["toolchain", "list"]);
        await_telemetry(f, 3);
        let config = baml.toml.Table.parse(f.read("baml/wrapper-telemetry.toml"));
        let identity = baml.json.path<string>(config.to_json(), ".anonymous_id");
        let salt = baml.json.path<string>(config.to_json(), ".salt");
        require(identity.length() == 64 && salt.length() == 32, "identity lengths differ from CLI config");
        let first = captures(f)[0];
        require(
            first.event == "wrapper_invocation" && first.distinct_id == identity,
            "wrong event or identity",
        );
        require(first.api_key.starts_with("phc_"), "PostHog ingestion key was not embedded");
        require(capture_string(first, "command") == "toolchain", "wrong command");
        require(
            first.properties.get("robot") == 1 && first.properties.get("is_ci") == true,
            "CI metadata missing",
        );
        require(first.properties.get("$process_person_profile") == false, "event creates a person profile");
        require(
            capture_string(first, "wrapper_implementation")
                == (
                    if (c.label.starts_with("rust")) {
                        "rust"
                    } else {
                        "baml"
                    }
                ),
            "implementation missing",
        );
        require(
            capture_string(first, "wrapper_version") == (c.version ?? "0.2.5"),
            "wrapper version missing",
        );
        let host = baml.sys.host_info();
        require(first.properties.get("cpu_count") == host.cpu_count, "CPU count differs");
        require(capture_string(first, "system_platform") == host.platform, "platform differs");
        require(capture_string(first, "system_architecture") == host.architecture, "architecture differs");
        let project_path = if (f.windows) {
            `${f.root}/project`.replace_all("/", "\\")
        } else {
            `${f.root}/project`
        };
        let digest = baml.crypto.Sha256.new();
        digest.update(`${salt}${project_path}`.to_utf8());
        require(
            capture_string(first, "project_id") == digest.finish().to_hex(),
            "project hash differs from CLI",
        );
        f.env["DO_NOT_TRACK"] = "false";
        let resolved = f.manage(c, ["toolchain", "status", "--json"]);
        baml.json.decode<ResolutionResult>(resolved.stdout.to_string());
        await_telemetry(f, 6);
        let second = captures(f)[3];
        require(second.distinct_id == identity, "identity changed between invocations");
        require(
            capture_string(first, "$session_id") != capture_string(second, "$session_id"),
            "session did not rotate",
        );
        require(
            !f.read("telemetry.jsonl").includes(salt) && !f.read("telemetry.jsonl").includes(f.root),
            "private context leaked",
        );
        return null;
    }
    if (id == "T02-optout") {
        for (let value in ["1", "true", "TrUe", " yes ", "on", "", "invalid"]) {
            f.env["DO_NOT_TRACK"] = value;
            f.manage(c, ["toolchain", "list"]);
        }
        require(captures(f).length() == 0, "DO_NOT_TRACK emitted an event");
        require(!baml.fs.exists(`${f.root}/baml/wrapper-telemetry.toml`), "opt-out created an identity");
        return null;
    }
    if (id == "T03-disabled") {
        seed_telemetry(f, enabled = false);
        seed_queue(f, "sealed_old.jsonl");
        f.manage(c, ["toolchain", "list"]);
        await_telemetry(f, 0);
        require(captures(f).length() == 0, "persistent opt-out sent an event");
        return null;
    }
    if (id == "T04-unwritable") {
        baml.fs.mkdir(
            `${f.root}/baml/wrapper-telemetry.toml`,
            baml.fs.MkdirOptions { recursive: true },
        );
        let output = f.manage(c, ["toolchain", "list"]);
        require(output.stderr.to_string() == "", "telemetry I/O failure affected diagnostics");
        require(captures(f).length() == 0, "invalid state emitted telemetry");
        f.os(["remove", "baml/wrapper-telemetry.toml"]);
        f.write("baml/wrapper-telemetry.toml", "enabled = true\nanonymous_id = \"\"\nsalt = \"\"\n");
        f.manage(c, ["toolchain", "list"]);
        require(captures(f).length() == 0, "empty identity or salt emitted telemetry");
        return null;
    }
    if (id == "T05-retry") {
        f.write("telemetry.status", "500");
        f.manage(c, ["toolchain", "list"]);
        await_telemetry(f, 1, empty = false);
        let attempts = 0;
        while (
            !telemetry_pending(f).some((name) -> bool {
                name.starts_with("sealed_")
            })
                && attempts < 1000
        ) {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10));
            attempts += 1;
        }
        require(
            telemetry_pending(f).some((name) -> bool {
                name.starts_with("sealed_")
            }),
            "failed delivery was not retained",
        );
        f.os(["remove", "telemetry.status"]);
        f.manage(c, ["toolchain", "list"]);
        await_telemetry(f, 7);
        require(captures(f).length() == 7, "retry did not deliver the old and new event");
        return null;
    }
    if (id == "T06-purge") {
        seed_telemetry(f);
        for (let name in ["live_old.jsonl", "sealed_old.jsonl", "sending_old.jsonl"]) {
            seed_queue(f, name);
        }
        f.env["DO_NOT_TRACK"] = "1";
        f.manage(c, ["__flush-wrapper-telemetry"]);
        require(
            telemetry_pending(f).length() == 0 && captures(f).length() == 0,
            "opt-out did not purge backlog",
        );
        return null;
    }
    if (id == "T07-recovery") {
        seed_telemetry(f);
        for (
            let name in [
                "live_old.jsonl",
                "live_fresh.jsonl",
                "sealed_stale.jsonl",
                "sending_stale.jsonl",
            ]
        ) {
            seed_queue(f, name);
        }
        f.os(["age", "baml/wrapper-telemetry/live_old.jsonl", "1900"]);
        f.os(["age", "baml/wrapper-telemetry/sealed_stale.jsonl", "90000"]);
        f.os(["age", "baml/wrapper-telemetry/sending_stale.jsonl", "90000"]);
        let output = f.manage(c, ["__flush-wrapper-telemetry"]);
        require(
            output.stdout.to_string() == "" && output.stderr.to_string() == "",
            "flush helper emitted output",
        );
        require(captures(f).length() == 1, "orphan was not sent exactly once");
        require(
            telemetry_pending(f) == ["live_fresh.jsonl"],
            "queue sweep retained stale work or sealed a live writer",
        );
        return null;
    }
    if (id == "T08-concurrent") {
        seed_telemetry(f);
        seed_queue(f, "sealed_concurrent.jsonl");
        f.write("telemetry.delay", "0.1");
        let first = spawn { f.manage(c, ["__flush-wrapper-telemetry"]) };
        let second = spawn { f.manage(c, ["__flush-wrapper-telemetry"]) };
        await first;
        await second;
        require(
            captures(f).length() == 1 && telemetry_pending(f).length() == 0,
            "concurrent flushes duplicated or lost a claim",
        );
        return null;
    }
    if (id == "T09-delegation") {
        f.check(c, "0.11.0");
        await_telemetry(f, 2);
        require(captures(f).length() == 2, "delegation duplicated wrapper events");
        require(
            captures(f)[0].event == "wrapper_invocation"
                && captures(f)[1].event == "wrapper_resolution",
            "delegation used CLI events",
        );
        return null;
    }
    if (id == "T10-privacy") {
        f.manage(c, ["toolchain", "use", "--local", "private-selector-secret"], success = false);
        await_telemetry(f, 2);
        let body = f.read("telemetry.jsonl");
        require(
            !body.includes("private-selector-secret") && !body.includes(f.root),
            "telemetry includes an argument or path",
        );
        require(
            capture_string(captures(f)[0], "command") == "toolchain",
            "error event contains an arbitrary command",
        );
        return null;
    }
    if (id == "T11-help") {
        f.manage(c, ["toolchain", "--help"]);
        f.manage(c, ["self-update", "--help"]);
        f.behavior("0.11.0", "0.11.0\n", args = ["--version"]);
        require(f.run(c, ["--version"]).ok(), "version invocation failed");
        await_telemetry(f, 4);
        f.write("baml/config.toml", "[invalid");
        f.manage(c, ["__flush-wrapper-telemetry"]);
        require(captures(f).length() == 4, "flush helper recorded itself");
        return null;
    }
    if (id == "T12-operations") {
        f.manage(c, ["toolchain", "use", "--local", "0.11.0"]);
        await_telemetry(f, 2);
        f.absent("0.12.0");
        f.manage(c, ["toolchain", "install", "0.12.0"]);
        await_telemetry(f, 4);
        f.config("canary");
        f.os(["channel", "canary", "0.12.0"]);
        f.manage(c, ["toolchain", "update"]);
        await_telemetry(f, 8);
        f.manage(c, ["self-update"], success = false);
        await_telemetry(f, 10);
        let events = captures(f);
        for (let name in ["wrapper_selection", "wrapper_install", "wrapper_update"]) {
            require(
                events.some((event) -> bool {
                    event.event == name && event.properties.get("outcome") == "success"
                }),
                `missing ${name} success`,
            );
        }
        require(
            events.some((event) -> bool {
                event.event == "wrapper_self_update" && event.properties.get("outcome") == "failure"
            }),
            "missing self-update failure",
        );
        return null;
    }
    if (id == "T13-global-optout") {
        f.write("baml/telemetry.toml", "enabled = false\n");
        seed_queue(f, "sealed_old.jsonl");
        f.manage(c, ["toolchain", "list"]);
        await_telemetry(f, 0);
        require(captures(f).length() == 0, "global telemetry preference was ignored");
        require(
            !baml.fs.exists(`${f.root}/baml/wrapper-telemetry.toml`),
            "global opt-out initialized wrapper telemetry",
        );
        return null;
    }
    if (id == "T15-backlog-timeout" || id == "T16-backlog-rejected") {
        seed_telemetry(f);
        for (let name in ["sealed_one.jsonl", "sealed_two.jsonl", "sealed_three.jsonl"]) {
            seed_queue(f, name);
        }
        if (id == "T15-backlog-timeout") {
            f.write("telemetry.delay", "6");
        } else {
            f.write("telemetry.status", "500");
        }
        let start = baml.time.Instant.now().to_timestamp_milliseconds();
        f.manage(c, ["__flush-wrapper-telemetry"]);
        require(
            baml.time.Instant.now().to_timestamp_milliseconds() - start < 5000n,
            "unavailable host stalled the full backlog",
        );
        require(telemetry_pending(f).length() == 3, "failed delivery discarded backlog");
        require(
            captures(f).length()
                == (
                    if (id == "T15-backlog-timeout") {
                        1
                    } else {
                        3
                    }
                ),
            "drain did not distinguish timeout from HTTP rejection",
        );
        return null;
    }
    if (id == "T14-timeout") {
        seed_telemetry(f);
        seed_queue(f, "sealed_timeout.jsonl");
        f.write("telemetry.delay", "6");
        let start = baml.time.Instant.now().to_timestamp_milliseconds();
        f.manage(c, ["__flush-wrapper-telemetry"]);
        let elapsed = baml.time.Instant.now().to_timestamp_milliseconds() - start;
        require(elapsed < 5000n, "delivery ignored its two-second request deadline");
        require(telemetry_pending(f) == ["sealed_timeout.jsonl"], "timeout discarded its retry");
        return null;
    }
    require(false, `unknown telemetry case ${id}`);
    null
}
