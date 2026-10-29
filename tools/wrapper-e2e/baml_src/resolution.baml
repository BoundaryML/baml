// Structured status resolves toolchains without launching them.

testset "toolchain status" {
    let candidate = Candidate.from_environment();

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status describes installed version" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status reports failed best effort install" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = false,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status with install downloads absent version" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status resolves recorded channel" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("nightly", key = "channel");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status with install records new channel" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status describes local binary without launching" with failure_intended("management unexpectedly invoked baml-cli") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = result.toolchain_path,
            version = null,
            managed = false,
            installed = true,
        );
        let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() == 0, "local status used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status rejects missing local binary" with failure_intended("management exit 0: ") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        fixture.remove(`project/build/baml-cli${fixture.suffix}`);
        fixture.manage(candidate, args, success = false, diagnostic = "not found");
        require(fixture.requests().length() == 0, "invalid local resolution used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "status help does not contact release server" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);

        let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
        require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
        require(fixture.requests().length() == 0, "status help used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "plain status reports selection and latest releases" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
        require(
            output.includes("active selector:") && output.includes("latest canary:"),
            "plain status omitted selection or releases",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status installs unrecorded channel" with failure_intended("URL must use HTTPS") {
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

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "plain json status installs the recorded version without advancing the channel" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        require(fixture.read(CHANNEL_STATE_FILE) == state, "status advanced the recorded channel");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "json status cannot resolve an unrecorded channel after installation fails" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.write("server/nightly.json.status", "500");
        fixture.manage(candidate, ["toolchain", "status", "--json"], success = false);
        require(
            !baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`),
            "failed resolution recorded a channel",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status inspects local executables with and without install" with failure_intended("management unexpectedly invoked baml-cli") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");
        for (let flags in [[], ["--install"]]) {
            let output = fixture.manage(candidate, ["toolchain", "status", "--json"].concat(flags));
            let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
            let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
            require(
                fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
                "status returned the wrong local executable",
            );
            require(
                fixture.canonicalize(result.selector) == fixture.canonicalize(binary),
                "status returned the wrong local selector",
            );
            require(
                result.schema_version == 1
                    && result.version == null
                    && !result.managed
                    && result.installed,
                "unexpected local resolution state",
            );
            require(
                result.source.ends_with("baml.toml") && baml.fs.exists(result.wrapper_path),
                "local resolution omitted its origin or wrapper",
            );
        }
        require(!fixture.requests(), "local status installation flag used the network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "json status identifies environment user default and fallback sources" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        let environment = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        require(
            environment.selector == NEXT_VERSION
                && environment.version == NEXT_VERSION
                && environment.source == "$BAML_TOOLCHAIN",
            "status did not identify the environment override",
        );
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.write(PROJECT_MANIFEST_FILE, "");
        let user = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        require(
            user.selector == BASE_VERSION
                && user.version == BASE_VERSION
                && fixture.canonicalize(user.source)
                    == fixture.canonicalize(`${fixture.root}/${USER_CONFIG_FILE}`),
            "status did not identify the user default",
        );
        fixture.write(USER_CONFIG_FILE, "[update]\nauto_check = false\n");
        let fallback = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        require(
            fallback.selector == "canary"
                && fallback.version == BASE_VERSION
                && fallback.source == "default canary",
            "status did not identify the implicit canary fallback",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "removed resolve command is rejected and absent from wrapper help" with failure_intended("management exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.manage(
            candidate,
            ["toolchain", "resolve", "--json"],
            success = false,
            diagnostic = "resolve",
        );
        for (let args in [["--help"], ["toolchain", "--help"], ["toolchain", "status", "--help"]]) {
            let help = fixture.manage(candidate, args).stdout.to_string();
            require(
                !help.split("\n").some((line) -> bool {
                    line.trim() == "resolve" || line.trim().starts_with("resolve ")
                }),
                "help advertised the removed resolve command",
            );
            require(!help.includes("--directory"), "help advertised the removed directory option");
            require(!help.includes("--manifest-base-url"), "help advertised the removed manifest option");
        }
        require(!fixture.requests(), "removed command or wrapper help used the network");
    }
}
