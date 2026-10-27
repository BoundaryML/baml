function case_ids() -> string[] {
    [
        "S30-home-boundary",
        "S31-relative-unpinned",
        "I26-concurrent-cache",
        "S28-query-global-option",
        "S20-project-prefix",
        "S21-project-inline",
        "S22-directory",
        "S23-project-env",
        "S24-target-args",
        "S25-query-project",
        "S26-directory-project",
        "S27-project-management",
        "R01-installed",
        "R02-absent",
        "R03-install",
        "R04-channel",
        "R05-channel-install",
        "R06-local",
        "R07-invalid-local",
        "R08-help",
        "E13-project-missing",
        "E13-project-absent",
        "E13-project-duplicate",
        "I22-bare-project",
        "I23-bare-env",
        "I24-bare-channel",
        "I25-bare-force",
        "U01-cached-outdated",
        "U02-cached-current",
        "U03-refresh",
        "U04-offline",
        "U05-optout",
        "M28-previous",
        "M29-previous-channel",
        "M30-update-guidance",
        "M31-upgrade-guidance",
        "D01-human-error",
        "S29-directory-env-relative",
        "R09-default-json",
        "R10-unrecorded-channel",
        "U06-status-cache",
        "U07-local",
        "U08-exact",
        "S01",
        "S02",
        "S03",
        "S04",
        "S05",
        "S06",
        "S07",
        "S08-empty",
        "S08-space",
        "S09",
        "S10",
        "S11",
        "S12",
        "S13",
        "S15-syntax",
        "S15-field",
        "S16-valid",
        "S16-invalid",
        "S17",
        "S18",
        "S19",
        "L01",
        "L02",
        "L03",
        "L04",
        "L05",
        "L06",
        "L06-symlink",
        "L07",
        "L08",
        "L09",
        "L10",
        "L14",
        "L15",
        "L16",
        "I01",
        "I21-stale-lock",
        "I02",
        "I03",
        "I04",
        "I05",
        "I06",
        "I07",
        "I08",
        "I09-binary",
        "I09-VERSION",
        "I09-mismatch",
        "I09-link",
        "I09-file",
        "I10",
        "I11",
        "I11-failure",
        "I12-unreachable",
        "I12-timeout",
        "I12-404",
        "I12-500",
        "I12-json",
        "I12-schema",
        "I12-target",
        "I13-404",
        "I13-disconnect",
        "I13-checksum",
        "I14-cli",
        "I14-layout",
        "I15-traversal",
        "I15-link",
        "I16",
        "I17",
        "I20",
        "I21",
        "M01",
        "M02",
        "M03",
        "M04",
        "M05",
        "M06-canary",
        "M06-nightly",
        "M07",
        "M08-version",
        "M08-channel",
        "M08-path",
        "M09",
        "M10-project-download",
        "M10-local-download",
        "M10-project-path",
        "M10-local-path",
        "M11",
        "M13",
        "M14-env-version",
        "M14-env-path",
        "M14-project-version",
        "M14-project-path",
        "M14-global-version",
        "M14-global-path",
        "M15-project",
        "M15-env",
        "M16",
        "M17-absent",
        "M17-path",
        "M17-channel",
        "M18-version",
        "M18-channel",
        "M19-empty",
        "M19-installed",
        "M19-override",
        "M20",
        "M21",
        "M22",
        "M23",
        "M24",
        "M25",
        "M26-explicit",
        "M26-home",
        "M27",
        "V01",
        "E01",
        "E02",
        "E02-metadata",
        "E03",
        "E04",
        "E05-env",
        "E05-env-prerelease",
        "E05-env-overflow",
        "E05-project",
        "E05-state",
        "E06-version",
        "E06-channel",
        "E07-config",
        "E07-state",
        "E08",
        "E09",
        "E09-dependencies",
        "E09-scripts",
        "E09-generator",
        "E09-test",
        "E09-date",
        "E10-project",
        "E10-local",
        "E11-update",
        "E11-auto",
        "E12-config",
        "E12-state",
        "E12-project",
        "W01",
        "W02",
        "W03-download",
        "W03-checksum",
        "W03-replace",
        "W04",
        "F01",
        "F02",
        "F03",
        "F04",
        "F05",
        "F06",
        "F07",
        "F08-env",
        "F08-override",
    ]
}

function channel_version(f: Fixture, channel: string) -> string {
    let document = baml.toml.Table.parse(f.read("baml/state.toml"));
    baml.json.path<string>(document.to_json(), `.channels.${channel}.active_version`)
}

function failed_check(f: Fixture, c: Candidate, diagnostic: string = "") -> null {
    let output = f.run(c, ["check"]);
    require(!output.ok(), "expected launch to fail");
    require(
        output.stderr.to_string().includes(diagnostic),
        `missing ${diagnostic}: ${output.stderr.to_string()}`,
    );
    require(f.calls().length() == 0, "failed selection invoked a toolchain");
    null
}

function scenario(f: Fixture, c: Candidate, id: string) -> null {
    if (id == "S31-relative-unpinned") {
        f.project("0.12.0");
        for (let flag in ["--project", "--directory"]) {
            let args = [flag, "../outside", "check"];
            f.behavior("0.11.0", "0.11.0\n", args = args);
            f.check(c, "0.11.0", args = args);
        }
        return null;
    }
    if (id == "S30-home-boundary") {
        f.write("baml.toml", "[toolchain]\nversion = \"0.12.0\"\n");
        f.cwd = `${f.root}/home/nested/deep`;
        baml.fs.mkdir(f.cwd, baml.fs.MkdirOptions { recursive: true });
        if (f.windows) {
            f.env["HOME"] = `${f.root}/home`.replace_all("/", "\\");
            f.env["USERPROFILE"] = f.env["HOME"];
        }
        f.check(c, "0.11.0");
        let before = f.read("baml.toml");
        f.manage(c, ["toolchain", "use", "0.12.0"], success = false);
        require(f.read("baml.toml") == before, "use crossed the home-directory boundary");
        require(f.requests().length() == 0, "home boundary check used network");
        return null;
    }
    if (id == "I26-concurrent-cache") {
        let value = "0.11.0";
        f.absent(value);
        let path = `server/version/${value}.json`;
        f.write(path, f.read(path).replace_all("2026-01-01T00:00:00Z", "x".repeat(262144)));
        let first = spawn { f.manage(c, ["toolchain", "install", value]) };
        let second = spawn { f.manage(c, ["toolchain", "install", value]) };
        let third = spawn { f.manage(c, ["toolchain", "install", value]) };
        let fourth = spawn { f.manage(c, ["toolchain", "install", value]) };
        await first;
        await second;
        await third;
        await fourth;
        let requests = f.requests().length();
        f.write(`${path}.status`, "500");
        f.absent(value);
        f.manage(c, ["toolchain", "install", value]);
        require(f.requests().length() == requests + 1, "cached install unexpectedly fetched its manifest");
        f.check(c, value);
        return null;
    }
    if (id.starts_with("S2")) {
        f.project("0.11.0");
        f.write(
            "outside/baml.toml",
            "[package]\nname = \"outside\"\n[toolchain]\nversion = \"0.12.0\"\n",
        );
        let args = ["--project", "../outside", "check"];
        let expected = "0.12.0";
        if (id == "S21-project-inline") {
            args = ["check", "--project=../outside"];
        }
        if (id == "S22-directory") {
            args = ["--directory", "../outside", "check"];
        }
        if (id == "S23-project-env") {
            f.env["BAML_TOOLCHAIN"] = "0.11.0";
            expected = "0.11.0";
        }
        if (id == "S24-target-args") {
            args = ["run", "main", "--", "--project", "../outside"];
            expected = "0.11.0";
        }
        if (id == "S25-query-project") {
            args = ["query", "--project", "org/cloud-project"];
            expected = "0.11.0";
        }
        if (id == "S28-query-global-option") {
            args = ["--output-preset", "agent", "query", "--project", "org/cloud-project"];
            expected = "0.11.0";
        }
        if (id == "S29-directory-env-relative") {
            args = ["--directory", "../outside", "check"];
            f.env["BAML_TOOLCHAIN"] = "./build/baml-cli";
            expected = "L2";
        }
        if (id == "S26-directory-project") {
            args = ["--directory=../outside", "--project=../project", "check"];
            expected = "0.11.0";
        }
        if (id == "S27-project-management") {
            f.manage(c, ["--project", "../outside", "toolchain", "use", "0.13.0"]);
            require(f.read("project/baml.toml").includes("0.11.0"), "use changed cwd project");
            expected = "0.13.0";
        }
        f.behavior(expected, `${expected}\n`, args = args);
        f.check(c, expected, args = args);
        return null;
    }
    if (id.starts_with("R")) {
        f.project("0.11.0");
        let args = ["toolchain", "resolve", "--json"];
        if (id == "R09-default-json") {
            args = ["toolchain", "resolve"];
        }
        if (id == "R10-unrecorded-channel") {
            f.project("nightly", key = "channel");
            f.os(["remove", "baml/state.toml"]);
            f.manage(c, args, success = false, diagnostic = "resolve --install");
            require(f.requests().length() == 0, "unrecorded resolution used network");
            return null;
        }
        let expected: string? = "0.11.0";
        let installed = true;
        let managed = true;
        if (id == "R02-absent" || id == "R03-install") {
            f.absent("0.11.0");
            installed = id == "R03-install";
            if (installed) {
                args.push("--install");
            }
        }
        if (id == "R04-channel" || id == "R05-channel-install") {
            f.project("nightly", key = "channel");
            expected = "0.12.0";
            if (id == "R05-channel-install") {
                f.os(["remove", "baml/state.toml"]);
                f.absent("0.12.0");
                args.push("--install");
            }
        }
        if (id == "R06-local" || id == "R07-invalid-local") {
            f.project("./build/baml-cli", key = "path");
            expected = null;
            managed = false;
            if (id == "R07-invalid-local") {
                f.os(["remove", `project/build/baml-cli${f.suffix}`]);
                f.manage(c, args, success = false, diagnostic = "not found");
                require(f.requests().length() == 0, "invalid local resolution used network");
                return null;
            }
        }
        if (id == "R08-help") {
            let output = f.manage(c, ["toolchain", "resolve", "--help"]);
            require(!output.stdout.to_string().ends_with("null\n"), "help printed return value");
            require(f.requests().length() == 0, "resolve help used network");
            return null;
        }
        let output = f.manage(c, args);
        let result = baml.json.decode<ResolutionResult>(output.stdout.to_string());
        require(result.schema_version == 1 && result.version == expected, "unexpected resolved version");
        require(result.managed == managed && result.installed == installed, "unexpected resolution state");
        require(result.source.ends_with("baml.toml"), "resolution omitted selection source");
        require(baml.fs.exists(result.wrapper_path), "wrapper path does not exist");
        let binary = if (managed) {
            `${f.root}/baml/toolchains/${expected ?? ""}/bin/baml-cli${f.suffix}`
        } else {
            `${f.root}/project/build/baml-cli${f.suffix}`
        };
        require(
            if (installed) {
                baml.fs.canonicalize(result.toolchain_path) == baml.fs.canonicalize(binary)
            } else {
                result.toolchain_path.replace_all("\\", "/") == binary.replace_all("\\", "/")
            },
            "wrong toolchain path",
        );
        if (id == "R03-install" || id == "R05-channel-install") {
            require(f.requests().length() > 0, "resolve --install did not download");
            f.check(c, expected ?? "");
        } else {
            require(f.requests().length() == 0, "resolution without --install used network");
        }
        return null;
    }
    if (id.starts_with("E13")) {
        let args = if (id == "E13-project-missing") {
            ["--project"]
        } else if (id == "E13-project-absent") {
            ["--project", "../absent", "check"]
        } else {
            ["--project", ".", "--project", ".", "check"]
        };
        let result = f.run(c, args);
        require(!result.ok(), "invalid project flags succeeded");
        require(
            f.calls().length() == 0 && f.requests().length() == 0,
            "invalid project flags caused side effects",
        );
        return null;
    }
    if (id.starts_with("I2") && id != "I20" && id != "I21" && id != "I21-stale-lock") {
        let selected = "0.12.0";
        f.project(selected);
        if (id == "I23-bare-env") {
            selected = "0.13.0";
            f.env["BAML_TOOLCHAIN"] = selected;
        }
        if (id == "I24-bare-channel" || id == "I25-bare-force") {
            f.project("nightly", key = "channel");
            f.os(["channel", "nightly", "0.13.0"]);
        }
        let state = f.read("baml/state.toml");
        let args = ["toolchain", "install"];
        if (id == "I25-bare-force") {
            args.push("--force");
            f.os(["release", selected, "valid", "A2"]);
        } else {
            f.absent(selected);
        }
        f.manage(c, args);
        require(f.read("baml/state.toml") == state, "bare install advanced an existing channel");
        f.check(
            c,
            if (id == "I25-bare-force") {
                "A2"
            } else {
                selected
            },
        );
        return null;
    }
    if (id.starts_with("U")) {
        if (id == "U07-local" || id == "U08-exact") {
            f.write("baml/config.toml", "[update]\nauto_check = true\n");
            if (id == "U07-local") {
                f.project("./build/baml-cli", key = "path");
                f.check(c, "L1");
            } else {
                f.project("0.11.0");
                f.check(c, "0.11.0");
            }
            require(f.requests().length() == 0, "non-channel selection refreshed metadata");
            return null;
        }
        f.project("canary", key = "channel");
        let warning = "warning: Your version of baml for toolchain: canary is outdated. Update it with baml toolchain update.\n";
        if (id == "U01-cached-outdated" || id == "U03-refresh") {
            f.os(["channel", "canary", "0.12.0"]);
        }
        if (id == "U01-cached-outdated" || id == "U02-cached-current" || id == "U06-status-cache") {
            f.manage(c, ["toolchain", "status"]);
        }
        if (id == "U03-refresh" || id == "U04-offline" || id == "U06-status-cache") {
            f.write("baml/config.toml", "[update]\nauto_check = true\n");
        }
        if (id == "U04-offline") {
            f.write("server/canary.json.status", "500");
        }
        f.check(
            c,
            "0.11.0",
            expected_stderr = if (id == "U01-cached-outdated" || id == "U03-refresh") {
                warning
            } else {
                ""
            },
        );
        let requests = f.requests().length();
        require(
            requests
                == if (id == "U05-optout") {
                    0
                } else {
                    1
                },
            "unexpected freshness request count",
        );
        f.check(
            c,
            "0.11.0",
            expected_stderr = if (id == "U01-cached-outdated" || id == "U03-refresh") {
                warning
            } else {
                ""
            },
        );
        require(f.requests().length() == requests, "freshness attempt was not throttled");
        return null;
    }
    if (id == "M28-previous" || id == "M29-previous-channel") {
        f.project("0.11.0");
        let selector = "0.12.0";
        if (id == "M29-previous-channel") {
            f.project("canary", key = "channel");
            selector = "nightly";
        }
        let output = f.manage(c, ["toolchain", "use", selector]).stdout.to_string();
        require(
            output.includes("previous:") && output.includes("0.11.0") && output.includes("0.12.0"),
            "selection output omitted concrete previous/new version",
        );
        f.check(c, "0.12.0");
        return null;
    }
    if (id == "M30-update-guidance" || id == "M31-upgrade-guidance") {
        f.write("project/baml.toml", "invalid toml [");
        f.env["BAML_TOOLCHAIN"] = "not-a-version";
        let command = if (id == "M30-update-guidance") {
            "update"
        } else {
            "upgrade"
        };
        let output = f.manage(c, [command], success = false).stderr.to_string();
        require(
            output.includes("baml toolchain update") && output.includes("baml self-update"),
            "missing update guidance",
        );
        require(f.requests().length() == 0, "update guidance used network");
        return null;
    }
    if (id == "D01-human-error") {
        let output = f.manage(c, ["toolchain", "use", "./missing/baml-cli"], success = false);
        require(output.stderr.to_string().starts_with("error:"), "management error was not human-readable");
        require(output.stdout.to_string() == "", "management error polluted stdout");
        f.write("project/baml.toml", "[invalid");
        let malformed = f.manage(c, ["toolchain", "resolve"], success = false);
        let diagnostic = malformed.stderr.to_string();
        require(
            diagnostic.starts_with("error:") && diagnostic.includes("baml.toml"),
            "parse error omitted context",
        );
        require(!diagnostic.includes("{\"message\":"), "parse error serialized an error object");
        return null;
    }
    let a = "0.11.0";
    let b = "0.12.0";
    let v = "0.13.0";
    let config_before = f.read("baml/config.toml");
    let project_before = f.read("project/baml.toml");
    if (id.starts_with("S")) {
        let expected = a;
        if (id == "S02" || id == "S13") {
            f.config("canary");
        }
        if (id == "S03") {
            f.config("nightly");
            expected = b;
        }
        if (id == "S04") {
            f.project(b);
            expected = b;
        }
        if (id == "S05") {
            f.project(a);
            f.env["BAML_TOOLCHAIN"] = b;
            expected = b;
        }
        if (id == "S06") {
            f.project(a);
            f.env["BAML_TOOLCHAIN"] = "nightly";
            expected = b;
        }
        if (id == "S07") {
            f.project(a);
            f.env["BAML_TOOLCHAIN"] = `${f.root}/project/build/baml-cli${f.suffix}`;
            expected = "L1";
        }
        if (id.starts_with("S08") || id == "S09") {
            f.project(b);
            f.env["BAML_TOOLCHAIN"]
                = if (id == "S08-empty") {
                    ""
                } else if (id == "S08-space") {
                    " \t "
                } else {
                    ` ${b} \n`
                };
            expected = b;
        }
        if (id == "S10") {
            f.project(b);
            f.cwd = `${f.root}/project/nested/deep`;
            expected = b;
        }
        if (id == "S11" || id == "S12" || id == "S19") {
            f.project(a);
            f.write(
                "project/nested/deep/baml.toml",
                if (id == "S11") {
                    `[toolchain]\nversion = "${b}"\n`
                } else {
                    "[package]\nname = \"nearest\"\n"
                },
            );
            f.cwd = `${f.root}/project/nested/deep`;
            if (id == "S19") {
                f.write("baml/config.toml", "[update]\nauto_check = false\n");
                f.write(
                    "baml/state.toml",
                    `[channels.canary]\nactive_version = "${b}"\nresolved_at = "x"\nmanifest_path = "x"\n`,
                );
            } else {
                f.config(b);
            }
            expected = b;
        }
        if (id == "S13") {
            f.write("baml/config.toml", "[update]\nauto_check = false\n");
        }
        if (id.starts_with("S15")) {
            f.write(
                "project/baml.toml",
                if (id == "S15-syntax") {
                    "[invalid"
                } else {
                    "[toolchain]\nversion = 42\n"
                },
            );
            failed_check(f, c, diagnostic = "baml.toml");
            require(f.requests().length() == 0, "invalid project used network");
            return null;
        }
        if (id.starts_with("S16")) {
            f.write(
                "project/baml.toml",
                `[toolchain]\nversion = "${a}"\nchannel = ${if (id == "S16-valid") { "\"nightly\"" } else { "42" }}\npath = ${if (id == "S16-valid") { "\"./build/baml-cli\"" } else { "false" }}\n`,
            );
        }
        if (id == "S17") {
            f.write("project/baml.toml", "[invalid");
            f.env["BAML_TOOLCHAIN"] = b;
            expected = b;
        }
        if (id == "S18") {
            f.project(a);
            f.check(c, a);
            f.write("outside/baml.toml", `[toolchain]\nversion = "${b}"\n`);
            f.cwd = `${f.root}/outside`;
            expected = b;
        }
        let before_config = f.read("baml/config.toml");
        let before_project = f.read("project/baml.toml");
        f.check(c, expected);
        require(
            f.read("baml/config.toml") == before_config
                && f.read("project/baml.toml") == before_project,
            "launch changed selectors",
        );
        require(f.requests().length() == 0, "installed selection contacted release server");
        return null;
    }
    if (id.starts_with("L")) {
        let expected = "L1";
        f.env["BAML_TOOLCHAIN"] = `${f.root}/project/build/baml-cli${f.suffix}`;
        if (id == "L02" || id == "L03") {
            f.cwd = `${f.root}/project/nested/deep`;
        }
        if (id == "L02") {
            f.env["BAML_TOOLCHAIN"] = `./build/baml-cli${f.suffix}`;
            expected = "L2";
        }
        if (id == "L03") {
            f.env["BAML_TOOLCHAIN"] = "";
            f.project(`./build/baml-cli${f.suffix}`, key = "path");
        }
        if (id == "L04") {
            f.env["BAML_TOOLCHAIN"] = `~/build/baml-cli${f.suffix}`;
        }
        if (id == "L05") {
            f.env["BAML_TOOLCHAIN"] = `${f.root}/project/space ünicode/baml-cli${f.suffix}`;
        }
        if (id == "L06") {
            f.cwd = `${f.root}/project/nested/deep`;
            f.env["BAML_TOOLCHAIN"] = `../../build/baml-cli${f.suffix}`;
        }
        if (id == "L06-symlink") {
            f.os(["symlink", "project/link", "outside/build"]);
            f.env["BAML_TOOLCHAIN"] = "./link/../build/baml-cli";
            expected = "L2";
        }
        if (id == "L07") {
            f.env["BAML_TOOLCHAIN"] = "";
            f.config(`./build/baml-cli${f.suffix}`);
            f.check(c, "L1");
            f.cwd = `${f.root}/outside`;
            expected = "L2";
        }
        if (id == "L08") {
            f.os(["symlink", "project/current", "project/build/baml-cli"]);
            f.env["BAML_TOOLCHAIN"] = `${f.root}/project/current`;
            f.check(c, "L1");
            f.os(["symlink", "project/current", "outside/build/baml-cli"]);
            expected = "L2";
        }
        if (id == "L09" || id == "L10") {
            f.env["BAML_TOOLCHAIN"]
                = if (id == "L09") {
                    `${f.root}/missing/baml-cli`
                } else {
                    `${f.root}/project/build`
                };
            failed_check(
                f,
                c,
                diagnostic = if (id == "L09") {
                    "BAML_TOOLCHAIN"
                } else {
                    "directory"
                },
            );
            require(f.requests().length() == 0, "local failure contacted server");
            return null;
        }
        if (id == "L14") {
            f.env["BAML_TOOLCHAIN"] = `${f.root}/project/build/baml-cli.exe`.replace_all("/", "\\");
        }
        if (id == "L15") {
            f.env["BAML_TOOLCHAIN"] = `${f.root}/project/build/baml-cli`;
        }
        f.check(c, expected);
        require(f.requests().length() == 0, "local toolchain contacted release server");
        return null;
    }
    if (id.starts_with("I")) {
        if (id == "I07") {
            f.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
            f.check(c, a);
            require(f.requests().length() == 0, "installed toolchain contacted server");
            return null;
        }
        if (id.starts_with("I09")) {
            if (id == "I09-binary") {
                f.os(["remove", `baml/toolchains/${a}/bin/baml-cli${f.suffix}`]);
            }
            if (id == "I09-VERSION") {
                f.os(["remove", `baml/toolchains/${a}/VERSION`]);
            }
            if (id == "I09-mismatch") {
                f.write(`baml/toolchains/${a}/VERSION`, b);
            }
            if (id == "I09-link") {
                f.absent(a);
                f.os(["symlink", `baml/toolchains/${a}`, "missing-installation"]);
            }
            if (id == "I09-file") {
                f.absent(a);
                f.os(["copy", baml.env.get_or_panic("WRAPPER_STUB"), `baml/toolchains/${a}`]);
            }
            failed_check(f, c, diagnostic = "--force");
            require(f.requests().length() == 0, "damaged installation was repaired");
            f.manage(c, ["toolchain", "install", a, "--force"]);
            f.check(c, a);
            return null;
        }
        if (id == "I10") {
            f.absent(a);
            f.config(b);
            f.manage(c, ["toolchain", "install", a]);
            f.check(c, b);
            require(f.read("baml/config.toml").includes(b), "install changed default");
            return null;
        }
        if (id == "I11" || id == "I11-failure") {
            f.os(
                [
                    "release",
                    a,
                    if (id == "I11") {
                        "valid"
                    } else {
                        "checksum"
                    },
                    "A2",
                ],
            );
            f.manage(c, ["toolchain", "install", a, "--force"], success = id == "I11");
            f.check(
                c,
                if (id == "I11") {
                    "A2"
                } else {
                    a
                },
            );
            return null;
        }
        f.absent(a);
        if (id == "I21-stale-lock") {
            f.write(`baml/toolchains/.${a}.lock`, "stale owner metadata\n");
        }
        let expected = a;
        if (id == "I02") {
            f.config(b);
            f.project(a);
        }
        if (id == "I03") {
            f.project(a);
            f.env["BAML_TOOLCHAIN"] = b;
            f.absent(b);
            expected = b;
        }
        if (id == "I04") {
            f.config("canary");
            f.os(["channel", "canary", b]);
        }
        if (id == "I05" || id == "I06") {
            f.config("canary");
            f.write("baml/state.toml", "");
        }
        if (id == "I06") {
            f.os(["remove", "baml"]);
        }
        if (id == "I21") {
            f.behavior(a, "{\"ok\":true}\n");
        }
        if (
            id.starts_with("I12")
                || id.starts_with("I13")
                || id.starts_with("I14")
                || id.starts_with("I15")
                || id == "I16"
                || id == "I17"
        ) {
            if (id == "I12-timeout") {
                f.write(`server/version/${a}.json.delay`, "31");
            }
            if (id == "I12-unreachable") {
                f.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
            }
            if (id == "I12-404" || id == "I12-500" || id == "I17") {
                f.write(
                    `server/version/${a}.json.status`,
                    if (id == "I12-500") {
                        "500"
                    } else {
                        "404"
                    },
                );
            }
            if (id == "I12-json") {
                f.write(`server/version/${a}.json`, "not json");
            }
            if (id == "I12-schema" || id == "I12-target" || id == "I13-checksum") {
                f.os(["release", a, id.split("-")[1], ""]);
            }
            let extension = if (f.windows) {
                ".zip"
            } else {
                ".tar.gz"
            };
            if (id == "I13-404") {
                f.write(`server/${a}${extension}.status`, "404");
            }
            if (id == "I13-disconnect") {
                f.write(`server/${a}${extension}.disconnect`, "true");
            }
            if (id == "I14-cli" || id == "I14-layout" || id.starts_with("I15")) {
                f.os(
                    [
                        "release",
                        a,
                        if (id == "I14-cli") {
                            "missing-cli"
                        } else {
                            id.split("-")[1]
                        },
                        "",
                    ],
                );
            }
            if (id == "I16") {
                f.os(["chmod", "baml/toolchains", "555"]);
            }
            failed_check(f, c);
            if (id == "I16") {
                f.os(["chmod", "baml/toolchains", "755"]);
                f.config(b);
                f.check(c, b);
            }
            if (id != "I16") {
                require(
                    !baml.fs.exists(`${f.root}/baml/toolchains/${a}/VERSION`),
                    "failed install published VERSION",
                );
            }
            require(
                !baml.fs.exists(`${f.root}/baml/escaped`)
                    && !baml.fs.exists(`${f.root}/baml/toolchains/escaped`),
                "archive escaped staging",
            );
            if (id == "I17") {
                f.os(["remove", `server/version/${a}.json.status`]);
                f.check(c, a);
            }
            if (id.starts_with("I13")) {
                f.config(b);
                f.check(c, b);
            }
            return null;
        }
        f.check(
            c,
            expected,
            args = ["check"],
            output = if (id == "I21") {
                "{\"ok\":true}\n"
            } else {
                null
            },
        );
        require(
            f.read(`baml/toolchains/${expected}/VERSION`).trim() == expected,
            "installation VERSION differs",
        );
        let requests = f.requests();
        require(
            requests.some((path) -> bool {
                path.includes(
                    if (id == "I05" || id == "I06") {
                        "canary.json"
                    } else {
                        `/version/${expected}.json`
                    },
                )
            }),
            "missing manifest request",
        );
        require(
            requests.some((path) -> bool {
                path.ends_with(".tar.gz") || path.ends_with(".zip")
            }),
            "missing archive request",
        );
        if (id == "I04") {
            require(!requests.includes("/canary.json"), "recorded channel silently advanced");
        }
        if (id == "I05" || id == "I06") {
            require(channel_version(f, "canary") == a, "channel not recorded");
        }
        if (id == "I06" || id == "I08") {
            f.behavior(
                a,
                "changed",
                code = 7,
                args = ["format", "--flag", "a b"],
                stderr = "stderr",
            );
            f.check(c, a, args = ["format", "--flag", "a b"], output = "changed", code = 7);
            require(f.requests().length() == requests.length(), "reused toolchain downloaded again");
        }
        return null;
    }
    if (id.starts_with("M")) {
        if (id == "M01" || id == "M05") {
            f.config(b);
            if (id == "M05") {
                f.absent(a);
            }
            let before = f.read("baml/config.toml");
            f.manage(c, ["toolchain", "use", a]);
            require(f.read("baml/config.toml") == before, "project use changed user config");
            require(f.read("project/baml.toml").includes(a), "project not selected");
            f.check(c, a);
            f.cwd = `${f.root}/outside`;
            f.check(c, b);
            return null;
        }
        if (id == "M02" || id.starts_with("M06")) {
            let channel = if (id == "M06-canary") {
                "canary"
            } else {
                "nightly"
            };
            let expected = if (channel == "canary") {
                a
            } else {
                b
            };
            f.write("baml/state.toml", "");
            f.absent(expected);
            f.manage(c, ["toolchain", "use", channel]);
            require(f.read("baml/config.toml") == config_before, "channel use changed global default");
            require(channel_version(f, channel) == expected, "channel not recorded");
            require(f.read("project/baml.toml").includes(channel), "channel selector not written");
            f.check(c, expected);
            return null;
        }
        if (id == "M03" || id == "M04") {
            f.project(a);
            let before = f.read("project/baml.toml");
            f.cwd = `${f.root}/project/nested/deep`;
            f.manage(
                c,
                [
                    "toolchain",
                    "use",
                    "--local",
                    if (id == "M03") {
                        `./build/baml-cli${f.suffix}`
                    } else {
                        b
                    },
                ],
            );
            require(f.read("project/baml.toml") == before, "--local changed project");
            f.check(c, a);
            f.cwd = `${f.root}/outside`;
            f.check(
                c,
                if (id == "M03") {
                    "L2"
                } else {
                    b
                },
            );
            return null;
        }
        if (id == "M07") {
            f.cwd = `${f.root}/project/nested/deep`;
            f.manage(c, ["toolchain", "use", `./build/baml-cli${f.suffix}`]);
            f.check(c, "L1");
            f.cwd = `${f.root}/project`;
            f.check(c, "L1");
            return null;
        }
        if (id.starts_with("M08")) {
            let key = id.split("-")[1];
            let old = if (key == "channel") {
                "canary"
            } else if (key == "path") {
                "./build/baml-cli"
            } else {
                a
            };
            f.write(
                "project/baml.toml",
                `# retain me\n[package]\nname = "fixture"\n[toolchain]\n${key} = "${old}" # selector comment\n[custom]\nvalue = 42\ndate = 2026-01-01\n`,
            );
            f.manage(c, ["toolchain", "use", b]);
            let updated = f.read("project/baml.toml");
            require(
                updated.includes("retain me")
                    && updated.includes("selector comment")
                    && updated.includes("value = 42"),
                "comments or unknown data lost",
            );
            require(
                !updated.includes("channel =") && !updated.includes("path ="),
                "multiple selectors remain",
            );
            f.check(c, b);
            return null;
        }
        if (id == "M09") {
            f.cwd = `${f.root}/outside`;
            f.absent(v);
            f.manage(c, ["toolchain", "use", v], success = false, diagnostic = "--local");
            require(
                f.read("baml/config.toml") == config_before
                    && !baml.fs.exists(`${f.root}/outside/baml.toml`)
                    && f.requests().length() == 0,
                "use outside project mutated state",
            );
            return null;
        }
        if (id.starts_with("M10")) {
            f.project(a);
            f.absent(b);
            let before = f.read("project/baml.toml");
            let args = ["toolchain", "use"];
            if (id.includes("local")) {
                args.push("--local");
            }
            args.push(
                if (id.ends_with("path")) {
                    "./missing/baml-cli"
                } else {
                    b
                },
            );
            f.write(`server/version/${b}.json.status`, "500");
            f.manage(c, args, success = false);
            require(
                f.read("project/baml.toml") == before && f.read("baml/config.toml") == config_before,
                "failed use changed selectors",
            );
            f.check(c, a);
            return null;
        }
        if (
            id == "M11"
                || id == "M13"
                || id.starts_with("M15")
                || id == "M22"
                || id == "M23"
                || id == "M24"
                || id == "M25"
        ) {
            f.config("canary");
            let channel = "canary";
            let expected = v;
            if (id == "M11") {
                expected = b;
            }
            if (id.starts_with("M15")) {
                channel = "nightly";
                if (id == "M15-env") {
                    f.env["BAML_TOOLCHAIN"] = "nightly";
                } else {
                    f.project("nightly", key = "channel");
                }
            }
            if (id == "M22") {
                f.config("nightly");
                f.project("nightly", key = "channel");
                f.env["BAML_TOOLCHAIN"] = "canary";
            }
            if (id == "M23") {
                f.write("baml/config.toml", "[update]\nauto_check = false\n");
            }
            if (id == "M24") {
                f.project("nightly", key = "channel");
                f.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
                f.cwd = `${f.root}/project/nested/deep`;
            }
            if (id == "M25") {
                f.project("nightly", key = "channel");
                f.write("outside/baml.toml", "[toolchain]\nchannel = \"nightly\"\n");
                channel = "nightly";
            }
            f.os(["channel", channel, expected]);
            f.absent(expected);
            let before_project = f.read("project/baml.toml");
            let before_config = f.read("baml/config.toml");
            if (id == "M13") {
                f.write(`server/${channel}.json.status`, "500");
            }
            f.manage(c, ["toolchain", "update"], success = id != "M13");
            require(
                f.read("project/baml.toml") == before_project
                    && f.read("baml/config.toml") == before_config,
                "update changed selector",
            );
            if (id == "M13") {
                f.check(c, a);
            } else {
                require(channel_version(f, channel) == expected, "wrong shared channel updated");
                f.check(c, expected);
            }
            if (id.starts_with("M15")) {
                require(channel_version(f, "canary") == a, "unselected channel changed");
            }
            if (id == "M22") {
                require(channel_version(f, "nightly") == b, "unselected channel changed");
            }
            if (id == "M25") {
                f.cwd = `${f.root}/outside`;
                f.check(c, expected);
            }
            return null;
        }
        if (id.starts_with("M14")) {
            let selected = if (id.ends_with("path")) {
                `${f.root}/project/build/baml-cli${f.suffix}`
            } else {
                a
            };
            if (id.includes("env")) {
                f.env["BAML_TOOLCHAIN"] = selected;
            } else if (id.includes("project")) {
                f.project(
                    selected,
                    key = if (id.ends_with("path")) {
                        "path"
                    } else {
                        "version"
                    },
                );
            } else {
                f.config(selected);
            }
            let state = f.read("baml/state.toml");
            f.manage(
                c,
                ["toolchain", "update"],
                success = true,
                diagnostic = if (id.ends_with("path")) {
                    "local path"
                } else {
                    "does not advance"
                },
            );
            require(
                f.requests().length() == 0 && f.read("baml/state.toml") == state,
                "non-channel update performed work",
            );
            return null;
        }
        if (id == "M16" || id.starts_with("M17") || id.starts_with("M18")) {
            if (id == "M18-channel") {
                f.config("canary");
                f.os(["channel", "canary", b]);
            }
            let state = f.read("baml/state.toml");
            let config = f.read("baml/config.toml");
            let selected = if (id == "M16") {
                b
            } else if (id == "M17-absent") {
                "9.9.9"
            } else if (id == "M17-path") {
                `${f.root}/project/build/baml-cli${f.suffix}`
            } else if (id == "M17-channel") {
                "canary"
            } else {
                a
            };
            f.manage(c, ["toolchain", "uninstall", selected], success = !id.starts_with("M17"));
            require(
                f.read("baml/state.toml") == state && f.read("baml/config.toml") == config,
                "uninstall changed selection or mapping",
            );
            if (!id.starts_with("M17")) {
                require(
                    !baml.fs.exists(`${f.root}/baml/toolchains/${selected}`),
                    "uninstall did not remove directory",
                );
            }
            f.check(c, a);
            return null;
        }
        if (id.starts_with("M19")) {
            if (id == "M19-empty") {
                f.os(["remove", "baml/toolchains"]);
            }
            if (id == "M19-override") {
                f.env["BAML_TOOLCHAIN"] = b;
            }
            let before = f.read("baml/state.toml");
            let listing = f.manage(c, ["toolchain", "list"]).stdout.to_string();
            require(listing.includes(`default selector: ${a}`), "list omitted the user default");
            if (id == "M19-empty") {
                require(
                    listing.includes("installed toolchains: (none)"),
                    "list omitted empty installation state",
                );
            } else {
                require(listing.includes(b) && listing.includes(v), "list omitted installed versions");
            }
            if (id == "M19-override") {
                require(listing.includes(`active selector: ${b}`), "list omitted the active override");
            }
            require(
                f.requests().length() == 0
                    && f.read("baml/config.toml") == config_before
                    && f.read("baml/state.toml") == before,
                "list changed state or downloaded",
            );
            return null;
        }
        if (id == "M20" || id == "M21") {
            f.config("canary");
            f.absent(b);
            f.os(["channel", "canary", b]);
            if (id == "M21") {
                f.write("server/canary.json.status", "500");
            }
            let before = f.read("baml/state.toml");
            f.manage(
                c,
                ["toolchain", "status"],
                success = id == "M20",
                diagnostic = if (id == "M20") {
                    "update available"
                } else {
                    ""
                },
            );
            require(
                f.read("baml/state.toml") == before
                    && !baml.fs.exists(`${f.root}/baml/toolchains/${b}`),
                "status installed or changed state",
            );
            f.check(c, a);
            return null;
        }
        if (id.starts_with("M26")) {
            f.cwd = `${f.root}/outside`;
            f.absent(b);
            if (id == "M26-home") {
                f.env["BAML_HOME"] = "";
                f.os(["remove", "baml"]);
            }
            f.manage(c, ["toolchain", "use", "--local", b]);
            require(!baml.fs.exists(`${f.root}/outside/baml.toml`), "--local created manifest");
            f.check(c, b);
            return null;
        }
        if (id == "M27") {
            f.project(a);
            f.write("project/nested/deep/baml.toml", "[package]\nname = \"nearest\"\n");
            let before = f.read("project/baml.toml");
            f.cwd = `${f.root}/project/nested/deep`;
            f.manage(c, ["toolchain", "use", b]);
            require(
                f.read("project/baml.toml") == before && f.read("baml/config.toml") == config_before,
                "use changed parent/global",
            );
            f.check(c, b);
            return null;
        }
    }
    if (id == "V01") {
        let output = f.manage(c, ["--version"]);
        require(
            output.stdout.to_string().includes("wrapper") && output.stdout.to_string().includes(a),
            "version does not identify wrapper and toolchain",
        );
        if let expected: string = c.version {
            require(
                output.stdout.to_string().includes(`baml wrapper ${expected}`),
                "wrapper version differs from build version",
            );
        }
        return null;
    }
    if (id.starts_with("E")) {
        if (id == "E01") {
            f.write("baml/config.toml", `[default]\nselector = "${a}"\n[update]\nauto_check = []\n`);
            f.check(c, a);
            return null;
        }
        if (id == "E02") {
            f.config("canary");
            f.write(
                "baml/state.toml",
                `[channels.canary]\nactive_version = "${a}"\nresolved_at = "x"\nmanifest_path = "x"\n[channels.nightly]\nactive_version = []\n`,
            );
            f.check(c, a);
            return null;
        }
        if (id == "E02-metadata") {
            f.config("canary");
            f.write(
                "baml/state.toml",
                `[channels.canary]\nactive_version = "${a}"\nresolved_at = []\nmanifest_path = 42\n`,
            );
            f.check(c, a);
            return null;
        }
        if (id == "E03") {
            f.write(
                "baml/config.toml",
                `${config_before}\n[custom]\nvalue = 42\ndate = 2026-01-01\n`,
            );
            f.write(
                "baml/state.toml",
                f.read("baml/state.toml") + "\n[custom]\nvalue = 43\ndate = 2026-01-01\n",
            );
            f.manage(c, ["toolchain", "use", "--local", "nightly"]);
            require(
                f.read("baml/config.toml").includes("value = 42")
                    && f.read("baml/state.toml").includes("value = 43"),
                "unknown fields lost",
            );
            f.check(c, b);
            return null;
        }
        if (id == "E04") {
            f.config("canary");
            f.os(["channel", "canary", b]);
            f.absent(b);
            // Prime metadata before denying the final state commit. Unique
            // temporary filenames must not be part of this failure fixture.
            f.manage(c, ["toolchain", "status"]);
            let blocked = if (f.windows) {
                "baml/state.toml"
            } else {
                "baml"
            };
            f.os(
                [
                    "chmod",
                    blocked,
                    if (f.windows) {
                        "444"
                    } else {
                        "555"
                    },
                ],
            );
            let state = f.read("baml/state.toml");
            let output = f.run(c, ["toolchain", "update"]);
            f.os(
                [
                    "chmod",
                    blocked,
                    if (f.windows) {
                        "644"
                    } else {
                        "755"
                    },
                ],
            );
            require(!output.ok(), "state commit unexpectedly succeeded");
            require(
                baml.fs.exists(`${f.root}/baml/toolchains/${b}/VERSION`),
                "update failed before installation",
            );
            require(f.read("baml/state.toml") == state, "failed commit changed channel");
            f.check(c, a);
            return null;
        }
        if (id.starts_with("E05")) {
            if (id.starts_with("E05-env")) {
                f.env["BAML_TOOLCHAIN"]
                    = if (id == "E05-env-prerelease") {
                        "1.2.3-01"
                    } else if (id == "E05-env-overflow") {
                        "18446744073709551616.0.0"
                    } else {
                        "not-a-version"
                    };
            } else if (id == "E05-project") {
                f.project("../../escape");
            } else {
                f.config("canary");
                f.write(
                    "baml/state.toml",
                    "[channels.canary]\nactive_version = \"../../escape\"\nresolved_at = \"x\"\nmanifest_path = \"x\"\n",
                );
            }
            failed_check(f, c);
            require(f.requests().length() == 0, "invalid managed version contacted server");
            return null;
        }
        if (id.starts_with("E06")) {
            f.absent(a);
            if (id == "E06-channel") {
                f.config("nightly");
                f.write("baml/state.toml", "");
                f.write("server/nightly.json", f.read("server/canary.json"));
            } else {
                f.write(`server/version/${a}.json`, f.read(`server/version/${b}.json`));
            }
            failed_check(f, c);
            require(
                !baml.fs.exists(`${f.root}/baml/toolchains/${a}/VERSION`),
                "inconsistent manifest activated",
            );
            return null;
        }
        if (id.starts_with("E07")) {
            if (id == "E07-config") {
                f.write("baml/config.toml", "[default]\nselector = []\n");
            } else {
                f.config("canary");
                f.write("baml/state.toml", "[channels.canary]\nactive_version = []\n");
            }
            failed_check(f, c);
            require(f.requests().length() == 0, "malformed resolution treated as absent");
            return null;
        }
        let path = "baml/config.toml";
        let content = "[default]\nselector = \"0.11.0\"\n[update]\nauto_check = []\n";
        let args = ["toolchain", "use", "--local", b];
        f.absent(b);
        if (id.starts_with("E09") || id == "E10-project" || id == "E12-project") {
            path = "project/baml.toml";
            args = ["toolchain", "use", b];
            content
                = if (id.starts_with("E09")) {
                    "[package]\nname = 42\n"
                } else {
                    "[toolchain]\nversion = []\n"
                };
        }
        if (id == "E09-dependencies") {
            content = "[dependencies]\nhelper = { path = 42 }\n";
        }
        if (id == "E09-scripts") {
            content = "[scripts]\ndev = 42\n";
        }
        if (id == "E09-generator") {
            content = "[generator.client]\noutput_type = 42\n";
        }
        if (id == "E09-test") {
            content = "[test.profiles.ci]\nargs = 42\n";
        }
        if (id == "E09-date") {
            content = "[generator.client]\noutput_type = 2026-01-01\n";
        }
        if (id == "E10-local") {
            content = "[default]\nselector = []\n";
        }
        if (id.starts_with("E11") || id == "E12-state") {
            path = "baml/state.toml";
            f.config("canary");
            f.os(["channel", "canary", b]);
            content
                = `[channels.canary]\nactive_version = "${a}"\nresolved_at = "x"\nmanifest_path = "x"\n[channels.nightly]\nactive_version = []\n`;
            args
                = if (id == "E11-auto") {
                    ["check"]
                } else {
                    ["toolchain", "update"]
                };
            if (id == "E11-auto") {
                f.absent(a);
            }
        }
        if (id.starts_with("E12")) {
            content = "[invalid";
        }
        f.write(path, content);
        f.manage(c, args, success = false);
        require(
            f.read(path) == content
                && f.requests().length() == 0
                && !baml.fs.exists(`${f.root}/baml/toolchains/${b}`),
            "write did not preflight complete document",
        );
        return null;
    }
    if (id.starts_with("W")) {
        if (id == "W01") {
            f.manage(c, ["self-update"], success = false, diagnostic = "package manager");
            require(f.requests().length() == 0, "disabled self-update downloaded");
            return null;
        }
        require(c.command.length() == 1, "self-update requires a copied executable candidate");
        f.os(["copy", c.command[0], `wrapper${f.suffix}`]);
        let copy = Candidate { label: c.label, command: [`${f.root}/wrapper${f.suffix}`], self_update: true };
        f.os(
            [
                "wrapper",
                if (id == "W03-checksum") {
                    "checksum"
                } else {
                    "valid"
                },
            ],
        );
        if (id == "W03-download") {
            f.write("server/wrapper.json.status", "500");
        }
        if (id == "W03-replace") {
            f.os(["chmod", `wrapper${f.suffix}`, "555"]);
        }
        if (id == "W03-replace") {
            baml.fs.mkdir(`${f.root}/wrapper.tmp`, baml.fs.MkdirOptions { recursive: true });
            baml.fs.mkdir(`${f.root}/wrapper.exe.tmp`, baml.fs.MkdirOptions { recursive: true });
        }
        let state = f.read("baml/state.toml");
        f.manage(copy, ["self-update"], success = id == "W02" || id == "W04");
        if (id == "W02" || id == "W04") {
            f.behavior("replacement", "replacement\n");
            let attempts = 0;
            while (attempts < 100) {
                let output = f.run(copy, ["check"]);
                if (output.stdout.to_string() == "replacement\n") {
                    break;
                }
                attempts += 1;
                baml.sys.sleep(baml.time.Duration.from_milliseconds(50));
            }
            require(
                f.calls().some((call) -> bool {
                    call.identity == "replacement"
                }),
                "replacement wrapper did not take effect",
            );
        } else {
            f.check(copy, a);
        }
        require(
            f.read("baml/state.toml") == state
                && f.read("baml/config.toml") == config_before
                && f.read("project/baml.toml") == project_before,
            "self-update changed toolchain selection",
        );
        return null;
    }
    if (id.starts_with("F08")) {
        let insecure = "http://releases.invalid/manifest";
        let args = ["toolchain", "install", a];
        if (id == "F08-env") {
            f.env["BAML_MANIFEST_BASE_URL"] = insecure;
        } else {
            args = args.concat(["--manifest-base-url", insecure]);
        }
        let output = f.manage(c, args, success = false);
        require(output.stderr.to_string().includes("HTTPS"), "insecure manifest source was not rejected");
        require(f.requests().length() == 0, "invalid source contacted the fixture server");
        return null;
    }
    if (id == "F07") {
        let release = "0.11.1-nightly.20261026.a+build.001";
        f.os(["release", release, "valid", ""]);
        f.behavior(release, `${release}\n`);
        f.env["BAML_TOOLCHAIN"] = release;
        f.check(c, release);
        return null;
    }
    if (id == "F05") {
        f.absent(a);
        let url = f.env["BAML_MANIFEST_BASE_URL"];
        f.env["BAML_MANIFEST_BASE_URL"] = "http://127.0.0.1:1";
        f.manage(c, ["toolchain", "--manifest-base-url", url, "install", a]);
        f.env["BAML_MANIFEST_BASE_URL"] = url;
        f.check(c, a);
        return null;
    }
    if (id == "F06") {
        f.write(
            "baml/state.toml",
            `[channels.canary]\nactive_version = "${a}"\nresolved_at = "x"\nmanifest_path = "x"\nmanifest_base_url = "http://127.0.0.1:1"\n`,
        );
        f.os(["channel", "canary", v]);
        f.absent(v);
        f.manage(
            c,
            ["toolchain", "use", "canary", `--manifest-base-url=${f.env["BAML_MANIFEST_BASE_URL"]}`],
        );
        f.check(c, v);
        return null;
    }
    if (id == "F04") {
        duplex(f, c);
        return null;
    }
    if (id == "F01") {
        f.behavior(
            a,
            "",
            code = 23,
            args = ["check", "--weird", "a b", "ü"],
            stderr = "diagnostic",
            echo_stdin = true,
        );
        f.check(
            c,
            a,
            args = ["check", "--weird", "a b", "ü"],
            output = "stdin\n\nno final newline",
            code = 23,
            stdin = "stdin\n\nno final newline",
            expected_stderr = "diagnostic",
        );
        return null;
    }
    if (id == "F02") {
        let output = f.manage(c, ["toolchain", "pin", a], success = false);
        require(f.requests().length() == 0, "removed pin command used network");
        return null;
    }
    if (id == "F03") {
        f.manage(c, ["toolchain", "use", "--unknown", a], success = false);
        f.manage(c, ["toolchain", "update", "extra"], success = false);
        require(f.requests().length() == 0, "invalid wrapper arguments used network");
        return null;
    }
    require(false, `unimplemented scenario ${id}`);
    null
}

function duplex(f: Fixture, c: Candidate) -> null {
    let payload = "x".repeat(262144);
    f.behavior("0.11.0", payload, stderr = payload, echo_stdin = true);
    let child = baml.sys.start_process(
        c.command[0],
        c.command.slice(1, c.command.length()).concat(["check"]),
        baml.sys.ProcessOptions {
            cwd: f.cwd,
            env: f.env,
            timeout_ms: 15000,
            stderr: baml.sys.StderrMode.Pipe,
        },
    );
    defer { child.close(); }
    let out = spawn { child.stdout.as<baml.io.Read>.bytes() };
    let err = spawn { (child.stderr ?? baml.sys.panic("missing stderr pipe")).as<baml.io.Read>.bytes() };
    let input = payload.to_utf8().concat(uint8array.from_hex("00ff80fe0a"));
    child.stdin.as<baml.io.Write>.write(input);
    child.stdin.close();
    require(child.wait().ok(), "duplex child failed");
    require(
        (await out).to_hex() == payload.to_utf8().concat(input).to_hex(),
        "stdout bytes changed or were truncated",
    );
    require((await err).to_hex() == payload.to_utf8().to_hex(), "stderr bytes changed or were truncated");
    let calls = f.calls();
    require(
        calls.length() == 1 && calls[0].identity == "0.11.0" && calls[0].args == ["check"],
        "unexpected duplex invocation list",
    );
    null
}

class ResolutionResult {
    schema_version: int,
    selector: string,
    version: string?,
    source: string,
    wrapper_path: string,
    toolchain_path: string,
    managed: bool,
    installed: bool,
}
