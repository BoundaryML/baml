// Project/global selection, updates, removal, and management preflight.

testset "toolchain management" {
    let candidate = root.harness.Candidate.from_environment();

    test "use updates project and preserves global default" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config(NEXT_VERSION);
        let before = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "use", BASE_VERSION]);
        root.harness.require(fixture.read(USER_CONFIG_FILE) == before, "project use changed user config");
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE).includes(BASE_VERSION), "project not selected");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use nightly installs and records channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "nightly"]);
        root.harness.require(fixture.read(USER_CONFIG_FILE) == config_before, "channel use changed global default");
        root.harness.require(fixture.channel_version("nightly") == NEXT_VERSION, "channel not recorded");
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE).includes("nightly"), "channel selector not written");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "global use resolves path from cwd" with root.harness.failure_intended("management exit 1: error: unexpected argument '--global' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(
            candidate,
            ["toolchain", "use", "--global", `./build/baml-cli${fixture.suffix}`],
        );
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE) == before, "--global changed project");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "global use preserves project selection" with root.harness.failure_intended("management exit 1: error: unexpected argument '--global' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE) == before, "--global changed project");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use installs absent project version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config(NEXT_VERSION);
        fixture.absent(BASE_VERSION);
        let before = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "use", BASE_VERSION]);
        root.harness.require(fixture.read(USER_CONFIG_FILE) == before, "project use changed user config");
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE).includes(BASE_VERSION), "project not selected");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use canary installs and records channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "canary"]);
        root.harness.require(fixture.read(USER_CONFIG_FILE) == config_before, "channel use changed global default");
        root.harness.require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE).includes("canary"), "channel selector not written");
        fixture.check(candidate, BASE_VERSION);
    }

    test "project use resolves path from project root" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(candidate, ["toolchain", "use", `./build/baml-cli${fixture.suffix}`]);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        fixture.cwd = `${fixture.root}/project`;
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
    }

    test "use replaces version and preserves comments" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                # retain me
                [package]
                name = "fixture"
                [toolchain]
                version = "${BASE_VERSION}" # selector comment
                [custom]
                value = 42
                date = 2026-01-01\n
            `,
        );
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
        let updated = fixture.read(PROJECT_MANIFEST_FILE);
        root.harness.require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        root.harness.require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use replaces channel and preserves comments" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                # retain me
                [package]
                name = "fixture"
                [toolchain]
                channel = "canary" # selector comment
                [custom]
                value = 42
                date = 2026-01-01\n
            `,
        );
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
        let updated = fixture.read(PROJECT_MANIFEST_FILE);
        root.harness.require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        root.harness.require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use replaces path and preserves comments" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                # retain me
                [package]
                name = "fixture"
                [toolchain]
                path = "./build/baml-cli" # selector comment
                [custom]
                value = 42
                date = 2026-01-01\n
            `,
        );
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
        let updated = fixture.read(PROJECT_MANIFEST_FILE);
        root.harness.require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        root.harness.require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "project use outside project requires global flag" with root.harness.failure_intended("missing diagnostic --global:") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(LATEST_VERSION);
        fixture.manage(
            candidate,
            ["toolchain", "use", LATEST_VERSION],
            success = false,
            diagnostic = "--global",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config_before
                && !baml.fs.exists(`${fixture.root}/outside/baml.toml`)
                && fixture.requests().length() == 0,
            "use outside project mutated state",
        );
    }

    test "failed project use download preserves selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push(NEXT_VERSION);
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "failed global use download preserves selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("--global");
        args.push(NEXT_VERSION);
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "failed project use path preserves selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("./missing/baml-cli");
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "failed global use path preserves selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("--global");
        args.push("./missing/baml-cli");
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "update advances global channel" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.channel("canary", NEXT_VERSION);
        fixture.absent(NEXT_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("canary") == NEXT_VERSION, "wrong shared channel updated");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "failed update preserves usable toolchain" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.write(`server/canary.json.status`, "500");
        fixture.manage(candidate, ["toolchain", "update"], success = false);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "update does not advance environment version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = BASE_VERSION;
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update does not advance environment path" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.env["BAML_TOOLCHAIN"] = selected;
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update does not advance project version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION, key = "version");
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update does not advance project path" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.project(selected, key = "path");
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update does not advance global version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config(BASE_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update does not advance global path" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.config(selected);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        root.harness.require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
    }

    test "update advances project channel only" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        root.harness.require(fixture.channel_version("canary") == BASE_VERSION, "unselected channel changed");
    }

    test "update advances environment channel only" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.env["BAML_TOOLCHAIN"] = "nightly";
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        root.harness.require(fixture.channel_version("canary") == BASE_VERSION, "unselected channel changed");
    }

    test "uninstall removes unused version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", NEXT_VERSION], success = true);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "uninstall rejects absent version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", "9.9.9"], success = false);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "uninstall rejects local path" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.manage(candidate, ["toolchain", "uninstall", selected], success = false);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "uninstall rejects channel name" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", "canary"], success = false);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "uninstalled selected version is reinstalled on launch" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION], success = true);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "uninstalled channel version is reinstalled without advancing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION], success = true);
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "list installs absent active toolchain" with root.harness.failure_intended("list did not install the absent active toolchain") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.remove("baml/toolchains");
        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        root.harness.require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        root.harness.require(listing.includes(`  ${BASE_VERSION}`), "list did not install the absent active toolchain");
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
    }

    test "list reports installed toolchains" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        root.harness.require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        root.harness.require(
            listing.includes(NEXT_VERSION) && listing.includes(LATEST_VERSION),
            "list omitted installed versions",
        );
        root.harness.require(
            fixture.requests().length() == 0
                && fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
    }

    test "list reports environment override" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        root.harness.require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        root.harness.require(
            listing.includes(NEXT_VERSION) && listing.includes(LATEST_VERSION),
            "list omitted installed versions",
        );
        root.harness.require(listing.includes(`active selector: ${NEXT_VERSION}`), "list omitted the active override");
        root.harness.require(
            fixture.requests().length() == 0
                && fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
    }

    test "status reports available update without installing" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.absent(NEXT_VERSION);
        fixture.channel("canary", NEXT_VERSION);
        let before = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "status"],
            success = true,
            diagnostic = "update available",
        );
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == before
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "status installed or changed state",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "failed status preserves selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.absent(NEXT_VERSION);
        fixture.channel("canary", NEXT_VERSION);
        fixture.write("server/canary.json.status", "500");
        let before = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "status"], success = false, diagnostic = "");
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == before
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "status installed or changed state",
        );
        fixture.check(candidate, BASE_VERSION);
    }

    test "environment channel update preserves other channels" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");
        fixture.config("nightly");
        fixture.project("nightly", key = "channel");
        fixture.env["BAML_TOOLCHAIN"] = "canary";
        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        root.harness.require(fixture.channel_version("nightly") == NEXT_VERSION, "unselected channel changed");
    }

    test "update uses canary when no default is configured" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = false\n
            `,
        );
        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
    }

    test "update ignores parent of unpinned closest project" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.project("nightly", key = "channel");
        fixture.write(
            "project/nested/deep/baml.toml",
            `
                [package]
                name = "nearest"\n
            `,
        );
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
    }

    test "channel update is shared across projects" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.project("nightly", key = "channel");
        fixture.write(
            "outside/baml.toml",
            `
                [toolchain]
                channel = "nightly"\n
            `,
        );
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        root.harness.require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, LATEST_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "global use bootstraps explicit baml home" with root.harness.failure_intended("management exit 1: error: unexpected argument '--global' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        root.harness.require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--global created manifest");
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "global use bootstraps default baml home" with root.harness.failure_intended("management exit 1: error: unexpected argument '--global' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(NEXT_VERSION);
        fixture.env["BAML_HOME"] = "";
        fixture.remove("baml");
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        root.harness.require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--global created manifest");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "use updates unpinned closest project only" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.write(
            "project/nested/deep/baml.toml",
            `
                [package]
                name = "nearest"\n
            `,
        );
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION]);
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "use changed parent/global",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "use reports previous and new versions" with root.harness.failure_intended("selection output omitted concrete previous/new version") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let selector = NEXT_VERSION;
        let output = fixture.manage(candidate, ["toolchain", "use", selector]).stdout.to_string();
        root.harness.require(
            output.includes("previous:")
                && output.includes(BASE_VERSION)
                && output.includes(NEXT_VERSION),
            "selection output omitted concrete previous/new version",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "channel use reports previous and new versions" with root.harness.failure_intended("selection output omitted concrete previous/new version") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        let output = fixture.manage(candidate, ["toolchain", "use", "nightly"]).stdout.to_string();
        root.harness.require(
            output.includes("previous:")
                && output.includes(BASE_VERSION)
                && output.includes(NEXT_VERSION),
            "selection output omitted concrete previous/new version",
        );
        fixture.check(candidate, NEXT_VERSION);
    }

    test "update command is delegated to selected cli" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.behavior(BASE_VERSION, "selected CLI guidance\n", args = ["update"]);
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["update"],
            output = "selected CLI guidance\n",
        );
        root.harness.require(fixture.requests().length() == 0, "installed delegated update used network");
    }

    test "upgrade command is delegated to selected cli" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.behavior(BASE_VERSION, "selected CLI guidance\n", args = ["upgrade"]);
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["upgrade"],
            output = "selected CLI guidance\n",
        );
        root.harness.require(fixture.requests().length() == 0, "installed delegated update used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "list ensures active toolchain" with root.harness.failure_intended("list did not ensure active CLI") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "list did not ensure active CLI",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "preflight installs recorded channel version" with root.harness.failure_intended("channel preflight did not install expected version") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.project("canary", key = "channel");
        fixture.channel("canary", NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "channel preflight did not install expected version",
        );
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE).includes(`active_version = "${BASE_VERSION}"`),
            "incorrect channel mapping",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "preflight resolves unrecorded channel" with root.harness.failure_intended("state.toml") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.project("canary", key = "channel");
        fixture.channel("canary", NEXT_VERSION);
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "channel preflight did not install expected version",
        );
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE).includes(`active_version = "${NEXT_VERSION}"`),
            "incorrect channel mapping",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "continues after failed best effort install" with root.harness.failure_intended("installation failure was silent") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.write(`server/${BASE_VERSION}.tar.gz.status`, "500");
        fixture.write(`server/${BASE_VERSION}.zip.status`, "500");
        let result = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(result.stderr.to_string().includes("could not ensure"), "installation failure was silent");
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    test "explicit force install repairs damaged active toolchain" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        root.harness.require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "preflight blocked explicit repair",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "uninstall leaves active toolchain removed" with root.harness.failure_intended("management exit 1: BAML toolchain 0.11.0 is not installed") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION]);
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall reinstalled its target",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "explicit install also ensures active toolchain" with root.harness.failure_intended("explicit install target replaced active preflight") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "install", NEXT_VERSION]);
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`)
                && baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "explicit install target replaced active preflight",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "status install requires successful preflight" with root.harness.failure_intended("management exit 1: error: unexpected argument '--json' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.write(`server/${BASE_VERSION}.tar.gz.status`, "500");
        fixture.write(`server/${BASE_VERSION}.zip.status`, "500");
        let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
        root.harness.require(
            !baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string()).installed,
            "plain status misreported failed installation",
        );
        fixture.manage(candidate, ["toolchain", "status", "--install", "--json"], success = false);
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "status rejects damaged installation without repairing" with root.harness.failure_intended("missing diagnostic --force:") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
        fixture.manage(
            candidate,
            ["toolchain", "status", "--json"],
            success = false,
            diagnostic = "--force",
        );
        root.harness.require(fixture.requests().length() == 0, "damaged installation was repaired automatically");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "all management commands ensure active toolchain" with root.harness.failure_intended("toolchain command skipped installation: toolchain use 0.12.0") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        for (
            let args in [
                ["toolchain", "use", NEXT_VERSION],
                ["toolchain", "install", NEXT_VERSION],
                ["toolchain", "update"],
                ["toolchain", "status", "--json"],
                ["toolchain", "list"],
                ["toolchain", "uninstall", LATEST_VERSION],
            ]
        ) {
            fixture.project(BASE_VERSION);
            fixture.absent(BASE_VERSION);
            fixture.manage(candidate, args);
            root.harness.require(
                baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
                `toolchain command skipped installation: ${args.join(" ")}`,
            );
        }
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "active installation finishes before installing or selecting an explicit target" with root.harness.failure_intended("URL must use HTTPS") {
        for (let command in ["install", "use"]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.project(BASE_VERSION);
            fixture.absent(BASE_VERSION);
            fixture.absent(NEXT_VERSION);
            let state = fixture.read(CHANNEL_STATE_FILE);
            fixture.manage(candidate, ["toolchain", command, NEXT_VERSION]);
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
                        `/version/${NEXT_VERSION}.json`,
                        `/${NEXT_VERSION}${extension}`,
                    ],
                "explicit target ran before the active installation completed",
            );
            root.harness.require(fixture.read(CHANNEL_STATE_FILE) == state, "exact installation changed channels");
            fixture.check(
                candidate,
                if (command == "use") {
                    NEXT_VERSION
                } else {
                    BASE_VERSION
                },
            );
        }
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "list continues after detecting damage without repairing it" with root.harness.failure_intended("damage diagnostic omitted explicit repair") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(`baml/toolchains/${BASE_VERSION}/VERSION`, "damaged-version");
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            [BASE_VERSION, NEXT_VERSION, LATEST_VERSION].every((version) -> bool {
                output.stdout.to_string().includes(version)
            }),
            "list omitted an installed fixture toolchain",
        );
        root.harness.require(output.stderr.to_string().includes("--force"), "damage diagnostic omitted explicit repair");
        root.harness.require(!fixture.requests(), "best effort installation repaired damage");
        root.harness.require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`) == "damaged-version"
                && fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(CHANNEL_STATE_FILE) == state,
            "best effort installation modified damaged state or selection",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "list continues after missing local executable without downloading it" with root.harness.failure_intended("local validation failure was silent") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/missing/baml-cli${fixture.suffix}`;
        let output = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            [BASE_VERSION, NEXT_VERSION, LATEST_VERSION].every((version) -> bool {
                output.stdout.to_string().includes(version)
            }),
            "list omitted an installed fixture toolchain",
        );
        root.harness.require(
            output.stderr.to_string().includes("could not ensure"),
            "local validation failure was silent",
        );
        root.harness.require(!fixture.requests(), "best effort installation downloaded a local executable");
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/missing`),
            "local validation created a download destination",
        );
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "explicit channel install preserves recorded version" with root.harness.failure_intended("explicit channel install advanced beyond recorded version") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "install", "nightly"]);
        root.harness.require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "explicit channel install advanced beyond recorded version",
        );
        root.harness.require(fixture.read(CHANNEL_STATE_FILE) == state, "explicit channel install advanced its mapping");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "short global flag after selector changes only the user default" with root.harness.failure_intended("management exit 1: error: unexpected argument '-g' found") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION, "-g"]);
        root.harness.require(fixture.read(PROJECT_MANIFEST_FILE) == project, "-g changed the project manifest");
        root.harness.require(
            fixture.read(USER_CONFIG_FILE).includes(NEXT_VERSION),
            "-g did not select the user default",
        );
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "removed local flag is rejected without changing selections" with root.harness.failure_intended("management exit 0: ") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "use", "--local", NEXT_VERSION],
            success = false,
            diagnostic = "--local",
        );
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config,
            "removed --local flag changed a selection",
        );
        root.harness.require(!fixture.requests(), "removed --local flag contacted the release server");
        fixture.check(candidate, BASE_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "repeated global flags and aliases are rejected before side effects" with root.harness.failure_intended("missing diagnostic global:") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        for (let flags in [["--global", "--global"], ["-g", "-g"], ["--global", "-g"]]) {
            fixture.manage(
                candidate,
                ["toolchain", "use", NEXT_VERSION].concat(flags),
                success = false,
                diagnostic = "global",
            );
        }
        root.harness.require(
            fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config,
            "repeated global flags changed a selection",
        );
        root.harness.require(!fixture.requests(), "repeated global flags contacted the release server");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "use help documents global scope and the short alias without resolving a toolchain" with root.harness.failure_intended("use help omitted a global flag spelling") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        let output = fixture.manage(candidate, ["toolchain", "use", "--help"]).stdout.to_string();
        root.harness.require(
            output.includes("--global") && output.includes("-g"),
            "use help omitted a global flag spelling",
        );
        root.harness.require(!output.includes("--local"), "use help advertised the removed local flag");
        root.harness.require(!fixture.requests(), "use help contacted the release server");
    }
}
