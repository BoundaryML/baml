// Wrapper help, CLI delegation, argument validation, and standard streams.

testset "CLI delegation" {
    let candidate = Candidate.from_environment();

    test "delegation preserves arguments stdio and exit code" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.behavior(
            BASE_VERSION,
            "",
            code = 23,
            args = ["check", "--weird", "a b", "ü"],
            stderr = "diagnostic",
            echo_stdin = true,
        );
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["check", "--weird", "a b", "ü"],
            output = `
                stdin

                no final newline
            `,
            code = 23,
            stdin = `
                stdin

                no final newline
            `,
            expected_stderr = "diagnostic",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "removed pin command is rejected" with failure_intended("management exit 0: ") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.manage(candidate, ["toolchain", "pin", BASE_VERSION], success = false);
        require(fixture.requests().length() == 0, "removed pin command used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "invalid management arguments are rejected" with failure_intended("management exit 0: ") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.manage(candidate, ["toolchain", "use", "--unknown", BASE_VERSION], success = false);
        fixture.manage(candidate, ["toolchain", "update", "extra"], success = false);
        require(fixture.requests().length() == 0, "invalid wrapper arguments used network");
    }

    test "delegation supports simultaneous binary stdio" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let payload = "x".repeat(STRESS_PAYLOAD_BYTES);
        fixture.behavior(BASE_VERSION, payload, stderr = payload, echo_stdin = true);
        let child = baml.sys.start_process(
            candidate.command[0],
            candidate.command.slice(1, candidate.command.length()).concat(["check"]),
            baml.sys.ProcessOptions {
                cwd: fixture.cwd,
                env: fixture.env,
                timeout_ms: 15000,
                stderr: baml.sys.StderrMode.Pipe,
            },
        );
        defer { child.close(); }
        let out = spawn { child.stdout.as<baml.io.Read>.bytes() };
        let err = spawn { (child.stderr ?? baml.sys.panic("missing stderr pipe")).as<baml.io.Read>.bytes() };
        let input = payload.to_utf8().concat(uint8array.from_hex("00ff80fe0a"));
        child.stdin.as<baml.io.Write>.write(input);
        child.stdin.close();
        require(child.wait().ok(), "duplex child failed");
        require(
            (await out).to_hex() == payload.to_utf8().concat(input).to_hex(),
            "stdout bytes changed or were truncated",
        );
        require(
            (await err).to_hex() == payload.to_utf8().to_hex(),
            "stderr bytes changed or were truncated",
        );
        let calls = fixture.calls();
        require(
            calls.length() == 1 && calls[0].identity == BASE_VERSION && calls[0].args == ["check"],
            "unexpected duplex invocation list",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "removed manifest option is rejected before installing" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        let url = fixture.env["BAML_MANIFEST_BASE_URL"];
        fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        fixture.manage(
            candidate,
            ["toolchain", "--manifest-base-url", url, "install", BASE_VERSION],
            success = false,
            diagnostic = "--manifest-base-url",
        );
        require(fixture.requests().length() == 0, "removed metadata option was accepted");
        fixture.env["BAML_MANIFEST_BASE_URL"] = url;
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION]);
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "channel use ignores old manifest source" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = "${BASE_VERSION}"
                resolved_at = "x"
                manifest_path = "x"
                manifest_base_url = "http://127.0.0.1:1"\n
            `,
        );
        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "canary"]);
        fixture.check(candidate, LATEST_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "prerelease and build metadata are valid selectors" with failure_intended("exit 1: BAML toolchain 0.11.1-nightly.20261026.a+build.001 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let release = "0.11.1-nightly.20261026.a+build.001";
        fixture.release(release, "valid");
        fixture.behavior(release, `${release}\n`);
        fixture.env["BAML_TOOLCHAIN"] = release;
        fixture.check(candidate, release);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "insecure manifest environment is rejected" with failure_intended("insecure manifest source was not rejected") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let insecure = "http://releases.invalid/manifest";
        let args = ["toolchain", "install", BASE_VERSION];
        fixture.env["BAML_MANIFEST_BASE_URL"] = insecure;
        let output = fixture.manage(candidate, args, success = false);
        require(output.stderr.to_string().includes("HTTPS"), "insecure manifest source was not rejected");
        require(fixture.requests().length() == 0, "invalid source contacted the fixture server");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "removed manifest override is rejected" with failure_intended("missing diagnostic --manifest-base-url:") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let insecure = "http://releases.invalid/manifest";
        let args = ["toolchain", "install", BASE_VERSION];
        fixture.manage(
            candidate,
            args.concat(["--manifest-base-url", insecure]),
            success = false,
            diagnostic = "--manifest-base-url",
        );
        require(fixture.requests().length() == 0, "removed option used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "root help lists wrapper commands without installing" with failure_intended("management exit 1: BAML toolchain not-a-version is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        for (let args in [[], ["--help"], ["-h"], ["help"]]) {
            let result = fixture.manage(candidate, args);
            require(
                result.stdout.to_string().includes("toolchain")
                    && result.stdout.to_string().includes("self-update"),
                "root help omitted wrapper commands",
            );
            require(
                !result.stdout.to_string().includes("__flush-wrapper-telemetry"),
                "hidden helper leaked into help",
            );
        }
        require(fixture.requests().length() == 0, "wrapper help downloaded a toolchain");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "wrapper command help does not install" with failure_intended("management exit 1: BAML toolchain not-a-version is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        for (
            let args in [
                ["help", "toolchain"],
                ["help", "toolchain", "status"],
                ["toolchain", "status", "--help"],
                ["toolchain", "help", "status"],
                ["help", "self-update"],
                ["self-update", "--help"],
            ]
        ) {
            fixture.manage(candidate, args);
        }
        require(fixture.requests().length() == 0, "wrapper help downloaded a toolchain");
    }

    test "cli command help is forwarded" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        fixture.env["BAML_TOOLCHAIN"] = BASE_VERSION;
        for (let args in [["check", "--help"], ["help", "check"], ["help", "generate", "python"]]) {
            fixture.behavior(BASE_VERSION, "CLI help\n", args = args);
            fixture.check(candidate, BASE_VERSION, args = args, output = "CLI help\n");
        }
        require(fixture.requests().length() == 0, "wrapper help downloaded a toolchain");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "language cli commands and unknown commands are forwarded" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        for (
            let args in [
                ["update"],
                ["upgrade"],
                ["future-command", "--flag"],
                ["pack", "--unknown"],
                ["run", "main", "--", "--project", "target"],
                ["query", "--project", "cloud/name"],
            ]
        ) {
            fixture.behavior(BASE_VERSION, "forwarded\n", args = args);
            fixture.check(candidate, BASE_VERSION, args = args, output = "forwarded\n");
        }
        require(fixture.requests().length() > 0, "pass-through did not auto-install");
    }
}
