// Local toolchain paths are resolved without release requests.

function absolute_local_path_is_launched(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L01" {
    configured_case("L01", absolute_local_path_is_launched);
}

function environment_relative_path_resolves_from_cwd(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.env["BAML_TOOLCHAIN"] = `./build/baml-cli${fixture.suffix}`;

    fixture.check(candidate, "L2");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L02" {
    configured_case("L02", environment_relative_path_resolves_from_cwd);
}

function project_relative_path_resolves_from_project_root(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.env["BAML_TOOLCHAIN"] = "";
    fixture.project(`./build/baml-cli${fixture.suffix}`, key = "path");

    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L03" {
    configured_case("L03", project_relative_path_resolves_from_project_root);
}

function tilde_path_resolves_from_home(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `~/build/baml-cli${fixture.suffix}`;

    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L04" {
    configured_case("L04", tilde_path_resolves_from_home);
}

function local_path_supports_spaces_and_unicode(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"]
        = `${fixture.root}/project/space ünicode/baml-cli${fixture.suffix}`;

    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L05" {
    configured_case("L05", local_path_supports_spaces_and_unicode);
}

function local_path_resolves_parent_components(fixture: Fixture, candidate: Candidate) -> null {
    fixture.cwd = `${fixture.root}/project/nested/deep`;
    fixture.env["BAML_TOOLCHAIN"] = `../../build/baml-cli${fixture.suffix}`;
    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L06" {
    configured_case("L06", local_path_resolves_parent_components);
}

function parent_components_preserve_symlink_semantics(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["symlink", "project/link", "outside/build"]);
    fixture.env["BAML_TOOLCHAIN"] = "./link/../build/baml-cli";

    fixture.check(candidate, "L2");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L06-symlink" {
    configured_case("L06-symlink", parent_components_preserve_symlink_semantics);
}

function global_relative_path_resolves_from_each_cwd(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["BAML_TOOLCHAIN"] = "";
    fixture.config(`./build/baml-cli${fixture.suffix}`);
    fixture.check(candidate, "L1");
    fixture.cwd = `${fixture.root}/outside`;
    fixture.check(candidate, "L2");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L07" {
    configured_case("L07", global_relative_path_resolves_from_each_cwd);
}

function symlink_target_is_resolved_for_each_launch(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["symlink", "project/current", "project/build/baml-cli"]);
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/current`;
    fixture.check(candidate, "L1");
    fixture.os(["symlink", "project/current", "outside/build/baml-cli"]);
    fixture.check(candidate, "L2");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L08" {
    configured_case("L08", symlink_target_is_resolved_for_each_launch);
}

function missing_local_binary_reports_environment_source(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/missing/baml-cli`;
    failed_check(fixture, candidate, diagnostic = "BAML_TOOLCHAIN");
    require(fixture.requests().length() == 0, "local failure contacted server");
    null
}

test "L09" {
    configured_case("L09", missing_local_binary_reports_environment_source);
}

function local_directory_is_rejected_as_a_binary(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build`;
    failed_check(fixture, candidate, diagnostic = "directory");
    require(fixture.requests().length() == 0, "local failure contacted server");
    null
}

test "L10" {
    configured_case("L10", local_directory_is_rejected_as_a_binary);
}

function windows_backslash_path_is_launched(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"]
        = `${fixture.root}/project/build/baml-cli.exe`.replace_all("/", "\\");
    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L14" {
    configured_case("L14", windows_backslash_path_is_launched);
}

function windows_local_path_infers_executable_suffix(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli`;
    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L15" {
    configured_case("L15", windows_local_path_infers_executable_suffix);
}

function local_path_launch_uses_no_release_server(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_TOOLCHAIN"] = `${fixture.root}/project/build/baml-cli${fixture.suffix}`;

    fixture.check(candidate, "L1");
    require(fixture.requests().length() == 0, "local toolchain contacted release server");
    null
}

test "L16" {
    configured_case("L16", local_path_launch_uses_no_release_server);
}
