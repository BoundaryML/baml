// The release fingerprint gate.
//
// Every BAML-built native binary embeds the commit it was built from
// (`baml_artifact::BUILD_FINGERPRINT`), and canary runtimes only load
// artifacts carrying their own fingerprint. A build job that loses the commit
// ships binaries that reject everything the rest of the release produces, so
// the release workflow runs this over every uploaded build artifact before any
// publisher:
//
//   baml-cli run --from tools/release-fingerprints check -- \
//     --artifacts <one subdirectory per artifact> --commit <release commit>
//
// Every artifact must belong to exactly one family below. A new artifact fails
// the gate until it is classified, so nothing ships unchecked by omission.

/// A family whose native binaries must each carry the release commit.
class Fingerprinted {
    /// Archives in the artifact's top level to unpack before matching.
    unpack: string[],
    /// Globs that must each match at least one binary.
    binaries: string[],
}

/// A family with no BAML-built native code.
class Unfingerprinted {
    why: string,
}

/// The artifacts one build job uploads.
class Family {
    /// The artifact name, or its prefix when it ends in `-` (one artifact per
    /// target or platform).
    name: string,
    expectation: Fingerprinted | Unfingerprinted,

    function contains(self, artifact: string) -> bool {
        if (self.name.ends_with("-")) {
            artifact.starts_with(self.name)
        } else {
            artifact == self.name
        }
    }
}

function families() -> Family[] {
    [
        Family {
            name: "toolchain-",
            expectation: Fingerprinted {
                unpack: ["*.tar.gz", "*.zip"],
                binaries: ["**/bin/{baml-cli,baml-cli.exe}", "**/bin/{baml-pack-host,baml-pack-host.exe}"],
            },
        },
        Family {
            name: "bridge-cffi-",
            expectation: Fingerprinted { unpack: [], binaries: ["*baml_cffi-*.{so,dylib,dll}"] },
        },
        Family {
            name: "python-sdk-",
            expectation: Fingerprinted { unpack: ["*.whl"], binaries: ["**/baml_bridge/baml_py*.{so,pyd}"] },
        },
        Family {
            name: "nodejs-sdk-",
            expectation: Fingerprinted { unpack: [], binaries: ["**/*.node"] },
        },
        Family {
            name: "java-sdk-natives-",
            expectation: Fingerprinted {
                unpack: ["*.jar"],
                binaries: ["**/native/**/*bridge_java.{so,dylib,dll}"],
            },
        },
        Family {
            name: "csharp-product-package",
            expectation: Fingerprinted {
                unpack: ["*.nupkg"],
                binaries: ["**/runtimes/*/native/*bridge_cffi.{so,dylib,dll}"],
            },
        },
        Family {
            name: "swift-xcframework",
            expectation: Fingerprinted { unpack: ["*.xcframework.zip"], binaries: ["**/*.xcframework/**/*.a"] },
        },
        Family {
            name: "web-sdk-dist",
            expectation: Fingerprinted { unpack: [], binaries: ["**/bridge_web_core_bg.wasm"] },
        },
        Family {
            name: "wrapper-",
            expectation: Unfingerprinted { why: "the toolchain selector does not link baml_artifact" },
        },
        Family {
            name: "baml-playground-dist",
            expectation: Unfingerprinted { why: "playground assets run no BAML engine" },
        },
        Family {
            name: "baml-vscode-vsix",
            expectation: Unfingerprinted { why: "the extension runs no BAML engine" },
        },
        Family {
            name: "rust-sdk-crate",
            expectation: Unfingerprinted { why: "source crate that loads bridge-cffi at run time" },
        },
        Family {
            name: "go-sdk-mirror",
            expectation: Unfingerprinted { why: "source mirror that loads bridge-cffi at run time" },
        },
        Family {
            name: "swift-sdk-mirror",
            expectation: Unfingerprinted { why: "source mirror of the swift-xcframework bindings" },
        },
        Family {
            name: "java-sdk-main",
            expectation: Unfingerprinted { why: "JVM classes; the natives are java-sdk-natives-*" },
        },
        Family {
            name: "nodejs-umbrella-dist",
            expectation: Unfingerprinted { why: "JavaScript umbrella; the natives are nodejs-sdk-*" },
        },
        // Uploaded by jobs after the gate; present only when a run is retried.
        Family { name: "release-plan", expectation: Unfingerprinted { why: "release metadata" } },
        Family {
            name: "release-manifests",
            expectation: Unfingerprinted { why: "release metadata" },
        },
        Family {
            name: "dry-run-release-files",
            expectation: Unfingerprinted { why: "release metadata" },
        },
        Family {
            name: "csharp-verification-fixtures",
            expectation: Unfingerprinted { why: "post-release verification inputs" },
        },
    ]
}

/// Whether `value` is a Git object name: 40 (SHA-1) or 64 (SHA-256) lowercase
/// hex digits, the only form `baml_artifact` accepts as a fingerprint.
function is_commit_id(value: string) -> bool {
    let digits = value.chars();
    let hex = "0123456789abcdef".chars();
    (digits.length() == 40 || digits.length() == 64)
        && digits.every((digit) -> bool {
            hex.includes(digit)
        })
}

/// Files under `dir` matching `pattern`, as sorted absolute paths.
function files(dir: string, pattern: string) -> string[] {
    let options = baml.glob.ScanOptions {
        cwd: dir,
        dot: null,
        absolute: true,
        follow_symlinks: null,
        throw_error_on_broken_symlink: null,
        only_files: true,
    };
    baml.glob.new(pattern).scan(options).sort()
}

/// Unpacks `archive` next to itself, returning a problem if it cannot be.
function unpack(archive: string) -> string? {
    let destination = `${archive}.unpacked`;
    baml.fs.mkdir(destination, baml.fs.MkdirOptions { recursive: true });
    let result = if (archive.ends_with(".tar.gz")) {
        baml.sys.exec("tar", ["-xzf", archive, "-C", destination], null)
    } else {
        baml.sys.exec("unzip", ["-q", "-o", archive, "-d", destination], null)
    };
    if (result.ok()) {
        return null;
    }
    `could not unpack ${archive} (exit ${result.exit_code})`
}

/// Returns a problem unless `binary` contains `commit`.
///
/// A plain byte search is exact here: nothing but the build fingerprint embeds
/// the full commit id (bridge_wasm's build stamp is 7 characters).
function fingerprint_problem(binary: string, commit: string) -> string? {
    let result = baml.sys.exec("grep", ["-a", "-q", "-F", "-e", commit, "--", binary], null);
    if (result.exit_code == 0) {
        return null;
    }
    if (result.exit_code == 1) {
        return `${binary} does not carry the release commit (built from another commit, or without one)`;
    }
    `could not search ${binary} (grep exit ${result.exit_code})`
}

/// Every problem with one artifact of a fingerprinted family.
function artifact_problems(dir: string, expectation: Fingerprinted, commit: string) -> string[] {
    let problems: string[] = [];
    for (let pattern in expectation.unpack) {
        for (let archive in files(dir, pattern)) {
            if let problem: string = unpack(archive) {
                problems.push(problem);
            }
        }
    }
    for (let pattern in expectation.binaries) {
        let binaries = files(dir, pattern);
        if (binaries.length() == 0) {
            problems.push(`${dir}: nothing matches ${pattern}`);
        }
        for (let binary in binaries) {
            if let problem: string = fingerprint_problem(binary, commit) {
                problems.push(problem);
            }
        }
    }
    problems
}

/// Checks every artifact in `artifacts` (one subdirectory each, as
/// `actions/download-artifact` lays them out) against `commit`, and exits
/// non-zero on any problem.
function check(artifacts: string, commit: string) -> null {
    if (!is_commit_id(commit)) {
        baml.io.eprintln(`--commit must be a full lowercase Git commit id, got "${commit}"`);
        baml.sys.exit(2);
    }

    let all_families = families();
    let seen: string[] = [];
    let problems: string[] = [];
    let names: string[] = [];
    for (let entry in baml.fs.read_dir(artifacts)) {
        if (entry.is_dir) {
            names.push(entry.name);
        }
    }

    for (let name in names.sort()) {
        let matching = all_families.filter((family) -> bool {
            family.contains(name)
        });
        if (matching.length() != 1) {
            problems.push(
                `${name}: belongs to ${matching.length()} artifact families; classify it in tools/release-fingerprints`,
            );
            continue;
        }
        let family = matching[0];
        seen.push(family.name);
        match (family.expectation) {
            let expectation: Fingerprinted => {
                let found = artifact_problems(`${artifacts}/${name}`, expectation, commit);
                if (found.length() == 0) {
                    baml.io.println(`ok       ${name}`);
                } else {
                    for (let problem in found) {
                        problems.push(`${name}: ${problem}`);
                    }
                }
            },
            let skipped: Unfingerprinted => {
                baml.io.println(`skipped  ${name} (${skipped.why})`);
            },
        }
    }

    // An empty or partial download must not pass by checking nothing.
    for (let family in all_families) {
        match (family.expectation) {
            Fingerprinted => {
                if (!seen.includes(family.name)) {
                    problems.push(`no ${family.name} artifact was downloaded`);
                }
            },
            Unfingerprinted => {},
        }
    }

    if (problems.length() != 0) {
        for (let problem in problems) {
            baml.io.eprintln(`FAIL     ${problem}`);
        }
        baml.sys.exit(1);
    }
    null
}

test "commit ids are full lowercase hex" {
    assert.is_true(is_commit_id("c8ba04b0aac115d43c3c6d02a59706e878aad0d6"));
    assert.is_true(!is_commit_id("C8BA04B0AAC115D43C3C6D02A59706E878AAD0D6"));
    assert.is_true(!is_commit_id("c8ba04b0a"));
    assert.is_true(!is_commit_id("0.20.0"));
    assert.is_true(!is_commit_id(""));
}

test "every artifact name belongs to exactly one family" {
    let names = [
        "toolchain-aarch64-unknown-linux-gnu",
        "bridge-cffi-x86_64-pc-windows-msvc",
        "python-sdk-aarch64-unknown-linux-musl",
        "nodejs-sdk-x86_64-apple-darwin",
        "nodejs-umbrella-dist",
        "java-sdk-natives-linux-aarch64-musl",
        "java-sdk-main",
        "csharp-product-package",
        "swift-xcframework",
        "swift-sdk-mirror",
        "web-sdk-dist",
        "wrapper-x86_64-unknown-linux-musl",
        "baml-playground-dist",
        "baml-vscode-vsix",
        "rust-sdk-crate",
        "go-sdk-mirror",
        "release-plan",
    ];
    for (let name in names) {
        let matching = families().filter((family) -> bool {
            family.contains(name)
        });
        assert.equal(matching.length(), 1);
    }
    assert.equal(
        families()
            .filter((family) -> bool {
                family.contains("new-artifact")
            })
            .length(),
        0,
    );
}
