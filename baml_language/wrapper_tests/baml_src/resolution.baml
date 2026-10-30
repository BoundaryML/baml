// Structured status resolves toolchains without launching them.

testset "toolchain status" {
    let candidate = root.harness.Candidate.from_environment();

    test "json status describes installed version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    test "json status reports failed best effort install" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = false,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
            "wrong toolchain path",
        );
    }

    test "json status with install downloads absent version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, BASE_VERSION);
    }

    test "json status resolves recorded channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("nightly", key = "channel");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
    }

    test "json status with install records new channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "json status describes local binary without launching" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = result.toolchain_path,
            version = null,
            managed = false,
            installed = true,
        );
        let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        root.harness.require(
            fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
            "wrong toolchain path",
        );
        root.harness.require(fixture.requests().length() == 0, "local status used network");
    }

    test "json status rejects missing local binary" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        fixture.remove(`project/build/baml-cli${fixture.suffix}`);
        fixture.manage(candidate, args, success = false, diagnostic = "not found");
        root.harness.require(fixture.requests().length() == 0, "invalid local resolution used network");
    }

    test "status help does not contact release server" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);

        let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
        root.harness.require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
        root.harness.require(fixture.requests().length() == 0, "status help used network");
    }

    test "plain status reports selection and latest releases" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
        root.harness.require(
            output.includes("active selector:") && output.includes("latest canary:"),
            "plain status omitted selection or releases",
        );
    }

    test "json status installs unrecorded channel" {
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

    test "plain json status installs the recorded version without advancing the channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        root.harness.require(fixture.read(CHANNEL_STATE_FILE) == state, "status advanced the recorded channel");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "json status cannot resolve an unrecorded channel after installation fails" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.write("server/nightly.json.status", "500");
        fixture.manage(candidate, ["toolchain", "status", "--json"], success = false);
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`),
            "failed resolution recorded a channel",
        );
    }

    test "json status inspects local executables with and without install" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");
        for (let flags in [[], ["--install"]]) {
            let output = fixture.manage(candidate, ["toolchain", "status", "--json"].concat(flags));
            let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
            let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
            root.harness.require(
                fixture.canonicalize(result.toolchain_path) == fixture.canonicalize(binary),
                "status returned the wrong local executable",
            );
            root.harness.require(
                fixture.canonicalize(result.selector) == fixture.canonicalize(binary),
                "status returned the wrong local selector",
            );
            root.harness.require(
                result.schema_version == 1
                    && result.version == null
                    && !result.managed
                    && result.installed,
                "unexpected local resolution state",
            );
            root.harness.require(
                result.source.ends_with("baml.toml") && baml.fs.exists(result.wrapper_path),
                "local resolution omitted its origin or wrapper",
            );
        }
        root.harness.require(!fixture.requests(), "local status installation flag used the network");
    }

    test "json status identifies environment user default and fallback sources" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["toolchain", "status", "--json"];
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        let environment = baml.json.decode<root.harness.ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        root.harness.require(
            environment.selector == NEXT_VERSION
                && environment.version == NEXT_VERSION
                && environment.source == "$BAML_TOOLCHAIN",
            "status did not identify the environment override",
        );
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.write(PROJECT_MANIFEST_FILE, "");
        let user = baml.json.decode<root.harness.ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        root.harness.require(
            user.selector == BASE_VERSION
                && user.version == BASE_VERSION
                && baml.fs.canonicalize(user.source)
                    == baml.fs.canonicalize(`${fixture.root}/${USER_CONFIG_FILE}`),
            "status did not identify the user default",
        );
        fixture.write(USER_CONFIG_FILE, "[update]\nauto_check = false\n");
        let fallback = baml.json.decode<root.harness.ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        root.harness.require(
            fallback.selector == "canary"
                && fallback.version == BASE_VERSION
                && fallback.source == "default canary",
            "status did not identify the implicit canary fallback",
        );
    }

    test "removed resolve command is rejected and absent from wrapper help" {
        let fixture = root.harness.Fixture.start();
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
            root.harness.require(
                !help.split("\n").some((line) -> bool {
                    line.trim() == "resolve" || line.trim().starts_with("resolve ")
                }),
                "help advertised the removed resolve command",
            );
            root.harness.require(!help.includes("--directory"), "help advertised the removed directory option");
            root.harness.require(!help.includes("--manifest-base-url"), "help advertised the removed manifest option");
        }
        root.harness.require(!fixture.requests(), "removed command or wrapper help used the network");
    }
}
