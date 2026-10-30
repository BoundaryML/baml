// Configuration validation, preservation, and diagnostics.

testset "configuration validation" {
    let candidate = root.harness.Candidate.from_environment();

    if (candidate.supports([root.harness.Capability.PosixPermissions])) {
        test "project edits preserve restricted permissions" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.chmod(PROJECT_MANIFEST_FILE, "600");
            fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
            root.harness.require(fixture.permissions(PROJECT_MANIFEST_FILE) == 384, "project permissions widened");
            fixture.check(candidate, NEXT_VERSION);
        }

        test "global edits preserve restricted permissions" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.chmod(USER_CONFIG_FILE, "640");
            fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
            root.harness.require(fixture.permissions(USER_CONFIG_FILE) == 416, "config permissions changed");
            fixture.check(candidate, NEXT_VERSION);
        }

        test "channel updates preserve restricted state permissions" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.config("canary");
            fixture.channel("canary", NEXT_VERSION);
            fixture.chmod(CHANNEL_STATE_FILE, "600");
            fixture.manage(candidate, ["toolchain", "update"]);
            root.harness.require(fixture.permissions(CHANNEL_STATE_FILE) == 384, "state permissions widened");
            fixture.check(candidate, NEXT_VERSION);
        }
    }

    test "project edits made during downloads are preserved" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.absent(NEXT_VERSION);
        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(
            `server/${NEXT_VERSION}${extension}.edit`,
            baml.json.stringify(
                {
                    "path": PROJECT_MANIFEST_FILE,
                    "text": fixture.read(PROJECT_MANIFEST_FILE) + "\n[editor]\nvalue = 42\n",
                },
            ),
        );
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE).includes("value = 42"), "project edit was lost");
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/server/${NEXT_VERSION}${extension}.edit`),
            "archive edit hook did not run",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    test "global edits made during downloads are preserved" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.absent(NEXT_VERSION);
        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(
            `server/${NEXT_VERSION}${extension}.edit`,
            baml.json.stringify(
                {
                    "path": USER_CONFIG_FILE,
                    "text": fixture.read(USER_CONFIG_FILE) + "\n[editor]\nvalue = 43\n",
                },
            ),
        );
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        root.harness.require(fixture.read(USER_CONFIG_FILE).includes("value = 43"), "config edit was lost");
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/server/${NEXT_VERSION}${extension}.edit`),
            "archive edit hook did not run",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    test "malformed unused update setting does not block launch" {
        let fixture = root.harness.Fixture.start();
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
        let fixture = root.harness.Fixture.start();
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
        let fixture = root.harness.Fixture.start();
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

    test "config writes preserve unknown fields" {
        let fixture = root.harness.Fixture.start();
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
        root.harness.require(
            fixture.read(USER_CONFIG_FILE).includes("value = 42")
                && fixture.read(CHANNEL_STATE_FILE).includes("value = 43"),
            "unknown fields lost",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    test "failed state commit preserves channel mapping" {
        let fixture = root.harness.Fixture.start();
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
        root.harness.require(!output.ok(), "state commit unexpectedly succeeded");
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "update failed before installation",
        );
        root.harness.require(fixture.read(CHANNEL_STATE_FILE) == state, "failed commit changed channel");
        fixture.check(candidate, BASE_VERSION);
    }

    test "invalid environment version is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        fixture.failed_check(candidate);
        root.harness.require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "invalid prerelease version is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "1.2.3-01";
        fixture.failed_check(candidate);
        root.harness.require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "overflowing version is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "18446744073709551616.0.0";
        fixture.failed_check(candidate);
        root.harness.require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "traversal in project version is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("../../escape");
        fixture.failed_check(candidate);
        root.harness.require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "traversal in recorded channel version is rejected" {
        let fixture = root.harness.Fixture.start();
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
        root.harness.require(fixture.requests().length() == 0, "invalid managed version contacted server");
    }

    test "inconsistent exact version manifest is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.write(
            `server/version/${BASE_VERSION}.json`,
            fixture.read(`server/version/${NEXT_VERSION}.json`),
        );
        fixture.failed_check(candidate);
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "inconsistent manifest activated",
        );
    }

    test "inconsistent channel manifest is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config("nightly");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.write("server/nightly.json", fixture.read("server/canary.json"));
        fixture.failed_check(candidate);
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "inconsistent manifest activated",
        );
    }

    test "malformed global selector is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            USER_CONFIG_FILE,
            `
                [default]
                selector = []\n
            `,
        );
        fixture.failed_check(candidate);
        root.harness.require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    }

    test "malformed active channel is rejected" {
        let fixture = root.harness.Fixture.start();
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
        root.harness.require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    }

    test "global use validates complete config before writing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = `
            [default]
            selector = "${BASE_VERSION}"
            [update]
            auto_check = []\n
        `;
        let args = ["toolchain", "use", "--global", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("config.toml"),
            `missing diagnostic config.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use validates package before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use validates dependencies before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use validates scripts before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use validates generator before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use validates test profile before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use rejects date in string field before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use cannot overwrite invalid selector" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "global use cannot overwrite invalid selector" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = `
            [default]
            selector = []\n
        `;
        let args = ["toolchain", "use", "--global", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("config.toml"),
            `missing diagnostic config.toml: ${output.stderr.to_string()}`,
        );
    }

    test "update validates all channels before installing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("state.toml"),
            `missing diagnostic state.toml: ${output.stderr.to_string()}`,
        );
    }

    test "automatic install validates all channels before writing" {
        let fixture = root.harness.Fixture.start();
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
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("state.toml"),
            `missing diagnostic state.toml: ${output.stderr.to_string()}`,
        );
    }

    test "global use rejects config syntax before installing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let path = USER_CONFIG_FILE;
        let content = "[invalid";
        let args = ["toolchain", "use", "--global", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("config.toml"),
            `missing diagnostic config.toml: ${output.stderr.to_string()}`,
        );
    }

    test "update rejects state syntax before installing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let path = CHANNEL_STATE_FILE;
        let content = "[invalid";
        let args = ["toolchain", "update"];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);

        fixture.write(path, content);
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("state.toml"),
            `missing diagnostic state.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project use rejects project syntax before installing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let path = PROJECT_MANIFEST_FILE;
        let content = "[invalid";
        let args = ["toolchain", "use", NEXT_VERSION];

        fixture.absent(NEXT_VERSION);
        fixture.absent(BASE_VERSION);

        fixture.write(path, content);
        let output = fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(path) == content
                && fixture.requests().length() == 0
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "write did not preflight complete document",
        );
        // Observe all preflight effects before accepting the known diagnostic gap.
        root.harness.require(
            output.stderr.to_string().includes("baml.toml"),
            `missing diagnostic baml.toml: ${output.stderr.to_string()}`,
        );
    }

    test "project option requires value" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["--project"];
        let result = fixture.run(candidate, args);
        root.harness.require(!result.ok(), "invalid project flags succeeded");
        root.harness.require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    test "project option requires existing project" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["--project", "../absent", "check"];
        let result = fixture.run(candidate, args);
        root.harness.require(!result.ok(), "invalid project flags succeeded");
        root.harness.require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    test "duplicate project option is rejected" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["--project", ".", "--project", ".", "check"];
        let result = fixture.run(candidate, args);
        root.harness.require(!result.ok(), "invalid project flags succeeded");
        root.harness.require(
            fixture.calls().length() == 0 && fixture.requests().length() == 0,
            "invalid project flags caused side effects",
        );
    }

    test "errors are human readable and keep stdout clean" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let output = fixture.manage(candidate, ["toolchain", "use", "./missing/baml-cli"], success = false);
        root.harness.require(output.stderr.to_string().starts_with("error:"), "management error was not human-readable");
        root.harness.require(output.stdout.to_string() == "", "management error polluted stdout");
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        let malformed = fixture.manage(candidate, ["toolchain", "status"], success = false);
        let diagnostic = malformed.stderr.to_string();
        root.harness.require(
            diagnostic.includes("error:") && diagnostic.includes("baml.toml"),
            "parse error omitted context",
        );
        root.harness.require(!diagnostic.includes("{\"message\":"), "parse error serialized an error object");
    }
}
