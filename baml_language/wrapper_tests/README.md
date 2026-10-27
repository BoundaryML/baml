The shared wrapper tests suite is written in BAML. It starts real Rust wrappers in isolated homes, uses an independently compiled configurable `baml-cli` stub, and serves real release manifests and archives over a loopback HTTP server. The fixture helper only prepares OS resources and serves requests; it never invokes the wrapper or installs a managed toolchain for an installation test.

Run `python3 baml_language/wrapper_tests/run` from the repository root. This builds an independent compiler, pack host and stub, snapshots both Rust self-update variants, and runs the same tests against both candidates. It uses `baml test` to report behavior-focused test names for each candidate, emits explicit capability skip diagnostics, and returns nonzero if any candidate fails. `--filter "toolchain selection"` runs the selection tests; `--filter "checksum"` selects checksum behavior. `--target <triple>` also builds the wrappers and pack host for that platform while retaining an independent native compiler/test runtime.

For a prebuilt candidate, use `--no-build --candidate '{"label":"baml","command":["/absolute/path/to/baml"],"self_update":true}'`. Repeat `--candidate` to compare wrapper variants or pass a command prefix in `command`. Adapters pass arguments unchanged and do not repair fixtures. Tests live in behavior files under `baml_src/`, such as `selection.baml`, `installation.baml`, and `telemetry.baml`. Each file groups related behavior in a named BAML `testset`, such as `toolchain selection` or `wrapper telemetry`. Every `test` has a descriptive name and shows its fixture setup, wrapper invocation, and assertions together. Tests create their own `root.harness.Fixture.start()` and register cleanup with `defer { fixture.close(); }`; `baml_src/ns_harness/harness.baml` owns the fixture resources and shared assertions. The Python runner sets one `WRAPPER_CANDIDATE` and invokes ordinary `baml test` separately for each wrapper variant. To add coverage, add a descriptive `test` block to the appropriate `testset`; BAML discovers and filters it directly.

For example, a test lives directly in its behavior group:

```baml
testset "toolchain selection" {
    let candidate = root.harness.Candidate.from_environment();

    test "project version overrides global default" {
        let fixture = root.harness.Fixture.start();
        defer { fixture.close(); }
        fixture.project(NEXT_VERSION);
        fixture.check_installed_selection(candidate, NEXT_VERSION);
    }
}
```

The test appears as `root::toolchain selection::project version overrides global default` in `baml test --list` and failure output. Use `--filter "project version overrides global default"` to run that behavior, or `--filter "toolchain selection"` to run its whole group.

Each fixture starts in an unpinned `project/` with global default `0.11.0`. Versions `0.11.0`, `0.12.0`, and `0.13.0` are already installed; canary is recorded at `0.11.0` and nightly at `0.12.0`. Local stubs identify themselves as `project-local-toolchain` under the project root and home, and `working-directory-toolchain` under the nested working directory and outside the project. Tests change this baseline explicitly before invoking a candidate.

Release gates use `--release-artifacts <downloaded-artifacts-dir> --target <triple> --wrapper-version <version>` to test the exact uploaded Rust and BAML archives. The runner requires every variant shipped for that target in `release/platforms.json`, stages each archived executable unchanged, and fails on missing or ambiguous archives. The harness, compiler, and fixture stub are still built independently.

Coverage includes selector precedence and project discovery, local paths, installation and recovery, management, version reporting, configuration validation, JSON status, passive freshness, wrapper replacement, argument validation, and standard-stream delegation. Delegation checks the full new invocation list, identity, argument array, cwd, and direct-CLI guard. Installation checks release requests and installed metadata; management checks selection changes and subsequent launches. Telemetry tests cover independent PostHog events, opt-out, retries, queue recovery, privacy, and operation outcomes; candidates enable this capability with `"posthog": true`.

Capability requirements are ordinary conditions in the enclosing `testset`. Unsupported tests are excluded during discovery with a `SKIP` diagnostic, so the passed count includes only tests that actually ran. Windows paths, suffix handling and replacement run on Windows. Symlink tests require privileges on Windows and are explicitly excluded there. POSIX permissions are tested on POSIX. Self-update cases are selected by the candidate's build capability; replacement always operates on a fixture copy. Unsupported-host diagnostics are covered by the platform resolver's unit tests, since release candidates are built only for supported hosts. All release targets, including both Linux libc variants and both Windows architectures, are scheduled by `.github/workflows/wrapper-e2e.yaml` using the repository platform contract.

All fixture invocations disable telemetry by default with `DO_NOT_TRACK=1`. Telemetry tests explicitly opt in only against the fixture's loopback capture server. Harness/compiler builds also opt out. Capture assertions cover wrapper implementations and self-update variants without contacting production PostHog.

This first stack layer defines the shared behavior contract. Its default runner exercises Rust variants; the BAML implementation and four-variant release integration are added in [the implementation PR](https://github.com/BoundaryML/baml/pull/5422). The contract tests can fail until subsequent implementation layers provide the specified behavior.
