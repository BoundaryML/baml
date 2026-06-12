//! Reusable Cargo Tests workflow -> `cargo-tests.reusable.yaml`.
//!
//! Mirrors the hand-written `.github/workflows/cargo-tests.reusable.yaml`: a
//! `workflow_call` reusable workflow with the standard cargo test matrix
//! (linux/macos/windows), the sdk-test matrix, wasm/msrv/doc/snapshot jobs, and
//! two artifact-merge fan-in jobs.
//!
//! Several steps are written inline rather than via the shared `steps::*`
//! helpers: the committed YAML's `with:`-key ordering, step names
//! (e.g. `Dump sccache stats (json)`), and exact `run:` bodies (e.g. the
//! `direnv allow .envrc` envrc loader, the per-job `cargo-timing-<id>` rename)
//! must be reproduced byte-for-byte for the drift check, and the generic
//! helpers diverge in those details. Helpers are used where they match exactly
//! (checkout, setup-mise, install-action, verify-sccache, cargo fetch).

#[allow(unused_imports)]
use crate::workflows::{
    runners, steps,
    steps::{FluentBuilder, NamedJob},
    vars,
};
use gh_workflow::{
    Env, Event, Expression, Job, RunJob, Step, Strategy, Workflow, WorkflowCall,
};
use serde_json::json;

use vars::WD;

// ---------------------------------------------------------------------------
// Small inline-step helpers local to this workflow.
// ---------------------------------------------------------------------------

/// H1: `useblacksmith/checkout@v1`, `persist-credentials: false` (unnamed).
fn checkout_blacksmith() -> Step<gh_workflow::Use> {
    let mut s = Step::new("checkout")
        .uses("useblacksmith", "checkout", "v1")
        .add_with(("persist-credentials", false));
    s.value.name = None;
    s
}

/// H2: `actions/checkout@v6`, `persist-credentials: false` (unnamed).
fn checkout_actions() -> Step<gh_workflow::Use> {
    let mut s = Step::new("checkout")
        .uses("actions", "checkout", "v6")
        .add_with(("persist-credentials", false));
    s.value.name = None;
    s
}

/// H3: `Install Rust toolchain` / `rustup show`, wd `baml_language`.
fn rustup_show() -> Step<gh_workflow::Run> {
    Step::new("Install Rust toolchain")
        .run("rustup show")
        .working_directory(WD)
}

/// H3 wasm variant: `rustup show` + add the wasm target.
fn rustup_show_wasm() -> Step<gh_workflow::Run> {
    Step::new("Install Rust toolchain")
        .run("rustup show\nrustup target add wasm32-unknown-unknown")
        .working_directory(WD)
}

/// H7: `Load .envrc with direnv`, wd defaults to repo root (no wd in YAML).
fn load_direnv() -> Step<gh_workflow::Run> {
    Step::new("Load .envrc with direnv").run(
        "set -euo pipefail\n\n\
         direnv allow .envrc\n\
         direnv export gha >> \"$GITHUB_ENV\"",
    )
}

/// H8: `Fetch cargo dependencies` / `cargo fetch`, wd `baml_language`.
fn cargo_fetch() -> Step<gh_workflow::Run> {
    Step::new("Fetch cargo dependencies")
        .run("cargo fetch")
        .working_directory(WD)
}

/// H12: `Verify sccache` / `sccache --version`.
fn verify_sccache() -> Step<gh_workflow::Run> {
    Step::new("Verify sccache").run("sccache --version")
}

/// H5: `Install sccache and direnv` setup-mise step (rename of the default).
/// Used by the jobs that don't need nextest (wasm / msrv / doc).
fn install_sccache_direnv() -> Step<gh_workflow::Use> {
    steps::setup_mise("sccache direnv").name("Install sccache and direnv")
}

/// H5 nextest variant: `Install tools (mise)`. mise installs sccache/direnv
/// AND cargo-nextest (cargo: backend -> cargo-binstall prebuilt), replacing
/// taiki-e/install-action.
fn install_tools_mise() -> Step<gh_workflow::Use> {
    steps::setup_mise("sccache direnv cargo:cargo-nextest").name("Install tools (mise)")
}

/// rust-cache (H4) emitting the committed `with:` key order:
/// `workspaces`, `cache-all-crates`, `cache-targets`, `save-if`, `shared-key`.
fn rust_cache_full(save_if: &str, shared_key: &str) -> Step<gh_workflow::Use> {
    let mut s = Step::new("rust-cache")
        .uses("Swatinem", "rust-cache", "v2")
        .add_with(("workspaces", "baml_language -> target"))
        .add_with(("cache-all-crates", true))
        .add_with(("cache-targets", false))
        .add_with(("save-if", save_if))
        .add_with(("shared-key", shared_key));
    s.value.name = None;
    s
}

fn always() -> Expression {
    Expression::new("always()")
}

fn failure() -> Expression {
    Expression::new("failure()")
}

/// H10: the 4-line sccache-stats block (3 steps), all `if: always()`. The
/// json file / artifact name use the supplied `id` stem.
fn sccache_stats_block(job: Job<RunJob>, id: &str) -> Job<RunJob> {
    job.add_step(
        Step::new("Show sccache stats")
            .run("sccache --show-stats")
            .if_condition(always()),
    )
    .add_step(
        Step::new("Dump sccache stats (json)")
            .run(format!(
                "sccache --show-stats --stats-format json > sccache-stats-{id}.json"
            ))
            .if_condition(always()),
    )
    .add_step(
        Step::new("Upload sccache stats")
            .uses("actions", "upload-artifact", "v7")
            .if_condition(always())
            .add_with(("name", format!("sccache-stats-{id}")))
            .add_with(("path", format!("sccache-stats-{id}.json")))
            .add_with(("if-no-files-found", "ignore")),
    )
}

/// H11: the 2-step cargo-timings block, both `if: always()`. Plain (non-wasm)
/// variant: unconditional rename, upload WITHOUT `if-no-files-found`.
fn cargo_timings_block(job: Job<RunJob>, id: &str) -> Job<RunJob> {
    job.add_step(
        Step::new("Rename cargo timings")
            .run(format!(
                "mv target/cargo-timings/cargo-timing.html target/cargo-timings/cargo-timing-{id}.html"
            ))
            .if_condition(always())
            .working_directory(WD),
    )
    .add_step(
        Step::new("Upload cargo timings")
            .uses("actions", "upload-artifact", "v7")
            .if_condition(always())
            .add_with(("name", format!("cargo-timings-{id}")))
            .add_with((
                "path",
                format!("baml_language/target/cargo-timings/cargo-timing-{id}.html"),
            ))
            .add_with(("if-no-files-found", "ignore")),
    )
}

// ---------------------------------------------------------------------------
// Jobs
// ---------------------------------------------------------------------------

/// `cargo-test-linux`.
fn cargo_test_linux() -> NamedJob {
    let mut job = Job::default()
        .name("cargo test (linux)")
        .runs_on(runners::BLACKSMITH_8VCPU)
        .timeout_minutes(25u32)
        .add_step(checkout_blacksmith())
        .add_step(rustup_show())
        .add_step(install_tools_mise())
        .add_step(verify_sccache())
        .add_step(rust_cache_full(vars::save_if_canary(), "linux-cargo"))
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(
            Step::new("Build tests with --timings")
                .run("cargo test --no-run --all-features --timings")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Run tests with nextest")
                .run(
                    "cargo nextest run --all-features -E 'not package(baml_tests) \
                     and not package(/^sdk_test_/)'",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("Upload test results on failure")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(failure())
                .add_with(("name", "test-failures-linux"))
                .add_with((
                    "path",
                    "baml_language/**/*.snap.new\nbaml_language/**/target/nextest/\n",
                )),
        );
    job = sccache_stats_block(job, "test-linux");
    job = cargo_timings_block(job, "test-linux");
    steps::named_job("cargo-test-linux", job)
}

/// `cargo-test-macos`.
fn cargo_test_macos() -> NamedJob {
    let mut job = Job::default()
        .name("cargo test (macos)")
        .cond(Expression::new("${{ github.event_name != 'merge_group' }}"))
        .runs_on(runners::BLACKSMITH_6VCPU_MACOS)
        .timeout_minutes(35u32)
        .add_step(checkout_actions())
        .add_step(rustup_show())
        .add_step(install_tools_mise())
        .add_step(verify_sccache())
        .add_step(rust_cache_full(vars::save_if_canary(), "macos-cargo"))
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(
            Step::new("Build tests with --timings")
                .run("cargo test --no-run --all-features --timings")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Run tests with nextest")
                .run(
                    "cargo nextest run --all-features -E 'not package(baml_tests) \
                     and not package(/^sdk_test_/)'",
                )
                .working_directory(WD),
        );
    job = sccache_stats_block(job, "test-macos");
    job = cargo_timings_block(job, "test-macos");
    steps::named_job("cargo-test-macos", job)
}

/// The Windows-only native sccache wrapper bootstrap step.
fn build_native_sccache_wrapper() -> Step<gh_workflow::Run> {
    Step::new("Build native sccache wrapper")
        .run(
            "set -euo pipefail\n\
             # The native RUSTC_WRAPPER (tools_sccache crate) maps the R2 creds and\n\
             # execs sccache; cargo launches it via CreateProcess (32767-char\n\
             # limit), avoiding cmd.exe's ~8191-char limit that windows-sys' huge\n\
             # `--check-cfg cfg(feature, values(...))` line blows past. The binary\n\
             # lives in the build tree, so build it first — .envrc leaves\n\
             # RUSTC_WRAPPER unset until it exists, so this bootstrap compiles with\n\
             # plain rustc — then point RUSTC_WRAPPER at it for the cargo steps\n\
             # below.\n\
             cargo build -p tools_sccache\n\
             echo \"RUSTC_WRAPPER=$(pwd -W)/target/debug/baml-sccache.exe\" >> \"$GITHUB_ENV\"",
        )
        .working_directory(WD)
}

/// The sdk-tests variant of the wrapper bootstrap: also oversubscribes
/// CARGO_BUILD_JOBS (the matrix job is shared across OSes, so the env line is
/// set in-script rather than at the job level; it only matters on Windows).
fn build_native_sccache_wrapper_sdk() -> Step<gh_workflow::Run> {
    Step::new("Build native sccache wrapper")
        .run(
            "set -euo pipefail\n\
             cargo build -p tools_sccache\n\
             echo \"RUSTC_WRAPPER=$(pwd -W)/target/debug/baml-sccache.exe\" >> \"$GITHUB_ENV\"\n\
             # CARGO_BUILD_JOBS oversubscription only matters for the Windows build.\n\
             echo \"CARGO_BUILD_JOBS=16\" >> \"$GITHUB_ENV\"",
        )
        .working_directory(WD)
}

/// `cargo-test-windows`.
fn cargo_test_windows() -> NamedJob {
    let mut job = Job::default()
        .name("cargo test (windows)")
        .cond(Expression::new("${{ github.event_name != 'merge_group' }}"))
        .runs_on(runners::BLACKSMITH_8VCPU_WINDOWS)
        // 8-vCPU + CARGO_BUILD_JOBS=16 (2x cores) was the value sweet spot for
        // the native Windows Rust builds (build-caching/04 Exp 7/9).
        .envs(Env::default().add("CARGO_BUILD_JOBS", "16"))
        .timeout_minutes(50u32)
        .add_step(checkout_actions())
        .add_step(rustup_show())
        .add_step(
            steps::setup_mise("sccache direnv cargo:cargo-nextest")
                .name("Install sccache, direnv, and cargo-nextest"),
        )
        .add_step(verify_sccache())
        .add_step(rust_cache_full(vars::save_if_canary(), "windows-cargo"))
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(build_native_sccache_wrapper())
        .add_step(
            Step::new("Build tests with --timings")
                .run("cargo test --no-run --all-features --timings")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Run tests")
                .run(
                    "cargo nextest run --all-features -E 'not package(baml_tests) \
                     and not package(/^sdk_test_/)'",
                )
                .working_directory(WD),
        );
    job = sccache_stats_block(job, "test-windows");
    job = cargo_timings_block(job, "test-windows");
    steps::named_job("cargo-test-windows", job)
}

/// The github-script body that builds the sdk test matrix.
const SDK_MATRIX_SCRIPT: &str = r#"const baseMatrix = [
  {
    "sdk-label": "python-pydantic2",
    "package-name": "sdk_test_python_pydantic2",
    "sdk-path": "python_pydantic2",
    "install-args": "sccache direnv python uv cargo:cargo-nextest",
    "os-label": "linux",
    "runs-on": "blacksmith-16vcpu-ubuntu-2404",
    "rust-cache-key": "linux-cargo",
    "timeout": 30,
  },
  {
    "sdk-label": "typescript-node",
    "package-name": "sdk_test_typescript_node",
    "sdk-path": "typescript_node",
    "install-args": "sccache direnv node npm:pnpm cargo:cargo-nextest",
    "os-label": "linux",
    "runs-on": "blacksmith-16vcpu-ubuntu-2404",
    "rust-cache-key": "linux-cargo",
    "timeout": 30,
  },
  {
    "sdk-label": "python-pydantic2",
    "package-name": "sdk_test_python_pydantic2",
    "sdk-path": "python_pydantic2",
    "install-args": "sccache direnv python uv cargo:cargo-nextest",
    "os-label": "macos",
    "runs-on": "blacksmith-6vcpu-macos-latest",
    "rust-cache-key": "macos-cargo",
    "timeout": 40,
  },
  {
    "sdk-label": "typescript-node",
    "package-name": "sdk_test_typescript_node",
    "sdk-path": "typescript_node",
    "install-args": "sccache direnv node npm:pnpm cargo:cargo-nextest",
    "os-label": "macos",
    "runs-on": "blacksmith-6vcpu-macos-latest",
    "rust-cache-key": "macos-cargo",
    "timeout": 40,
  },
  {
    "sdk-label": "python-pydantic2",
    "package-name": "sdk_test_python_pydantic2",
    "sdk-path": "python_pydantic2",
    "install-args": "sccache direnv python uv cargo:cargo-nextest",
    "os-label": "windows",
    "runs-on": "blacksmith-8vcpu-windows-2025",
    "rust-cache-key": "windows-cargo",
    "timeout": 60,
  },
  {
    "sdk-label": "typescript-node",
    "package-name": "sdk_test_typescript_node",
    "sdk-path": "typescript_node",
    "install-args": "sccache direnv node npm:pnpm cargo:cargo-nextest",
    "os-label": "windows",
    "runs-on": "blacksmith-8vcpu-windows-2025",
    "rust-cache-key": "windows-cargo",
    "timeout": 60,
  },
];

const matrix = context.eventName === "merge_group"
  ? baseMatrix.filter((entry) => entry["os-label"] === "linux")
  : baseMatrix;

core.setOutput("matrix", JSON.stringify(matrix, null, 2));
"#;

/// `sdk-test-matrix`.
fn sdk_test_matrix() -> NamedJob {
    let mut outputs = indexmap::IndexMap::new();
    outputs.insert(
        "matrix".to_string(),
        "${{ steps.matrix.outputs.matrix }}".to_string(),
    );
    let job = Job::default()
        .name("sdk test matrix")
        .runs_on(runners::UBUNTU_LATEST)
        .outputs(outputs)
        .add_step(
            Step::new("Build matrix")
                .uses("actions", "github-script", "v8")
                .id("matrix")
                .add_with(("script", SDK_MATRIX_SCRIPT)),
        );
    steps::named_job("sdk-test-matrix", job)
}

/// `sdk-tests` (matrix).
fn sdk_tests() -> NamedJob {
    let strategy = Strategy::default().fail_fast(false).matrix(json!({
        "include": "${{ fromJSON(needs.sdk-test-matrix.outputs.matrix) }}"
    }));

    let mut rust_cache = Step::new("rust-cache")
        .uses("Swatinem", "rust-cache", "v2")
        .add_with(("workspaces", "baml_language -> target"))
        .add_with(("cache-all-crates", true))
        .add_with(("cache-targets", false))
        .add_with(("save-if", false))
        .add_with(("shared-key", "${{ matrix.rust-cache-key }}"));
    rust_cache.value.name = None;

    let job = Job::default()
        .name("sdk tests (${{ matrix.sdk-label }}, ${{ matrix.os-label }})")
        .needs(vec!["sdk-test-matrix".to_string()])
        .strategy(strategy)
        .runs_on("${{ matrix.runs-on }}")
        // `timeout-minutes: ${{ matrix.timeout }}` is a templated expression the
        // fork's `timeout_minutes: Option<u32>` setter cannot hold. We emit a
        // sentinel value here that the generation driver rewrites into the
        // matrix expression. See `vars::TIMEOUT_MATRIX_SENTINEL` /
        // `tasks::workflows::apply_matrix_timeout`.
        .timeout_minutes(vars::TIMEOUT_MATRIX_SENTINEL)
        .add_step(checkout_actions())
        .add_step(rustup_show())
        .add_step(steps::setup_mise("${{ matrix.install-args }}").name("Install sdk test tools"))
        .add_step(verify_sccache())
        .add_step(rust_cache)
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(
            build_native_sccache_wrapper_sdk()
                .if_condition(Expression::new("matrix.os-label == 'windows'")),
        )
        // Windows Python only: setup.ps1 builds one baml_core wheel and strips
        // the editable source block from each fixture so `uv sync` installs
        // that prebuilt wheel instead of rebuilding baml_core per fixture. The
        // later test-time `uv run` processes also need this wheelhouse.
        .add_step(
            Step::new("Configure Python wheelhouse")
                .run("echo \"UV_FIND_LINKS=$(pwd -W)/target/wheels\" >> \"$GITHUB_ENV\"")
                .if_condition(Expression::new(
                    "matrix.os-label == 'windows' && matrix.sdk-label == 'python-pydantic2'",
                ))
                .working_directory(WD),
        )
        .add_step(
            Step::new("Build tests with --timings")
                .run("cargo test --no-run -p ${{ matrix.package-name }} --all-features --timings")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Show sccache stats")
                .run("sccache --show-stats")
                .if_condition(always()),
        )
        .add_step(
            Step::new("Run sdk test setup")
                .run(
                    "cargo nextest run -p ${{ matrix.package-name }} --all-features \
                     -E 'package(=${{ matrix.package-name }}) and test(setup_guard::ran)'",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("Run sdk tests")
                .run(
                    "cargo nextest run -p ${{ matrix.package-name }} --all-features \
                     -E 'package(=${{ matrix.package-name }})'",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("Upload test results on failure")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(failure())
                .add_with((
                    "name",
                    "sdk-test-failures-${{ matrix.sdk-label }}-${{ matrix.os-label }}",
                ))
                .add_with((
                    "path",
                    "baml_language/sdk_tests/crates/${{ matrix.sdk-path }}/*/generated/\n\
                     baml_language/target/nextest/\n",
                ))
                .add_with(("if-no-files-found", "ignore")),
        )
        .add_step(
            Step::new("Show sccache stats")
                .run("sccache --show-stats")
                .if_condition(always()),
        )
        .add_step(
            Step::new("Dump sccache stats (json)")
                .run(
                    "sccache --show-stats --stats-format json > \
                     sccache-stats-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}.json",
                )
                .if_condition(always()),
        )
        .add_step(
            Step::new("Upload sccache stats")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(always())
                .add_with((
                    "name",
                    "sccache-stats-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}",
                ))
                .add_with((
                    "path",
                    "sccache-stats-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}.json",
                ))
                .add_with(("if-no-files-found", "ignore")),
        )
        .add_step(
            Step::new("Rename cargo timings")
                .run(
                    "mv target/cargo-timings/cargo-timing.html \
                     target/cargo-timings/cargo-timing-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}.html",
                )
                .if_condition(always())
                .working_directory(WD),
        )
        .add_step(
            Step::new("Upload cargo timings")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(always())
                .add_with((
                    "name",
                    "cargo-timings-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}",
                ))
                .add_with((
                    "path",
                    "baml_language/target/cargo-timings/cargo-timing-sdk-tests-${{ matrix.sdk-label }}-${{ matrix.os-label }}.html",
                ))
                .add_with(("if-no-files-found", "ignore")),
        );
    steps::named_job("sdk-tests", job)
}

/// `cargo-test-wasm`.
fn cargo_test_wasm() -> NamedJob {
    let mut job = Job::default()
        .name("cargo test (wasm)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .timeout_minutes(45u32)
        .add_step(checkout_blacksmith())
        .add_step(rustup_show_wasm())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_full(vars::save_if_canary(), "linux-wasm"))
        .add_step(load_direnv())
        .add_step(
            Step::new("Fetch cargo dependencies")
                .run("cargo fetch --target wasm32-unknown-unknown")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Build for WASM")
                .run(
                    "WASM_PACKAGES=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.source==null) | select(if .metadata.ci.wasm_support == null then true else .metadata.ci.wasm_support end) | \"-p \\(.name)\"' | xargs)\n\
                     cargo build $WASM_PACKAGES --target wasm32-unknown-unknown --no-default-features --release --timings",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("List WASM artifacts")
                .run("ls -lh target/wasm32-unknown-unknown/release/*.wasm || true")
                .working_directory(WD),
        );
    job = sccache_stats_block(job, "test-wasm");
    // WASM timings rename is guarded because wasm may not emit a timing file.
    job = job
        .add_step(
            Step::new("Rename cargo timings")
                .run(
                    "if [[ -f target/cargo-timings/cargo-timing.html ]]; then\n  \
                     mv target/cargo-timings/cargo-timing.html target/cargo-timings/cargo-timing-test-wasm.html\nfi",
                )
                .if_condition(always())
                .working_directory(WD),
        )
        .add_step(
            Step::new("Upload cargo timings")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(always())
                .add_with(("name", "cargo-timings-test-wasm"))
                .add_with((
                    "path",
                    "baml_language/target/cargo-timings/cargo-timing-test-wasm.html",
                ))
                .add_with(("if-no-files-found", "ignore")),
        );
    steps::named_job("cargo-test-wasm", job)
}

/// `cargo-build-msrv`.
fn cargo_build_msrv() -> NamedJob {
    let msrv_env = || Env::default().add("MSRV", "${{ steps.msrv.outputs.value }}");

    let mut read_msrv = Step::new("read-msrv")
        .uses("SebRollen", "toml-action", "v1.2.0")
        .id("msrv")
        .add_with(("file", "baml_language/Cargo.toml"))
        .add_with(("field", "workspace.package.rust-version"));
    read_msrv.value.name = None;

    let mut job = Job::default()
        .name("cargo build (msrv)")
        .runs_on(runners::BLACKSMITH_16VCPU)
        .timeout_minutes(25u32)
        .add_step(checkout_blacksmith())
        .add_step(read_msrv)
        .add_step(
            Step::new("Install Rust toolchain (MSRV)")
                .run("rustup toolchain install \"${MSRV}\"\nrustup default \"${MSRV}\"")
                .working_directory(WD)
                .add_env(msrv_env()),
        )
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_full(vars::save_if_canary(), "linux-msrv"))
        .add_step(load_direnv())
        .add_step(
            Step::new("Fetch cargo dependencies")
                .run("cargo \"+${MSRV}\" fetch")
                .working_directory(WD)
                .add_env(msrv_env()),
        )
        .add_step(
            Step::new("Build with MSRV")
                .run("cargo \"+${MSRV}\" test --no-run --all-features --timings")
                .working_directory(WD)
                .add_env(msrv_env()),
        );
    job = sccache_stats_block(job, "msrv");
    job = cargo_timings_block(job, "msrv");
    steps::named_job("cargo-build-msrv", job)
}

/// `cargo-doc`.
fn cargo_doc() -> NamedJob {
    let mut job = Job::default()
        .name("cargo doc")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .timeout_minutes(25u32)
        .add_step(checkout_blacksmith())
        .add_step(rustup_show())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_full("false", "linux-cargo"))
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(
            Step::new("Generate documentation")
                .run("cargo doc --all --no-deps --timings")
                .working_directory(WD)
                .add_env(Env::default().add("RUSTDOCFLAGS", "-D warnings")),
        );
    job = sccache_stats_block(job, "doc");
    job = cargo_timings_block(job, "doc");
    steps::named_job("cargo-doc", job)
}

/// `snapshot-tests`.
fn snapshot_tests() -> NamedJob {
    let mut job = Job::default()
        .name("snapshot tests")
        .runs_on(runners::BLACKSMITH_8VCPU)
        .timeout_minutes(30u32)
        .add_step(checkout_blacksmith())
        .add_step(rustup_show())
        .add_step(
            steps::setup_mise("sccache direnv python uv cargo:cargo-insta cargo:cargo-nextest")
                .name("Install tools (mise)"),
        )
        .add_step(verify_sccache())
        .add_step(rust_cache_full("false", "linux-cargo"))
        .add_step(load_direnv())
        .add_step(cargo_fetch())
        .add_step(
            Step::new("Build tests with --timings")
                .run(
                    "cargo test --no-run -p baml_tests -p baml_cli -p baml_lsp2_actions \
                     --all-features --timings",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("Run snapshot tests")
                .run(
                    "cargo insta test --test-runner nextest -p baml_tests -p baml_cli \
                     -p baml_lsp2_actions --all-features --unreferenced=reject",
                )
                .working_directory(WD),
        )
        .add_step(
            Step::new("Check for snapshot changes").run(
                "pathspecs=(\n  \
                 ':(glob)baml_language/**/*.snap'\n  \
                 ':(glob)baml_language/**/*.snap.new'\n)\n\
                 if [ -n \"$(git status --porcelain -- \"${pathspecs[@]}\")\" ]; then\n  \
                 echo \"::error::Snapshot tests have uncommitted changes\"\n  \
                 git diff -- \"${pathspecs[@]}\"\n  \
                 exit 1\nfi",
            ),
        )
        .add_step(
            Step::new("Upload snapshot failures")
                .uses("actions", "upload-artifact", "v7")
                .if_condition(failure())
                .add_with(("name", "snapshot-failures"))
                .add_with((
                    "path",
                    "baml_language/**/*.snap\nbaml_language/**/*.snap.new\n",
                )),
        );
    job = sccache_stats_block(job, "snapshot");
    job = cargo_timings_block(job, "snapshot");
    steps::named_job("snapshot-tests", job)
}

/// The 8-job `needs` list shared by the two fan-in merge jobs.
fn merge_needs() -> Vec<String> {
    vec![
        "cargo-test-linux".to_string(),
        "cargo-test-macos".to_string(),
        "cargo-test-windows".to_string(),
        "sdk-tests".to_string(),
        "cargo-test-wasm".to_string(),
        "cargo-build-msrv".to_string(),
        "cargo-doc".to_string(),
        "snapshot-tests".to_string(),
    ]
}

/// `cargo-timings` (merged).
fn cargo_timings_merged() -> NamedJob {
    let job = Job::default()
        .name("cargo timings (merged)")
        .runs_on(runners::UBUNTU_LATEST)
        .needs(merge_needs())
        .add_step(
            Step::new("Merge cargo timings artifacts")
                .uses("actions", "upload-artifact/merge", "v4")
                .add_with(("name", "cargo-timings"))
                .add_with(("pattern", "cargo-timings-*"))
                .add_with(("delete-merged", true)),
        );
    steps::named_job("cargo-timings", job)
}

/// `sccache-stats` (merged).
fn sccache_stats_merged() -> NamedJob {
    let job = Job::default()
        .name("sccache stats (merged)")
        .runs_on(runners::UBUNTU_LATEST)
        .needs(merge_needs())
        .add_step(
            Step::new("Merge sccache stats artifacts")
                .uses("actions", "upload-artifact/merge", "v4")
                .add_with(("name", "sccache-stats"))
                .add_with(("pattern", "sccache-stats-*"))
                .add_with(("delete-merged", true)),
        );
    steps::named_job("sccache-stats", job)
}

/// Reusable cargo-tests workflow.
#[must_use]
pub fn workflow() -> Workflow {
    // Top-level: name + bash defaults, then on.workflow_call.secrets,
    // permissions, and the env block (standard four + the two R2 secrets).
    let mut secrets: indexmap::IndexMap<String, gh_workflow::WorkflowCallSecret> =
        indexmap::IndexMap::new();
    secrets.insert(
        "BAML_SCCACHE_R2_ACCESS_KEY_ID".to_string(),
        vars::workflow_call_secret("", false),
    );
    secrets.insert(
        "BAML_SCCACHE_R2_SECRET_ACCESS_KEY".to_string(),
        vars::workflow_call_secret("", false),
    );
    let workflow_call = WorkflowCall::default().secrets(secrets);

    let env = Env::default()
        .add(
            "BAML_SCCACHE_R2_ACCESS_KEY_ID",
            "${{ secrets.BAML_SCCACHE_R2_ACCESS_KEY_ID }}",
        )
        .add(
            "BAML_SCCACHE_R2_SECRET_ACCESS_KEY",
            "${{ secrets.BAML_SCCACHE_R2_SECRET_ACCESS_KEY }}",
        )
        .add("CARGO_INCREMENTAL", 0)
        .add("CARGO_NET_RETRY", 10)
        .add("CARGO_TERM_COLOR", "always")
        .add("RUSTUP_MAX_RETRIES", 10);

    let jobs: Vec<NamedJob> = vec![
        cargo_test_linux(),
        cargo_test_macos(),
        cargo_test_windows(),
        sdk_test_matrix(),
        sdk_tests(),
        cargo_test_wasm(),
        cargo_build_msrv(),
        cargo_doc(),
        snapshot_tests(),
        cargo_timings_merged(),
        sccache_stats_merged(),
    ];

    let mut wf = vars::reusable_workflow("Cargo Tests (Reusable)")
        .add_event(Event::default().workflow_call(workflow_call))
        .permissions(gh_workflow::Permissions::default().contents(gh_workflow::Level::Read))
        .envs(env);

    for nj in jobs {
        wf = nj.add_to(wf);
    }
    wf
}
