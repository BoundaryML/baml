// Configuration validation, preservation, and diagnostics.

testset "configuration validation" {
    let candidate = Candidate.from_environment();

    test "malformed unused update setting does not block launch" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            USER_CONFIG_FILE,
            `
                [default]
                selector = "${BASE_VERSION}"
                [update]
                auto_check = []\n
            `,
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "malformed unselected channel does not block launch" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = "${BASE_VERSION}"
                resolved_at = "x"
                manifest_path = "x"
                [channels.nightly]
                active_version = []\n
            `,
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "malformed unused channel metadata does not block launch" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = "${BASE_VERSION}"
                resolved_at = []
                manifest_path = 42\n
            `,
        );
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "config writes preserve unknown fields" with failure_intended("management exit 1: error: unexpected argument '--global' found") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.write(
            USER_CONFIG_FILE,
            `
                ${config_before}
                [custom]
                value = 42
                date = 2026-01-01\n
            `,
        );
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                ${fixture.read(CHANNEL_STATE_FILE)}
                [custom]
                value = 43
                date = 2026-01-01\n
            `,
        );
        fixture.manage(candidate, ["toolchain", "use", "--global", "nightly"]);
        require(
            fixture.read(USER_CONFIG_FILE).includes("value = 42")
                && fixture.read(CHANNEL_STATE_FILE).includes("value = 43"),
            "unknown fields lost",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    test "failed state commit preserves channel mapping" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);
        fixture.absent(NEXT_VERSION);
        // Prime metadata before denying the final state commit. Unique
        // temporary filenames must not be part of this failure fixture.
        fixture.manage(candidate, ["toolchain", "status"]);
        let blocked = if (fixture.windows) {
            CHANNEL_STATE_FILE
        } else {
            "baml"
        };
        fixture.chmod(
            blocked,
            if (fixture.windows) {
                "444"
            } else {
                "555"
            },
        );
        defer {
            fixture.chmod(
                blocked,
                if (fixture.windows) {
                    "644"
                } else {
                    "755"
                },
            );
        }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.run(candidate, ["toolchain", "update"]);
        fixture.chmod(
            blocked,
            if (fixture.windows) {
                "644"
            } else {
                "755"
            },
        );
        require(!output.ok(), "state commit unexpectedly succeeded");
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "update failed before installation",
        );
        require(fixture.read(CHANNEL_STATE_FILE) == state, "failed commit changed channel");
        fixture.check(candidate, BASE_VERSION);
    }

    test "invalid environment version is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "invalid prerelease version is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "1.2.3-01";
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "overflowing version is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "18446744073709551616.0.0";
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "traversal in project version is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("../../escape");
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "traversal in recorded channel version is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = "../../escape"
                resolved_at = "x"
                manifest_path = "x"\n
            `,
        );
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "inconsistent exact version manifest is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.write(
            `server/version/${BASE_VERSION}.json`,
            fixture.read(`server/version/${NEXT_VERSION}.json`),
        );
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "inconsistent manifest activated",
        );
    }

    test "inconsistent channel manifest is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config("nightly");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.write("server/nightly.json", fixture.read("server/canary.json"));
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "inconsistent manifest activated",
        );
    }

    test "malformed global selector is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            USER_CONFIG_FILE,
            `
                [default]
                selector = []\n
            `,
        );
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    }

    test "malformed active channel is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = []\n
            `,
        );
        fixture.failed_check(candidate);
        require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    }

    test "global use validates complete config before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = `
            [default]
            selector = "${BASE_VERSION}"
            [update]
            auto_check = []\n
        `;
        let args = ["toolchain", "use", "--local", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "config.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use validates package before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [package]
            name = 42\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use validates dependencies before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [dependencies]
            helper = { path = 42 }\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use validates scripts before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [scripts]
            dev = 42\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use validates generator before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [generator.client]
            output_type = 42\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use validates test profile before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [test.profiles.ci]
            args = 42\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use rejects date in string field before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [generator.client]
            output_type = 2026-01-01\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use cannot overwrite invalid selector" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = `
            [toolchain]
            version = []\n
        `;
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "global use cannot overwrite invalid selector" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = `
            [default]
            selector = []\n
        `;
        let args = ["toolchain", "use", "--local", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "config.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "update validates all channels before installing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = CHANNEL_STATE_FILE;
        let content = `
            [channels.canary]
            active_version = "${BASE_VERSION}"
            resolved_at = "x"
            manifest_path = "x"
            [channels.nightly]
            active_version = []\n
        `;
        let args = ["toolchain", "update"];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "state.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "automatic install validates all channels before writing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = CHANNEL_STATE_FILE;
        let content = `
            [channels.canary]
            active_version = "${BASE_VERSION}"
            resolved_at = "x"
            manifest_path = "x"
            [channels.nightly]
            active_version = []\n
        `;
        let args = ["check"];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "state.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "global use rejects config syntax before installing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = "[invalid";
        let args = ["toolchain", "use", "--local", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "config.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "update rejects state syntax before installing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = CHANNEL_STATE_FILE;
        let content = "[invalid";
        let args = ["toolchain", "update"];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "state.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    test "project use rejects project syntax before installing" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = "[invalid";
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        fixture.manage(candidate, args, success = false, diagnostic = "baml.toml");
        require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "project option requires value" with failure_intended("invalid project flags caused side effects") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["--project"];
        let result = fixture.run(candidate, args);
        require(!result.ok(), "invalid project flags succeeded");
        require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "project option requires existing project" with failure_intended("invalid project flags caused side effects") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["--project", "../absent", "check"];
        let result = fixture.run(candidate, args);
        require(!result.ok(), "invalid project flags succeeded");
        require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "duplicate project option is rejected" with failure_intended("invalid project flags caused side effects") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["--project", ".", "--project", ".", "check"];
        let result = fixture.run(candidate, args);
        require(!result.ok(), "invalid project flags succeeded");
        require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "errors are human readable and keep stdout clean" with failure_intended("management error was not human-readable") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let output = fixture.manage(candidate, ["toolchain", "use", "./missing/baml-cli"], success = false);
        require(output.stderr.to_string().starts_with("error:"), "management error was not human-readable");
        require(output.stdout.to_string() == "", "management error polluted stdout");
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        let malformed = fixture.manage(candidate, ["toolchain", "status"], success = false);
        let diagnostic = malformed.stderr.to_string();
        require(
            diagnostic.includes("error:") && diagnostic.includes("baml.toml"),
            "parse error omitted context",
        );
        require(!diagnostic.includes("{\"message\":"), "parse error serialized an error object");
    }
}
