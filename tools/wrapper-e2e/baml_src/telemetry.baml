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
    let candidate = Candidate.from_environment();

    if (candidate.supports([Capability.Posthog])) {
        test "payload has stable identity and private context" {
            let fixture = Fixture.start();
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
            require(
                identity.length() == 64 && salt.length() == 32,
                "identity lengths differ from CLI config",
            );
            let first = fixture.captures()[0];
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
            require(
                first.properties.get("$process_person_profile") == false,
                "event creates a person profile",
            );
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
            let host = fixture.host;
            require(first.properties.get("cpu_count") == host.cpu_count, "CPU count differs");
            require(capture_string(first, "system_platform") == host.platform, "platform differs");
            require(
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
            require(
                capture_string(first, "project_id") == digest.finish().to_hex(),
                "project hash differs from CLI",
            );
            fixture.env["DO_NOT_TRACK"] = "false";
            let resolved = fixture.manage(candidate, ["toolchain", "status", "--json"]);
            baml.json.decode<ResolutionResult>(resolved.stdout.to_string());
            fixture.await_telemetry(6);
            let second = fixture.captures()[3];
            require(second.distinct_id == identity, "identity changed between invocations");
            require(
                capture_string(first, "$session_id") != capture_string(second, "$session_id"),
                "session did not rotate",
            );
            require(
                !fixture.read("telemetry.jsonl").includes(salt)
                    && !fixture.read("telemetry.jsonl").includes(fixture.root)
                    && !fixture.read("telemetry.jsonl").includes(
                        fixture.root.replace_all("\\", "\\\\"),
                    ),
                "private context leaked",
            );
        }

        test "do not track prevents identity and events" {
            let fixture = Fixture.start();
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
            while (deadline.elapsed().to_milliseconds() < baml.time.Duration.from_milliseconds(2500).to_milliseconds()) {
                require(fixture.captures().length() == 0, "DO_NOT_TRACK emitted an event");
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
            }
            require(fixture.captures().length() == 0, "DO_NOT_TRACK emitted an event");
            require(
                !baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`),
                "opt-out created an identity",
            );
        }

        test "persistent telemetry opt out prevents delivery" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            fixture.seed_telemetry(enabled = false);
            fixture.seed_queue("sealed_old.jsonl");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(0);
            require(fixture.captures().length() == 0, "persistent opt-out sent an event");
        }

        test "invalid telemetry state does not affect command" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";
            baml.fs.mkdir(
                `${fixture.root}/baml/wrapper-telemetry.toml`,
                baml.fs.MkdirOptions { recursive: true },
            );
            let output = fixture.manage(candidate, ["toolchain", "list"]);
            require(output.stderr.to_string() == "", "telemetry I/O failure affected diagnostics");
            require(fixture.captures().length() == 0, "invalid state emitted telemetry");
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
            require(fixture.captures().length() == 0, "empty identity or salt emitted telemetry");
        }

        test "retries old events after server recovery" {
            let fixture = Fixture.start();
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
            require(
                fixture.telemetry_pending().some((name) -> bool {
                    name.starts_with("sealed_")
                }),
                "failed delivery was not retained",
            );
            fixture.remove("telemetry.status");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(7);
            require(fixture.captures().length() == 7, "retry did not deliver the old and new event");
        }

        test "opt out purges pending events" {
            let fixture = Fixture.start();
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
            require(
                fixture.telemetry_pending().length() == 0 && fixture.captures().length() == 0,
                "opt-out did not purge backlog",
            );
        }

        test "recovers orphaned queue files" {
            let fixture = Fixture.start();
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
            require(
                output.stdout.to_string() == "" && output.stderr.to_string() == "",
                "flush helper emitted output",
            );
            require(fixture.captures().length() == 1, "orphan was not sent exactly once");
            require(
                fixture.telemetry_pending() == ["live_fresh.jsonl"],
                "queue sweep retained stale work or sealed a live writer",
            );
        }

        test "retries only unacknowledged events in a queue file" {
            let fixture = Fixture.start();
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
            require(fixture.captures().length() == 2, "first drain did not attempt both events");
            fixture.remove("telemetry.reject-event");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            let events = fixture.captures();
            require(
                events.length() == 3 && events[2].event == "retry",
                "retry duplicated an acknowledged event",
            );
            require(!fixture.telemetry_pending(), "retry did not consume the remaining queue");
        }

        test "recovers a claim left by an interrupted helper" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.seed_telemetry();
            fixture.seed_queue("sending_interrupted.jsonl");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            require(
                fixture.captures().length() == 1 && !fixture.telemetry_pending(),
                "interrupted claim was not delivered once",
            );
        }

        test "concurrent telemetry drains do not duplicate events" {
            let fixture = Fixture.start();
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
            require(outputs[0].ok() && outputs[1].ok(), "concurrent flush helper failed");
            require(
                fixture.captures().length() == 1 && fixture.telemetry_pending().length() == 0,
                "concurrent flushes duplicated or lost a claim",
            );
        }

        test "delegation records wrapper events once" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.check(candidate, BASE_VERSION);
            fixture.await_telemetry(2);
            require(fixture.captures().length() == 2, "delegation duplicated wrapper events");
            require(
                fixture.captures()[0].event == "wrapper_invocation"
                    && fixture.captures()[1].event == "wrapper_resolution",
                "delegation used CLI events",
            );
        }

        test "omits sensitive arguments and paths" {
            let fixture = Fixture.start();
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
            require(
                !body.includes("private-selector-secret")
                    && !body.includes(fixture.root)
                    && !body.includes(fixture.root.replace_all("\\", "\\\\")),
                "telemetry includes an argument or path",
            );
            require(
                capture_string(fixture.captures()[0], "command") == "toolchain",
                "error event contains an arbitrary command",
            );
        }

        test "help and version record events without recording flush helper" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.manage(candidate, ["toolchain", "--help"]);
            fixture.manage(candidate, ["self-update", "--help"]);
            fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = ["--version"]);
            require(fixture.run(candidate, ["--version"]).ok(), "version invocation failed");
            fixture.await_telemetry(4);
            fixture.write(USER_CONFIG_FILE, "[invalid");
            fixture.manage(candidate, ["__flush-wrapper-telemetry"]);
            require(fixture.captures().length() == 4, "flush helper recorded itself");
        }

        test "records management operation outcomes" {
            let fixture = Fixture.start();
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
                require(
                    events.some((event) -> bool {
                        event.event == name && event.properties.get("outcome") == "success"
                    }),
                    `missing ${name} success`,
                );
            }
            require(
                events.some((event) -> bool {
                    event.event == "wrapper_self_update"
                        && event.properties.get("outcome") == "failure"
                }),
                "missing self-update failure",
            );
        }

        test "global telemetry preference prevents events" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.env["DO_NOT_TRACK"] = "0";
            fixture.env["BAML_WRAPPER_TELEMETRY_HOST"] = fixture.env["BAML_MANIFEST_BASE_URL"];
            fixture.env["CI"] = "true";

            fixture.write("baml/telemetry.toml", "enabled = false\n");
            fixture.seed_queue("sealed_old.jsonl");
            fixture.manage(candidate, ["toolchain", "list"]);
            fixture.await_telemetry(0);
            require(fixture.captures().length() == 0, "global telemetry preference was ignored");
            require(
                !baml.fs.exists(`${fixture.root}/baml/wrapper-telemetry.toml`),
                "global opt-out initialized wrapper telemetry",
            );
        }

        test "delivery obeys request deadline" {
            let fixture = Fixture.start();
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
            require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                "delivery ignored its two-second request deadline",
            );
            require(fixture.telemetry_pending() == ["sealed_timeout.jsonl"], "timeout discarded its retry");
        }

        test "timeout stops backlog drain" {
            let fixture = Fixture.start();
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
            require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${fixture.captures().length()}`,
            );
            require(fixture.telemetry_pending().length() == 3, "failed delivery discarded backlog");
            require(
                fixture.captures().length() == 1,
                "drain did not distinguish timeout from HTTP rejection",
            );
        }

        test "http rejection continues backlog drain" {
            let fixture = Fixture.start();
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
            require(
                elapsed < TELEMETRY_DELIVERY_TEST_LIMIT.to_milliseconds(),
                `unavailable host stalled the full backlog: elapsed ${elapsed}, requests ${fixture.captures().length()}`,
            );
            require(fixture.telemetry_pending().length() == 3, "failed delivery discarded backlog");
            require(
                fixture.captures().length() == 3,
                "drain did not distinguish timeout from HTTP rejection",
            );
        }

        if (candidate.supports([Capability.PosixPermissions])) {
            test "unreadable queue file does not block readable backlog" {
                let fixture = Fixture.start();
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

                require(fixture.captures().length() == 1, "unreadable file blocked readable backlog");
                require(
                    fixture.telemetry_pending() == [first],
                    "unreadable file was not retained for retry",
                );
            }
        }
    }
}
