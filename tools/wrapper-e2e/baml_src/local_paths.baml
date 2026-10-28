// Local toolchain paths are resolved without release requests.

test "local paths: absolute local path is launched" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: environment relative path resolves from cwd" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = `./build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: project relative path resolves from project root" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: tilde path resolves from home" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = `~/build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: local path supports spaces and unicode" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"]
            = `${fixture.root}/project/space ünicode/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: local path resolves parent components" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = `../../build/baml-cli${fixture.suffix}`;
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: parent components preserve symlink semantics" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.symlink("project/link", "outside/build");
            fixture.env["BAML_TOOLCHAIN"] = "./link/../build/baml-cli";

            fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
            require(fixture.requests().length() == 0, "local toolchain contacted release server");
            null
        },
        capabilities = [Capability.Symlinks],
    );
}

test "local paths: global relative path resolves from each cwd" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.config(`./build/baml-cli${fixture.suffix}`);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
        require(fixture.requests().length() == 0, "local toolchain contacted release server");
        null
    });
}

test "local paths: symlink target is resolved for each launch" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.symlink("project/current", "project/build/baml-cli");
            fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/current`;
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            fixture.symlink("project/current", "outside/build/baml-cli");
            fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
            require(fixture.requests().length() == 0, "local toolchain contacted release server");
            null
        },
        capabilities = [Capability.Symlinks],
    );
}

test "local paths: missing local binary reports environment source" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/missing/baml-cli`;
        fixture.failed_check(candidate, diagnostic = "BAML_TOOLCHAIN");
        require(fixture.requests().length() == 0, "local failure contacted server");
        null
    });
}

test "local paths: local directory is rejected as a binary" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build`;
        fixture.failed_check(candidate, diagnostic = "directory");
        require(fixture.requests().length() == 0, "local failure contacted server");
        null
    });
}

test "local paths: windows backslash path is launched" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.env["BAML_TOOLCHAIN"]
                = `${fixture.root}/project/build/baml-cli.exe`.replace_all("/", "\\");
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            require(fixture.requests().length() == 0, "local toolchain contacted release server");
            null
        },
        capabilities = [Capability.Windows],
    );
}

test "local paths: windows local path infers executable suffix" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli`;
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            require(fixture.requests().length() == 0, "local toolchain contacted release server");
            null
        },
        capabilities = [Capability.Windows],
    );
}
