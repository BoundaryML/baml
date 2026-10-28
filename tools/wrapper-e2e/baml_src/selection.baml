// Selector precedence and project discovery.

function global_version_is_used_without_project_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    check_installed_selection(fixture, candidate, "0.11.0");
    null
}

test "S01" {
    configured_case("S01", global_version_is_used_without_project_selection);
}

function global_canary_uses_its_recorded_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    check_installed_selection(fixture, candidate, "0.11.0");
    null
}

test "S02" {
    configured_case("S02", global_canary_uses_its_recorded_version);
}

function global_nightly_uses_its_recorded_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("nightly");

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S03" {
    configured_case("S03", global_nightly_uses_its_recorded_version);
}

function project_version_overrides_global_default(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.12.0");

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S04" {
    configured_case("S04", project_version_overrides_global_default);
}

function environment_version_overrides_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.env["BAML_TOOLCHAIN"] = "0.12.0";

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S05" {
    configured_case("S05", environment_version_overrides_project);
}

function environment_channel_overrides_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.env["BAML_TOOLCHAIN"] = "nightly";

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S06" {
    configured_case("S06", environment_channel_overrides_project);
}

function environment_path_overrides_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

    check_installed_selection(fixture, candidate, "L1");
    null
}

test "S07" {
    configured_case("S07", environment_path_overrides_project);
}

function empty_environment_falls_back_to_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.12.0");
    fixture.env["BAML_TOOLCHAIN"] = "";

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S08-empty" {
    configured_case("S08-empty", empty_environment_falls_back_to_project);
}

function whitespace_environment_falls_back_to_project(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.12.0");
    fixture.env["BAML_TOOLCHAIN"] = " \t ";

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S08-space" {
    configured_case("S08-space", whitespace_environment_falls_back_to_project);
}

function environment_selector_is_trimmed(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.12.0");
    fixture.env["BAML_TOOLCHAIN"] = ` 0.12.0 \n`;

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S09" {
    configured_case("S09", environment_selector_is_trimmed);
}

function project_is_discovered_from_nested_directory(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.12.0");
    fixture.cwd = `${fixture.root}/project/nested/deep`;

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S10" {
    configured_case("S10", project_is_discovered_from_nested_directory);
}

function closest_project_selector_wins(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.write("project/nested/deep/baml.toml", `[toolchain]\nversion = "0.12.0"\n`);
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.config("0.12.0");

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S11" {
    configured_case("S11", closest_project_selector_wins);
}

function unpinned_closest_project_uses_global_default(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.config("0.12.0");

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S12" {
    configured_case("S12", unpinned_closest_project_uses_global_default);
}

function absent_global_default_falls_back_to_canary(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");

    fixture.write("baml/config.toml", "[update]\nauto_check = false\n");
    check_installed_selection(fixture, candidate, "0.11.0");
    null
}

test "S13" {
    configured_case("S13", absent_global_default_falls_back_to_canary);
}

function malformed_project_syntax_rejects_launch(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("project/baml.toml", "[invalid");
    failed_check(fixture, candidate, diagnostic = "baml.toml");
    require(fixture.requests().length() == 0, "invalid project used network");
    null
}

test "S15-syntax" {
    configured_case("S15-syntax", malformed_project_syntax_rejects_launch);
}

function malformed_project_selector_rejects_launch(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("project/baml.toml", "[toolchain]\nversion = 42\n");
    failed_check(fixture, candidate, diagnostic = "baml.toml");
    require(fixture.requests().length() == 0, "invalid project used network");
    null
}

test "S15-field" {
    configured_case("S15-field", malformed_project_selector_rejects_launch);
}

function version_takes_priority_over_other_selectors(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write(
        "project/baml.toml",
        `[toolchain]\nversion = "0.11.0"\nchannel = "nightly"\npath = "./build/baml-cli"\n`,
    );
    check_installed_selection(fixture, candidate, "0.11.0");
    null
}

test "S16-valid" {
    configured_case("S16-valid", version_takes_priority_over_other_selectors);
}

function unused_project_selectors_are_not_validated(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write(
        "project/baml.toml",
        `[toolchain]\nversion = "0.11.0"\nchannel = 42\npath = false\n`,
    );
    check_installed_selection(fixture, candidate, "0.11.0");
    null
}

test "S16-invalid" {
    configured_case("S16-invalid", unused_project_selectors_are_not_validated);
}

function environment_bypasses_malformed_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("project/baml.toml", "[invalid");
    fixture.env["BAML_TOOLCHAIN"] = "0.12.0";
    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S17" {
    configured_case("S17", environment_bypasses_malformed_project);
}

function selection_is_rediscovered_after_changing_directory(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.check(candidate, "0.11.0");
    fixture.write("outside/baml.toml", `[toolchain]\nversion = "0.12.0"\n`);
    fixture.cwd = `${fixture.root}/outside`;
    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S18" {
    configured_case("S18", selection_is_rediscovered_after_changing_directory);
}

function unpinned_closest_project_uses_default_canary(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.write("baml/config.toml", "[update]\nauto_check = false\n");
    fixture.write(
        "baml/state.toml",
        `[channels.canary]\nactive_version = "0.12.0"\nresolved_at = "x"\nmanifest_path = "x"\n`,
    );

    check_installed_selection(fixture, candidate, "0.12.0");
    null
}

test "S19" {
    configured_case("S19", unpinned_closest_project_uses_default_canary);
}

function project_prefix_selects_another_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["--project", "../outside", "check"];

    fixture.behavior("0.12.0", `0.12.0\n`, args = args);
    fixture.check(candidate, "0.12.0", args = args);
    null
}

test "S20-project-prefix" {
    configured_case("S20-project-prefix", project_prefix_selects_another_project);
}

function inline_project_option_selects_another_project(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["check", "--project=../outside"];

    fixture.behavior("0.12.0", `0.12.0\n`, args = args);
    fixture.check(candidate, "0.12.0", args = args);
    null
}

test "S21-project-inline" {
    configured_case("S21-project-inline", inline_project_option_selects_another_project);
}

function directory_option_is_forwarded_without_affecting_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["--directory", "../outside", "check"];

    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S22-directory" {
    configured_case("S22-directory", directory_option_is_forwarded_without_affecting_selection);
}

function environment_overrides_explicit_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["--project", "../outside", "check"];
    fixture.env["BAML_TOOLCHAIN"] = "0.11.0";

    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S23-project-env" {
    configured_case("S23-project-env", environment_overrides_explicit_project);
}

function project_option_after_separator_is_forwarded(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["run", "main", "--", "--project", "../outside"];

    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S24-target-args" {
    configured_case("S24-target-args", project_option_after_separator_is_forwarded);
}

function query_project_argument_does_not_select_local_project(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["query", "--project", "org/cloud-project"];
    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S25-query-project" {
    configured_case("S25-query-project", query_project_argument_does_not_select_local_project);
}

function project_option_is_independent_of_forwarded_directory(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );

    let args = ["--directory=../outside", "--project=../project", "check"];
    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S26-directory-project" {
    configured_case("S26-directory-project", project_option_is_independent_of_forwarded_directory);
}

function use_updates_the_explicit_project(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );
    let args = ["--project", "../outside", "check"];

    fixture.manage(candidate, ["--project", "../outside", "toolchain", "use", "0.13.0"]);
    require(fixture.read("project/baml.toml").includes("0.11.0"), "use changed cwd project");
    fixture.behavior("0.13.0", `0.13.0\n`, args = args);
    fixture.check(candidate, "0.13.0", args = args);
    null
}

test "S27-project-management" {
    configured_case("S27-project-management", use_updates_the_explicit_project);
}

function query_project_is_forwarded_after_global_options(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );

    let args = ["--output-preset", "agent", "query", "--project", "org/cloud-project"];
    fixture.behavior("0.11.0", `0.11.0\n`, args = args);
    fixture.check(candidate, "0.11.0", args = args);
    null
}

test "S28-query-global-option" {
    configured_case("S28-query-global-option", query_project_is_forwarded_after_global_options);
}

function directory_option_does_not_rebase_environment_path(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    fixture.write(
        "outside/baml.toml",
        "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
    );

    let args = ["--directory", "../outside", "check"];
    fixture.env["BAML_TOOLCHAIN"] = "./build/baml-cli";
    fixture.behavior("L1", `L1\n`, args = args);
    fixture.check(candidate, "L1", args = args);
    null
}

test "S29-directory-env-relative" {
    configured_case("S29-directory-env-relative", directory_option_does_not_rebase_environment_path);
}

function project_search_stops_at_home_directory(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("baml.toml", "[toolchain]\nversion = \"0.12.0\"\n");
    fixture.cwd = `${fixture.root}/home/nested/deep`;
    baml.fs.mkdir(fixture.cwd, baml.fs.MkdirOptions { recursive: true });
    if (fixture.windows) {
        fixture.env["HOME"] = `${fixture.root}/home`.replace_all("/", "\\");
        fixture.env["USERPROFILE"] = fixture.env["HOME"];
    }
    fixture.check(candidate, "0.11.0");
    let before = fixture.read("baml.toml");
    fixture.manage(candidate, ["toolchain", "use", "0.12.0"], success = false);
    require(fixture.read("baml.toml") == before, "use crossed the home-directory boundary");
    require(fixture.requests().length() == 0, "home boundary check used network");
    null
}

test "S30-home-boundary" {
    configured_case("S30-home-boundary", project_search_stops_at_home_directory);
}

function explicit_unpinned_project_uses_global_default(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.12.0");
    for (let flag in ["--project"]) {
        let args = [flag, "../outside", "check"];
        fixture.behavior("0.11.0", "0.11.0\n", args = args);
        fixture.check(candidate, "0.11.0", args = args);
    }
    null
}

test "S31-relative-unpinned" {
    configured_case("S31-relative-unpinned", explicit_unpinned_project_uses_global_default);
}

// Installed selection must preserve selectors and avoid release requests.
function check_installed_selection(
    fixture: Fixture,
    candidate: Candidate,
    identity: string,
) -> null {
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.check(candidate, identity);
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "launch changed selectors",
    );
    require(fixture.requests().length() == 0, "installed selection contacted release server");
    null
}
