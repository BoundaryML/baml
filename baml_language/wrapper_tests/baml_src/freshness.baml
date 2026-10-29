// Passive channel freshness checks, caching, and retry throttling.

testset "passive update checks" {
    let candidate = root.harness.Candidate.from_environment();

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "cached newer channel emits update warning" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        let warning = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\n";
        fixture.channel("canary", NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "status"]);
        fixture.check(candidate, BASE_VERSION, expected_stderr = warning);
        let requests = fixture.requests().length();
        root.harness.require(requests == 1, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = warning);
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "cached current channel emits no warning" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.manage(candidate, ["toolchain", "status"]);
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        let requests = fixture.requests().length();
        root.harness.require(requests == 1, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "passive refresh warns and is throttled" with root.harness.failure_intended("stderr changed: expected \"warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\\n\", got \"\"") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        let warning = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\n";
        fixture.channel("canary", NEXT_VERSION);
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = true\n
            `,
        );
        fixture.check(candidate, BASE_VERSION, expected_stderr = warning);
        let requests = fixture.requests().length();
        root.harness.require(requests == 1, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = warning);
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    test "offline passive refresh is throttled" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = true\n
            `,
        );
        fixture.write("server/canary.json.status", "500");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        let requests = fixture.requests().length();
        root.harness.require(requests == 1, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    test "disabled passive refresh uses no network" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        let requests = fixture.requests().length();
        root.harness.require(requests == 0, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "passive refresh reuses status cache" with root.harness.failure_intended("URL must use HTTPS") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project("canary", key = "channel");
        fixture.manage(candidate, ["toolchain", "status"]);
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = true\n
            `,
        );
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        let requests = fixture.requests().length();
        root.harness.require(requests == 1, "unexpected freshness request count");
        fixture.check(candidate, BASE_VERSION, expected_stderr = "");
        root.harness.require(fixture.requests().length() == requests, "freshness attempt was not throttled");
    }

    test "passive refresh skips local selection" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = true\n
            `,
        );
        fixture.project("./build/baml-cli", key = "path");
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "non-channel selection refreshed metadata");
    }

    test "passive refresh skips exact version" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.write(
            USER_CONFIG_FILE,
            `
                [update]
                auto_check = true\n
            `,
        );
        fixture.project(BASE_VERSION);
        fixture.check(candidate, BASE_VERSION);
        root.harness.require(fixture.requests().length() == 0, "non-channel selection refreshed metadata");
    }
}
