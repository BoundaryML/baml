// Structured status resolves toolchains without launching them.

testset "toolchain status" {
    let candidate = root.harness.Candidate.from_environment();

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status describes installed version" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status reports failed best effort install" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = false);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status with install downloads absent version" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status resolves recorded channel" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("nightly", key = "channel");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = NEXT_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status with install records new channel" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = NEXT_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status describes local binary without launching" with root.harness.failure_intended("management unexpectedly invoked baml-cli") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = null, managed = false, installed = true);
        let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() == 0, "local status used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status rejects missing local binary" with root.harness.failure_intended("management exit 0: ") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        fixture.remove(`project/build/baml-cli${fixture.suffix}`);
        fixture.manage(candidate, args, success = false, diagnostic = "not found");
        root.harness.require(fixture.requests().length() == 0, "invalid local resolution used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "status help does not contact release server" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);

        let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
        root.harness.require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
        root.harness.require(fixture.requests().length() == 0, "resolve help used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "plain status reports selection and latest releases" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
        root.harness.require(
            output.includes("active selector:") && output.includes("latest canary:"),
            "plain status omitted selection or releases",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status installs unrecorded channel" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        let result = baml.json.decode<root.harness.ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        root.harness.require(result.version == NEXT_VERSION && result.installed, "unrecorded channel was not installed");
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE).includes(NEXT_VERSION),
            "channel mapping was not recorded",
        );
    }
}
