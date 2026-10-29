// Selector precedence and project discovery.

testset "toolchain selection" {
    let candidate = Candidate.from_environment();

    test "global version is used without project selection" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "global canary uses its recorded version" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "global nightly uses its recorded version" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("nightly");

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "global selector is trimmed before validation" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config(" \tcanary\n");

        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "project version overrides global default" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "environment version overrides project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "environment channel overrides project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = "nightly";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "environment path overrides project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

        fixture.check_installed_selection(candidate, PROJECT_TOOLCHAIN_IDENTITY);
    }

    test "empty environment falls back to project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = "";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "whitespace environment falls back to project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = " \t ";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "environment selector is trimmed" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = ` ${NEXT_VERSION} \n`;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "project is discovered from nested directory" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.cwd = `${fixture.root}/project/nested/deep`;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "closest project selector wins" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "project/nested/deep/baml.toml",
            `
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.config(NEXT_VERSION);

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "unpinned closest project uses global default" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "project/nested/deep/baml.toml",
            `
                [package]
                name = "nearest"\n
            `,
        );
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.config(NEXT_VERSION);

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "absent global default falls back to canary" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.config("canary");

        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = false\n
            `,
        );
        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "malformed project syntax rejects launch" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.failed_check(candidate, diagnostic = "baml.toml");
        require(fixture.requests().length() == 0, "invalid project used network");
    }

    test "malformed project selector rejects launch" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                [toolchain]
                version = 42\n
            `,
        );
        fixture.failed_check(candidate, diagnostic = "baml.toml");
        require(fixture.requests().length() == 0, "invalid project used network");
    }

    test "version takes priority over other selectors" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                [toolchain]
                version = "${BASE_VERSION}"
                channel = "nightly"
                path = "./build/baml-cli"\n
            `,
        );
        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "unused project selectors are not validated" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                [toolchain]
                version = "${BASE_VERSION}"
                channel = 42
                path = false\n
            `,
        );
        fixture.check_installed_selection(candidate, BASE_VERSION);
    }

    test "environment bypasses malformed project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "selection is rediscovered after changing directory" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.check(candidate, BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    test "unpinned closest project uses default canary" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "project/nested/deep/baml.toml",
            `
                [package]
                name = "nearest"\n
            `,
        );
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = false\n
            `,
        );
        fixture.write(
            CHANNEL_STATE_FILE,
            `
                [channels.canary]
                active_version = "${NEXT_VERSION}"
                resolved_at = "x"
                manifest_path = "x"\n
            `,
        );

        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "project prefix selects another project" with failure_intended("\"args\":[\"--project\",\"../outside\",\"check\"]") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["--project", "../outside", "check"];

        fixture.behavior(NEXT_VERSION, `${NEXT_VERSION}\n`, args = args);
        fixture.check(candidate, NEXT_VERSION, args = args);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "inline project option selects another project" with failure_intended("\"args\":[\"check\",\"--project=../outside\"]") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["check", "--project=../outside"];

        fixture.behavior(NEXT_VERSION, `${NEXT_VERSION}\n`, args = args);
        fixture.check(candidate, NEXT_VERSION, args = args);
    }

    test "removed directory flag is forwarded without changing the selected project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["--directory", "../outside", "check"];

        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    test "environment overrides explicit project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["--project", "../outside", "check"];
        fixture.env["BAML_TOOLCHAIN"] = BASE_VERSION;

        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    test "project option after separator is forwarded" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["run", "main", "--", "--project", "../outside"];

        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    test "query project argument does not select local project" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["query", "--project", "org/cloud-project"];
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    test "project option is independent of forwarded directory" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );

        let args = ["--directory=../outside", "--project=../project", "check"];
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "use updates the explicit project" with failure_intended("\"args\":[\"--project\",\"../outside\",\"toolchain\",\"use\",\"0.13.0\"]") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        let args = ["--project", "../outside", "check"];

        fixture.manage(candidate, ["--project", "../outside", "toolchain", "use", LATEST_VERSION]);
        require(fixture.read(PROJECT_MANIFEST_FILE).includes(BASE_VERSION), "use changed cwd project");
        fixture.behavior(LATEST_VERSION, `${LATEST_VERSION}\n`, args = args);
        fixture.check(candidate, LATEST_VERSION, args = args);
    }

    test "query project is forwarded after global options" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );

        let args = ["--output-preset", "agent", "query", "--project", "org/cloud-project"];
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }

    test "removed directory flag does not rebase an environment path" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(BASE_VERSION);
        fixture.write(
            "outside/baml.toml",
            `
                [package]
                name = "outside"
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );

        let args = ["--directory", "../outside", "check"];
        fixture.env["BAML_TOOLCHAIN"] = "./build/baml-cli";
        fixture.behavior(PROJECT_TOOLCHAIN_IDENTITY, `${PROJECT_TOOLCHAIN_IDENTITY}\n`, args = args);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY, args = args);
    }

    test "project search stops at home directory" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            "baml.toml",
            `
                [toolchain]
                version = "${NEXT_VERSION}"\n
            `,
        );
        fixture.cwd = `${fixture.root}/home/nested/deep`;
        baml.fs.mkdir(fixture.cwd, baml.fs.MkdirOptions { recursive: true });
        if (fixture.windows) {
            fixture.env["HOME"] = `${fixture.root}/home`.replace_all("/", "\\");
            fixture.env["USERPROFILE"] = fixture.env["HOME"];
        }
        fixture.check(candidate, BASE_VERSION);
        let before = fixture.read("baml.toml");
        fixture.manage(candidate, ["toolchain", "use", NEXT_VERSION], success = false);
        require(fixture.read("baml.toml") == before, "use crossed the home-directory boundary");
        require(fixture.requests().length() == 0, "home boundary check used network");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "explicit unpinned project uses global default" with failure_intended("\"args\":[\"--project\",\"../outside\",\"check\"]") {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.write("outside/baml.toml", `[package]\nname = "outside"\n`);
        let args = ["--project", "../outside", "check"];
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
        fixture.check(candidate, BASE_VERSION, args = args);
    }
    // Installed selection must preserve selectors and avoid release requests.
}
