// Self-update operates on fixture copies of candidate executables.

function disabled_self_update_reports_package_manager_guidance(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    fixture.manage(candidate, ["self-update"], success = false, diagnostic = "package manager");
    require(fixture.requests().length() == 0, "disabled self-update downloaded");
    null
}

test "W01" {
    configured_case("W01", disabled_self_update_reports_package_manager_guidance);
}

function self_update_replaces_wrapper_copy(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");
    let project_before = fixture.read("project/baml.toml");

    require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
    fixture.os(["copy", candidate.command[0], `wrapper${fixture.suffix}`]);
    let copy = Candidate {
        label: candidate.label,
        command: [`${fixture.root}/wrapper${fixture.suffix}`],
        self_update: true,
    };
    fixture.os(["wrapper", "valid"]);
    let state = fixture.read("baml/state.toml");
    fixture.manage(copy, ["self-update"], success = true);
    fixture.behavior("replacement", "replacement\n");
    let attempts = 0;
    while (attempts < 100) {
        let output = fixture.run(copy, ["check"]);
        if (output.stdout.to_string() == "replacement\n") {
            break;
        }
        attempts += 1;
        baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
    }
    require(
        fixture.calls().some((call) -> bool {
            call.identity == "replacement"
        }),
        "replacement wrapper did not take effect",
    );
    require(
        fixture.read("baml/state.toml") == state
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("project/baml.toml") == project_before,
        "self-update changed toolchain selection",
    );
    null
}

test "W02" {
    configured_case("W02", self_update_replaces_wrapper_copy);
}

function failed_self_update_download_preserves_wrapper(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");
    let project_before = fixture.read("project/baml.toml");

    require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
    fixture.os(["copy", candidate.command[0], `wrapper${fixture.suffix}`]);
    let copy = Candidate {
        label: candidate.label,
        command: [`${fixture.root}/wrapper${fixture.suffix}`],
        self_update: true,
    };
    fixture.os(["wrapper", "valid"]);
    fixture.write("server/wrapper.json.status", "500");
    let state = fixture.read("baml/state.toml");
    fixture.manage(copy, ["self-update"], success = false);
    fixture.check(copy, "0.11.0");
    require(
        fixture.read("baml/state.toml") == state
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("project/baml.toml") == project_before,
        "self-update changed toolchain selection",
    );
    null
}

test "W03-download" {
    configured_case("W03-download", failed_self_update_download_preserves_wrapper);
}

function failed_self_update_checksum_preserves_wrapper(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");
    let project_before = fixture.read("project/baml.toml");

    require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
    fixture.os(["copy", candidate.command[0], `wrapper${fixture.suffix}`]);
    let copy = Candidate {
        label: candidate.label,
        command: [`${fixture.root}/wrapper${fixture.suffix}`],
        self_update: true,
    };
    fixture.os(["wrapper", "checksum"]);
    let state = fixture.read("baml/state.toml");
    fixture.manage(copy, ["self-update"], success = false);
    fixture.check(copy, "0.11.0");
    require(
        fixture.read("baml/state.toml") == state
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("project/baml.toml") == project_before,
        "self-update changed toolchain selection",
    );
    null
}

test "W03-checksum" {
    configured_case("W03-checksum", failed_self_update_checksum_preserves_wrapper);
}

function failed_self_update_replacement_preserves_wrapper(
    fixture: Fixture,
    candidate: Candidate,
) -> null {
    let config_before = fixture.read("baml/config.toml");
    let project_before = fixture.read("project/baml.toml");

    require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
    fixture.os(["copy", candidate.command[0], `wrapper${fixture.suffix}`]);
    let copy = Candidate {
        label: candidate.label,
        command: [`${fixture.root}/wrapper${fixture.suffix}`],
        self_update: true,
    };
    fixture.os(["wrapper", "valid"]);
    fixture.os(["chmod", `wrapper${fixture.suffix}`, "555"]);
    baml.fs.mkdir(`${fixture.root}/wrapper.tmp`, baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(`${fixture.root}/wrapper.exe.tmp`, baml.fs.MkdirOptions { recursive: true });
    let state = fixture.read("baml/state.toml");
    fixture.manage(copy, ["self-update"], success = false);
    fixture.check(copy, "0.11.0");
    require(
        fixture.read("baml/state.toml") == state
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("project/baml.toml") == project_before,
        "self-update changed toolchain selection",
    );
    null
}

test "W03-replace" {
    configured_case("W03-replace", failed_self_update_replacement_preserves_wrapper);
}

function windows_self_update_replaces_wrapper_copy(fixture: Fixture, candidate: Candidate) -> null {
    let config_before = fixture.read("baml/config.toml");
    let project_before = fixture.read("project/baml.toml");

    require(candidate.command.length() == 1, "self-update requires a copied executable candidate");
    fixture.os(["copy", candidate.command[0], `wrapper${fixture.suffix}`]);
    let copy = Candidate {
        label: candidate.label,
        command: [`${fixture.root}/wrapper${fixture.suffix}`],
        self_update: true,
    };
    fixture.os(["wrapper", "valid"]);
    let state = fixture.read("baml/state.toml");
    fixture.manage(copy, ["self-update"], success = true);
    fixture.behavior("replacement", "replacement\n");
    let attempts = 0;
    while (attempts < 100) {
        let output = fixture.run(copy, ["check"]);
        if (output.stdout.to_string() == "replacement\n") {
            break;
        }
        attempts += 1;
        baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
    }
    require(
        fixture.calls().some((call) -> bool {
            call.identity == "replacement"
        }),
        "replacement wrapper did not take effect",
    );
    require(
        fixture.read("baml/state.toml") == state
            && fixture.read("baml/config.toml") == config_before
            && fixture.read("project/baml.toml") == project_before,
        "self-update changed toolchain selection",
    );
    null
}

test "W04" {
    configured_case("W04", windows_self_update_replaces_wrapper_copy);
}
