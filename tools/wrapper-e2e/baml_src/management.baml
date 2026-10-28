// Project/global selection, updates, removal, and management preflight.

function use_updates_project_and_preserves_global_default(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("0.12.0");
    let before = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "use", "0.11.0"]);
    require(fixture.read("baml/config.toml") == before, "project use changed user config");
    require(fixture.read("project/baml.toml").includes("0.11.0"), "project not selected");
    fixture.check(candidate, "0.11.0");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "0.12.0");
    null
}

test "M01" {
    configured_case("M01", use_updates_project_and_preserves_global_default);
}

function use_nightly_installs_and_records_channel(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.write("baml/state.toml", "");
    fixture.absent("0.12.0");
    fixture.manage(candidate, ["toolchain", "use", "nightly"]);
    require(fixture.read("baml/config.toml") == config_before, "channel use changed global default");
    require(channel_version(fixture, "nightly") == "0.12.0", "channel not recorded");
    require(fixture.read("project/baml.toml").includes("nightly"), "channel selector not written");
    fixture.check(candidate, "0.12.0");
    null
}

test "M02" {
    configured_case("M02", use_nightly_installs_and_records_channel);
}

function local_use_resolves_path_from_cwd(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let before = fixture.read("project/baml.toml");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.manage(candidate, ["toolchain", "use", "--local", `./build/baml-cli${fixture.suffix}`]);
    require(fixture.read("project/baml.toml") == before, "--local changed project");
    fixture.check(candidate, "0.11.0");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "L2");
    null
}

test "M03" {
    configured_case("M03", local_use_resolves_path_from_cwd);
}

function local_use_preserves_project_selection(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let before = fixture.read("project/baml.toml");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.manage(candidate, ["toolchain", "use", "--local", "0.12.0"]);
    require(fixture.read("project/baml.toml") == before, "--local changed project");
    fixture.check(candidate, "0.11.0");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "0.12.0");
    null
}

test "M04" {
    configured_case("M04", local_use_preserves_project_selection);
}

function use_installs_absent_project_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("0.12.0");
    fixture.absent("0.11.0");
    let before = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "use", "0.11.0"]);
    require(fixture.read("baml/config.toml") == before, "project use changed user config");
    require(fixture.read("project/baml.toml").includes("0.11.0"), "project not selected");
    fixture.check(candidate, "0.11.0");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "0.12.0");
    null
}

test "M05" {
    configured_case("M05", use_installs_absent_project_version);
}

function use_canary_installs_and_records_channel(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.write("baml/state.toml", "");
    fixture.absent("0.11.0");
    fixture.manage(candidate, ["toolchain", "use", "canary"]);
    require(fixture.read("baml/config.toml") == config_before, "channel use changed global default");
    require(channel_version(fixture, "canary") == "0.11.0", "channel not recorded");
    require(fixture.read("project/baml.toml").includes("canary"), "channel selector not written");
    fixture.check(candidate, "0.11.0");
    null
}

test "M06-canary" {
    configured_case("M06-canary", use_canary_installs_and_records_channel);
}

function use_nightly_writes_project_channel(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.write("baml/state.toml", "");
    fixture.absent("0.12.0");
    fixture.manage(candidate, ["toolchain", "use", "nightly"]);
    require(fixture.read("baml/config.toml") == config_before, "channel use changed global default");
    require(channel_version(fixture, "nightly") == "0.12.0", "channel not recorded");
    require(fixture.read("project/baml.toml").includes("nightly"), "channel selector not written");
    fixture.check(candidate, "0.12.0");
    null
}

test "M06-nightly" {
    configured_case("M06-nightly", use_nightly_writes_project_channel);
}

function project_use_resolves_path_from_project_root(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.manage(candidate, ["toolchain", "use", `./build/baml-cli${fixture.suffix}`]);
    fixture.check(candidate, "L1");
    fixture.cwd = `${fixture.root}/project`;
    fixture.check(candidate, "L1");
    null
}

test "M07" {
    configured_case("M07", project_use_resolves_path_from_project_root);
}

function use_replaces_version_and_preserves_comments(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write(
        "project/baml.toml",
        `# retain me\n[package]\nname = "fixture"\n[toolchain]\nversion = "0.11.0" # selector comment\n[custom]\nvalue = 42\ndate = 2026-01-01\n`,
    );
    fixture.manage(candidate, ["toolchain", "use", "0.12.0"]);
    let updated = fixture.read("project/baml.toml");
    require(
        updated.includes("retain me")
            && updated.includes("selector comment")
            && updated.includes("value = 42"),
        "comments or unknown data lost",
    );
    require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
    fixture.check(candidate, "0.12.0");
    null
}

test "M08-version" {
    configured_case("M08-version", use_replaces_version_and_preserves_comments);
}

function use_replaces_channel_and_preserves_comments(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write(
        "project/baml.toml",
        `# retain me\n[package]\nname = "fixture"\n[toolchain]\nchannel = "canary" # selector comment\n[custom]\nvalue = 42\ndate = 2026-01-01\n`,
    );
    fixture.manage(candidate, ["toolchain", "use", "0.12.0"]);
    let updated = fixture.read("project/baml.toml");
    require(
        updated.includes("retain me")
            && updated.includes("selector comment")
            && updated.includes("value = 42"),
        "comments or unknown data lost",
    );
    require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
    fixture.check(candidate, "0.12.0");
    null
}

test "M08-channel" {
    configured_case("M08-channel", use_replaces_channel_and_preserves_comments);
}

function use_replaces_path_and_preserves_comments(fixture: Fixture, candidate: Candidate) -> null {
    fixture.write(
        "project/baml.toml",
        `# retain me\n[package]\nname = "fixture"\n[toolchain]\npath = "./build/baml-cli" # selector comment\n[custom]\nvalue = 42\ndate = 2026-01-01\n`,
    );
    fixture.manage(candidate, ["toolchain", "use", "0.12.0"]);
    let updated = fixture.read("project/baml.toml");
    require(
        updated.includes("retain me")
            && updated.includes("selector comment")
            && updated.includes("value = 42"),
        "comments or unknown data lost",
    );
    require(!updated.includes("channel =") && !updated.includes("path ="), "multiple selectors remain");
    fixture.check(candidate, "0.12.0");
    null
}

test "M08-path" {
    configured_case("M08-path", use_replaces_path_and_preserves_comments);
}

function project_use_outside_project_requires_local_flag(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.cwd = `${fixture.root}/outside`;
    fixture.absent("0.13.0");
    fixture.manage(
        candidate,
        ["toolchain", "use", "0.13.0"],
        success = false,
        diagnostic = "--local",
    );
    require(
        fixture.read("baml/config.toml") == config_before
            && !baml.fs.exists(`${fixture.root}/outside/baml.toml`)
            && fixture.requests().length() == 0,
        "use outside project mutated state",
    );
    null
}

test "M09" {
    configured_case("M09", project_use_outside_project_requires_local_flag);
}

function failed_project_use_download_preserves_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.project("0.11.0");
    fixture.absent("0.12.0");
    let before = fixture.read("project/baml.toml");
    let args = ["toolchain", "use"];
    args.push("0.12.0");
    fixture.write(`server/version/0.12.0.json.status`, "500");
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read("project/baml.toml") == before
            && fixture.read("baml/config.toml") == config_before,
        "failed use changed selectors",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M10-project-download" {
    configured_case("M10-project-download", failed_project_use_download_preserves_selection);
}

function failed_local_use_download_preserves_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.project("0.11.0");
    fixture.absent("0.12.0");
    let before = fixture.read("project/baml.toml");
    let args = ["toolchain", "use"];
    args.push("--local");
    args.push("0.12.0");
    fixture.write(`server/version/0.12.0.json.status`, "500");
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read("project/baml.toml") == before
            && fixture.read("baml/config.toml") == config_before,
        "failed use changed selectors",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M10-local-download" {
    configured_case("M10-local-download", failed_local_use_download_preserves_selection);
}

function failed_project_use_path_preserves_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.project("0.11.0");
    fixture.absent("0.12.0");
    let before = fixture.read("project/baml.toml");
    let args = ["toolchain", "use"];
    args.push("./missing/baml-cli");
    fixture.write(`server/version/0.12.0.json.status`, "500");
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read("project/baml.toml") == before
            && fixture.read("baml/config.toml") == config_before,
        "failed use changed selectors",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M10-project-path" {
    configured_case("M10-project-path", failed_project_use_path_preserves_selection);
}

function failed_local_use_path_preserves_selection(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.project("0.11.0");
    fixture.absent("0.12.0");
    let before = fixture.read("project/baml.toml");
    let args = ["toolchain", "use"];
    args.push("--local");
    args.push("./missing/baml-cli");
    fixture.write(`server/version/0.12.0.json.status`, "500");
    fixture.manage(candidate, args, success = false);
    require(
        fixture.read("project/baml.toml") == before
            && fixture.read("baml/config.toml") == config_before,
        "failed use changed selectors",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M10-local-path" {
    configured_case("M10-local-path", failed_local_use_path_preserves_selection);
}

function update_advances_global_channel(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.absent("0.12.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "canary") == "0.12.0", "wrong shared channel updated");
    fixture.check(candidate, "0.12.0");
    null
}

test "M11" {
    configured_case("M11", update_advances_global_channel);
}

function failed_update_preserves_usable_toolchain(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    fixture.os(["channel", "canary", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.write(`server/canary.json.status`, "500");
    fixture.manage(candidate, ["toolchain", "update"], success = false);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M13" {
    configured_case("M13", failed_update_preserves_usable_toolchain);
}

function update_does_not_advance_environment_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "0.11.0";
    let state = fixture.read("baml/state.toml");
    fixture.manage(
        candidate,
        ["toolchain", "update"],
        success = true,
        diagnostic = "does not advance",
    );
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-env-version" {
    configured_case("M14-env-version", update_does_not_advance_environment_version);
}

function update_does_not_advance_environment_path(fixture: Fixture, candidate: Candidate) -> null {
    let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
    fixture.env["BAML_TOOLCHAIN"] = selected;
    let state = fixture.read("baml/state.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true, diagnostic = "local path");
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-env-path" {
    configured_case("M14-env-path", update_does_not_advance_environment_path);
}

function update_does_not_advance_project_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0", key = "version");
    let state = fixture.read("baml/state.toml");
    fixture.manage(
        candidate,
        ["toolchain", "update"],
        success = true,
        diagnostic = "does not advance",
    );
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-project-version" {
    configured_case("M14-project-version", update_does_not_advance_project_version);
}

function update_does_not_advance_project_path(fixture: Fixture, candidate: Candidate) -> null {
    let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
    fixture.project(selected, key = "path");
    let state = fixture.read("baml/state.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true, diagnostic = "local path");
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-project-path" {
    configured_case("M14-project-path", update_does_not_advance_project_path);
}

function update_does_not_advance_global_version(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("0.11.0");
    let state = fixture.read("baml/state.toml");
    fixture.manage(
        candidate,
        ["toolchain", "update"],
        success = true,
        diagnostic = "does not advance",
    );
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-global-version" {
    configured_case("M14-global-version", update_does_not_advance_global_version);
}

function update_does_not_advance_global_path(fixture: Fixture, candidate: Candidate) -> null {
    let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
    fixture.config(selected);
    let state = fixture.read("baml/state.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true, diagnostic = "local path");
    require(
        fixture.requests().length() == 0 && fixture.read("baml/state.toml") == state,
        "non-channel update performed work",
    );
    null
}

test "M14-global-path" {
    configured_case("M14-global-path", update_does_not_advance_global_path);
}

function update_advances_project_channel_only(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    fixture.project("nightly", key = "channel");
    fixture.os(["channel", "nightly", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "nightly") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    require(channel_version(fixture, "canary") == "0.11.0", "unselected channel changed");
    null
}

test "M15-project" {
    configured_case("M15-project", update_advances_project_channel_only);
}

function update_advances_environment_channel_only(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    fixture.env["BAML_TOOLCHAIN"] = "nightly";
    fixture.os(["channel", "nightly", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "nightly") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    require(channel_version(fixture, "canary") == "0.11.0", "unselected channel changed");
    null
}

test "M15-env" {
    configured_case("M15-env", update_advances_environment_channel_only);
}

function uninstall_removes_unused_version(fixture: Fixture, candidate: Candidate) -> null {
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "uninstall", "0.12.0"], success = true);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "uninstall did not remove directory",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M16" {
    configured_case("M16", uninstall_removes_unused_version);
}

function uninstall_rejects_absent_version(fixture: Fixture, candidate: Candidate) -> null {
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "uninstall", "9.9.9"], success = false);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M17-absent" {
    configured_case("M17-absent", uninstall_rejects_absent_version);
}

function uninstall_rejects_local_path(fixture: Fixture, candidate: Candidate) -> null {
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    let selected = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;
    fixture.manage(candidate, ["toolchain", "uninstall", selected], success = false);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M17-path" {
    configured_case("M17-path", uninstall_rejects_local_path);
}

function uninstall_rejects_channel_name(fixture: Fixture, candidate: Candidate) -> null {
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "uninstall", "canary"], success = false);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M17-channel" {
    configured_case("M17-channel", uninstall_rejects_channel_name);
}

function uninstalled_selected_version_is_reinstalled_on_launch(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "uninstall", "0.11.0"], success = true);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0`),
        "uninstall did not remove directory",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M18-version" {
    configured_case("M18-version", uninstalled_selected_version_is_reinstalled_on_launch);
}

function uninstalled_channel_version_is_reinstalled_without_advancing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);
    let state = fixture.read("baml/state.toml");
    let config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "uninstall", "0.11.0"], success = true);
    require(
        fixture.read("baml/state.toml") == state && fixture.read("baml/config.toml") == config,
        "uninstall changed selection or mapping",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0`),
        "uninstall did not remove directory",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M18-channel" {
    configured_case("M18-channel", uninstalled_channel_version_is_reinstalled_without_advancing);
}

function list_installs_absent_active_toolchain(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.os(["remove", "baml/toolchains"]);
    let before = fixture.read("baml/state.toml");
    let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
    require(listing.includes(`default selector: 0.11.0`), "list omitted the user default");
    require(listing.includes(`  0.11.0`), "list did not install the absent active toolchain");
    require(
        fixture.read("baml/config.toml") == config_before
            && fixture.read("baml/state.toml") == before,
        "list changed state or downloaded",
    );
    null
}

test "M19-empty" {
    configured_case("M19-empty", list_installs_absent_active_toolchain);
}

function list_reports_installed_toolchains(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    let before = fixture.read("baml/state.toml");
    let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
    require(listing.includes(`default selector: 0.11.0`), "list omitted the user default");
    require(listing.includes("0.12.0") && listing.includes("0.13.0"), "list omitted installed versions");
    require(
        fixture.requests().length() == 0
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("baml/state.toml") == before,
        "list changed state or downloaded",
    );
    null
}

test "M19-installed" {
    configured_case("M19-installed", list_reports_installed_toolchains);
}

function list_reports_environment_override(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.env["BAML_TOOLCHAIN"] = "0.12.0";
    let before = fixture.read("baml/state.toml");
    let listing = fixture.manage(candidate, ["toolchain", "list"]).stdout.to_string();
    require(listing.includes(`default selector: 0.11.0`), "list omitted the user default");
    require(listing.includes("0.12.0") && listing.includes("0.13.0"), "list omitted installed versions");
    require(listing.includes(`active selector: 0.12.0`), "list omitted the active override");
    require(
        fixture.requests().length() == 0
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("baml/state.toml") == before,
        "list changed state or downloaded",
    );
    null
}

test "M19-override" {
    configured_case("M19-override", list_reports_environment_override);
}

function status_reports_available_update_without_installing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.absent("0.12.0");
    fixture.os(["channel", "canary", "0.12.0"]);
    let before = fixture.read("baml/state.toml");
    fixture.manage(
        candidate,
        ["toolchain", "status"],
        success = true,
        diagnostic = "update available",
    );
    require(
        fixture.read("baml/state.toml") == before
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "status installed or changed state",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M20" {
    configured_case("M20", status_reports_available_update_without_installing);
}

function failed_status_preserves_selection(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");
    fixture.absent("0.12.0");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.write("server/canary.json.status", "500");
    let before = fixture.read("baml/state.toml");
    fixture.manage(candidate, ["toolchain", "status"], success = false, diagnostic = "");
    require(
        fixture.read("baml/state.toml") == before
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0`),
        "status installed or changed state",
    );
    fixture.check(candidate, "0.11.0");
    null
}

test "M21" {
    configured_case("M21", failed_status_preserves_selection);
}

function environment_channel_update_preserves_other_channels(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");
    fixture.config("nightly");
    fixture.project("nightly", key = "channel");
    fixture.env["BAML_TOOLCHAIN"] = "canary";
    fixture.os(["channel", "canary", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "canary") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    require(channel_version(fixture, "nightly") == "0.12.0", "unselected channel changed");
    null
}

test "M22" {
    configured_case("M22", environment_channel_update_preserves_other_channels);
}

function update_uses_canary_when_no_default_is_configured(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");

    fixture.write("baml/config.toml", "[update]\nauto_check = false\n");
    fixture.os(["channel", "canary", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "canary") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    null
}

test "M23" {
    configured_case("M23", update_uses_canary_when_no_default_is_configured);
}

function update_ignores_parent_of_unpinned_closest_project(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.config("canary");

    fixture.project("nightly", key = "channel");
    fixture.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.os(["channel", "canary", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "canary") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    null
}

test "M24" {
    configured_case("M24", update_ignores_parent_of_unpinned_closest_project);
}

function channel_update_is_shared_across_projects(fixture: Fixture, candidate: Candidate) -> null {
    fixture.config("canary");

    fixture.project("nightly", key = "channel");
    fixture.write("outside/baml.toml", "[toolchain]\nchannel = \"nightly\"\n");
    fixture.os(["channel", "nightly", "0.13.0"]);
    fixture.absent("0.13.0");
    let before_project = fixture.read("project/baml.toml");
    let before_config = fixture.read("baml/config.toml");
    fixture.manage(candidate, ["toolchain", "update"], success = true);
    require(
        fixture.read("project/baml.toml") == before_project
            && fixture.read("baml/config.toml") == before_config,
        "update changed selector",
    );
    require(channel_version(fixture, "nightly") == "0.13.0", "wrong shared channel updated");
    fixture.check(candidate, "0.13.0");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "0.13.0");
    null
}

test "M25" {
    configured_case("M25", channel_update_is_shared_across_projects);
}

function local_use_bootstraps_explicit_baml_home(fixture: Fixture, candidate: Candidate) -> null {
    fixture.cwd = `${fixture.root}/outside`;
    fixture.absent("0.12.0");
    fixture.manage(candidate, ["toolchain", "use", "--local", "0.12.0"]);
    require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--local created manifest");
    fixture.check(candidate, "0.12.0");
    null
}

test "M26-explicit" {
    configured_case("M26-explicit", local_use_bootstraps_explicit_baml_home);
}

function local_use_bootstraps_default_baml_home(fixture: Fixture, candidate: Candidate) -> null {
    fixture.cwd = `${fixture.root}/outside`;
    fixture.absent("0.12.0");
    fixture.env["BAML_HOME"] = "";
    fixture.os(["remove", "baml"]);
    fixture.manage(candidate, ["toolchain", "use", "--local", "0.12.0"]);
    require(!baml.fs.exists(`${fixture.root}/outside/baml.toml`), "--local created manifest");
    fixture.check(candidate, "0.12.0");
    null
}

test "M26-home" {
    configured_case("M26-home", local_use_bootstraps_default_baml_home);
}

function use_updates_unpinned_closest_project_only(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");

    fixture.project("0.11.0");
    fixture.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
    let before = fixture.read("project/baml.toml");
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.manage(candidate, ["toolchain", "use", "0.12.0"]);
    require(
        fixture.read("project/baml.toml") == before
            && fixture.read("baml/config.toml") == config_before,
        "use changed parent/global",
    );
    fixture.check(candidate, "0.12.0");
    null
}

test "M27" {
    configured_case("M27", use_updates_unpinned_closest_project_only);
}

function use_reports_previous_and_new_versions(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let selector = "0.12.0";
    let output = fixture.manage(candidate, ["toolchain", "use", selector]).stdout.to_string();
    require(
        output.includes("previous:") && output.includes("0.11.0") && output.includes("0.12.0"),
        "selection output omitted concrete previous/new version",
    );
    fixture.check(candidate, "0.12.0");
    null
}

test "M28-previous" {
    configured_case("M28-previous", use_reports_previous_and_new_versions);
}

function channel_use_reports_previous_and_new_versions(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("canary", key = "channel");
    let output = fixture.manage(candidate, ["toolchain", "use", "nightly"]).stdout.to_string();
    require(
        output.includes("previous:") && output.includes("0.11.0") && output.includes("0.12.0"),
        "selection output omitted concrete previous/new version",
    );
    fixture.check(candidate, "0.12.0");
    null
}

test "M29-previous-channel" {
    configured_case("M29-previous-channel", channel_use_reports_previous_and_new_versions);
}

function update_command_is_delegated_to_selected_cli(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.behavior("0.11.0", "selected CLI guidance\n", args = ["update"]);
    fixture.check(candidate, "0.11.0", args = ["update"], output = "selected CLI guidance\n");
    require(fixture.requests().length() == 0, "installed delegated update used network");
    null
}

test "M30-update-guidance" {
    configured_case("M30-update-guidance", update_command_is_delegated_to_selected_cli);
}

function upgrade_command_is_delegated_to_selected_cli(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.behavior("0.11.0", "selected CLI guidance\n", args = ["upgrade"]);
    fixture.check(candidate, "0.11.0", args = ["upgrade"], output = "selected CLI guidance\n");
    require(fixture.requests().length() == 0, "installed delegated update used network");
    null
}

test "M31-upgrade-guidance" {
    configured_case("M31-upgrade-guidance", upgrade_command_is_delegated_to_selected_cli);
}

function management_list_ensures_active_toolchain(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.manage(candidate, ["toolchain", "list"]);
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "list did not ensure active CLI",
    );
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B01-list-install" {
    configured_case("B01-list-install", management_list_ensures_active_toolchain);
}

function management_preflight_installs_recorded_channel_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.project("canary", key = "channel");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.manage(candidate, ["toolchain", "list"]);
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "channel preflight did not install expected version",
    );
    require(
        fixture.read("baml/state.toml").includes(`active_version = "0.11.0"`),
        "incorrect channel mapping",
    );
    null
}

test "B02-recorded-channel" {
    configured_case("B02-recorded-channel", management_preflight_installs_recorded_channel_version);
}

function management_preflight_resolves_unrecorded_channel(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.project("canary", key = "channel");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.os(["remove", "baml/state.toml"]);
    fixture.manage(candidate, ["toolchain", "list"]);
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0/VERSION`),
        "channel preflight did not install expected version",
    );
    require(
        fixture.read("baml/state.toml").includes(`active_version = "0.12.0"`),
        "incorrect channel mapping",
    );
    null
}

test "B03-new-channel" {
    configured_case("B03-new-channel", management_preflight_resolves_unrecorded_channel);
}

function management_continues_after_failed_best_effort_install(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.write(`server/version/0.11.0.json.status`, "500");
    fixture.write(`server/0.11.0.tar.gz.status`, "500");
    fixture.write(`server/0.11.0.zip.status`, "500");
    let result = fixture.manage(candidate, ["toolchain", "list"]);
    require(result.stderr.to_string().includes("could not ensure"), "installation failure was silent");
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B04-failed-preflight" {
    configured_case("B04-failed-preflight", management_continues_after_failed_best_effort_install);
}

function explicit_force_install_repairs_damaged_active_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.os(["remove", `baml/toolchains/0.11.0/VERSION`]);
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "preflight blocked explicit repair",
    );
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B05-repair" {
    configured_case("B05-repair", explicit_force_install_repairs_damaged_active_toolchain);
}

function uninstall_leaves_active_toolchain_removed(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.manage(candidate, ["toolchain", "uninstall", "0.11.0"]);
    require(!baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0`), "uninstall reinstalled its target");
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B06-uninstall" {
    configured_case("B06-uninstall", uninstall_leaves_active_toolchain_removed);
}

function explicit_install_also_ensures_active_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.absent("0.12.0");
    fixture.manage(candidate, ["toolchain", "install", "0.12.0"]);
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`)
            && baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0/VERSION`),
        "explicit install target replaced active preflight",
    );
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B07-explicit-target" {
    configured_case("B07-explicit-target", explicit_install_also_ensures_active_toolchain);
}

function status_install_requires_successful_preflight(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.absent("0.11.0");
    fixture.write(`server/version/0.11.0.json.status`, "500");
    fixture.write(`server/0.11.0.tar.gz.status`, "500");
    fixture.write(`server/0.11.0.zip.status`, "500");
    let output = fixture.manage(candidate, ["toolchain", "status", "--json"]);
    require(
        !baml.json.decode<ResolutionResult>(output.stdout.to_string()).installed,
        "plain status misreported failed installation",
    );
    fixture.manage(candidate, ["toolchain", "status", "--install", "--json"], success = false);
    require(
        fixture.read("baml/config.toml") == config && fixture.read("project/baml.toml") == project,
        "preflight changed selectors",
    );
    null
}

test "B08-status-required" {
    configured_case("B08-status-required", status_install_requires_successful_preflight);
}

function status_rejects_damaged_installation_without_repairing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("0.11.0");
    let config = fixture.read("baml/config.toml");
    let project = fixture.read("project/baml.toml");
    fixture.os(["remove", `baml/toolchains/0.11.0/VERSION`]);
    fixture.manage(
        candidate,
        ["toolchain", "status", "--json"],
        success = false,
        diagnostic = "--force",
    );
    require(fixture.requests().length() == 0, "damaged installation was repaired automatically");
    null
}

test "B09-damaged-status" {
    configured_case("B09-damaged-status", status_rejects_damaged_installation_without_repairing);
}

function all_management_commands_ensure_active_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    for (
        let args in [
            ["toolchain", "use", "0.12.0"],
            ["toolchain", "install", "0.12.0"],
            ["toolchain", "update"],
            ["toolchain", "status", "--json"],
            ["toolchain", "list"],
            ["toolchain", "uninstall", "0.13.0"],
        ]
    ) {
        fixture.project("0.11.0");
        fixture.absent("0.11.0");
        fixture.manage(candidate, args);
        require(
            baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
            `toolchain command skipped installation: ${args.join(" ")}`,
        );
    }
    null
}

test "B10-all-commands" {
    configured_case("B10-all-commands", all_management_commands_ensure_active_toolchain);
}

function explicit_channel_install_preserves_recorded_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["channel", "nightly", "0.13.0"]);
    fixture.absent("0.12.0");
    let state = fixture.read("baml/state.toml");
    fixture.manage(candidate, ["toolchain", "install", "nightly"]);
    require(
        baml.fs.exists(`${fixture.root}/baml/toolchains/0.12.0/VERSION`),
        "explicit channel install advanced beyond recorded version",
    );
    require(fixture.read("baml/state.toml") == state, "explicit channel install advanced its mapping");
    null
}

test "B11-channel-install" {
    configured_case("B11-channel-install", explicit_channel_install_preserves_recorded_version);
}
