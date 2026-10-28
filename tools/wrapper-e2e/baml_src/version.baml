// Version reporting invokes the selected CLI and propagates failures.

function version_reports_wrapper_and_selected_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.behavior("0.11.0", `0.11.0\n`, args = ["--version"]);
    let output = fixture.run(candidate, ["--version"]);
    require(output.ok() && fixture.calls().length() == 1, "version did not invoke the selected CLI");
    require(
        output.stdout.to_string().includes("wrapper")
            && output.stdout.to_string().includes("0.11.0"),
        "version does not identify wrapper and toolchain",
    );
    if let expected: string = candidate.version {
        require(
            output.stdout.to_string().includes(`baml wrapper ${expected}`),
            "wrapper version differs from build version",
        );
    }
    null;
}

test "V01" {
    configured_case("V01", version_reports_wrapper_and_selected_toolchain);
}

function version_installs_and_invokes_selected_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let args = ["-V"];
    fixture.absent("0.11.0");
    fixture.behavior("0.11.0", "CLI version\n", code = 0, args = args);
    let result = fixture.run(candidate, args);
    require(result.exit_code == 0, "version hid CLI failure");
    require(
        result.stdout.to_string().includes("baml wrapper")
            && result.stdout.to_string().includes("CLI version"),
        "version omitted wrapper or CLI output",
    );
    require(
        fixture.calls().length() == 1
            && fixture.calls()[0].identity == "0.11.0"
            && fixture.calls()[0].args == args,
        "version did not invoke selected CLI with original flag",
    );
    null
}

test "V02-install" {
    configured_case("V02-install", version_installs_and_invokes_selected_toolchain);
}

function version_propagates_cli_exit_code(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["-V"];
    fixture.absent("0.11.0");
    fixture.behavior("0.11.0", "CLI version\n", code = 23, args = args);
    let result = fixture.run(candidate, args);
    require(result.exit_code == 23, "version hid CLI failure");
    require(
        result.stdout.to_string().includes("baml wrapper")
            && result.stdout.to_string().includes("CLI version"),
        "version omitted wrapper or CLI output",
    );
    require(
        fixture.calls().length() == 1
            && fixture.calls()[0].identity == "0.11.0"
            && fixture.calls()[0].args == args,
        "version did not invoke selected CLI with original flag",
    );
    null
}

test "V03-failure" {
    configured_case("V03-failure", version_propagates_cli_exit_code);
}

function version_invokes_selected_local_cli(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["-V"];
    fixture.absent("0.11.0");
    fixture.project("./build/baml-cli", key = "path");
    fixture.behavior("L1", "CLI version\n", code = 0, args = args);
    let result = fixture.run(candidate, args);
    require(result.exit_code == 0, "version hid CLI failure");
    require(
        result.stdout.to_string().includes("baml wrapper")
            && result.stdout.to_string().includes("CLI version"),
        "version omitted wrapper or CLI output",
    );
    require(
        fixture.calls().length() == 1
            && fixture.calls()[0].identity == "L1"
            && fixture.calls()[0].args == args,
        "version did not invoke selected CLI with original flag",
    );
    null
}

test "V04-local" {
    configured_case("V04-local", version_invokes_selected_local_cli);
}

function version_rejects_missing_local_cli(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("./missing/baml-cli", key = "path");
    let result = fixture.run(candidate, ["--version"]);
    require(!result.ok() && fixture.calls().length() == 0, "version hid selection or installation failure");
    null
}

test "V05-missing-local" {
    configured_case("V05-missing-local", version_rejects_missing_local_cli);
}

function version_propagates_install_failure(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.write("server/version/0.11.0.json.status", "500");
    let result = fixture.run(candidate, ["--version"]);
    require(!result.ok() && fixture.calls().length() == 0, "version hid selection or installation failure");
    null
}

test "V06-install-failure" {
    configured_case("V06-install-failure", version_propagates_install_failure);
}
