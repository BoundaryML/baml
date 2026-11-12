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

    test "global use resolves path from cwd" {
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

    test "global use preserves project selection" {
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

    test "project use outside project requires global flag" {
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

    test "list reports empty installations without bootstrapping" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.remove("baml/toolchains");
        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        root.harness.require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        root.harness.require(listing.includes("installed toolchains: (none)"), "list did not report empty installations");
        root.harness.require(!fixture.requests(), "list bootstrapped the absent active toolchain");
        root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains`), "list created an installation directory");
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

    test "status reports available update without advancing channel" {
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

    test "global use bootstraps explicit baml home" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        root.harness.require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--global created manifest");
        fixture.check(candidate, NEXT_VERSION);
    }

    test "global use bootstraps default baml home" {
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

    test "use reports previous and new versions" {
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

    test "channel use reports previous and new versions" {
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

    test "list observes absent active toolchain without acquisition" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.absent(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        let tree = fixture.tree("baml");
        let output = fixture.manage(candidate, ["toolchain", "list"]);
        // Listing selectors and directories requires no active executable.
        root.harness.require(
            output.stdout.to_string().includes(BASE_VERSION),
            "list omitted active selector",
        );
        root.harness.require(!fixture.requests(), "list acquired the absent active toolchain");
        root.harness.require(fixture.tree("baml") == tree, "list modified local toolchain state");
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "list changed selectors",
        );
    }

    test "list preserves recorded channel without installing it" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.absent(BASE_VERSION);
        fixture.channel("canary", NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            output.stdout.to_string().includes(`canary: ${BASE_VERSION}`),
            "list omitted recorded channel",
        );
        root.harness.require(!fixture.requests(), "list resolved or installed the channel");
        root.harness.require(
            fixture.read(CHANNEL_STATE_FILE) == state,
            "list changed channel mapping",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "list installed recorded version",
        );
    }

    test "list leaves unrecorded channel unresolved" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.channel("canary", NEXT_VERSION);
        fixture.absent(NEXT_VERSION);
        fixture.remove(CHANNEL_STATE_FILE);
        let output = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(
            output.stdout.to_string().includes("canary"),
            "list omitted active selector",
        );
        root.harness.require(!fixture.requests(), "list resolved an unrecorded channel");
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`),
            "list recorded a channel mapping",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "list installed channel version",
        );
    }

    test "list does not attempt unavailable active release" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        let result = fixture.manage(candidate, ["toolchain", "list"]);
        root.harness.require(!fixture.requests(), "list contacted an unavailable release");
        root.harness.require(
            !result.stderr.to_string().includes("could not ensure"),
            "list attempted optional installation",
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

    test "uninstall leaves active toolchain removed" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION]);
        root.harness.require(!fixture.requests(), "uninstall acquired an executable");
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

    test "explicit install leaves unrelated active toolchain absent" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "install", NEXT_VERSION]);
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`)
                && baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "explicit install acquired an unrelated active toolchain or missed its target",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "explicit install changed selectors",
        );
    }

    test "status install is strict while plain status fails open" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/${BASE_VERSION}.tar.gz.status`, "404");
        fixture.write(`server/${BASE_VERSION}.zip.status`, "404");
        let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
        root.harness.require(
            !baml.json.decode<root.harness.ResolutionResult>(output.stdout.to_string()).installed,
            "plain status misreported failed installation",
        );
        root.harness.require(
            fixture.requests()
                == [
                    `/version/${BASE_VERSION}.json`,
                    `/${BASE_VERSION}${if (fixture.windows) { ".zip" } else { ".tar.gz" }}`,
                    "/canary.json",
                    "/nightly.json",
                ],
            "plain status did not attempt acquisition exactly once",
        );
        fixture.manage(candidate, ["toolchain", "status", "--install", "--json"], success = false);
        // The optional attempt populated the exact-version cache. The strict
        // invocation must make one artifact request, without a best-effort retry.
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
                    `/${BASE_VERSION}${extension}`,
                ],
            "status --install retried failed acquisition",
        );
        root.harness.require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
    }

    test "status rejects damaged installation without repairing" {
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

    test "non-channel update and uninstall do not acquire active toolchain" {
        for (let args in [["toolchain", "update"], ["toolchain", "uninstall", LATEST_VERSION]]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.project(BASE_VERSION);
            fixture.absent(BASE_VERSION);
            let state = fixture.read(CHANNEL_STATE_FILE);
            let project = fixture.read(PROJECT_MANIFEST_FILE);
            let config = fixture.read(USER_CONFIG_FILE);
            fixture.manage(candidate, args);
            root.harness.require(!fixture.requests(), `management acquired the active toolchain: ${args.join(" ")}`);
            root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`), "management installed absent active version");
            root.harness.require(
                fixture.read(CHANNEL_STATE_FILE) == state
                    && fixture.read(PROJECT_MANIFEST_FILE) == project
                    && fixture.read(USER_CONFIG_FILE) == config,
                "management changed selection or channel mapping",
            );
        }
    }

    test "uninstall rejects absent active version without acquisition" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.absent(BASE_VERSION);
        let tree = fixture.tree("baml");
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION], success = false, diagnostic = "not installed");
        root.harness.require(!fixture.requests(), "uninstall downloaded its absent target");
        root.harness.require(fixture.tree("baml") == tree, "uninstall modified absent target or selection");
    }

    test "uninstall does not resolve or validate active selection" {
        for (let selection in ["unrecorded channel", "invalid environment", "damaged version"]) {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            if (selection == "unrecorded channel") {
                fixture.project("canary", key = "channel");
                fixture.remove(CHANNEL_STATE_FILE);
                fixture.absent(BASE_VERSION);
            } else if (selection == "invalid environment") {
                fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
            } else {
                fixture.project(BASE_VERSION);
                fixture.write(`baml/toolchains/${BASE_VERSION}/VERSION`, "damaged-version");
            }
            fixture.manage(candidate, ["toolchain", "uninstall", NEXT_VERSION]);
            root.harness.require(!fixture.requests(), "uninstall resolved or installed the active selection");
            root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`), "uninstall left its requested target installed");
            if (selection == "unrecorded channel") {
                root.harness.require(!baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`), "uninstall recorded an active channel");
                root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`), "uninstall installed the active channel");
            }
        }
    }

    test "channel update acquires only its requested release" {
        for (let recorded in [true, false]) {
            for (let succeeds in [true, false]) {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                fixture.project("canary", key = "channel");
                fixture.channel("canary", NEXT_VERSION);
                fixture.absent(BASE_VERSION);
                fixture.absent(NEXT_VERSION);
                let state = fixture.read(CHANNEL_STATE_FILE);
                let project = fixture.read(PROJECT_MANIFEST_FILE);
                let config = fixture.read(USER_CONFIG_FILE);
                if (!recorded) {
                    fixture.remove(CHANNEL_STATE_FILE);
                }
                let extension = if (fixture.windows) { ".zip" } else { ".tar.gz" };
                if (!succeeds) {
                    fixture.write(`server/${NEXT_VERSION}${extension}.status`, "404");
                }
                fixture.manage(candidate, ["toolchain", "update"], success = succeeds);
                root.harness.require(
                    fixture.requests() == ["/canary.json", `/${NEXT_VERSION}${extension}`],
                    "update bootstrapped the old version or duplicated requested acquisition",
                );
                root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`), "update installed old active version");
                root.harness.require(
                    fixture.read(PROJECT_MANIFEST_FILE) == project && fixture.read(USER_CONFIG_FILE) == config,
                    "update changed selectors",
                );
                if (succeeds) {
                    root.harness.require(fixture.channel_version("canary") == NEXT_VERSION, "update did not record requested release");
                    fixture.check(candidate, NEXT_VERSION);
                } else {
                    root.harness.require(!baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`), "failed update published a release");
                    if (recorded) {
                        root.harness.require(fixture.read(CHANNEL_STATE_FILE) == state, "failed update changed channel mapping");
                    } else {
                        root.harness.require(!baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`), "failed update recorded a new channel");
                    }
                }
            }
        }
    }

    test "explicit target acquisition skips absent old selection" {
        for (let active in ["project version", "project channel", "environment"]) {
            for (
                let args in [
                    ["install", NEXT_VERSION],
                    ["use", NEXT_VERSION],
                    ["use", "--global", NEXT_VERSION],
                ]
            ) {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                if (active == "project channel") {
                    fixture.project("canary", key = "channel");
                } else {
                    fixture.project(BASE_VERSION);
                }
                if (active == "environment") {
                    fixture.env["BAML_TOOLCHAIN"] = BASE_VERSION;
                }
                fixture.absent(BASE_VERSION);
                fixture.absent(NEXT_VERSION);
                let state = fixture.read(CHANNEL_STATE_FILE);
                fixture.manage(candidate, ["toolchain"].concat(args));
                let extension = if (fixture.windows) {
                    ".zip"
                } else {
                    ".tar.gz"
                };
                root.harness.require(
                    fixture.requests()
                        == [`/version/${NEXT_VERSION}.json`, `/${NEXT_VERSION}${extension}`],
                    "explicit target acquired the old active selection",
                );
                root.harness.require(
                    fixture.read(CHANNEL_STATE_FILE) == state,
                    "exact installation changed channels",
                );
                if (args.includes("--global")) {
                    fixture.cwd = `${fixture.root}/outside`;
                }
                fixture.env["BAML_TOOLCHAIN"] = "";
                // Ordinary delegation still prepares its own selected executable.
                fixture.check(
                    candidate,
                    if (args[0] == "use") {
                        NEXT_VERSION
                    } else {
                        BASE_VERSION
                    },
                );
            }
        }
    }

    test "same target acquisition failure is attempted once" {
        for (let selection in ["version", "recorded channel", "unrecorded channel"]) {
            let selector = if (selection == "version") {
                BASE_VERSION
            } else {
                "canary"
            };
            for (
                let args in [
                    ["install", selector],
                    ["install", selector, "--force"],
                    ["install"],
                    ["install", "--force"],
                    ["use", selector],
                    ["use", "--global", selector],
                    ["status", "--install", "--json"],
                ]
            ) {
                let fixture = root.harness.Fixture.start();
                defer { fixture.close(); }
                fixture.project(
                    selector,
                    key = if (selection == "version") {
                        "version"
                    } else {
                        "channel"
                    },
                );
                fixture.absent(BASE_VERSION);
                if (selection == "unrecorded channel") {
                    fixture.remove(CHANNEL_STATE_FILE);
                }
                let config = fixture.read(USER_CONFIG_FILE);
                let project = fixture.read(PROJECT_MANIFEST_FILE);
                let extension = if (fixture.windows) {
                    ".zip"
                } else {
                    ".tar.gz"
                };
                // A non-retryable HTTP failure isolates command-level acquisition
                // from the Rust release downloader's existing transient retries.
                fixture.write(`server/${BASE_VERSION}${extension}.status`, "404");
                fixture.manage(
                    candidate,
                    ["toolchain"].concat(args),
                    success = false,
                );
                let manifest = if (selection == "unrecorded channel") {
                    "/canary.json"
                } else {
                    `/version/${BASE_VERSION}.json`
                };
                root.harness.require(
                    fixture.requests() == [manifest, `/${BASE_VERSION}${extension}`],
                    `required acquisition was duplicated: ${selection}, ${args.join(" ")}`,
                );
                root.harness.require(
                    fixture.read(USER_CONFIG_FILE) == config
                        && fixture.read(PROJECT_MANIFEST_FILE) == project,
                    "failed acquisition changed selectors",
                );
                root.harness.require(
                    !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
                    "failed acquisition published a toolchain",
                );
            }
        }
    }

    test "list inventories damaged installations without verifying or repairing them" {
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
        // Directory inventory does not promise executable validation; status does.
        root.harness.require(
            !output.stderr.to_string().includes("could not ensure"),
            "list verified the active executable",
        );
        root.harness.require(!fixture.requests(), "list repaired damage");
        root.harness.require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`) == "damaged-version"
                && fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(CHANNEL_STATE_FILE) == state,
            "list modified damaged state or selection",
        );
    }

    test "list continues after missing local executable without downloading it" {
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
            !output.stderr.to_string().includes("could not ensure"),
            "list tried to validate a local executable",
        );
        root.harness.require(
            !fixture.requests(),
            "list downloaded a local executable",
        );
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/missing`),
            "local validation created a download destination",
        );
    }

    test "explicit channel install preserves recorded version" {
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

    test "short global flag after selector changes only the user default" {
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

    test "removed local flag is rejected without changing selections" {
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

    test "repeated global flags and aliases are rejected before side effects" {
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

    test "use help documents global scope and the short alias without resolving a toolchain" {
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
