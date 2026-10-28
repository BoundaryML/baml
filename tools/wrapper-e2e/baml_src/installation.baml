// Automatic installation, explicit repair, and failure recovery.

test "installation: absent global version is installed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
    });
}

test "installation: absent project version is installed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.config(NEXT_VERSION);
        fixture.project(BASE_VERSION);
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
    });
}

test "installation: absent environment version is installed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.project(BASE_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = NEXT_VERSION;
        fixture.absent(NEXT_VERSION);

        fixture.check(candidate, NEXT_VERSION);
        require(
            fixture.read(`baml/toolchains/${NEXT_VERSION}/VERSION`).trim() == NEXT_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${NEXT_VERSION}.json`)
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
    });
}

test "installation: recorded channel installs without advancing" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.channel("canary", NEXT_VERSION);
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
    });
}

test "installation: unrecorded channel is resolved and installed" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
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
        require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
        null
    });
}

test "installation: empty home bootstraps default canary" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.config("canary");
        fixture.write(CHANNEL_STATE_FILE, "");
        fixture.remove("baml");
        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
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
        require(fixture.channel_version("canary") == BASE_VERSION, "channel not recorded");
        fixture.behavior(
            BASE_VERSION,
            "changed",
            code = 7,
            args = ["format", "--flag", "a b"],
            stderr = "stderr",
        );
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["format", "--flag", "a b"],
            output = "changed",
            code = 7,
        );
        require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
        null
    });
}

test "installation: installed toolchain launches offline" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        fixture.check(candidate, BASE_VERSION);
        require(fixture.requests().length() == 0, "installed toolchain contacted server");
        null
    });
}

test "installation: installed toolchain is reused with new arguments" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
            BASE_VERSION,
            "changed",
            code = 7,
            args = ["format", "--flag", "a b"],
            stderr = "stderr",
        );
        fixture.check(
            candidate,
            BASE_VERSION,
            args = ["format", "--flag", "a b"],
            output = "changed",
            code = 7,
        );
        require(fixture.requests().length() == requests.length(), "reused toolchain downloaded again");
        null
    });
}

test "installation: missing managed binary requires explicit repair" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.remove(`baml/toolchains/${BASE_VERSION}/bin/baml-cli${fixture.suffix}`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: missing version file requires explicit repair" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.remove(`baml/toolchains/${BASE_VERSION}/VERSION`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: mismatched version file requires explicit repair" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.write(`baml/toolchains/${BASE_VERSION}/VERSION`, NEXT_VERSION);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: dangling installation symlink requires explicit repair" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.absent(BASE_VERSION);
            fixture.symlink(`baml/toolchains/${BASE_VERSION}`, "missing-installation");
            fixture.failed_check(candidate, diagnostic = "--force");
            require(fixture.requests().length() == 0, "damaged installation was repaired");
            fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
            fixture.check(candidate, BASE_VERSION);
            null
        },
        capabilities = [Capability.Symlinks],
    );
}

test "installation: installation file requires explicit repair" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.copy(baml.env.get_or_panic("WRAPPER_STUB"), `baml/toolchains/${BASE_VERSION}`);
        fixture.failed_check(candidate, diagnostic = "--force");
        require(fixture.requests().length() == 0, "damaged installation was repaired");
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"]);
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: explicit install preserves global selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.config(NEXT_VERSION);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION]);
        fixture.check(candidate, NEXT_VERSION);
        require(fixture.read(USER_CONFIG_FILE).includes(NEXT_VERSION), "install changed default");
        null
    });
}

test "installation: forced install replaces usable toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.release(BASE_VERSION, "valid", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(candidate, ["toolchain", "install", BASE_VERSION, "--force"], success = true);
        fixture.check(candidate, REINSTALLED_TOOLCHAIN_IDENTITY);
        null
    });
}

test "installation: failed forced install preserves usable toolchain" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.release(BASE_VERSION, "checksum", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(
            candidate,
            ["toolchain", "install", BASE_VERSION, "--force"],
            success = false,
        );
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: unreachable manifest server rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: manifest timeout rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.delay`, "31");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: missing manifest rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: manifest server error rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "500");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: invalid manifest json rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json`, "not json");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: unsupported manifest schema rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "schema");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: missing host artifact rejects install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "target");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: missing archive preserves other toolchains" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(`server/${BASE_VERSION}${extension}.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "installation: interrupted archive preserves other toolchains" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        let extension = if (fixture.windows) {
            ".zip"
        } else {
            ".tar.gz"
        };
        fixture.write(`server/${BASE_VERSION}${extension}.disconnect`, "true");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "installation: checksum mismatch preserves other toolchains" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.release(BASE_VERSION, "checksum");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.config(NEXT_VERSION);
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "installation: archive without cli is rejected" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "missing-cli");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: archive with invalid layout is rejected" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "layout");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: archive path traversal is rejected" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "traversal");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: archive link entry is rejected" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.release(BASE_VERSION, "link");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        null
    });
}

test "installation: unwritable install destination preserves existing selection" {
    for_each_candidate(
        (fixture: Fixture, candidate: Candidate) -> null {
            fixture.absent(BASE_VERSION);
            fixture.chmod("baml/toolchains", "555");
            fixture.failed_check(candidate);
            fixture.chmod("baml/toolchains", "755");
            fixture.config(NEXT_VERSION);
            fixture.check(candidate, NEXT_VERSION);
            require(
                !baml.fs.exists(`${fixture.root}/baml/escaped`)
                    && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
                "archive escaped staging",
            );
            null
        },
        capabilities = [Capability.PosixPermissions],
    );
}

test "installation: failed install can be retried after server recovery" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.write(`server/version/${BASE_VERSION}.json.status`, "404");
        fixture.failed_check(candidate);
        require(
            !baml.fs.exists(`${fixture.root}/baml/toolchains/${BASE_VERSION}/VERSION`),
            "failed install published VERSION",
        );
        require(
            !baml.fs.exists(`${fixture.root}/baml/escaped`)
                && !baml.fs.exists(`${fixture.root}/baml/toolchains/escaped`),
            "archive escaped staging",
        );
        fixture.remove(`server/version/${BASE_VERSION}.json.status`);
        fixture.check(candidate, BASE_VERSION);
        null
    });
}

test "installation: implicit install preserves cli json stdout" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);

        fixture.behavior(BASE_VERSION, "{\"ok\":true}\n");
        fixture.check(candidate, BASE_VERSION, args = ["check"], output = "{\"ok\":true}\n");
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
    });
}

test "installation: stale install lock does not block install" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.absent(BASE_VERSION);
        fixture.write(`baml/toolchains/.${BASE_VERSION}.lock`, "stale owner metadata\n");

        fixture.check(candidate, BASE_VERSION);
        require(
            fixture.read(`baml/toolchains/${BASE_VERSION}/VERSION`).trim() == BASE_VERSION,
            "installation VERSION differs",
        );
        let requests = fixture.requests();
        require(
            requests.some((path) -> bool {
                path.includes(`/version/${BASE_VERSION}.json`)
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
    });
}

test "installation: bare install uses project selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "installation: bare install uses environment selection" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project(NEXT_VERSION);
        fixture.env["BAML_TOOLCHAIN"] = LATEST_VERSION;
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(LATEST_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, LATEST_VERSION);
        null
    });
}

test "installation: bare install preserves recorded channel version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install"];
        fixture.absent(NEXT_VERSION);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, NEXT_VERSION);
        null
    });
}

test "installation: bare forced install replaces recorded channel version" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        fixture.project("nightly", key = "channel");
        fixture.channel("nightly", LATEST_VERSION);
        let state = fixture.read(CHANNEL_STATE_FILE);
        let args = ["toolchain", "install", "--force"];
        fixture.release(NEXT_VERSION, "valid", identity = REINSTALLED_TOOLCHAIN_IDENTITY);
        fixture.manage(candidate, args);
        require(fixture.read(CHANNEL_STATE_FILE) == state, "bare install advanced an existing channel");
        fixture.check(candidate, REINSTALLED_TOOLCHAIN_IDENTITY);
        null
    });
}

test "installation: concurrent installs publish reusable manifest cache" {
    for_each_candidate((fixture: Fixture, candidate: Candidate) -> null {
        let value = BASE_VERSION;
        fixture.absent(value);
        let path = `server/version/${value}.json`;
        fixture.write(
            path,
            fixture.read(path).replace_all("2026-01-01T00:00:00Z", "x".repeat(STRESS_PAYLOAD_BYTES)),
        );
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
    });
}
