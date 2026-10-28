// Automatic installation, explicit repair, and failure recovery.

function absent_global_version_is_installed(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I01" {
    configured_case("I01", absent_global_version_is_installed);
}

function absent_project_version_is_installed(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.config("0.12.0");
    fixture.project("0.11.0");
    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I02" {
    configured_case("I02", absent_project_version_is_installed);
}

function absent_environment_version_is_installed(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.project("0.11.0");
    fixture.env["BAML_TOOLCHAIN"] = "0.12.0";
    fixture.absent("0.12.0");

    fixture.check(candidate, "0.12.0");
    require(
        fixture.read(`baml/toolchains/0.12.0/VERSION`).trim() == "0.12.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.12.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I03" {
    configured_case("I03", absent_environment_version_is_installed);
}

function recorded_channel_installs_without_advancing(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.config("canary");
    fixture.os(["channel", "canary", "0.12.0"]);
    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    require(!requests.includes("/canary.json"), "recorded channel silently advanced");
    null
}

test "I04" {
    configured_case("I04", recorded_channel_installs_without_advancing);
}

function unrecorded_channel_is_resolved_and_installed(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.config("canary");
    fixture.write("baml/state.toml", "");
    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes("canary.json")
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    require(channel_version(fixture, "canary") == "0.11.0", "channel not recorded");
    null
}

test "I05" {
    configured_case("I05", unrecorded_channel_is_resolved_and_installed);
}

function empty_home_bootstraps_default_canary(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.config("canary");
    fixture.write("baml/state.toml", "");
    fixture.os(["remove", "baml"]);
    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes("canary.json")
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    require(channel_version(fixture, "canary") == "0.11.0", "channel not recorded");
    fixture.behavior(
        "0.11.0",
        "changed",
        code = 7,
        args = ["format", "--flag", "a b"],
        stderr = "stderr",
    );
    fixture.check(
        candidate,
        "0.11.0",
        args = ["format", "--flag", "a b"],
        output = "changed",
        code = 7,
    );
    require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
    null
}

test "I06" {
    configured_case("I06", empty_home_bootstraps_default_canary);
}

function installed_toolchain_launches_offline(fixture: Fixture, candidate: Candidate) -> null {
    fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
    fixture.check(candidate, "0.11.0");
    require(fixture.requests().length() == 0, "installed toolchain contacted server");
    null
}

test "I07" {
    configured_case("I07", installed_toolchain_launches_offline);
}

function installed_toolchain_is_reused_with_new_arguments(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    fixture.behavior(
        "0.11.0",
        "changed",
        code = 7,
        args = ["format", "--flag", "a b"],
        stderr = "stderr",
    );
    fixture.check(
        candidate,
        "0.11.0",
        args = ["format", "--flag", "a b"],
        output = "changed",
        code = 7,
    );
    require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
    null
}

test "I08" {
    configured_case("I08", installed_toolchain_is_reused_with_new_arguments);
}

function missing_managed_binary_requires_explicit_repair(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["remove", `baml/toolchains/0.11.0/bin/baml-cli${fixture.suffix}`]);
    failed_check(fixture, candidate, diagnostic = "--force");
    require(fixture.requests().length() == 0, "damaged installation was repaired");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I09-binary" {
    configured_case("I09-binary", missing_managed_binary_requires_explicit_repair);
}

function missing_version_file_requires_explicit_repair(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["remove", `baml/toolchains/0.11.0/VERSION`]);
    failed_check(fixture, candidate, diagnostic = "--force");
    require(fixture.requests().length() == 0, "damaged installation was repaired");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I09-VERSION" {
    configured_case("I09-VERSION", missing_version_file_requires_explicit_repair);
}

function mismatched_version_file_requires_explicit_repair(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.write(`baml/toolchains/0.11.0/VERSION`, "0.12.0");
    failed_check(fixture, candidate, diagnostic = "--force");
    require(fixture.requests().length() == 0, "damaged installation was repaired");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I09-mismatch" {
    configured_case("I09-mismatch", mismatched_version_file_requires_explicit_repair);
}

function dangling_installation_symlink_requires_explicit_repair(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.os(["symlink", `baml/toolchains/0.11.0`, "missing-installation"]);
    failed_check(fixture, candidate, diagnostic = "--force");
    require(fixture.requests().length() == 0, "damaged installation was repaired");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I09-link" {
    configured_case("I09-link", dangling_installation_symlink_requires_explicit_repair);
}

function installation_file_requires_explicit_repair(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.os(["copy", baml.env.get_or_panic("WRAPPER_STUB"), `baml/toolchains/0.11.0`]);
    failed_check(fixture, candidate, diagnostic = "--force");
    require(fixture.requests().length() == 0, "damaged installation was repaired");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I09-file" {
    configured_case("I09-file", installation_file_requires_explicit_repair);
}

function explicit_install_preserves_global_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.config("0.12.0");
    fixture.manage(candidate, ["toolchain", "install", "0.11.0"]);
    fixture.check(candidate, "0.12.0");
    require(fixture.read("baml/config.toml").includes("0.12.0"), "install changed default");
    null
}

test "I10" {
    configured_case("I10", explicit_install_preserves_global_selection);
}

function forced_install_replaces_usable_toolchain(fixture: Fixture, candidate: Candidate) -> null {
    fixture.os(["release", "0.11.0", "valid", "A2"]);
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"], success = true);
    fixture.check(candidate, "A2");
    null
}

test "I11" {
    configured_case("I11", forced_install_replaces_usable_toolchain);
}

function failed_forced_install_preserves_usable_toolchain(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.os(["release", "0.11.0", "checksum", "A2"]);
    fixture.manage(candidate, ["toolchain", "install", "0.11.0", "--force"], success = false);
    fixture.check(candidate, "0.11.0");
    null
}

test "I11-failure" {
    configured_case("I11-failure", failed_forced_install_preserves_usable_toolchain);
}

function unreachable_manifest_server_rejects_install(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-unreachable" {
    configured_case("I12-unreachable", unreachable_manifest_server_rejects_install);
}

function manifest_timeout_rejects_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.write(`server/version/0.11.0.json.delay`, "31");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-timeout" {
    configured_case("I12-timeout", manifest_timeout_rejects_install);
}

function missing_manifest_rejects_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.write(`server/version/0.11.0.json.status`, "404");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-404" {
    configured_case("I12-404", missing_manifest_rejects_install);
}

function manifest_server_error_rejects_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.write(`server/version/0.11.0.json.status`, "500");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-500" {
    configured_case("I12-500", manifest_server_error_rejects_install);
}

function invalid_manifest_json_rejects_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.write(`server/version/0.11.0.json`, "not json");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-json" {
    configured_case("I12-json", invalid_manifest_json_rejects_install);
}

function unsupported_manifest_schema_rejects_install(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.os(["release", "0.11.0", "schema", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-schema" {
    configured_case("I12-schema", unsupported_manifest_schema_rejects_install);
}

function missing_host_artifact_rejects_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");

    fixture.os(["release", "0.11.0", "target", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I12-target" {
    configured_case("I12-target", missing_host_artifact_rejects_install);
}

function missing_archive_preserves_other_toolchains(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    let extension = if (fixture.windows) {
        ".zip"
    } else {
        ".tar.gz"
    };
    fixture.write(`server/0.11.0${extension}.status`, "404");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    fixture.config("0.12.0");
    fixture.check(candidate, "0.12.0");
    null
}

test "I13-404" {
    configured_case("I13-404", missing_archive_preserves_other_toolchains);
}

function interrupted_archive_preserves_other_toolchains(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    let extension = if (fixture.windows) {
        ".zip"
    } else {
        ".tar.gz"
    };
    fixture.write(`server/0.11.0${extension}.disconnect`, "true");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    fixture.config("0.12.0");
    fixture.check(candidate, "0.12.0");
    null
}

test "I13-disconnect" {
    configured_case("I13-disconnect", interrupted_archive_preserves_other_toolchains);
}

function checksum_mismatch_preserves_other_toolchains(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.os(["release", "0.11.0", "checksum", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    fixture.config("0.12.0");
    fixture.check(candidate, "0.12.0");
    null
}

test "I13-checksum" {
    configured_case("I13-checksum", checksum_mismatch_preserves_other_toolchains);
}

function archive_without_cli_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.os(["release", "0.11.0", "missing-cli", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I14-cli" {
    configured_case("I14-cli", archive_without_cli_is_rejected);
}

function archive_with_invalid_layout_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.os(["release", "0.11.0", "layout", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I14-layout" {
    configured_case("I14-layout", archive_with_invalid_layout_is_rejected);
}

function archive_path_traversal_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.os(["release", "0.11.0", "traversal", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I15-traversal" {
    configured_case("I15-traversal", archive_path_traversal_is_rejected);
}

function archive_link_entry_is_rejected(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.os(["release", "0.11.0", "link", ""]);
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I15-link" {
    configured_case("I15-link", archive_link_entry_is_rejected);
}

function unwritable_install_destination_preserves_existing_selection(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");
    fixture.os(["chmod", "baml/toolchains", "555"]);
    failed_check(fixture, candidate);
    fixture.os(["chmod", "baml/toolchains", "755"]);
    fixture.config("0.12.0");
    fixture.check(candidate, "0.12.0");
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    null
}

test "I16" {
    configured_case("I16", unwritable_install_destination_preserves_existing_selection);
}

function failed_install_can_be_retried_after_server_recovery(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.write(`server/version/0.11.0.json.status`, "404");
    failed_check(fixture, candidate);
    require(
        !baml.fs.exists(`${fixture.root}/baml/toolchains/0.11.0/VERSION`),
        "failed install published VERSION",
    );
    require(
        !baml.fs.exists(`${fixture.root}/baml/escaped`)
            && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
        "archive escaped staging",
    );
    fixture.os(["remove", `server/version/0.11.0.json.status`]);
    fixture.check(candidate, "0.11.0");
    null
}

test "I17" {
    configured_case("I17", failed_install_can_be_retried_after_server_recovery);
}

function unsupported_host_requires_a_separate_platform_build(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I20" {
    configured_case("I20", unsupported_host_requires_a_separate_platform_build);
}

function implicit_install_preserves_cli_json_stdout(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.absent("0.11.0");

    fixture.behavior("0.11.0", "{\"ok\":true}\n");
    fixture.check(candidate, "0.11.0", args = ["check"], output = "{\"ok\":true}\n");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I21" {
    configured_case("I21", implicit_install_preserves_cli_json_stdout);
}

function stale_install_lock_does_not_block_install(fixture: Fixture, candidate: Candidate) -> null {
    fixture.absent("0.11.0");
    fixture.write(`baml/toolchains/.0.11.0.lock`, "stale owner metadata\n");

    fixture.check(candidate, "0.11.0");
    require(
        fixture.read(`baml/toolchains/0.11.0/VERSION`).trim() == "0.11.0",
        "installation VERSION differs",
    );
    let requests = fixture.requests();
    require(
        requests.some((path) -> bool {
            path.includes(`/version/0.11.0.json`)
        }),
        "missing manifest request",
    );
    require(
        requests.some((path) -> bool {
            path.ends_with(".tar.gz") || path.ends_with(".zip")
        }),
        "missing archive request",
    );
    null
}

test "I21-stale-lock" {
    configured_case("I21-stale-lock", stale_install_lock_does_not_block_install);
}

function bare_install_uses_project_selection(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.12.0");
    let state = fixture.read("baml/state.toml");
    let args = ["toolchain", "install"];
    fixture.absent("0.12.0");
    fixture.manage(candidate, args);
    require(fixture.read("baml/state.toml") == state, "bare install advanced an existing channel");
    fixture.check(candidate, "0.12.0");
    null
}

test "I22-bare-project" {
    configured_case("I22-bare-project", bare_install_uses_project_selection);
}

function bare_install_uses_environment_selection(fixture: Fixture, candidate: Candidate) -> null {
    fixture.project("0.12.0");
    fixture.env["BAML_TOOLCHAIN"] = "0.13.0";
    let state = fixture.read("baml/state.toml");
    let args = ["toolchain", "install"];
    fixture.absent("0.13.0");
    fixture.manage(candidate, args);
    require(fixture.read("baml/state.toml") == state, "bare install advanced an existing channel");
    fixture.check(candidate, "0.13.0");
    null
}

test "I23-bare-env" {
    configured_case("I23-bare-env", bare_install_uses_environment_selection);
}

function bare_install_preserves_recorded_channel_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("nightly", key = "channel");
    fixture.os(["channel", "nightly", "0.13.0"]);
    let state = fixture.read("baml/state.toml");
    let args = ["toolchain", "install"];
    fixture.absent("0.12.0");
    fixture.manage(candidate, args);
    require(fixture.read("baml/state.toml") == state, "bare install advanced an existing channel");
    fixture.check(candidate, "0.12.0");
    null
}

test "I24-bare-channel" {
    configured_case("I24-bare-channel", bare_install_preserves_recorded_channel_version);
}

function bare_forced_install_replaces_recorded_channel_version(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.project("nightly", key = "channel");
    fixture.os(["channel", "nightly", "0.13.0"]);
    let state = fixture.read("baml/state.toml");
    let args = ["toolchain", "install", "--force"];
    fixture.os(["release", "0.12.0", "valid", "A2"]);
    fixture.manage(candidate, args);
    require(fixture.read("baml/state.toml") == state, "bare install advanced an existing channel");
    fixture.check(candidate, "A2");
    null
}

test "I25-bare-force" {
    configured_case("I25-bare-force", bare_forced_install_replaces_recorded_channel_version);
}

function concurrent_installs_publish_reusable_manifest_cache(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let value = "0.11.0";
    fixture.absent(value);
    let path = `server/version/${value}.json`;
    fixture.write(path, fixture.read(path).replace_all("2026-01-01T00:00:00Z", "x".repeat(262144)));
    let first = spawn { fixture.manage(candidate, ["toolchain", "install", value]) };
    let second = spawn { fixture.manage(candidate, ["toolchain", "install", value]) };
    let third = spawn { fixture.manage(candidate, ["toolchain", "install", value]) };
    let fourth = spawn { fixture.manage(candidate, ["toolchain", "install", value]) };
    await first;
    await second;
    await third;
    await fourth;
    let requests = fixture.requests().length();
    fixture.write(`${path}.status`, "500");
    fixture.absent(value);
    fixture.manage(candidate, ["toolchain", "install", value]);
    require(
        fixture.requests().length() == requests + 1,
        "cached install unexpectedly fetched its manifest",
    );
    fixture.check(candidate, value);
    null
}

test "I26-concurrent-cache" {
    configured_case("I26-concurrent-cache", concurrent_installs_publish_reusable_manifest_cache);
}
