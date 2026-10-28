// Selector precedence and project discovery.

test "selection: global version is used without project selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.check_installed_selection(candidate, BASE_VERSION);
        null
    });
}

test "selection: global canary uses its recorded version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.check_installed_selection(candidate, BASE_VERSION);
        null
    });
}

test "selection: global nightly uses its recorded version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("nightly");

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: project version overrides global default" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: environment version overrides project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: environment channel overrides project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = "nightly";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: environment path overrides project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

        fixture.check_installed_selection(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        null
    });
}

test "selection: empty environment falls back to project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = "";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: whitespace environment falls back to project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = " \t ";

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: environment selector is trimmed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = ` ${NEXT_VERSION} \n`;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: project is discovered from nested directory" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        fixture.cwd = `${fixture.root}/project/nested/deep`;

        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: closest project selector wins" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: unpinned closest project uses global default" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: absent global default falls back to canary" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.config("canary");

        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = false\n
            `,
        );
        fixture.check_installed_selection(candidate, BASE_VERSION);
        null
    });
}

test "selection: malformed project syntax rejects launch" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.failed_check(candidate, diagnostic = "baml.toml");
        require(fixture.requests().length() == 0, "invalid project used network");
        null
    });
}

test "selection: malformed project selector rejects launch" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.write(
            PROJECT_MANIFEST_FILE,
            `
                [toolchain]
                version = 42\n
            `,
        );
        fixture.failed_check(candidate, diagnostic = "baml.toml");
        require(fixture.requests().length() == 0, "invalid project used network");
        null
    });
}

test "selection: version takes priority over other selectors" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: unused project selectors are not validated" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: environment bypasses malformed project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.write(PROJECT_MANIFEST_FILE, "[invalid");
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        fixture.check_installed_selection(candidate, NEXT_VERSION);
        null
    });
}

test "selection: selection is rediscovered after changing directory" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: unpinned closest project uses default canary" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: project prefix selects another project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: inline project option selects another project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: removed directory flag is forwarded without changing the selected project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: environment overrides explicit project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: project option after separator is forwarded" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: query project argument does not select local project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: project option is independent of forwarded directory" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: use updates the explicit project" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: query project is forwarded after global options" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: removed directory flag does not rebase an environment path" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        fixture.behavior(PROJECT_TOOLCHAIN_IDENTITY, `L1\n`, args = args);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY, args = args);
        null
    });
}

test "selection: project search stops at home directory" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
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
        null
    });
}

test "selection: explicit unpinned project uses global default" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        for (let flag in ["--project"]) {
            let args = [flag, "../outside", "check"];
            fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = args);
            fixture.check(candidate, BASE_VERSION, args = args);
        }
        null
    });
}

// Installed selection must preserve selectors and avoid release requests.
