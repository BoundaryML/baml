// Local toolchain paths are resolved without release requests.

testset "local toolchain paths" {
    let candidate = root.harness.Candidate.from_environment();

    test "absolute local path is launched" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    test "environment relative path resolves from cwd" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = `./build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    test "project relative path resolves from project root" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    test "tilde path resolves from home" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `~/build/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    test "local path supports spaces and unicode" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"]
            = `${fixture.root}/project/space ünicode/baml-cli${fixture.suffix}`;

        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    test "local path resolves parent components" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.cwd = `${fixture.root}/project/nested/deep`;
        fixture.env["BAML_TOOLCHAIN"] = `../../build/baml-cli${fixture.suffix}`;
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    if (candidate.supports([root.harness.Capability.Symlinks])) {
        // Failure intended: the current Rust wrapper does not yet satisfy this contract.
        // Once fixed, remove this marker and assert expected behavior.
        test "parent components preserve symlink semantics" with root.harness.failure_intended("stdout: project-local-toolchain") {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.symlink("project/link", "outside/build");
            fixture.env["BAML_TOOLCHAIN"] = "./link/../build/baml-cli";

            fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
            root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
        }
    }

    // Failure intended: the current Rust wrapper does not yet satisfy this contract.
    // Once fixed, remove this marker and assert expected behavior.
    test "relative paths written directly to user config resolve from each cwd" with root.harness.failure_intended("relative path") {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = "";
        fixture.config(`./build/baml-cli${fixture.suffix}`);
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        fixture.cwd = `${fixture.root}/outside`;
        fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
        root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
    }

    if (candidate.supports([root.harness.Capability.Symlinks])) {
        test "symlink target is resolved for each launch" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.symlink("project/current", "project/build/baml-cli");
            fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/current`;
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            fixture.symlink("project/current", "outside/build/baml-cli");
            fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
            root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
        }
    }

    test "missing local binary reports environment source" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/missing/baml-cli`;
        let output = fixture.manage(candidate, ["check"], success = false);
        root.harness.require(
            output.stderr.to_string().includes("BAML_TOOLCHAIN"),
            "stderr omitted the environment source",
        );
        root.harness.require(
            output.stderr.to_string().replace_all("\\", "/").includes("missing/baml-cli"),
            "error omitted the missing path",
        );
        root.harness.require(fixture.requests().length() == 0, "local failure contacted server");
    }

    test "local directory is rejected as a binary" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build`;
        fixture.failed_check(candidate, diagnostic = "directory");
        root.harness.require(fixture.requests().length() == 0, "local failure contacted server");
    }

    test "local executable needs no managed version or channel state" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.remove("baml/toolchains");
        fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");
        root.harness.require(!baml.fs.exists(`${fixture.root}/project/VERSION`), "local fixture has managed metadata");
        fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
        root.harness.require(!fixture.requests(), "local executable used release metadata");
        root.harness.require(
            !baml.fs.exists(`${fixture.root}/${CHANNEL_STATE_FILE}`),
            "local launch created channel state",
        );
    }

    if (candidate.supports([root.harness.Capability.Windows])) {
        test "windows backslash path is launched" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["BAML_TOOLCHAIN"]
                = `${fixture.root}/project/build/baml-cli.exe`.replace_all("/", "\\");
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
        }

        test "windows local path infers executable suffix" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli`;
            fixture.check(candidate, PROJECT_TOOLCHAIN_IDENTITY);
            root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
        }
    }
}
