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

function telemetry_payload_has_stable_identity_and_private_context(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    baml.fs.mkdir(`${fixture.root}/project/.git`, baml.fs.MkdirOptions { recursive: true });
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.manage(candidate, ["toolchain", "list"]);
    await_telemetry(fixture, 3);
    let config = baml.toml.Table.parse(fixture.read("baml/wrapper-telemetry.toml"));
    let identity = baml.json.path<string>(config.to_json(), ".anonymous_id");
    let salt = baml.json.path<string>(config.to_json(), ".salt");
    require(identity.length() == 64 && salt.length() == 32, "identity lengths differ from CLI config");
    let first = captures(fixture)[0];
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
                if (candidate.label.starts_with("rust")) {
                    "rust"
                } else {
                    "baml"
                }
            ),
        "implementation missing",
    );
    require(
        capture_string(first, "wrapper_version") == (candidate.version ?? "0.2.5"),
        "wrapper version missing",
    );
    let host = baml.sys.host_info();
    require(first.properties.get("cpu_count") == host.cpu_count, "CPU count differs");
    require(capture_string(first, "system_platform") == host.platform, "platform differs");
    require(capture_string(first, "system_architecture") == host.architecture, "architecture differs");
    let project_path = if (fixture.windows) {
        `${fixture.root}/project`.replace_all("/", "\\")
    } else {
        `${fixture.root}/project`
    };
    let digest = baml.crypto.Sha256.new();
    digest.update(`${salt}${project_path}`.to_utf8());
    require(
        capture_string(first, "project_id") == digest.finish().to_hex(),
        "project hash differs from CLI",
    );
    fixture.env["DO_NOT_TRACK"] = "false";
    let resolved = fixture.manage(candidate, ["toolchain", "status", "--json"]);
    baml.json.decode<ResolutionResult>(resolved.stdout.to_string());
    await_telemetry(fixture, 6);
    let second = captures(fixture)[3];
    require(second.distinct_id == identity, "identity changed between invocations");
    require(
        capture_string(first, "$session_id") != capture_string(second, "$session_id"),
        "session did not rotate",
    );
    require(
        !fixture.read("telemetry.jsonl").includes(salt)
            && !fixture.read("telemetry.jsonl").includes(fixture.root),
        "private context leaked",
    );
    null
}

test "T01-payload" {
    configured_case("T01-payload", telemetry_payload_has_stable_identity_and_private_context);
}

function do_not_track_prevents_identity_and_events(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    for (let value in ["1", "true", "TrUe", " yes ", "on", "", "invalid"]) {
        fixture.env["DO_NOT_TRACK"] = value;
        fixture.manage(candidate, ["toolchain", "list"]);
    }
    require(captures(fixture).length() == 0, "DO_NOT_TRACK emitted an event");
    require(!baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`), "opt-out created an identity");
    null
}

test "T02-optout" {
    configured_case("T02-optout", do_not_track_prevents_identity_and_events);
}

function persistent_telemetry_opt_out_prevents_delivery(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    seed_telemetry(fixture, enabled = false);
    seed_queue(fixture, "sealed_old.jsonl");
    fixture.manage(candidate, ["toolchain", "list"]);
    await_telemetry(fixture, 0);
    require(captures(fixture).length() == 0, "persistent opt-out sent an event");
    null
}

test "T03-disabled" {
    configured_case("T03-disabled", persistent_telemetry_opt_out_prevents_delivery);
}

function invalid_telemetry_state_does_not_affect_command(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    baml.fs.mkdir(
        `${fixture.root}/baml/wrapper-telemetry.toml`,
        baml.fs.MkdirOptions { recursive: true },
    );
    let output = fixture.manage(candidate, ["toolchain", "list"]);
    require(output.stderr.to_string() == "", "telemetry I/O failure affected diagnostics");
    require(captures(fixture).length() == 0, "invalid state emitted telemetry");
    fixture.os(["remove", "baml/wrapper-telemetry.toml"]);
    fixture.write(
        "baml/wrapper-telemetry.toml",
        "enabled = true\nanonymous_id = \"\"\nsalt = \"\"\n",
    );
    fixture.manage(candidate, ["toolchain", "list"]);
    require(captures(fixture).length() == 0, "empty identity or salt emitted telemetry");
    null
}

test "T04-unwritable" {
    configured_case("T04-unwritable", invalid_telemetry_state_does_not_affect_command);
}

function telemetry_retries_old_events_after_server_recovery(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    fixture.write("telemetry.status", "500");
    fixture.manage(candidate, ["toolchain", "list"]);
    await_telemetry(fixture, 1, empty = false);
    let attempts = 0;
    while (
        !telemetry_pending(fixture).some((name) -> bool {
            name.starts_with("sealed_")
        })
            && attempts < 1000
    ) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(10));
        attempts += 1;
    }
    require(
        telemetry_pending(fixture).some((name) -> bool {
            name.starts_with("sealed_")
        }),
        "failed delivery was not retained",
    );
    fixture.os(["remove", "telemetry.status"]);
    fixture.manage(candidate, ["toolchain", "list"]);
    await_telemetry(fixture, 7);
    require(captures(fixture).length() == 7, "retry did not deliver the old and new event");
    null
}

test "T05-retry" {
    configured_case("T05-retry", telemetry_retries_old_events_after_server_recovery);
}

function telemetry_opt_out_purges_pending_events(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    seed_telemetry(fixture);
    for (let name in ["live_old.jsonl", "sealed_old.jsonl", "sending_old.jsonl"]) {
        seed_queue(fixture, name);
    }
    fixture.env["DO_NOT_TRACK"] = "1";
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    require(
        telemetry_pending(fixture).length() == 0 && captures(fixture).length() == 0,
        "opt-out did not purge backlog",
    );
    null
}

test "T06-purge" {
    configured_case("T06-purge", telemetry_opt_out_purges_pending_events);
}

function telemetry_recovers_orphaned_queue_files(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";
    seed_telemetry(fixture);
    for (
        let name in [
            "live_old.jsonl",
            "live_fresh.jsonl",
            "sealed_stale.jsonl",
            "sending_stale.jsonl",
        ]
    ) {
        seed_queue(fixture, name);
    }
    fixture.os(["age", "baml/wrapper-telemetry/live_old.jsonl", "1900"]);
    fixture.os(["age", "baml/wrapper-telemetry/sealed_stale.jsonl", "90000"]);
    fixture.os(["age", "baml/wrapper-telemetry/sending_stale.jsonl", "90000"]);
    let output = fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    require(
        output.stdout.to_string() == "" && output.stderr.to_string() == "",
        "flush helper emitted output",
    );
    require(captures(fixture).length() == 1, "orphan was not sent exactly once");
    require(
        telemetry_pending(fixture) == ["live_fresh.jsonl"],
        "queue sweep retained stale work or sealed a live writer",
    );
    null
}

test "T07-recovery" {
    configured_case("T07-recovery", telemetry_recovers_orphaned_queue_files);
}

function concurrent_telemetry_drains_do_not_duplicate_events(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    seed_telemetry(fixture);
    seed_queue(fixture, "sealed_concurrent.jsonl");
    fixture.write("telemetry.delay", "0.1");
    let first = spawn { fixture.manage(candidate, ["__flush-wrapper-telemetry"]) };
    let second = spawn { fixture.manage(candidate, ["__flush-wrapper-telemetry"]) };
    await first;
    await second;
    require(
        captures(fixture).length() == 1 && telemetry_pending(fixture).length() == 0,
        "concurrent flushes duplicated or lost a claim",
    );
    null
}

test "T08-concurrent" {
    configured_case("T08-concurrent", concurrent_telemetry_drains_do_not_duplicate_events);
}

function delegation_records_wrapper_events_once(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    fixture.check(candidate, "0.11.0");
    await_telemetry(fixture, 2);
    require(captures(fixture).length() == 2, "delegation duplicated wrapper events");
    require(
        captures(fixture)[0].event == "wrapper_invocation"
            && captures(fixture)[1].event == "wrapper_resolution",
        "delegation used CLI events",
    );
    null
}

test "T09-delegation" {
    configured_case("T09-delegation", delegation_records_wrapper_events_once);
}

function telemetry_omits_sensitive_arguments_and_paths(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    fixture.manage(
        candidate,
        ["toolchain", "use", "--local", "private-selector-secret"],
        success = false,
    );
    await_telemetry(fixture, 2);
    let body = fixture.read("telemetry.jsonl");
    require(
        !body.includes("private-selector-secret") && !body.includes(fixture.root),
        "telemetry includes an argument or path",
    );
    require(
        capture_string(captures(fixture)[0], "command") == "toolchain",
        "error event contains an arbitrary command",
    );
    null
}

test "T10-privacy" {
    configured_case("T10-privacy", telemetry_omits_sensitive_arguments_and_paths);
}

function help_and_version_record_events_without_recording_flush_helper(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    fixture.manage(candidate, ["toolchain", "--help"]);
    fixture.manage(candidate, ["self-update", "--help"]);
    fixture.behavior("0.11.0", "0.11.0\n", args = ["--version"]);
    require(fixture.run(candidate, ["--version"]).ok(), "version invocation failed");
    await_telemetry(fixture, 4);
    fixture.write("baml/config.toml", "[invalid");
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    require(captures(fixture).length() == 4, "flush helper recorded itself");
    null
}

test "T11-help" {
    configured_case("T11-help", help_and_version_record_events_without_recording_flush_helper);
}

function telemetry_records_management_operation_outcomes(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    fixture.manage(candidate, ["toolchain", "use", "--local", "0.11.0"]);
    await_telemetry(fixture, 2);
    fixture.absent("0.12.0");
    fixture.manage(candidate, ["toolchain", "install", "0.12.0"]);
    await_telemetry(fixture, 4);
    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.manage(candidate, ["toolchain", "update"]);
    await_telemetry(fixture, 8);
    fixture.manage(candidate, ["self-update"], success = false);
    await_telemetry(fixture, 10);
    let events = captures(fixture);
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
    null
}

test "T12-operations" {
    configured_case("T12-operations", telemetry_records_management_operation_outcomes);
}

function global_telemetry_preference_prevents_events(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    fixture.write("baml/telemetry.toml", "enabled = false\n");
    seed_queue(fixture, "sealed_old.jsonl");
    fixture.manage(candidate, ["toolchain", "list"]);
    await_telemetry(fixture, 0);
    require(captures(fixture).length() == 0, "global telemetry preference was ignored");
    require(
        !baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`),
        "global opt-out initialized wrapper telemetry",
    );
    null
}

test "T13-global-optout" {
    configured_case("T13-global-optout", global_telemetry_preference_prevents_events);
}

function telemetry_delivery_obeys_request_deadline(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    seed_telemetry(fixture);
    seed_queue(fixture, "sealed_timeout.jsonl");
    fixture.write("telemetry.delay", "6");
    let start = baml.time.Instant.now().to_timestamp_milliseconds();
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    let elapsed = baml.time.Instant.now().to_timestamp_milliseconds() - start;
    require(elapsed < 5000n, "delivery ignored its two-second request deadline");
    require(telemetry_pending(fixture) == ["sealed_timeout.jsonl"], "timeout discarded its retry");
    null
}

test "T14-timeout" {
    configured_case("T14-timeout", telemetry_delivery_obeys_request_deadline);
}

function telemetry_timeout_stops_backlog_drain(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    seed_telemetry(fixture);
    // Warm the packed host before timing network work; its first process
    // startup can include loading and verifying the entire embedded runtime.
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    for (let name in ["sealed_one.jsonl", "sealed_two.jsonl", "sealed_three.jsonl"]) {
        seed_queue(fixture, name);
    }
    fixture.write("telemetry.delay", "6");
    let start = baml.time.Instant.now().to_timestamp_milliseconds();
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    let elapsed = baml.time.Instant.now().to_timestamp_milliseconds() - start;
    require(
        elapsed < 5000n,
        `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${captures(fixture).length()}`,
    );
    require(telemetry_pending(fixture).length() == 3, "failed delivery discarded backlog");
    require(captures(fixture).length() == 1, "drain did not distinguish timeout from HTTP rejection");
    null
}

test "T15-backlog-timeout" {
    configured_case("T15-backlog-timeout", telemetry_timeout_stops_backlog_drain);
}

function telemetry_http_rejection_continues_backlog_drain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["DO_NOT_TRACK"] = "0";
    fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["CI"] = "true";

    seed_telemetry(fixture);
    // Warm the packed host before timing network work; its first process
    // startup can include loading and verifying the entire embedded runtime.
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    for (let name in ["sealed_one.jsonl", "sealed_two.jsonl", "sealed_three.jsonl"]) {
        seed_queue(fixture, name);
    }
    fixture.write("telemetry.status", "500");
    let start = baml.time.Instant.now().to_timestamp_milliseconds();
    fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
    let elapsed = baml.time.Instant.now().to_timestamp_milliseconds() - start;
    require(
        elapsed < 5000n,
        `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${captures(fixture).length()}`,
    );
    require(telemetry_pending(fixture).length() == 3, "failed delivery discarded backlog");
    require(captures(fixture).length() == 3, "drain did not distinguish timeout from HTTP rejection");
    null
}

test "T16-backlog-rejected" {
    configured_case("T16-backlog-rejected", telemetry_http_rejection_continues_backlog_drain);
}
