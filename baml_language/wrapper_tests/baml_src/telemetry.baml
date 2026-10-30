class PosthogCapture {
    api_key: string,
    event: string,
    distinct_id: string,
    properties: map<string, baml.json.json>,
}

function capture_string(event: PosthogCapture, key: string) -> string {
    match (event.properties.get(key)) {
        let value: string => value,
        _ => baml.sys.panic(`missing telemetry property ${key}`),
    }
}

testset "wrapper telemetry" {
    let candidate = root.harness.Candidate.from_environment();

    if (candidate.supports([root.harness.Capability.Posthog])) {
        test "contended flush helper retries after the current owner releases the lock" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.seed_telemetry();
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            fixture.seed_queue("sealed_contended.jsonl");
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            let lock = baml.fs.lock(`${fixture.root}/baml/.wrapper-telemetry-drain.lock`, 0);
            defer { lock.close(); }
            root.harness.require(fixture.captures().length() == 0, "telemetry was captured while the lock was held");
            let flush = baml.sys.subprocess(
                candidate.command[0],
                args = candidate.command.slice(1, candidate.command.length()).concat(
                    ["__flush-wrapper-telemetry"],
                ),
                cwd = fixture.cwd,
                env = root.harness.process_environment(fixture.env),
                inherit_env = false,
                stdin = "ignore",
                stdout = "pipe",
                stderr = "pipe",
            );
            defer { flush.kill(); flush.close(); }
            let stderr = flush.stderr
                ?? throw baml.errors.InvalidArgument { message: "flush stderr was not captured" };
            let wait = spawn { flush.wait(timeout = baml.time.Duration.from_seconds(45)) };
            let notification = match (stderr.lines().next()) {
                let line: string => line,
                baml.iter.Done => "",
            };
            // No startup-based delay: release immediately after the helper's
            // actual WouldBlock notification, before its retry budget expires.
            lock.close();
            root.harness.require(
                notification == "waiting for wrapper telemetry drain lock",
                "helper did not report a contended lock attempt",
            );
            root.harness.require((await wait).ok(), "contended flush failed");
            fixture.await_telemetry(1);
        }

        // Failure intended: Windows x64 and ARM intermittently exceed the non-blocking
        // deadline despite detached delivery. Once fixed, remove this marker
        // and assert expected behavior on every run. Other failures stay strict.
        test "stalled posthog delivery does not delay or change the delegated command" with root.harness.failure_intended(
            "command waited for PostHog delivery",
            enabled = baml.sys.host_info().platform == "windows",
            strict = false,
        ) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.behavior(BASE_VERSION, "cli output\n", code = 23, stderr = "cli diagnostic\n");
            fixture.seed_telemetry();
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            // Compare healthy and stalled delivery with the same telemetry setup
            // and helper startup cost, including Windows process/file scanning.
            // A transport wait still exceeds the one-second allowance.
            // Warm the executable and wait for the healthy helper to finish.
            fixture.check(
                candidate,
                BASE_VERSION,
                output = "cli output\n",
                code = 23,
                expected_stderr = "cli diagnostic\n",
            );
            fixture.await_telemetry(1);
            let baseline = baml.time.Instant.now();
            fixture.check(
                candidate,
                BASE_VERSION,
                output = "cli output\n",
                code = 23,
                expected_stderr = "cli diagnostic\n",
            );
            let allowance = baseline.elapsed().to_milliseconds() + 1000;
            fixture.await_telemetry(2);
            let captures = fixture.captures().length();
            fixture.write("telemetry.delay", "6");
            let started = baml.time.Instant.now();
            fixture.check(
                candidate,
                BASE_VERSION,
                output = "cli output\n",
                code = 23,
                expected_stderr = "cli diagnostic\n",
            );
            let elapsed = started.elapsed().to_milliseconds();
            fixture.await_telemetry(captures + 1, empty = false);
            root.harness.require(
                elapsed < allowance,
                `command waited for PostHog delivery: elapsed ${elapsed}ms, allowance ${allowance}ms`,
            );
        }

        test "posthog transport and rejection failures preserve the command result" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.write("telemetry.status", "403");
            fixture.check(candidate, BASE_VERSION, expected_stderr = "");
            fixture.await_telemetry(1, empty = false);
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = "http://127.0.0.1:1";
            fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        }

        if (candidate.label.starts_with("rust")) {
            test "rust wrapper sends only posthog even when bcs credentials are present" {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                fixture.env["DO_NOT_TRACK"] = "0";
                fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
                fixture.env["BAML_WRAPPER_BCS_TELEMETRY"] = "on";
                fixture.env["BOUNDARY_API_URL"] = fixture.env["BAML_MANIFEST_BASE_URL"];
                fixture.env["BOUNDARY_API_KEY"] = "bdry_secret_fixture";
                fixture.env["BOUNDARY_PROJECT"] = "fixture/project";
                fixture.env["CI"] = "true";
                fixture.manage(candidate, ["toolchain", "list"]);
                fixture.await_telemetry(3);
                let requests = fixture.read("post-requests.jsonl").split("\n").filter((line) -> bool {
                    line.trim() != ""
                });
                root.harness.require(requests.length() == 3, "Rust wrapper made unexpected telemetry requests");
                root.harness.require(
                    requests.every((line) -> bool {
                        baml.json.decode<string>(line) == "/capture/"
                    }),
                    "Rust wrapper attempted BCS delivery",
                );
            }
        }

        test "payload has stable identity and private context" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            baml.fs.mkdir(`${fixture.root}/project/.git`, baml.fs.MkdirOptions { recursive: true });
            fixture.cwd = `${fixture.root}/project/nested/deep`;
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(3);
            let config = baml.toml.Table.parse(fixture.read("baml/wrapper-telemetry.toml"));
            let identity = baml.json.path<string>(config.to_json(), ".anonymous_id");
            let salt = baml.json.path<string>(config.to_json(), ".salt");
            root.harness.require(
                identity.length() == 64 && salt.length() == 32,
                "identity lengths differ from CLI config",
            );
            let first = fixture.captures()[0];
            root.harness.require(
                first.event == "wrapper_invocation" && first.distinct_id == identity,
                "wrong event or identity",
            );
            root.harness.require(first.api_key.starts_with("phc_"), "PostHog ingestion key was not embedded");
            root.harness.require(capture_string(first, "command") == "toolchain", "wrong command");
            root.harness.require(first.properties.get("schema_version") == 1, "event schema version differs");
            root.harness.require(
                first.properties.get("robot") == 1 && first.properties.get("is_ci") == true,
                "CI metadata missing",
            );
            root.harness.require(
                first.properties.get("$process_person_profile") == false,
                "event creates a person profile",
            );
            root.harness.require(
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
            root.harness.require(
                capture_string(first, "wrapper_version") == (candidate.version ?? "0.2.5"),
                "wrapper version missing",
            );
            let host = fixture.host;
            root.harness.require(first.properties.get("cpu_count") == host.cpu_count, "CPU count differs");
            root.harness.require(capture_string(first, "system_platform") == host.platform, "platform differs");
            root.harness.require(
                capture_string(first, "system_architecture") == host.architecture,
                "architecture differs",
            );
            let project_path = if (fixture.windows) {
                `${fixture.root}/project`.replace_all("/", "\\")
            } else {
                `${fixture.root}/project`
            };
            let digest = baml.crypto.Sha256.new();
            digest.update(`${salt}${project_path}`.to_utf8());
            root.harness.require(
                capture_string(first, "project_id") == digest.finish().to_hex(),
                "project hash differs from CLI",
            );
            fixture.env["DO_NOT_TRACK"] = "false";
            let resolved = fixture.manage(candidate, ["toolchain", "status", "--json"]);
            baml.json.decode<root.harness.ResolutionResult>(resolved.stdout.to_string());
            fixture.await_telemetry(6);
            let second = fixture.captures()[3];
            root.harness.require(second.distinct_id == identity, "identity changed between invocations");
            root.harness.require(
                capture_string(first, "$session_id") != capture_string(second, "$session_id"),
                "session did not rotate",
            );
            root.harness.require(
                !fixture.read("telemetry.jsonl").includes(salt)
                    && !fixture.read("telemetry.jsonl").includes(fixture.root)
                    && !fixture.read("telemetry.jsonl").includes(
                        fixture.root.replace_all("\\", "\\\\"),
                    ),
                "private context leaked",
            );
        }

        test "do not track prevents identity and events" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            for (let value in ["1", "true", "TrUe", " yes ", "on", "", "invalid"]) {
                fixture.env["DO_NOT_TRACK"] = value;
                fixture.manage(candidate, ["toolchain", "list"]);
            }
            // A broken opt-out may dispatch after the command exits. Observe
            // the helper's bounded request interval before accepting silence.
            let deadline = baml.time.Instant.now();
            while (
                deadline.elapsed().to_milliseconds()
                    < baml.time.Duration.from_milliseconds(2500).to_milliseconds()
            ) {
                root.harness.require(fixture.captures().length() == 0, "DO_NOT_TRACK emitted an event");
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
            }
            root.harness.require(fixture.captures().length() == 0, "DO_NOT_TRACK emitted an event");
            root.harness.require(
                !baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`),
                "opt-out created an identity",
            );
        }

        test "persistent telemetry opt out prevents delivery" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.seed_telemetry(enabled = false);
            fixture.seed_queue("sealed_old.jsonl");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(0);
            root.harness.require(fixture.captures().length() == 0, "persistent opt-out sent an event");
        }

        test "invalid telemetry state does not affect command" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            baml.fs.mkdir(
                `${fixture.root}/baml/wrapper-telemetry.toml`,
                baml.fs.MkdirOptions { recursive: true },
            );
            let output = fixture.manage(candidate, ["toolchain", "list"]);
            root.harness.require(output.stderr.to_string() == "", "telemetry I/O failure affected diagnostics");
            root.harness.require(fixture.captures().length() == 0, "invalid state emitted telemetry");
            fixture.remove("baml/wrapper-telemetry.toml");
            fixture.write(
                "baml/wrapper-telemetry.toml",
                `
                    enabled = true
                    anonymous_id = ""
                    salt = ""\n
                `,
            );
            fixture.manage(candidate, ["toolchain", "list"]);
            root.harness.require(fixture.captures().length() == 0, "empty identity or salt emitted telemetry");
        }

        test "retries old events after server recovery" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.write("telemetry.status", "500");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(1, empty = false);
            let attempts = 0;
            while (
                !fixture.telemetry_pending().some((name) -> bool {
                    name.starts_with("sealed_")
                })
                    && attempts < 1000
            ) {
                baml.sys.sleep(baml.time.Duration.from_milliseconds(10));
                attempts += 1;
            }
            root.harness.require(
                fixture.telemetry_pending().some((name) -> bool {
                    name.starts_with("sealed_")
                }),
                "failed delivery was not retained",
            );
            fixture.remove("telemetry.status");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(7);
            root.harness.require(fixture.captures().length() == 7, "retry did not deliver the old and new event");
        }

        test "opt out purges pending events" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.seed_telemetry();
            for (let name in ["live_old.jsonl", "sealed_old.jsonl", "sending_old.jsonl"]) {
                fixture.seed_queue(name);
            }
            fixture.env["DO_NOT_TRACK"] = "1";
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            root.harness.require(
                fixture.telemetry_pending().length() == 0 && fixture.captures().length() == 0,
                "opt-out did not purge backlog",
            );
        }

        test "recovers orphaned queue files" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.seed_telemetry();
            for (
                let name in [
                    "live_old.jsonl",
                    "live_fresh.jsonl",
                    "sealed_stale.jsonl",
                    "sending_stale.jsonl",
                ]
            ) {
                fixture.seed_queue(name);
            }
            fixture.age("baml/wrapper-telemetry/live_old.jsonl", 1900);
            fixture.age("baml/wrapper-telemetry/sealed_stale.jsonl", 90000);
            fixture.age("baml/wrapper-telemetry/sending_stale.jsonl", 90000);
            let output = fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            root.harness.require(
                output.stdout.to_string() == "" && output.stderr.to_string() == "",
                "flush helper emitted output",
            );
            root.harness.require(fixture.captures().length() == 1, "orphan was not sent exactly once");
            root.harness.require(
                fixture.telemetry_pending() == ["live_fresh.jsonl"],
                "queue sweep retained stale work or sealed a live writer",
            );
        }

        test "retries only unacknowledged events in a queue file" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.seed_telemetry();
            fixture.seed_queue("sealed_partial.jsonl");
            let record = fixture.read("baml/wrapper-telemetry/sealed_partial.jsonl");
            fixture.write(
                "baml/wrapper-telemetry/sealed_partial.jsonl",
                record + record.replace_all("fixture", "retry"),
            );
            fixture.write("telemetry.reject-event", "retry");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            root.harness.require(fixture.captures().length() == 2, "first drain did not attempt both events");
            fixture.remove("telemetry.reject-event");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            let events = fixture.captures();
            root.harness.require(
                events.length() == 3 && events[2].event == "retry",
                "retry duplicated an acknowledged event",
            );
            root.harness.require(!fixture.telemetry_pending(), "retry did not consume the remaining queue");
        }

        test "recovers a claim left by an interrupted helper" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.seed_telemetry();
            fixture.seed_queue("sending_interrupted.jsonl");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            root.harness.require(
                fixture.captures().length() == 1 && !fixture.telemetry_pending(),
                "interrupted claim was not delivered once",
            );
        }

        test "concurrent telemetry drains do not duplicate events" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.seed_telemetry();
            fixture.seed_queue("sealed_concurrent.jsonl");
            fixture.write("telemetry.delay", "0.1");
            // Collect both child results before asserting so an early failure
            // cannot leave the other spawned drain unobserved.
            let first = spawn { fixture.run(candidate, ["__flush-wrapper-telemetry"]) };
            let second = spawn { fixture.run(candidate, ["__flush-wrapper-telemetry"]) };
            let outputs = [await first, await second];
            root.harness.require(outputs[0].ok() && outputs[1].ok(), "concurrent flush helper failed");
            root.harness.require(
                fixture.captures().length() == 1 && fixture.telemetry_pending().length() == 0,
                "concurrent flushes duplicated or lost a claim",
            );
        }

        test "delegation records wrapper events once" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.check(candidate, BASE_VERSION);
            fixture.await_telemetry(2);
            root.harness.require(fixture.captures().length() == 2, "delegation duplicated wrapper events");
            root.harness.require(
                fixture.captures()[0].event == "wrapper_invocation"
                    && fixture.captures()[1].event == "wrapper_resolution",
                "delegation used CLI events",
            );
        }

        test "omits sensitive arguments and paths" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.manage(
                candidate,
                ["toolchain", "use", "--global", "private-selector-secret"],
                success = false,
            );
            fixture.await_telemetry(2);
            let body = fixture.read("telemetry.jsonl");
            root.harness.require(
                !body.includes("private-selector-secret")
                    && !body.includes(fixture.root)
                    && !body.includes(fixture.root.replace_all("\\", "\\\\")),
                "telemetry includes an argument or path",
            );
            root.harness.require(
                capture_string(fixture.captures()[0], "command") == "toolchain",
                "error event contains an arbitrary command",
            );
        }

        test "help and version record events without recording flush helper" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.manage(candidate, ["toolchain", "--help"]);
            fixture.manage(candidate, ["self-update", "--help"]);
            fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = ["--version"]);
            root.harness.require(fixture.run(candidate, ["--version"]).ok(), "version invocation failed");
            fixture.await_telemetry(4);
            fixture.write(USER_CONFIG_FILE, "[invalid");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            root.harness.require(fixture.captures().length() == 4, "flush helper recorded itself");
        }

        test "records management operation outcomes" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.manage(candidate, ["toolchain", "use", "--global", BASE_VERSION]);
            fixture.await_telemetry(2);
            fixture.absent(NEXT_VERSION);
            fixture.manage(candidate, ["toolchain", "install", NEXT_VERSION]);
            fixture.await_telemetry(4);
            fixture.config("canary");
            fixture.channel("canary", NEXT_VERSION);
            fixture.manage(candidate, ["toolchain", "update"]);
            fixture.await_telemetry(8);
            fixture.manage(candidate, ["self-update"], success = false);
            fixture.await_telemetry(10);
            let events = fixture.captures();
            for (let name in ["wrapper_selection", "wrapper_install", "wrapper_update"]) {
                root.harness.require(
                    events.some((event) -> bool {
                        event.event == name && event.properties.get("outcome") == "success"
                    }),
                    `missing ${name} success`,
                );
            }
            root.harness.require(
                events.some((event) -> bool {
                    event.event == "wrapper_self_update"
                        && event.properties.get("outcome") == "failure"
                }),
                "missing self-update failure",
            );
            root.harness.require(
                events.some((event) -> bool {
                    event.event == "wrapper_selection" && event.properties.get("scope") == "user"
                }),
                "user selection omitted its scope",
            );
        }

        test "unknown command telemetry uses other and describes the winning selector" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
            let args = ["private-future-command", "private-argument"];
            fixture.behavior(NEXT_VERSION, "forwarded\n", args = args);
            fixture.check(candidate, NEXT_VERSION, args = args, output = "forwarded\n");
            fixture.await_telemetry(2);
            let events = fixture.captures();
            root.harness.require(events.length() == 2, "unknown command emitted unexpected events");
            root.harness.require(
                capture_string(events[0], "command") == "other",
                "unknown command escaped the allowlist",
            );
            root.harness.require(
                events[1].event == "wrapper_resolution"
                    && capture_string(events[1], "outcome") == "success"
                    && capture_string(events[1], "source") == "environment"
                    && capture_string(events[1], "toolchain_kind") == "version",
                "resolution event omitted the winning selector category",
            );
            root.harness.require(
                !fixture.read("telemetry.jsonl").includes("private-"),
                "unknown command leaked arguments",
            );
        }

        test "failed project selection records its outcome and scope" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.manage(candidate, ["toolchain", "use", "./missing/baml-cli"], success = false);
            fixture.await_telemetry(2);
            root.harness.require(
                fixture.captures().some((event) -> bool {
                    event.event == "wrapper_selection"
                        && event.properties.get("outcome") == "failure"
                        && event.properties.get("scope") == "project"
                        && event.properties.get("toolchain_kind") == "path"
                }),
                "failed project selection omitted its outcome or scope",
            );
        }

        test "failed explicit installation records its outcome" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.absent(NEXT_VERSION);
            fixture.release(NEXT_VERSION, "checksum");
            fixture.manage(candidate, ["toolchain", "install", NEXT_VERSION], success = false);
            fixture.await_telemetry(2);
            root.harness.require(
                fixture.captures().some((event) -> bool {
                    event.event == "wrapper_install" && event.properties.get("outcome") == "failure"
                }),
                "failed explicit installation omitted its outcome",
            );
        }

        test "failed channel update records its outcome" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.project("canary", key = "channel");
            fixture.absent(NEXT_VERSION);
            fixture.channel("canary", NEXT_VERSION);
            fixture.write("server/canary.json.status", "500");
            fixture.manage(candidate, ["toolchain", "update"], success = false);
            fixture.await_telemetry(2);
            root.harness.require(
                fixture.captures().some((event) -> bool {
                    event.event == "wrapper_update" && event.properties.get("outcome") == "failure"
                }),
                "failed channel update omitted its outcome",
            );
        }

        test "global telemetry preference prevents events" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.write("baml/telemetry.toml", "enabled = false\n");
            fixture.seed_queue("sealed_old.jsonl");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(0);
            root.harness.require(fixture.captures().length() == 0, "global telemetry preference was ignored");
            root.harness.require(
                !baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`),
                "global opt-out initialized wrapper telemetry",
            );
        }

        test "delivery obeys request deadline" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.seed_telemetry();
            // Warm the packed host before timing network work.
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            fixture.seed_queue("sealed_timeout.jsonl");
            fixture.write("telemetry.delay", "6");
            let start = baml.time.Instant.now();
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            let elapsed = start.elapsed().to_milliseconds();
            root.harness.require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                "delivery ignored its two-second request deadline",
            );
            root.harness.require(fixture.telemetry_pending() == ["sealed_timeout.jsonl"], "timeout discarded its retry");
        }

        test "timeout stops backlog drain" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.seed_telemetry();
            // Warm the packed host before timing network work; its first process
            // startup can include loading and verifying the entire embedded runtime.
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            for (let name in ["sealed_one.jsonl", "sealed_two.jsonl", "sealed_three.jsonl"]) {
                fixture.seed_queue(name);
            }
            fixture.write("telemetry.delay", "6");
            let start = baml.time.Instant.now();
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            let elapsed = start.elapsed().to_milliseconds();
            root.harness.require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${fixture.captures().length()}`,
            );
            root.harness.require(fixture.telemetry_pending().length() == 3, "failed delivery discarded backlog");
            root.harness.require(
                fixture.captures().length() == 1,
                "drain did not distinguish timeout from HTTP rejection",
            );
        }

        test "http rejection continues backlog drain" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.seed_telemetry();
            // Warm the packed host before timing network work; its first process
            // startup can include loading and verifying the entire embedded runtime.
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            for (let name in ["sealed_one.jsonl", "sealed_two.jsonl", "sealed_three.jsonl"]) {
                fixture.seed_queue(name);
            }
            fixture.write("telemetry.status", "500");
            let start = baml.time.Instant.now();
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            let elapsed = start.elapsed().to_milliseconds();
            root.harness.require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${fixture.captures().length()}`,
            );
            root.harness.require(fixture.telemetry_pending().length() == 3, "failed delivery discarded backlog");
            root.harness.require(
                fixture.captures().length() == 3,
                "drain did not distinguish timeout from HTTP rejection",
            );
        }

        if (candidate.supports([root.harness.Capability.PosixPermissions])) {
            test "unreadable queue file does not block readable backlog" {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                fixture.env["DO_NOT_TRACK"] = "0";
                fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
                fixture.env["CI"] = "true";
                fixture.seed_telemetry();
                fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
                fixture.seed_queue("sealed_one.jsonl");
                fixture.seed_queue("sealed_two.jsonl");
                // Use the filesystem's enumeration order, as the BAML drain does,
                // so the unreadable item precedes the readable one in the regression.
                let directory = `${fixture.root}/baml/wrapper-telemetry`;
                let first = baml.fs.read_dir(directory)[0].name;
                let unreadable = `baml/wrapper-telemetry/${first}`;
                fixture.chmod(unreadable, "000");
                defer { fixture.chmod(unreadable, "600"); }

                fixture.manage(candidate, ["__flush-wrapper-telemetry"]);

                root.harness.require(fixture.captures().length() == 1, "unreadable file blocked readable backlog");
                root.harness.require(
                    fixture.telemetry_pending() == [first],
                    "unreadable file was not retained for retry",
                );
            }
        }
    }
}
