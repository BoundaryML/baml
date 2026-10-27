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
        test "parent components preserve symlink semantics" {
            let fixture = root.harness.Fixture.start();
            defer { fixture.close(); }
            fixture.symlink("project/link", "outside/build");
            fixture.env["BAML_TOOLCHAIN"] = "./link/../build/baml-cli";

            fixture.check(candidate, CWD_TOOLCHAIN_IDENTITY);
            root.harness.require(fixture.requests().length() == 0, "local toolchain contacted release server");
        }
    }

    test "relative paths written directly to user config resolve from each cwd" {
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
        fixture.failed_check(candidate, diagnostic = "BAML_TOOLCHAIN");
        root.harness.require(fixture.requests().length() == 0, "local failure contacted server");
    }

    test "local directory is rejected as a binary" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build`;
        fixture.failed_check(candidate, diagnostic = "directory");
        root.harness.require(fixture.requests().length() == 0, "local failure contacted server");
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
