// Wrapper help, CLI delegation, argument validation, and standard streams.

function delegation_preserves_arguments_stdio_and_exit_code(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.behavior(
        "0.11.0",
        "",
        code = 23,
        args = ["check", "--weird", "a b", "ü"],
        stderr = "diagnostic",
        echo_stdin = true,
    );
    fixture.check(
        candidate,
        "0.11.0",
        args = ["check", "--weird", "a b", "ü"],
        output = "stdin\n\nno final newline",
        code = 23,
        stdin = "stdin\n\nno final newline",
        expected_stderr = "diagnostic",
    );
    null
}

test "F01" {
    configured_case("F01", delegation_preserves_arguments_stdio_and_exit_code);
}

function removed_pin_command_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.manage(candidate, ["toolchain", "pin", "0.11.0"], success = false);
    require(fixture.requests().length() == 0, "removed pin command used network");
    null
}

test "F02" {
    configured_case("F02", removed_pin_command_is_rejected);
}

function invalid_management_arguments_are_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.manage(candidate, ["toolchain", "use", "--unknown", "0.11.0"], success = false);
    fixture.manage(candidate, ["toolchain", "update", "extra"], success = false);
    require(fixture.requests().length() == 0, "invalid wrapper arguments used network");
    null
}

test "F03" {
    configured_case("F03", invalid_management_arguments_are_rejected);
}

function delegation_supports_simultaneous_binary_stdio(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let payload = "x".repeat(262144);
    fixture.behavior("0.11.0", payload, stderr = payload, echo_stdin = true);
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
    require((await err).to_hex() == payload.to_utf8().to_hex(), "stderr bytes changed or were truncated");
    let calls = fixture.calls();
    require(
        calls.length() == 1 && calls[0].identity == "0.11.0" && calls[0].args == ["check"],
        "unexpected duplex invocation list",
    );
    null
}

test "F04" {
    configured_case("F04", delegation_supports_simultaneous_binary_stdio);
}

function removed_manifest_option_is_rejected_before_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    let url = fixture.env["BAML_MANIFEST_BASE_URL"];
    fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
    fixture.manage(
        candidate,
        ["toolchain", "--manifest-base-url", url, "install", "0.11.0"],
        success = false,
        diagnostic = "--manifest-base-url",
    );
    require(fixture.requests().length() == 0, "removed metadata option was accepted");
    fixture.env["BAML_MANIFEST_BASE_URL"] = url;
    fixture.manage(candidate, ["toolchain", "install", "0.11.0"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "F05" {
    configured_case("F05", removed_manifest_option_is_rejected_before_installing);
}

function channel_use_ignores_old_manifest_source(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write(
        "baml/state.toml",
        `[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "x"\nmanifest_path = "x"\nmanifest_base_url = "http://127.0.0.1:1"\n`,
    );
    fixture.os(["channel", "canary", "0.13.0"]);
    fixture.absent("0.13.0");
    fixture.manage(candidate, ["toolchain", "use", "canary"]);
    fixture.check(candidate, "0.13.0");
    null
}

test "F06" {
    configured_case("F06", channel_use_ignores_old_manifest_source);
}

function prerelease_and_build_metadata_are_valid_selectors(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let release = "0.11.1-nightly.20261026.a+build.001";
    fixture.os(["release", release, "valid", ""]);
    fixture.behavior(release, `${release}\n`);
    fixture.env["BAML_TOOLCHAIN"] = release;
    fixture.check(candidate, release);
    null
}

test "F07" {
    configured_case("F07", prerelease_and_build_metadata_are_valid_selectors);
}

function insecure_manifest_environment_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    let insecure = "http://releases.invalid/manifest";
    let args = ["toolchain", "install", "0.11.0"];
    fixture.env["BAML_MANIFEST_BASE_URL"] = insecure;
    let output = fixture.manage(candidate, args, success = false);
    require(output.stderr.to_string().includes("HTTPS"), "insecure manifest source was not rejected");
    require(fixture.requests().length() == 0, "invalid source contacted the fixture server");
    null
}

test "F08-env" {
    configured_case("F08-env", insecure_manifest_environment_is_rejected);
}

function removed_manifest_override_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    let insecure = "http://releases.invalid/manifest";
    let args = ["toolchain", "install", "0.11.0"];
    fixture.manage(
        candidate,
        args.concat(["--manifest-base-url", insecure]),
        success = false,
        diagnostic = "--manifest-base-url",
    );
    require(fixture.requests().length() == 0, "removed option used network");
    null
}

test "F08-override" {
    configured_case("F08-override", removed_manifest_override_is_rejected);
}

function root_help_lists_wrapper_commands_without_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
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
    null
}

test "H01-root" {
    configured_case("H01-root", root_help_lists_wrapper_commands_without_installing);
}

function wrapper_command_help_does_not_install(fixture: Fixture, candidate: Candidate) -> null {
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
    null
}

test "H02-owned-help" {
    configured_case("H02-owned-help", wrapper_command_help_does_not_install);
}

function cli_command_help_is_forwarded(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
    fixture.env["BAML_TOOLCHAIN"] = "0.11.0";
    for (let args in [["check", "--help"], ["help", "check"], ["help", "generate", "python"]]) {
        fixture.behavior("0.11.0", "CLI help\n", args = args);
        fixture.check(candidate, "0.11.0", args = args, output = "CLI help\n");
    }
    require(fixture.requests().length() == 0, "wrapper help downloaded a toolchain");
    null
}

test "H03-delegated-help" {
    configured_case("H03-delegated-help", cli_command_help_is_forwarded);
}

function unknown_and_toolchain_commands_are_forwarded(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
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
        fixture.behavior("0.11.0", "forwarded\n", args = args);
        fixture.check(candidate, "0.11.0", args = args, output = "forwarded\n");
    }
    require(fixture.requests().length() > 0, "pass-through did not auto-install");
    null
}

test "P01-pass-through" {
    configured_case("P01-pass-through", unknown_and_toolchain_commands_are_forwarded);
}
