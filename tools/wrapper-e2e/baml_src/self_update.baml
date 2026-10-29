// Self-update operates on fixture copies of candidate executables.

testset "wrapper self update" {
    let candidate = Candidate.from_environment();

    if (candidate.supports([Capability.NoSelfUpdate])) {
        test "disabled self update reports package manager guidance" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.manage(
                candidate,
                ["self-update"],
                success = false,
                diagnostic = "package manager",
            );
            require(fixture.requests().length() == 0, "disabled self-update downloaded");
        }
    }

    if (candidate.supports([Capability.SelfUpdate])) {
        test "replaces wrapper copy" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            let config_before = fixture.read(USER_CONFIG_FILE);
            let project_before = fixture.read(PROJECT_MANIFEST_FILE);

            require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
            fixture.copy(candidate.command[0], `wrapper${fixture.suffix}`);
            let copy = Candidate {
                label: candidate.label,
                command: [`${fixture.root}/wrapper${fixture.suffix}`],
                self_update: true,
            };
            fixture.wrapper_archive(mode = "valid");
            let state = fixture.read(CHANNEL_STATE_FILE);
            fixture.manage(copy, ["self-update"], success = true);
            fixture.behavior(REPLACEMENT_WRAPPER_IDENTITY, "replacement\n");
            let attempts = 0;
            while (attempts < 100) {
                let output = fixture.run(copy, ["check"]);
                if (output.stdout.to_string() == "replacement\n") {
                    break;
                }
                attempts += 1;
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
            }
            require(
                fixture.calls().some((call) -> bool {
                    call.identity == REPLACEMENT_WRAPPER_IDENTITY
                }),
                "replacement wrapper did not take effect",
            );
            require(
                fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(USER_CONFIG_FILE) == config_before
                    && fixture.read(PROJECT_MANIFEST_FILE) == project_before,
                "self-update changed toolchain selection",
            );
        }

        test "failed self update download preserves wrapper" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            let config_before = fixture.read(USER_CONFIG_FILE);
            let project_before = fixture.read(PROJECT_MANIFEST_FILE);

            require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
            fixture.copy(candidate.command[0], `wrapper${fixture.suffix}`);
            let copy = Candidate {
                label: candidate.label,
                command: [`${fixture.root}/wrapper${fixture.suffix}`],
                self_update: true,
            };
            fixture.wrapper_archive(mode = "valid");
            fixture.write("server/wrapper.json.status", "500");
            let state = fixture.read(CHANNEL_STATE_FILE);
            fixture.manage(copy, ["self-update"], success = false);
            fixture.check(copy, BASE_VERSION);
            require(
                fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(USER_CONFIG_FILE) == config_before
                    && fixture.read(PROJECT_MANIFEST_FILE) == project_before,
                "self-update changed toolchain selection",
            );
        }

        test "failed self update checksum preserves wrapper" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            let config_before = fixture.read(USER_CONFIG_FILE);
            let project_before = fixture.read(PROJECT_MANIFEST_FILE);

            require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
            fixture.copy(candidate.command[0], `wrapper${fixture.suffix}`);
            let copy = Candidate {
                label: candidate.label,
                command: [`${fixture.root}/wrapper${fixture.suffix}`],
                self_update: true,
            };
            fixture.wrapper_archive(mode = "checksum");
            let state = fixture.read(CHANNEL_STATE_FILE);
            let result = fixture.manage(copy, ["self-update"], success = false);
            require(
                fixture.requests().includes(
                    if (fixture.windows) {
                        "/wrapper.zip"
                    } else {
                        "/wrapper.tar.gz"
                    },
                ),
                "checksum failure happened before archive download",
            );
            require(
                result.stderr.to_string().includes("sha256 mismatch"),
                `unexpected checksum diagnostic: ${result.stderr.to_string()}`,
            );
            fixture.check(copy, BASE_VERSION);
            require(
                fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(USER_CONFIG_FILE) == config_before
                    && fixture.read(PROJECT_MANIFEST_FILE) == project_before,
                "self-update changed toolchain selection",
            );
        }

        if (candidate.supports([Capability.PosixPermissions])) {
            test "self update restores executable permissions" {
                let fixture = Fixture.start();
                defer { fixture.close(); }
                require(
                    candidate.command.length() == 1,
                    "self-update requires a copied executable candidate",
                );
                fixture.copy(candidate.command[0], "wrapper");
                let copy = Candidate {
                    label: candidate.label,
                    command: [`${fixture.root}/wrapper`],
                    self_update: true,
                };
                fixture.wrapper_archive(mode = "not-executable");
                fixture.manage(copy, ["self-update"]);
                fixture.behavior(REPLACEMENT_WRAPPER_IDENTITY, "replacement\n");
                let output = fixture.run(copy, ["check"]);
                require(
                    output.ok() && output.stdout.to_string() == "replacement\n",
                    "replacement is not executable",
                );
                require(
                    fixture.calls()[0].identity == REPLACEMENT_WRAPPER_IDENTITY,
                    "replacement identity changed",
                );
            }

            test "failed self update staging preserves wrapper" {
                let fixture = Fixture.start();
                defer { fixture.close(); }
                let config_before = fixture.read(USER_CONFIG_FILE);
                let project_before = fixture.read(PROJECT_MANIFEST_FILE);

                require(
                    candidate.command.length() == 1,
                    "self-update requires a copied executable candidate",
                );
                fixture.copy(candidate.command[0], `wrapper${fixture.suffix}`);
                let copy = Candidate {
                    label: candidate.label,
                    command: [`${fixture.root}/wrapper${fixture.suffix}`],
                    self_update: true,
                };
                fixture.wrapper_archive(mode = "valid");
                // Block staging in the executable's parent without depending on
                // implementation-specific temporary filenames.
                fixture.chmod(".", "555");
                defer { fixture.chmod(".", "755"); }
                let state = fixture.read(CHANNEL_STATE_FILE);
                fixture.manage(copy, ["self-update"], success = false);
                require(
                    fixture.requests().includes("/wrapper.tar.gz"),
                    "staging failure happened before archive download",
                );
                fixture.chmod(".", "755");
                fixture.check(copy, BASE_VERSION);
                require(
                    fixture.read(CHANNEL_STATE_FILE) == state
                        && fixture.read(USER_CONFIG_FILE) == config_before
                        && fixture.read(PROJECT_MANIFEST_FILE) == project_before,
                    "self-update changed toolchain selection",
                );
            }
        }
    }

    if (candidate.supports([Capability.Windows, Capability.SelfUpdate])) {
        test "windows self update replaces wrapper copy" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            let config_before = fixture.read(USER_CONFIG_FILE);
            let project_before = fixture.read(PROJECT_MANIFEST_FILE);

            require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
            fixture.copy(candidate.command[0], `wrapper${fixture.suffix}`);
            let copy = Candidate {
                label: candidate.label,
                command: [`${fixture.root}/wrapper${fixture.suffix}`],
                self_update: true,
            };
            fixture.wrapper_archive(mode = "valid");
            let state = fixture.read(CHANNEL_STATE_FILE);
            fixture.manage(copy, ["self-update"], success = true);
            fixture.behavior(REPLACEMENT_WRAPPER_IDENTITY, "replacement\n");
            let attempts = 0;
            while (attempts < 100) {
                let output = fixture.run(copy, ["check"]);
                if (output.stdout.to_string() == "replacement\n") {
                    break;
                }
                attempts += 1;
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
            }
            require(
                fixture.calls().some((call) -> bool {
                    call.identity == REPLACEMENT_WRAPPER_IDENTITY
                }),
                "replacement wrapper did not take effect",
            );
            require(
                fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(USER_CONFIG_FILE) == config_before
                    && fixture.read(PROJECT_MANIFEST_FILE) == project_before,
                "self-update changed toolchain selection",
            );
        }
    }
}
