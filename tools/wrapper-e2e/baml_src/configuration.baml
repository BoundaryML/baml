// Configuration validation, preservation, and diagnostics.

function malformed_unused_update_setting_does_not_block_launch(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write("baml/config.toml", `[default]\nselector = "0.11.0"\n[update]\nauto_check = []\n`);
    fixture.check(candidate, "0.11.0");
    null
}

test "E01" {
    configured_case("E01", malformed_unused_update_setting_does_not_block_launch);
}

function malformed_unselected_channel_does_not_block_launch(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.write(
        "baml/state.toml",
        `[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "x"\nmanifest_path = "x"\n[channels.nightly]\nactive_version = []\n`,
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "E02" {
    configured_case("E02", malformed_unselected_channel_does_not_block_launch);
}

function malformed_unused_channel_metadata_does_not_block_launch(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.write(
        "baml/state.toml",
        `[channels.canary]\nactive_version = "0.11.0"\nresolved_at = []\nmanifest_path = 42\n`,
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "E02-metadata" {
    configured_case("E02-metadata", malformed_unused_channel_metadata_does_not_block_launch);
}

function config_writes_preserve_unknown_fields(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.write("baml/config.toml", `${config_before}\n[custom]\nvalue = 42\ndate = 2026-01-01\n`);
    fixture.write(
        "baml/state.toml",
        fixture.read("baml/state.toml") + "\n[custom]\nvalue = 43\ndate = 2026-01-01\n",
    );
    fixture.manage(candidate, ["toolchain", "use", "--local", "nightly"]);
    require(
        fixture.read("baml/config.toml").includes("value = 42")
            && fixture.read("baml/state.toml").includes("value = 43"),
        "unknown fields lost",
    );
    fixture.check(candidate, "0.12.0");
    null
}

test "E03" {
    configured_case("E03", config_writes_preserve_unknown_fields);
}

function failed_state_commit_preserves_channel_mapping(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.absent("0.12.0");
    // Prime metadata before denying the final state commit. Unique
    // temporary filenames must not be part of this failure fixture.
    fixture.manage(candidate, ["toolchain", "status"]);
    let blocked = if (fixture.windows) {
        "baml/state.toml"
    } else {
        "baml"
    };
    fixture.os(
        [
            "chmod",
            blocked,
            if (fixture.windows) {
                "444"
            } else {
                "555"
            },
        ],
    );
    let state = fixture.read("baml/state.toml");
    let output = fixture.run(candidate, ["toolchain", "update"]);
    fixture.os(
        [
            "chmod",
            blocked,
            if (fixture.windows) {
                "644"
            } else {
                "755"
            },
        ],
    );
    require(!output.ok(), "state commit unexpectedly succeeded");
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0/VERSION`),
        "update failed before installation",
    );
    require(fixture.read("baml/state.toml") == state, "failed commit changed channel");
    fixture.check(candidate, "0.11.0");
    null
}

test "E04" {
    configured_case("E04", failed_state_commit_preserves_channel_mapping);
}

function invalid_environment_version_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "not-a-version";
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "invalid managed version contacted server");
    null
}

test "E05-env" {
    configured_case("E05-env", invalid_environment_version_is_rejected);
}

function invalid_prerelease_version_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "1.2.3-01";
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "invalid managed version contacted server");
    null
}

test "E05-env-prerelease" {
    configured_case("E05-env-prerelease", invalid_prerelease_version_is_rejected);
}

function overflowing_version_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "18446744073709551616.0.0";
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "invalid managed version contacted server");
    null
}

test "E05-env-overflow" {
    configured_case("E05-env-overflow", overflowing_version_is_rejected);
}

function traversal_in_project_version_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("../../escape");
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "invalid managed version contacted server");
    null
}

test "E05-project" {
    configured_case("E05-project", traversal_in_project_version_is_rejected);
}

function traversal_in_recorded_channel_version_is_rejected(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.write(
        "baml/state.toml",
        "[channels.canary]\nactive_version = \"../../escape\"\nresolved_at = \"x\"\nmanifest_path = \"x\"\n",
    );
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "invalid managed version contacted server");
    null
}

test "E05-state" {
    configured_case("E05-state", traversal_in_recorded_channel_version_is_rejected);
}

function inconsistent_exact_version_manifest_is_rejected(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.write(`server/version/0.11.0.json`, fixture.read(`server/version/0.12.0.json`));
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "inconsistent manifest activated",
    );
    null
}

test "E06-version" {
    configured_case("E06-version", inconsistent_exact_version_manifest_is_rejected);
}

function inconsistent_channel_manifest_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.config("nightly");
    fixture.write("baml/state.toml", "");
    fixture.write("server/nightly.json", fixture.read("server/canary.json"));
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "inconsistent manifest activated",
    );
    null
}

test "E06-channel" {
    configured_case("E06-channel", inconsistent_channel_manifest_is_rejected);
}

function malformed_global_selector_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write("baml/config.toml", "[default]\nselector = []\n");
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    null
}

test "E07-config" {
    configured_case("E07-config", malformed_global_selector_is_rejected);
}

function malformed_active_channel_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");
    fixture.write("baml/state.toml", "[channels.canary]\nactive_version = []\n");
    failed_check(fixture, candidate);
    require(fixture.requests().length() == 0, "malformed resolution treated as absent");
    null
}

test "E07-state" {
    configured_case("E07-state", malformed_active_channel_is_rejected);
}

function local_use_validates_complete_config_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/config.toml";
    let content = "[default]\nselector = \"0.11.0\"\n[update]\nauto_check = []\n";
    let args = ["toolchain", "use", "--local", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E08" {
    configured_case("E08", local_use_validates_complete_config_before_writing);
}

function project_use_validates_package_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[package]\nname = 42\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09" {
    configured_case("E09", project_use_validates_package_before_writing);
}

function project_use_validates_dependencies_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[dependencies]\nhelper = { path = 42 }\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09-dependencies" {
    configured_case("E09-dependencies", project_use_validates_dependencies_before_writing);
}

function project_use_validates_scripts_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[scripts]\ndev = 42\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09-scripts" {
    configured_case("E09-scripts", project_use_validates_scripts_before_writing);
}

function project_use_validates_generator_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[generator.client]\noutput_type = 42\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09-generator" {
    configured_case("E09-generator", project_use_validates_generator_before_writing);
}

function project_use_validates_test_profile_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[test.profiles.ci]\nargs = 42\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09-test" {
    configured_case("E09-test", project_use_validates_test_profile_before_writing);
}

function project_use_rejects_date_in_string_field_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[generator.client]\noutput_type = 2026-01-01\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E09-date" {
    configured_case("E09-date", project_use_rejects_date_in_string_field_before_writing);
}

function project_use_cannot_overwrite_invalid_selector(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[toolchain]\nversion = []\n";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E10-project" {
    configured_case("E10-project", project_use_cannot_overwrite_invalid_selector);
}

function local_use_cannot_overwrite_invalid_selector(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/config.toml";
    let content = "[default]\nselector = []\n";
    let args = ["toolchain", "use", "--local", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E10-local" {
    configured_case("E10-local", local_use_cannot_overwrite_invalid_selector);
}

function update_validates_all_channels_before_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/state.toml";
    let content = `[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "x"\nmanifest_path = "x"\n[channels.nightly]\nactive_version = []\n`;
    let args = ["toolchain", "update"];

    fixture.absent("0.12.0");

    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E11-update" {
    configured_case("E11-update", update_validates_all_channels_before_installing);
}

function automatic_install_validates_all_channels_before_writing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/state.toml";
    let content = `[channels.canary]\nactive_version = "0.11.0"\nresolved_at = "x"\nmanifest_path = "x"\n[channels.nightly]\nactive_version = []\n`;
    let args = ["check"];

    fixture.absent("0.12.0");

    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);

    fixture.absent("0.11.0");
    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E11-auto" {
    configured_case("E11-auto", automatic_install_validates_all_channels_before_writing);
}

function local_use_rejects_config_syntax_before_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/config.toml";
    let content = "[invalid";
    let args = ["toolchain", "use", "--local", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E12-config" {
    configured_case("E12-config", local_use_rejects_config_syntax_before_installing);
}

function update_rejects_state_syntax_before_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "baml/state.toml";
    let content = "[invalid";
    let args = ["toolchain", "update"];

    fixture.absent("0.12.0");

    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E12-state" {
    configured_case("E12-state", update_rejects_state_syntax_before_installing);
}

function project_use_rejects_project_syntax_before_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let path = "project/baml.toml";
    let content = "[invalid";
    let args = ["toolchain", "use", "0.12.0"];

    fixture.absent("0.12.0");

    fixture.write(path, content);
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read(path) == content
            && fixture.requests().length() == 0
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "write did not preflight complete document",
    );
    null
}

test "E12-project" {
    configured_case("E12-project", project_use_rejects_project_syntax_before_installing);
}

function project_option_requires_value(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["--project"];
    let result = fixture.run(candidate, args);
    require(!result.ok(), "invalid project flags succeeded");
    require(
        fixture.calls().length() == 0 && fixture.requests().length() == 0,
        "invalid project flags caused side effects",
    );
    null
}

test "E13-project-missing" {
    configured_case("E13-project-missing", project_option_requires_value);
}

function project_option_requires_existing_project(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["--project", "../absent", "check"];
    let result = fixture.run(candidate, args);
    require(!result.ok(), "invalid project flags succeeded");
    require(
        fixture.calls().length() == 0 && fixture.requests().length() == 0,
        "invalid project flags caused side effects",
    );
    null
}

test "E13-project-absent" {
    configured_case("E13-project-absent", project_option_requires_existing_project);
}

function duplicate_project_option_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    let args = ["--project", ".", "--project", ".", "check"];
    let result = fixture.run(candidate, args);
    require(!result.ok(), "invalid project flags succeeded");
    require(
        fixture.calls().length() == 0 && fixture.requests().length() == 0,
        "invalid project flags caused side effects",
    );
    null
}

test "E13-project-duplicate" {
    configured_case("E13-project-duplicate", duplicate_project_option_is_rejected);
}

function errors_are_human_readable_and_keep_stdout_clean(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let output = fixture.manage(candidate, ["toolchain", "use", "./missing/baml-cli"], success = false);
    require(output.stderr.to_string().starts_with("error:"), "management error was not human-readable");
    require(output.stdout.to_string() == "", "management error polluted stdout");
    fixture.write("project/baml.toml", "[invalid");
    let malformed = fixture.manage(candidate, ["toolchain", "status"], success = false);
    let diagnostic = malformed.stderr.to_string();
    require(
        diagnostic.includes("error:") && diagnostic.includes("baml.toml"),
        "parse error omitted context",
    );
    require(!diagnostic.includes("{\"message\":"), "parse error serialized an error object");
    null
}

test "D01-human-error" {
    configured_case("D01-human-error", errors_are_human_readable_and_keep_stdout_clean);
}
