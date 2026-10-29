// Automatic installation, explicit repair, and failure recovery.

testset "toolchain installation" {
    let candidate = Candidate.from_environment();

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "absent global version is installed" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "absent project version is installed" with failure_intended("but it isn't installed") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config(NEXT_VERSION);
        fixture.project(BASE_VERSION);
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "absent environment version is installed" with failure_intended("exit 1: BAML toolchain 0.12.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        fixture.absent(NEXT_VERSION);

        fixture.check(candidate, NEXT_VERSION);
        require(
            fixture.read(`baml/toolchains/${NEXT_VERSION}/VERSION`).trim() == NEXT_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${NEXT_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "recorded channel installs without advancing" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
        require(!requests.includes("/canary.json"), "recorded channel silently advanced");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "unrecorded channel is resolved and installed" with failure_intended("exit 1: error: no BAML toolchain is installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes("canary.json")
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
        require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "empty home bootstraps default canary" with failure_intended("exit 1: error: no BAML toolchain is installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.remove("baml");
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes("canary.json")
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
        require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
        fixture.behavior(
            BASE_VERSION,
            "changed",
            code = 7,
            args = ["format", "--flag", "a b"],
            stderr = "stderr",
        );
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["format", "--flag", "a b"],
            output = "changed",
            code = 7,
        );
        require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
    }

    test "installed toolchain launches offline" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        fixture.check(candidate, BASE_VERSION);
        require(fixture.requests().length() == 0, "installed toolchain contacted server");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "installed toolchain is reused with new arguments" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
        fixture.behavior(
            BASE_VERSION,
            "changed",
            code = 7,
            args = ["format", "--flag", "a b"],
            stderr = "stderr",
        );
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["format", "--flag", "a b"],
            output = "changed",
            code = 7,
        );
        require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "missing managed binary requires explicit repair" with failure_intended("missing --force: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.remove(`baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "missing version file requires explicit repair" with failure_intended("missing --force: failed to read toolchain VERSION") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "mismatched version file requires explicit repair" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(`baml/toolchains/${BASE_VERSION}/VERSION`, NEXT_VERSION);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
    }

    if (candidate.supports([Capability.Symlinks])) {
        // Failure intended: the current Rust wrapper does not yet satisfy this contract.
        // Once fixed, remove this marker and assert expected behavior.
        test "dangling installation symlink requires explicit repair" with failure_intended("missing --force: BAML toolchain 0.11.0 is not installed.") {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.absent(BASE_VERSION);
            fixture.symlink(`baml/toolchains/${BASE_VERSION}`, "missing-installation");
            fixture.failed_check(candidate, diagnostic = "--force");
            require(fixture.requests().length() == 0, "damaged installation was repaired");
            fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
            fixture.check(candidate, BASE_VERSION);
        }
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "installation file requires explicit repair" with failure_intended("missing --force: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.copy(baml.env.get_or_panic("WRAPPER_STUB"), `baml/toolchains/${BASE_VERSION}`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "explicit install preserves global selection" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.config(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION]);
        fixture.check(candidate, NEXT_VERSION);
        require(fixture.read(USER_CONFIG_FILE).includes(NEXT_VERSION), "install changed default");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "forced install replaces usable toolchain" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.release(BASE_VERSION, "valid", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"], success = true);
        fixture.check(candidate, REINSTALLED_TOOLCHAIN_IDENTITY);
    }

    test "failed forced install preserves usable toolchain" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.release(BASE_VERSION, "checksum", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(
            candidate,
            ["toolchain", "install", BASE_VERSION, "--force"],
            success = false,
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "unreachable manifest server rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "manifest timeout rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.delay`, "31");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "missing manifest rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "manifest server error rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "invalid manifest json rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json`, "not json");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "unsupported manifest schema rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "schema");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "missing host artifact rejects install" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "target");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "missing archive preserves other toolchains" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(`server/${BASE_VERSION}${extension}.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
    }

    test "interrupted archive preserves other toolchains" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(`server/${BASE_VERSION}${extension}.disconnect`, "true");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
    }

    test "checksum mismatch preserves other toolchains" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "checksum");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
    }

    test "archive without cli is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "missing-cli");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "archive with invalid layout is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "layout");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "archive path traversal is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "traversal");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    test "archive link entry is rejected" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "link");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
    }

    if (candidate.supports([Capability.PosixPermissions])) {
        test "unwritable install destination preserves existing selection" {
            let fixture = Fixture.start();
            defer { fixture.close(); }
            fixture.absent(BASE_VERSION);
            fixture.chmod("baml/toolchains", "555");
            defer { fixture.chmod("baml/toolchains", "755"); }
            fixture.failed_check(candidate);
            fixture.chmod("baml/toolchains", "755");
            fixture.config(NEXT_VERSION);
            fixture.check(candidate, NEXT_VERSION);
            require(
                !baml.fs.exists(`${fixture.root}/baml/escaped`)
                    && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
                "archive escaped staging",
            );
        }
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "failed install can be retried after server recovery" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.remove(`server/version/${BASE_VERSION}.json.status`);
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "implicit install preserves cli json stdout" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);

        fixture.behavior(BASE_VERSION, "{\"ok\":true}\n");
        fixture.check(candidate, BASE_VERSION, args = ["check"], output = "{\"ok\":true}\n");
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "stale install lock does not block install" with failure_intended("exit 1: BAML toolchain 0.11.0 is not installed.") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.write(`baml/toolchains/.${BASE_VERSION}.lock`, "stale owner metadata\n");

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "bare install uses project selection" with failure_intended("management exit 1: usage: baml toolchain install <canary|nightly|version>") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "bare install uses environment selection" with failure_intended("management exit 1: usage: baml toolchain install <canary|nightly|version>") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = LATEST_VERSION;
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(LATEST_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, LATEST_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "bare install preserves recorded channel version" with failure_intended("management exit 1: usage: baml toolchain install <canary|nightly|version>") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "bare forced install replaces recorded channel version" with failure_intended("/version/--force.json") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install", "--force"];
        fixture.release(NEXT_VERSION, "valid", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, REINSTALLED_TOOLCHAIN_IDENTITY);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "concurrent installs publish reusable manifest cache" with failure_intended("URL must use HTTPS") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let value = BASE_VERSION;
        fixture.absent(value);
        let path = `server/version/${value}.json`;
        fixture.write(
            path,
            fixture.read(path).replace_all("2026-01-01T00:00:00Z", "x".repeat(STRESS_PAYLOAD_BYTES)),
        );
        let first = spawn { fixture.run(candidate, ["toolchain", "install", value]) };
        let second = spawn { fixture.run(candidate, ["toolchain", "install", value]) };
        let third = spawn { fixture.run(candidate, ["toolchain", "install", value]) };
        let fourth = spawn { fixture.run(candidate, ["toolchain", "install", value]) };
        // Observe every child before asserting, including when an install fails.
        let outputs = [await first, await second, await third, await fourth];
        for (let output in outputs) {
            require(output.ok(), `concurrent install failed: ${output.stderr.to_string()}`);
        }
        let requests = fixture.requests().length();
        fixture.write(`${path}.status`, "500");
        fixture.absent(value);
        fixture.manage(candidate, ["toolchain", "install", value]);
        require(
            fixture.requests().length() == requests + 1,
            "cached install unexpectedly fetched its manifest",
        );
        fixture.check(candidate, value);
    }
}
