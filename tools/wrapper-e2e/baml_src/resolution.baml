// Structured status resolves toolchains without launching them.

test "resolution: json status describes installed version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(
            result.schema_version == 1 && result.version == BASE_VERSION,
            "unexpected resolved version",
        );
        require(result.managed && result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
            "wrong toolchain path",
        );
        null
    });
}

test "resolution: json status reports failed best effort install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json"];

        fixture.absent(BASE_VERSION);
        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");

        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(
            result.schema_version == 1 && result.version == BASE_VERSION,
            "unexpected resolved version",
        );
        require(result.managed && !result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
            "wrong toolchain path",
        );
        null
    });
}

test "resolution: json status with install downloads absent version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.absent(BASE_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(
            result.schema_version == 1 && result.version == BASE_VERSION,
            "unexpected resolved version",
        );
        require(result.managed && result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "resolution: json status resolves recorded channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let args = ["toolchain", "status", "--json"];

        fixture.project("nightly", key = "channel");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(
            result.schema_version == 1 && result.version == NEXT_VERSION,
            "unexpected resolved version",
        );
        require(result.managed && result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
            "wrong toolchain path",
        );
        null
    });
}

test "resolution: json status with install records new channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let args = ["toolchain", "status", "--json", "--install"];

        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        fixture.absent(NEXT_VERSION);
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(
            result.schema_version == 1 && result.version == NEXT_VERSION,
            "unexpected resolved version",
        );
        require(result.managed && result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/baml/toolchains/${NEXT_VERSION}/bin/baml-cli${fixture.suffix}`;
        require(
            baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() > 0, "status --install did not download");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "resolution: json status describes local binary without launching" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        let output = fixture.manage(candidate, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(result.schema_version == 1 && result.version == null, "unexpected resolved version");
        require(!result.managed && result.installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
        require(
            baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
            "wrong toolchain path",
        );
        require(fixture.requests().length() == 0, "local status used network");
        null
    });
}

test "resolution: json status rejects missing local binary" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let args = ["toolchain", "status", "--json"];

        fixture.project("./build/baml-cli", key = "path");
        fixture.remove(`project/build/baml-cli${fixture.suffix}`);
        fixture.manage(candidate, args, success = false, diagnostic = "not found");
        require(fixture.requests().length() == 0, "invalid local resolution used network");
        null
    });
}

test "resolution: status help does not contact release server" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);

        let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
        require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
        require(fixture.requests().length() == 0, "resolve help used network");
        null
    });
}

test "resolution: plain status reports selection and latest releases" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(BASE_VERSION);
        let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
        require(
            output.includes("active selector:") && output.includes("latest canary:"),
            "plain status omitted selection or releases",
        );
        null
    });
}

test "resolution: json status installs unrecorded channel" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let args = ["toolchain", "status", "--json"];
        fixture.project("nightly", key = "channel");
        fixture.remove(CHANNEL_STATE_FILE);
        let result = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
        require(result.version == NEXT_VERSION && result.installed, "unrecorded channel was not installed");
        require(
            fixture.read(CHANNEL_STATE_FILE).includes(NEXT_VERSION),
            "channel mapping was not recorded",
        );
        null
    });
}
