// Version reporting invokes the selected CLI and propagates failures.

testset "version reporting" {
    let candidate = Candidate.from_environment();

    test "version reports wrapper and selected toolchain" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.behavior(BASE_VERSION, `${BASE_VERSION}\n`, args = ["--version"]);
        let output = fixture.run(candidate, ["--version"]);
        require(output.ok() && fixture.calls().length() == 1, "version did not invoke the selected CLI");
        require(
            output.stdout.to_string().includes("wrapper")
                && output.stdout.to_string().includes(BASE_VERSION),
            "version does not identify wrapper and toolchain",
        );
        if let expected: string = candidate.version {
            require(
                output.stdout.to_string().includes(`baml wrapper ${expected}`),
                "wrapper version differs from build version",
            );
        }
    }

    test "version installs and invokes selected toolchain" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.behavior(BASE_VERSION, "CLI version\n", code = 0, args = args);
        let result = fixture.run(candidate, args);
        require(result.exit_code == 0, "version hid CLI failure");
        require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == BASE_VERSION
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version propagates cli exit code" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.behavior(BASE_VERSION, "CLI version\n", code = 23, args = args);
        let result = fixture.run(candidate, args);
        require(result.exit_code == 23, "version hid CLI failure");
        require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == BASE_VERSION
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version invokes selected local cli" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        let args = ["-V"];
        fixture.absent(BASE_VERSION);
        fixture.project("./build/baml-cli", key = "path");
        fixture.behavior(PROJECT_TOOLCHAIN_IDENTITY, "CLI version\n", code = 0, args = args);
        let result = fixture.run(candidate, args);
        require(result.exit_code == 0, "version hid CLI failure");
        require(
            result.stdout.to_string().includes("baml wrapper")
                && result.stdout.to_string().includes("CLI version"),
            "version omitted wrapper or CLI output",
        );
        require(
            fixture.calls().length() == 1
                && fixture.calls()[0].identity == PROJECT_TOOLCHAIN_IDENTITY
                && fixture.calls()[0].args == args,
            "version did not invoke selected CLI with original flag",
        );
    }

    test "version rejects missing local cli" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.project("./missing/baml-cli", key = "path");
        let result = fixture.run(candidate, ["--version"]);
        require(
            !result.ok() && fixture.calls().length() == 0,
            "version hid selection or installation failure",
        );
    }

    test "version propagates install failure" {
        let fixture = Fixture.start();
        defer { fixture.close(); }
        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        let result = fixture.run(candidate, ["--version"]);
        require(
            !result.ok() && fixture.calls().length() == 0,
            "version hid selection or installation failure",
        );
    }
}
