// Project/global selection, updates, removal, and management preflight.

test "management: use updates project and preserves global default" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config(NEXT_VERSION);
        let before = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "use", BASE_VERSION]);
        require(fixture.read(USER_CONFIG_FILE) == before, "project use changed user config");
        require(fixture.read(PROJECT_MANIFEST_FILE).includes(BASE_VERSION), "project not selected");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use nightly installs and records channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "nightly"]);
        require(fixture.read(USER_CONFIG_FILE) == config_before, "channel use changed global default");
        require(fixture.channel_version("nightly") == NEXT_VERSION, "channel not recorded");
        require(fixture.read(PROJECT_MANIFEST_FILE).includes("nightly"), "channel selector not written");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: global use resolves path from cwd" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(
            candidate,
            ["toolchain", "use", "--global", `./build/baml-cli${fixture.suffix}`],
        );
        require(fixture.read(PROJECT_MANIFEST_FILE) == before, "--global changed project");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
        null
    });
}

test "management: global use preserves project selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        require(fixture.read(PROJECT_MANIFEST_FILE) == before, "--global changed project");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use installs absent project version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config(NEXT_VERSION);
        fixture.absent(BASE_VERSION);
        let before = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "use", BASE_VERSION]);
        require(fixture.read(USER_CONFIG_FILE) == before, "project use changed user config");
        require(fixture.read(PROJECT_MANIFEST_FILE).includes(BASE_VERSION), "project not selected");
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use canary installs and records channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "canary"]);
        require(fixture.read(USER_CONFIG_FILE) == config_before, "channel use changed global default");
        require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
        require(fixture.read(PROJECT_MANIFEST_FILE).includes("canary"), "channel selector not written");
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: project use resolves path from project root" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.manage(candidate, ["toolchain", "use", `./build/baml-cli${fixture.suffix}`]);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        fixture.cwd = `${fixture.root}/project`;
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        null
    });
}

test "management: use replaces version and preserves comments" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use replaces channel and preserves comments" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use replaces path and preserves comments" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            updated.includes("retain me")
                && updated.includes("selector comment")
                && updated.includes("value = 42"),
            "comments or unknown data lost",
        );
        require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: project use outside project requires global flag" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(LATEST_VERSION);
        fixture.manage(
            candidate,
            ["toolchain", "use", LATEST_VERSION],
            success = false,
            diagnostic = "--global",
        );
        require(
            fixture.read(USER_CONFIG_FILE) == config_before
                && !baml.fs.exists(`${fixture.root}/outside/baml.toml`)
                && fixture.requests().length() == 0,
            "use outside project mutated state",
        );
        null
    });
}

test "management: failed project use download preserves selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push(NEXT_VERSION);
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: failed global use download preserves selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("--global");
        args.push(NEXT_VERSION);
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: failed project use path preserves selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("./missing/baml-cli");
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: failed global use path preserves selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.project(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        let before = fixture.read(PROJECT_MANIFEST_FILE);
        let args = ["toolchain", "use"];
        args.push("--global");
        args.push("./missing/baml-cli");
        fixture.write(`server/version/${NEXT_VERSION}.json.status`, "500");
        fixture.manage(candidate, args, success = false);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "failed use changed selectors",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: update advances global channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.channel("canary", NEXT_VERSION);
        fixture.absent(NEXT_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("canary") == NEXT_VERSION, "wrong shared channel updated");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: failed update preserves usable toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.write(`server/canary.json.status`, "500");
        fixture.manage(candidate, ["toolchain", "update"], success = false);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: update does not advance environment version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = BASE_VERSION;
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update does not advance environment path" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.env["BAML_TOOLCHAIN"] = selected;
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update does not advance project version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION, key = "version");
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update does not advance project path" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.project(selected, key = "path");
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update does not advance global version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config(BASE_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "does not advance",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update does not advance global path" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.config(selected);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "update"],
            success = true,
            diagnostic = "local path",
        );
        require(
            fixture.requests().length() == 0 && fixture.read(CHANNEL_STATE_FILE) == state,
            "non-channel update performed work",
        );
        null
    });
}

test "management: update advances project channel only" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        require(fixture.channel_version("canary") == BASE_VERSION, "unselected channel changed");
        null
    });
}

test "management: update advances environment channel only" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.env["BAML_TOOLCHAIN"] = "nightly";
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        require(fixture.channel_version("canary") == BASE_VERSION, "unselected channel changed");
        null
    });
}

test "management: uninstall removes unused version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", NEXT_VERSION], success = true);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: uninstall rejects absent version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", "9.9.9"], success = false);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: uninstall rejects local path" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        fixture.manage(candidate, ["toolchain", "uninstall", selected], success = false);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: uninstall rejects channel name" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", "canary"], success = false);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: uninstalled selected version is reinstalled on launch" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION], success = true);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: uninstalled channel version is reinstalled without advancing" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION], success = true);
        require(
            fixture.read(CHANNEL_STATE_FILE) == state && fixture.read(USER_CONFIG_FILE) == config,
            "uninstall changed selection or mapping",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall did not remove directory",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: list installs absent active toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.remove("baml/toolchains");
        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        require(listing.includes(`  ${BASE_VERSION}`), "list did not install the absent active toolchain");
        require(
            fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
        null
    });
}

test "management: list reports installed toolchains" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        require(
            listing.includes(NEXT_VERSION) && listing.includes(LATEST_VERSION),
            "list omitted installed versions",
        );
        require(
            fixture.requests().length() == 0
                && fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
        null
    });
}

test "management: list reports environment override" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let config_before = fixture.read(USER_CONFIG_FILE);

        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        let before = fixture.read(CHANNEL_STATE_FILE);
        let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
        require(listing.includes(`default selector: ${BASE_VERSION}`), "list omitted the user default");
        require(
            listing.includes(NEXT_VERSION) && listing.includes(LATEST_VERSION),
            "list omitted installed versions",
        );
        require(listing.includes(`active selector: ${NEXT_VERSION}`), "list omitted the active override");
        require(
            fixture.requests().length() == 0
                && fixture.read(USER_CONFIG_FILE) == config_before
                && fixture.read(CHANNEL_STATE_FILE) == before,
            "list changed state or downloaded",
        );
        null
    });
}

test "management: status reports available update without installing" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(CHANNEL_STATE_FILE) == before
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "status installed or changed state",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: failed status preserves selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");
        fixture.absent(NEXT_VERSION);
        fixture.channel("canary", NEXT_VERSION);
        fixture.write("server/canary.json.status", "500");
        let before = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "status"], success = false, diagnostic = "");
        require(
            fixture.read(CHANNEL_STATE_FILE) == before
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}`),
            "status installed or changed state",
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: environment channel update preserves other channels" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");
        fixture.config("nightly");
        fixture.project("nightly", key = "channel");
        fixture.env["BAML_TOOLCHAIN"] = "canary";
        fixture.channel("canary", LATEST_VERSION);
        fixture.absent(LATEST_VERSION);
        let before_project = fixture.read(PROJECT_MANIFEST_FILE);
        let before_config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(candidate, ["toolchain", "update"], success = true);
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        require(fixture.channel_version("nightly") == NEXT_VERSION, "unselected channel changed");
        null
    });
}

test "management: update uses canary when no default is configured" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        null
    });
}

test "management: update ignores parent of unpinned closest project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("canary") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        null
    });
}

test "management: channel update is shared across projects" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before_project
                && fixture.read(USER_CONFIG_FILE) == before_config,
            "update changed selector",
        );
        require(fixture.channel_version("nightly") == LATEST_VERSION, "wrong shared channel updated");
        fixture.check(candidate, LATEST_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, LATEST_VERSION);
        null
    });
}

test "management: global use bootstraps explicit baml home" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--global created manifest");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: global use bootstraps default baml home" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/outside`;
        fixture.absent(NEXT_VERSION);
        fixture.env["BAML_HOME"] = "";
        fixture.remove("baml");
        fixture.manage(candidate, ["toolchain", "use", "--global", NEXT_VERSION]);
        require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--global created manifest");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use updates unpinned closest project only" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == before
                && fixture.read(USER_CONFIG_FILE) == config_before,
            "use changed parent/global",
        );
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: use reports previous and new versions" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let selector = NEXT_VERSION;
        let output = fixture.manage(candidate, ["toolchain", "use", selector]).stdout.to_string();
        require(
            output.includes("previous:")
                && output.includes(BASE_VERSION)
                && output.includes(NEXT_VERSION),
            "selection output omitted concrete previous/new version",
        );
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: channel use reports previous and new versions" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project("canary", key = "channel");
        let output = fixture.manage(candidate, ["toolchain", "use", "nightly"]).stdout.to_string();
        require(
            output.includes("previous:")
                && output.includes(BASE_VERSION)
                && output.includes(NEXT_VERSION),
            "selection output omitted concrete previous/new version",
        );
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: update command is delegated to selected cli" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.behavior(BASE_VERSION, "selected CLI guidance\n", args = ["update"]);
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["update"],
            output = "selected CLI guidance\n",
        );
        require(fixture.requests().length() == 0, "installed delegated update used network");
        null
    });
}

test "management: upgrade command is delegated to selected cli" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.behavior(BASE_VERSION, "selected CLI guidance\n", args = ["upgrade"]);
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["upgrade"],
            output = "selected CLI guidance\n",
        );
        require(fixture.requests().length() == 0, "installed delegated update used network");
        null
    });
}

test "management: list ensures active toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "list"]);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "list did not ensure active CLI",
        );
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: preflight installs recorded channel version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.project("canary", key = "channel");
        fixture.channel("canary", NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "list"]);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "channel preflight did not install expected version",
        );
        require(
            fixture.read(CHANNEL_STATE_FILE).includes(`active_version = "${BASE_VERSION}"`),
            "incorrect channel mapping",
        );
        null
    });
}

test "management: preflight resolves unrecorded channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.project("canary", key = "channel");
        fixture.channel("canary", NEXT_VERSION);
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "list"]);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "channel preflight did not install expected version",
        );
        require(
            fixture.read(CHANNEL_STATE_FILE).includes(`active_version = "${NEXT_VERSION}"`),
            "incorrect channel mapping",
        );
        null
    });
}

test "management: continues after failed best effort install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.write(`server/${BASE_VERSION}.tar.gz.status`, "500");
        fixture.write(`server/${BASE_VERSION}.zip.status`, "500");
        let result = fixture.manage(candidate, ["toolchain", "list"]);
        require(result.stderr.to_string().includes("could not ensure"), "installation failure was silent");
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: explicit force install repairs damaged active toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "preflight blocked explicit repair",
        );
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: uninstall leaves active toolchain removed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.manage(candidate, ["toolchain", "uninstall", BASE_VERSION]);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}`),
            "uninstall reinstalled its target",
        );
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: explicit install also ensures active toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "install", NEXT_VERSION]);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`)
                && baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "explicit install target replaced active preflight",
        );
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: status install requires successful preflight" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let config = fixture.read(USER_CONFIG_FILE);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.write(`server/${BASE_VERSION}.tar.gz.status`, "500");
        fixture.write(`server/${BASE_VERSION}.zip.status`, "500");
        let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
        require(
            !baml.json.decode<ResolutionResult>(output.stdout.to_string()).installed,
            "plain status misreported failed installation",
        );
        fixture.manage(candidate, ["toolchain", "status", "--install", "--json"], success = false);
        require(
            fixture.read(USER_CONFIG_FILE) == config
                && fixture.read(PROJECT_MANIFEST_FILE) == project,
            "preflight changed selectors",
        );
        null
    });
}

test "management: status rejects damaged installation without repairing" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(fixture.requests().length() == 0, "damaged installation was repaired automatically");
        null
    });
}

test "management: all management commands ensure active toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
            require(
                baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
                `toolchain command skipped installation: ${args.join(" ")}`,
            );
        }
        null
    });
}

test "management: explicit channel install preserves recorded version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.channel("nightly", LATEST_VERSION);
        fixture.absent(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        fixture.manage(candidate, ["toolchain", "install", "nightly"]);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/${NEXT_VERSION}/VERSION`),
            "explicit channel install advanced beyond recorded version",
        );
        require(fixture.read(CHANNEL_STATE_FILE) == state, "explicit channel install advanced its mapping");
        null
    });
}

test "management: short global flag after selector changes only the user default" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION, "-g"]);
        require(fixture.read(PROJECT_MANIFEST_FILE) == project, "-g changed the project manifest");
        require(
            fixture.read(USER_CONFIG_FILE).includes(NEXT_VERSION),
            "-g did not select the user default",
        );
        fixture.check(candidate, BASE_VERSION);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "management: removed local flag is rejected without changing selections" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let project = fixture.read(PROJECT_MANIFEST_FILE);
        let config = fixture.read(USER_CONFIG_FILE);
        fixture.manage(
            candidate,
            ["toolchain", "use", "--local", NEXT_VERSION],
            success = false,
            diagnostic = "--local",
        );
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config,
            "removed --local flag changed a selection",
        );
        require(!fixture.requests(), "removed --local flag contacted the release server");
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "management: repeated global flags and aliases are rejected before side effects" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        require(
            fixture.read(PROJECT_MANIFEST_FILE) == project
                && fixture.read(USER_CONFIG_FILE) == config,
            "repeated global flags changed a selection",
        );
        require(!fixture.requests(), "repeated global flags contacted the release server");
        null
    });
}

test "management: use help documents global scope and the short alias without resolving a toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
        let output = fixture.manage(candidate, ["toolchain", "use", "--help"]).stdout.to_string();
        require(
            output.includes("--global") && output.includes("-g"),
            "use help omitted a global flag spelling",
        );
        require(!output.includes("--local"), "use help advertised the removed local flag");
        require(!fixture.requests(), "use help contacted the release server");
        null
    });
}
