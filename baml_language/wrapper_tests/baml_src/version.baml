// Version reporting is best effort until the CLI starts; CLI exit codes are preserved.

testset "version reporting" {
    let candidate = root.harness.Candidate.from_environment();

    test "version reports wrapper and selected toolchain" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = ["--version"]);
        let output = fixture.run(candidate, ["--version"]);
        root.harness.require(output.ok() && fixture.calls().length() == 1, "version did not invoke the selected CLI");
        root.harness.require(
            output.stdout.to_string().includes("wrapper")
                && output.stdout.to_string().includes(BASE_VERSION),
            "version does not identify wrapper and toolchain",
        );
        if let expected: string = candidate.version {
            root.harness.require(
                output.stdout.to_string().includes(`baml wrapper ${expected}`),
                "wrapper version differs from build version",
            );
        }
    }

    test "version installs and invokes selected toolchain" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.behavior(BASE_VERSION, "CLI version\n", code = 0, args = args);
        let result = fixture.run(candidate, args);
        root.harness.require(result.exit_code == 0, "version hid CLI failure");
        root.harness.require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        root.harness.require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == BASE_VERSION
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version propagates cli exit code" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.behavior(BASE_VERSION, "CLI version\n", code = 23, args = args);
        let result = fixture.run(candidate, args);
        root.harness.require(result.exit_code == 23, "version hid CLI failure");
        root.harness.require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        root.harness.require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == BASE_VERSION
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version invokes selected local cli" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.project("./build/baml-cli", key = "path");
        fixture.behavior(PROJECT_TOOLCHAIN_IDENTITY, "CLI version\n", code = 0, args = args);
        let result = fixture.run(candidate, args);
        root.harness.require(result.exit_code == 0, "version hid CLI failure");
        root.harness.require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        root.harness.require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == PROJECT_TOOLCHAIN_IDENTITY
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version reports wrapper when selected local cli is missing" {
        for (let flag in ["--version", "-V"]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.project("./missing/baml-cli", key = "path");
            let result = fixture.run(candidate, [flag]);
            root.harness.require(
                result.ok()
                    && result.stdout.to_string().includes("baml wrapper")
                    && result.stderr.to_string().includes(
                        "warning: could not report selected baml-cli version",
                    )
                    && !fixture.calls()
                    && !fixture.requests(),
                "version did not fail open for missing local CLI",
            );
        }
    }

    test "version fails open after one failed acquisition" {
        for (let flag in ["--version", "-V"]) {
            for (let failure in ["manifest", "artifact"]) {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                fixture.absent(BASE_VERSION);
                let extension = if (fixture.windows) {
                    ".zip"
                } else {
                    ".tar.gz"
                };
                let path = if (failure == "manifest") {
                    `/version/${BASE_VERSION}.json`
                } else {
                    `/${BASE_VERSION}${extension}`
                };
                // A missing artifact avoids the Rust fetcher's existing 5xx retries,
                // so this assertion measures version preparation attempts.
                fixture.write(`server${path}.status`, if (failure == "manifest") { "500" } else { "404" });
                let config = fixture.read(USER_CONFIG_FILE);
                let project = fixture.read(PROJECT_MANIFEST_FILE);
                let state = fixture.read(CHANNEL_STATE_FILE);
                let result = fixture.run(candidate, [flag]);
                root.harness.require(
                    result.ok()
                        && result.stdout.to_string().includes("baml wrapper")
                        && result.stderr.to_string().includes(
                            "warning: could not report selected baml-cli version",
                        )
                        && !fixture.calls(),
                    "version did not fail open after acquisition failure",
                );
                root.harness.require(
                    fixture.requests()
                        == if (failure == "manifest") {
                            [path]
                        } else {
                            [`/version/${BASE_VERSION}.json`, path]
                        },
                    "version retried failed acquisition",
                );
                root.harness.require(
                    fixture.read(USER_CONFIG_FILE) == config
                        && fixture.read(PROJECT_MANIFEST_FILE) == project
                        && fixture.read(CHANNEL_STATE_FILE) == state
                        && !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
                    "failed version acquisition changed selection or published an installation",
                );
            }
        }
    }

    test "version fails open for invalid selection or damaged installation" {
        for (let failure in ["selector", "installation"]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            if (failure == "selector") {
                fixture.env["BAML_TOOLCHAIN"] = "invalid-selector";
            } else {
                fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
            }
            let result = fixture.run(candidate, ["--version"]);
            root.harness.require(
                result.ok()
                    && result.stdout.to_string().includes("baml wrapper")
                    && result.stderr.to_string().includes(
                        "warning: could not report selected baml-cli version",
                    )
                    && !fixture.calls()
                    && !fixture.requests(),
                "version did not preserve wrapper reporting after preparation failed",
            );
        }
    }

    if (candidate.supports([root.harness.Capability.PosixPermissions])) {
        test "version fails open when local cli cannot start" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.project("./broken-cli", key = "path");
            fixture.write("project/broken-cli", "#!/missing-version-interpreter\n");
            fixture.chmod("project/broken-cli", "755");
            let result = fixture.run(candidate, ["--version"]);
            root.harness.require(
                result.ok()
                    && result.stdout.to_string().includes("baml wrapper")
                    && result.stderr.to_string().includes(
                        "warning: could not report selected baml-cli version",
                    )
                    && !fixture.calls()
                    && !fixture.requests(),
                "version did not fail open after launch failure",
            );
        }
    }
}
