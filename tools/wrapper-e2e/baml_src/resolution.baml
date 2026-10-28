// Structured status resolves toolchains without launching them.

function json_status_describes_installed_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let args = ["toolchain", "status", "--json"];

    let output = fixture.manage(candidate, args);
    let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
    require(result.schema_version == 1 && result.version == "0.11.0", "unexpected resolved version");
    require(result.managed && result.installed, "unexpected resolution state");
    require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
    require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
    let binary = `${fixture.root}/baml/toolchains/0.11.0/bin/baml-cli${fixture.suffix}`;
    require(
        baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
        "wrong toolchain path",
    );
    null
}

test "R01-installed" {
    configured_case("R01-installed", json_status_describes_installed_version);
}

function json_status_reports_failed_best_effort_install(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let args = ["toolchain", "status", "--json"];

    fixture.absent("0.11.0");
    fixture.write("server/version/0.11.0.json.status", "500");

    let output = fixture.manage(candidate, args);
    let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
    require(result.schema_version == 1 && result.version == "0.11.0", "unexpected resolved version");
    require(result.managed && !result.installed, "unexpected resolution state");
    require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
    require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
    let binary = `${fixture.root}/baml/toolchains/0.11.0/bin/baml-cli${fixture.suffix}`;
    require(
        result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/"),
        "wrong toolchain path",
    );
    null
}

test "R02-absent" {
    configured_case("R02-absent", json_status_reports_failed_best_effort_install);
}

function json_status_with_install_downloads_absent_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let args = ["toolchain", "status", "--json", "--install"];

    fixture.absent("0.11.0");
    let output = fixture.manage(candidate, args);
    let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
    require(result.schema_version == 1 && result.version == "0.11.0", "unexpected resolved version");
    require(result.managed && result.installed, "unexpected resolution state");
    require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
    require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
    let binary = `${fixture.root}/baml/toolchains/0.11.0/bin/baml-cli${fixture.suffix}`;
    require(
        baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
        "wrong toolchain path",
    );
    require(fixture.requests().length() > 0, "status --install did not download");
    fixture.check(candidate, "0.11.0");
    null
}

test "R03-install" {
    configured_case("R03-install", json_status_with_install_downloads_absent_version);
}

function json_status_resolves_recorded_channel(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["toolchain", "status", "--json"];

    fixture.project("nightly", key = "channel");
    let output = fixture.manage(candidate, args);
    let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
    require(result.schema_version == 1 && result.version == "0.12.0", "unexpected resolved version");
    require(result.managed && result.installed, "unexpected resolution state");
    require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
    require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
    let binary = `${fixture.root}/baml/toolchains/0.12.0/bin/baml-cli${fixture.suffix}`;
    require(
        baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
        "wrong toolchain path",
    );
    null
}

test "R04-channel" {
    configured_case("R04-channel", json_status_resolves_recorded_channel);
}

function json_status_with_install_records_new_channel(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let args = ["toolchain", "status", "--json", "--install"];

    fixture.project("nightly", key = "channel");
    fixture.os(["remove", "baml/state.toml"]);
    fixture.absent("0.12.0");
    let output = fixture.manage(candidate, args);
    let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
    require(result.schema_version == 1 && result.version == "0.12.0", "unexpected resolved version");
    require(result.managed && result.installed, "unexpected resolution state");
    require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
    require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
    let binary = `${fixture.root}/baml/toolchains/0.12.0/bin/baml-cli${fixture.suffix}`;
    require(
        baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary),
        "wrong toolchain path",
    );
    require(fixture.requests().length() > 0, "status --install did not download");
    fixture.check(candidate, "0.12.0");
    null
}

test "R05-channel-install" {
    configured_case("R05-channel-install", json_status_with_install_records_new_channel);
}

function json_status_describes_local_binary_without_launching(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
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
}

test "R06-local" {
    configured_case("R06-local", json_status_describes_local_binary_without_launching);
}

function json_status_rejects_missing_local_binary(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["toolchain", "status", "--json"];

    fixture.project("./build/baml-cli", key = "path");
    fixture.os(["remove", `project/build/baml-cli${fixture.suffix}`]);
    fixture.manage(candidate, args, success = false, diagnostic = "not found");
    require(fixture.requests().length() == 0, "invalid local resolution used network");
    null
}

test "R07-invalid-local" {
    configured_case("R07-invalid-local", json_status_rejects_missing_local_binary);
}

function status_help_does_not_contact_release_server(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");

    let output = fixture.manage(candidate, ["toolchain", "status", "--help"]);
    require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
    require(fixture.requests().length() == 0, "resolve help used network");
    null
}

test "R08-help" {
    configured_case("R08-help", status_help_does_not_contact_release_server);
}

function plain_status_reports_selection_and_latest_releases(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let output = fixture.manage(candidate, ["toolchain", "status"]).stdout.to_string();
    require(
        output.includes("active selector:") && output.includes("latest canary:"),
        "plain status omitted selection or releases",
    );
    null
}

test "R09-default-json" {
    configured_case("R09-default-json", plain_status_reports_selection_and_latest_releases);
}

function json_status_installs_unrecorded_channel(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["toolchain", "status", "--json"];
    fixture.project("nightly", key = "channel");
    fixture.os(["remove", "baml/state.toml"]);
    let result = baml.json.decode<ResolutionResult>(fixture.manage(candidate, args).stdout.to_string());
    require(result.version == "0.12.0" && result.installed, "unrecorded channel was not installed");
    require(fixture.read("baml/state.toml").includes("0.12.0"), "channel mapping was not recorded");
    null
}

test "R10-unrecorded-channel" {
    configured_case("R10-unrecorded-channel", json_status_installs_unrecorded_channel);
}
