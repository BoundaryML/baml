// Structured status resolves toolchains without launching them.

testset "toolchain status" {
    let candidate = root.harness.Candidate.from_environment();

    test "json status describes installed version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--output-format", "json"];

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

    test "output format is a global option of every wrapper command" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        for (
            let args in [
                ["--output-format", "json", "toolchain", "status"],
                ["--output-format=json", "toolchain", "status"],
                ["toolchain", "--output-format", "json", "status"],
                ["toolchain", "status", "--output-format=json"],
                ["--color", "never", "--output-format", "json", "toolchain", "status"],
            ]
        ) {
            let output = fixture.manage(candidate, args);
            let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
            root.harness.require(
                result.version == BASE_VERSION,
                `${args.join(" ")} did not report the selection as JSON`,
            );
        }
        // A command with no output accepts the option and has nothing to apply it to.
        fixture.manage(candidate, ["self", "update", "--output-format", "json", "--help"]);
        let text = fixture.manage(candidate, ["toolchain", "status", "--output-format", "text"]);
        root.harness.require(
            text.stdout.to_string().starts_with("active selector:"),
            "text output format did not report text",
        );
    }

    test "invalid invocations of wrapper commands are usage errors" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        for (
            let args in [
                ["toolchain", "status", "--output-format", "yaml"],
                ["toolchain", "update", "--output-format", "yaml"],
                ["self", "update", "--output-format", "yaml"],
                ["toolchain", "status", "--output-format"],
                ["toolchain", "status", "--json"],
                ["toolchain", "use"],
                ["toolchain", "bogus"],
                ["-q", "toolchain", "list"],
            ]
        ) {
            let result = fixture.manage(candidate, args, success = false);
            root.harness.require(
                result.exit_code == 2,
                `${args.join(" ")} exited ${result.exit_code}, not with the usage status`,
            );
        }
        root.harness.require(!fixture.requests(), "an invalid invocation contacted the release server");
    }

    test "json status installs absent version by default" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status", "--output-format", "json"]);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = true,
        );
        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        root.harness.require(
            fixture.requests()
                == [
                    `/version/${BASE_VERSION}.json`,
                    `/${BASE_VERSION}${extension}`,
                    "/canary.json",
                    "/nightly.json",
                ],
            "plain status did not acquire its target once",
        );
        root.harness.require(!fixture.calls(), "status launched the selected CLI");
        fixture.check(candidate, BASE_VERSION);
    }

    test "plain and json status warn and report absent version after failed acquisition" {
        for (let flags in [[], ["--output-format", "json"]]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.project(BASE_VERSION);
            fixture.absent(BASE_VERSION);
            let state = fixture.read(CHANNEL_STATE_FILE);
            let config = fixture.read(USER_CONFIG_FILE);
            let project = fixture.read(PROJECT_MANIFEST_FILE);
            let extension = if (fixture.windows) {
                ".zip"
            } else {
                ".tar.gz"
            };
            fixture.write(`server/${BASE_VERSION}${extension}.status`, "404");
            let output = fixture.manage(candidate, ["toolchain", "status"].concat(flags));
            if (flags) {
                let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
                result.expect_project_selection(
                    selector = BASE_VERSION,
                    version = BASE_VERSION,
                    managed = true,
                    installed = false,
                );
            } else {
                root.harness.require(
                    output.stdout.to_string().includes("installed: false"),
                    "status hid absent installation",
                );
            }
            root.harness.require(
                output.stderr.to_string().includes("warning: could not ensure"),
                "status omitted optional acquisition warning",
            );
            root.harness.require(
                fixture.requests()
                    == [
                        `/version/${BASE_VERSION}.json`,
                        `/${BASE_VERSION}${extension}`,
                        "/canary.json",
                        "/nightly.json",
                    ],
                "status retried failed acquisition or skipped metadata",
            );
            root.harness.require(
                !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`)
                    && fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(USER_CONFIG_FILE) == config
                    && fixture.read(PROJECT_MANIFEST_FILE) == project
                    && !fixture.calls(),
                "failed optional acquisition changed selection or launched the CLI",
            );
        }
    }

    test "json status fails open when selected manifest is unavailable" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        let output = fixture.manage(candidate, ["toolchain", "status", "--output-format", "json"]);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = BASE_VERSION,
            version = BASE_VERSION,
            managed = true,
            installed = false,
        );
        root.harness.require(
            output.stderr.to_string().includes("warning: could not ensure"),
            "manifest failure was not advisory",
        );
        root.harness.require(
            fixture.requests() == [`/version/${BASE_VERSION}.json`, "/canary.json", "/nightly.json"],
            "status retried unavailable manifest or skipped release metadata",
        );
        root.harness.require(!fixture.calls(), "status launched the CLI after acquisition failed");
    }

    test "json status with install downloads absent version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--output-format", "json", "--install"];

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
        let args = ["toolchain", "status", "--output-format", "json"];

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
        let args = ["toolchain", "status", "--output-format", "json", "--install"];

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
        let args = ["toolchain", "status", "--output-format", "json"];

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
        let args = ["toolchain", "status", "--output-format", "json"];

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

    test "json status initializes unrecorded channel by default" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status", "--output-format", "json"]);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        root.harness.require(
            fixture.requests() == ["/nightly.json", `/${NEXT_VERSION}${extension}`, "/nightly.json"],
            "plain status did not initialize the selected channel once",
        );
        root.harness.require(
            fixture.channel_version("nightly") == NEXT_VERSION,
            "plain status did not record its channel",
        );
    }

    test "plain json status installs absent recorded version without advancing channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "status", "--output-format", "json"]);
        let result = baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string());
        result.expect_project_selection(
            selector = "nightly",
            version = NEXT_VERSION,
            managed = true,
            installed = true,
        );
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state,
            "status advanced the recorded channel",
        );
        root.harness.require(
            fixture.requests()
                == [
                    `/version/${NEXT_VERSION}.json`,
                    `/${NEXT_VERSION}${if (fixture.windows) { ".zip" } else { ".tar.gz" }}`,
                    "/nightly.json",
                ],
            "plain channel status did not acquire only the recorded version",
        );
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "plain status did not install recorded version",
        );
    }

    test "json status leaves unavailable unrecorded channel untouched" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.write("server/nightly.json.status", "500");
        fixture.manage(
            candidate,
            ["toolchain", "status", "--output-format", "json"],
            success = false,
            diagnostic = "status --install",
        );
        root.harness.require(
            fixture.requests() == ["/nightly.json"],
            "plain status retried unavailable channel acquisition",
        );
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
            let output = fixture.manage(candidate, ["toolchain", "status", "--output-format", "json"].concat(flags));
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
                !output.stderr.to_string().includes("could not ensure"),
                "valid local status emitted a managed-toolchain installation warning",
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
        let args = ["toolchain", "status", "--output-format", "json"];
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
                && fixture.canonicalize(user.source)
                    == fixture.canonicalize(`${fixture.root}/${USER_CONFIG_FILE}`),
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
            ["toolchain", "resolve", "--output-format", "json"],
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
