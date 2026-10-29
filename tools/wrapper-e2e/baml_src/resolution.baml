// Structured status resolves toolchains without launching them.

testset "toolchain status" {
    let candidate = Candidate.from_environment();

    test "json status describes installed version" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    test "json status reports failed best effort install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = false);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
            "wrong toolchain path",
        );
    }

    test "json status with install downloads absent version" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = BASE_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, BASE_VERSION);
    }

    test "json status resolves recorded channel" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("nightly", key = "channel");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = NEXT_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    test "json status with install records new channel" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = NEXT_VERSION, managed = true, installed = true);
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "json status describes local binary without launching" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(version = null, managed = false, installed = true);
        let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() == 0, "local status used network");
    }

    test "json status rejects missing local binary" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        fixture.remove(`project/build/baml-cli${fixture.suffix}`);
        fixture.manage(candidate, args, success = false, diagnostic = "not found");
        require(fixture.requests().length() == 0, "invalid local resolution used network");
    }

    test "status help does not contact release server" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);

        let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
        require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
        require(fixture.requests().length() == 0, "resolve help used network");
    }

    test "plain status reports selection and latest releases" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
        require(
            output.includes("active selector:") && output.includes("latest canary:"),
            "plain status omitted selection or releases",
        );
    }

    test "json status installs unrecorded channel" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        let result = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        require(result.version == NEXT_VERSION && result.installed, "unrecorded channel was not installed");
        require(
            fixture.read(CHANNEL_STATE_FILE).includes(NEXT_VERSION),
            "channel mapping was not recorded",
        );
    }
}
