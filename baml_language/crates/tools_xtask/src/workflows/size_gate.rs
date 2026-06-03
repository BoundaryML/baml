//! Reusable Size Gate workflow -> `size-gate.reusable.yaml`.
//!
//! Individual platform jobs are `continue-on-error: true` and never block the
//! calling workflow; only `size-gate-report` aggregates the reports and can fail
//! CI on a policy violation or a missing platform report.
//!
//! These jobs compile the same release artifacts the cargo tests do, so they
//! share the sccache + Swatinem/rust-cache setup documented at the top of
//! cargo-tests.reusable.yaml: sccache holds the compiled `target/` tree (R2
//! backend), while rust-cache keeps the registry/git state. They are read-only
//! consumers of the shared rust-cache entries written by canary in
//! cargo-tests.reusable.yaml, and read the same sccache R2 cache via .envrc.

#[allow(unused_imports)]
use crate::workflows::{
    runners, steps,
    steps::{FluentBuilder, NamedJob},
    vars,
};
use gh_workflow::{
    Env, Event, Expression, Job, Level, Permissions, RunJob, Step, Workflow, WorkflowCall,
};
use indoc::indoc;

use crate::workflows::vars::WD;

// ---------------------------------------------------------------------------
// Small inline-step helpers local to this workflow (mirroring cargo_tests).
// ---------------------------------------------------------------------------

/// `useblacksmith/checkout@v1`, `persist-credentials: false` (unnamed).
fn checkout_blacksmith() -> Step<gh_workflow::Use> {
    let mut s = Step::new("checkout")
        .uses("useblacksmith", "checkout", "v1")
        .add_with(("persist-credentials", false));
    s.value.name = None;
    s
}

/// `actions/checkout@v6`, `persist-credentials: false` (unnamed).
fn checkout_actions() -> Step<gh_workflow::Use> {
    let mut s = Step::new("checkout")
        .uses("actions", "checkout", "v6")
        .add_with(("persist-credentials", false));
    s.value.name = None;
    s
}

/// `Load .envrc with direnv`, wd defaults to repo root (no wd in YAML).
fn load_direnv() -> Step<gh_workflow::Run> {
    Step::new("Load .envrc with direnv").run(
        "set -euo pipefail\n\n\
         direnv allow .envrc\n\
         direnv export gha >> \"$GITHUB_ENV\"",
    )
}

/// `Verify sccache` / `sccache --version`.
fn verify_sccache() -> Step<gh_workflow::Run> {
    Step::new("Verify sccache").run("sccache --version")
}

/// `Install sccache and direnv` setup-mise step (rename of the default).
fn install_sccache_direnv() -> Step<gh_workflow::Use> {
    steps::setup_mise("sccache direnv").name("Install sccache and direnv")
}

/// rust-cache emitting the committed `with:` key order. All size-gate jobs are
/// read-only consumers (`save-if: false`); the canary saver lives in
/// cargo-tests.reusable.yaml.
fn rust_cache_readonly(shared_key: &str) -> Step<gh_workflow::Use> {
    let mut s = Step::new("rust-cache")
        .uses("Swatinem", "rust-cache", "v2")
        .add_with(("workspaces", "baml_language -> target"))
        .add_with(("cache-all-crates", true))
        .add_with(("cache-targets", false))
        .add_with(("shared-key", shared_key))
        .add_with(("save-if", false));
    s.value.name = None;
    s
}

fn always() -> Expression {
    Expression::new("always()")
}

/// The Windows-only native sccache wrapper bootstrap step (same as the
/// cargo-tests one).
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

/// `cargo run --timings -p cargo-size-gate -- size-gate check ...` for one
/// platform, emitting `size-gate-<platform>.json` and an `exit_code` output.
fn run_size_gate_check(only: &str, platform: &str) -> Step<gh_workflow::Run> {
    Step::new("Run size-gate check")
        .run(format!(
            "cargo run --timings -p cargo-size-gate -- size-gate check \\\n  \
             --only {only} \\\n  \
             --format json > size-gate-{platform}.json 2> size-gate-stderr.txt || exit_code=$?\n\
             cat size-gate-stderr.txt >&2\n\
             echo \"exit_code=${{exit_code:-0}}\" >> \"$GITHUB_OUTPUT\""
        ))
        .id("check")
        .working_directory(WD)
}

/// The platform-job artifact tail: upload the size-gate report, the 3-step
/// sccache-stats block, then the cargo-timings upload (all `if: always()`).
fn upload_platform_artifacts(job: Job<RunJob>, platform: &str) -> Job<RunJob> {
    job.add_step(
        steps::upload_artifact(
            &format!("size-gate-{platform}"),
            &format!("{WD}/size-gate-{platform}.json"),
        )
        .name("Upload size-gate report")
        .add_with(("if-no-files-found", "ignore"))
        .if_condition(always()),
    )
    .add_step(
        Step::new("Show sccache stats")
            .run("sccache --show-stats")
            .if_condition(always()),
    )
    .add_step(
        Step::new("Dump sccache stats (json)")
            .run(format!(
                "sccache --show-stats --stats-format json > sccache-stats-size-gate-{platform}.json"
            ))
            .if_condition(always()),
    )
    .add_step(
        steps::upload_artifact(
            &format!("sccache-stats-size-gate-{platform}"),
            &format!("sccache-stats-size-gate-{platform}.json"),
        )
        .name("Upload sccache stats")
        .add_with(("if-no-files-found", "ignore"))
        .if_condition(always()),
    )
    .add_step(
        steps::upload_artifact(
            &format!("cargo-timings-size-gate-{platform}"),
            &format!("{WD}/target/cargo-timings/"),
        )
        .name("Upload cargo timings")
        .add_with(("if-no-files-found", "ignore"))
        .if_condition(always()),
    )
}

/// `size-gate-linux` — blacksmith checkout, `--only baml-cli,packed-program`.
fn size_gate_linux() -> NamedJob {
    let job = Job::default()
        .name("size-gate (linux)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .timeout_minutes(20u32)
        .continue_on_error(true)
        .add_step(checkout_blacksmith())
        .add_step(steps::rustup_show())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_readonly("linux-cargo"))
        .add_step(load_direnv())
        .add_step(steps::cargo_fetch())
        .add_step(run_size_gate_check("baml-cli,packed-program", "linux"));
    let job = upload_platform_artifacts(job, "linux");
    steps::named_job("size-gate-linux", job)
}

/// `size-gate-macos` — actions/checkout, `--only baml-cli,packed-program`.
fn size_gate_macos() -> NamedJob {
    let job = Job::default()
        .name("size-gate (macos)")
        .runs_on(runners::BLACKSMITH_6VCPU_MACOS)
        .timeout_minutes(20u32)
        .continue_on_error(true)
        .add_step(checkout_actions())
        .add_step(steps::rustup_show())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_readonly("macos-cargo"))
        .add_step(load_direnv())
        .add_step(steps::cargo_fetch())
        .add_step(run_size_gate_check("baml-cli,packed-program", "macos"));
    let job = upload_platform_artifacts(job, "macos");
    steps::named_job("size-gate-macos", job)
}

/// `size-gate-windows` — actions/checkout, `--only baml-cli,packed-program`.
/// Cargo linking uses rust-lld from the Rust toolchain on Windows; the extra
/// clang tool is installed through mise for llvm-strip, replacing the previous
/// Chocolatey LLVM install.
fn size_gate_windows() -> NamedJob {
    let job = Job::default()
        .name("size-gate (windows)")
        .runs_on(runners::BLACKSMITH_8VCPU_WINDOWS)
        .envs(Env::default().add("CARGO_BUILD_JOBS", "16"))
        // TODO: Reduce this once our changes to aws-sdk-rust are upstreamed.
        // Right now it needs to download our repo every time which is very slow.
        .timeout_minutes(45u32)
        .continue_on_error(true)
        .add_step(checkout_actions())
        .add_step(steps::rustup_show())
        .add_step(
            steps::setup_mise("sccache direnv clang").name("Install sccache, direnv, and clang"),
        )
        .add_step(verify_sccache())
        .add_step(rust_cache_readonly("windows-cargo"))
        .add_step(load_direnv())
        .add_step(steps::cargo_fetch())
        .add_step(build_native_sccache_wrapper())
        .add_step(run_size_gate_check("baml-cli,packed-program", "windows"));
    let job = upload_platform_artifacts(job, "windows");
    steps::named_job("size-gate-windows", job)
}

/// `size-gate-wasm` — blacksmith checkout, wasm toolchain/fetch, `--only
/// bridge_wasm`.
fn size_gate_wasm() -> NamedJob {
    let job = Job::default()
        .name("size-gate (wasm)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .timeout_minutes(25u32)
        .continue_on_error(true)
        .add_step(checkout_blacksmith())
        .add_step(steps::rustup_show_wasm())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_readonly("linux-wasm"))
        .add_step(load_direnv())
        .add_step(steps::cargo_fetch_wasm())
        .add_step(run_size_gate_check("bridge_wasm", "wasm"));
    let job = upload_platform_artifacts(job, "wasm");
    steps::named_job("size-gate-wasm", job)
}

/// `size-gate-report` — aggregates every platform JSON, posts a PR comment,
/// writes the job summary, and is the one job that can block CI.
fn size_gate_report() -> NamedJob {
    let result_env = [
        ("RESULT_LINUX", "${{ needs.size-gate-linux.result }}"),
        ("RESULT_MACOS", "${{ needs.size-gate-macos.result }}"),
        ("RESULT_WINDOWS", "${{ needs.size-gate-windows.result }}"),
        ("RESULT_WASM", "${{ needs.size-gate-wasm.result }}"),
    ];

    let compose = result_env.iter().fold(
        Step::new("Compose unified report")
            .add_env((
                "RUN_URL",
                "${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}",
            ))
            .add_env(("BIN", "baml_language/target/debug/cargo-size-gate")),
        |step, (k, v)| step.add_env((*k, *v)),
    )
    .run(indoc! {r#"
        # Track which expected platforms produced a JSON report.
        missing=()
        json_files=()

        declare -A platform_results=(
          [linux]="$RESULT_LINUX"
          [macos]="$RESULT_MACOS"
          [windows]="$RESULT_WINDOWS"
          [wasm]="$RESULT_WASM"
        )

        for platform in linux macos windows wasm; do
          f="reports/size-gate-${platform}.json"
          if [ -f "$f" ]; then
            json_files+=("$f")
          else
            missing+=("$platform (job result: ${platform_results[$platform]:-unknown})")
          fi
        done

        # Aggregate the JSONs we did receive.
        "$BIN" size-gate agg \
          "${json_files[@]}" \
          --run-url "$RUN_URL" \
          > unified-report.md || true

        # Prepend a "Missing platforms" callout if any expected platform
        # didn't produce a report. This fires whether the platform's job
        # was cancelled, errored before writing JSON, or never ran.
        if [ "${#missing[@]}" -gt 0 ]; then
          {
            echo "> ⚠️ **Missing size-gate report(s):**"
            for entry in "${missing[@]}"; do
              echo "> - \`${entry}\`"
            done
            echo ">"
            echo "> The unified report below only reflects platforms that produced output."
            echo ""
            cat unified-report.md
          } > unified-report.with-missing.md
          mv unified-report.with-missing.md unified-report.md
        fi

        # Persist for the failure step to consult. Gate the write on
        # the array length: `printf '%s\n' "${missing[@]}"` with zero
        # arguments still emits one trailing newline, which would make
        # the file non-empty (1 byte) and trip the `[ -s ... ]` check
        # in the failure step even when nothing is missing.
        : > missing-platforms.txt
        if [ "${#missing[@]}" -gt 0 ]; then
          printf '%s\n' "${missing[@]}" > missing-platforms.txt
        fi
    "#});

    let fail_step = result_env.iter().fold(
        Step::new("Fail if any platform is missing or any check failed"),
        |step, (k, v)| step.add_env((*k, *v)),
    )
    .run(indoc! {r#"
        fail=0

        if [ -s missing-platforms.txt ]; then
          echo "::error::Missing size-gate report(s):"
          while IFS= read -r entry; do
            [ -n "$entry" ] && echo "::error::  - ${entry}"
          done < missing-platforms.txt
          fail=1
        fi

        # Each platform's job is `continue-on-error: true`, so a job-level
        # 'failure' here means the job died before producing a report
        # (covered above) OR the job succeeded but uploaded a JSON marked
        # as a violation. Re-check the JSONs for the latter.
        for platform in linux macos windows wasm; do
          f="reports/size-gate-${platform}.json"
          if [ -f "$f" ] && jq -e '.ok == false' "$f" > /dev/null 2>&1; then
            echo "::error::size-gate (${platform}) reported a policy violation"
            fail=1
          fi
        done

        exit "$fail"
    "#});

    let job = Job::default()
        .name("size-gate (report)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .cond(Expression::new("always()"))
        .needs(vec![
            "size-gate-linux".to_string(),
            "size-gate-macos".to_string(),
            "size-gate-windows".to_string(),
            "size-gate-wasm".to_string(),
        ])
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .pull_requests(Level::Write),
        )
        .add_step(checkout_blacksmith())
        .add_step(
            Step::new("Download size-gate reports")
                .uses("actions", "download-artifact", "v8")
                .add_with(("pattern", "size-gate-*"))
                .add_with(("merge-multiple", true))
                .add_with(("path", "reports")),
        )
        // Build cargo-size-gate from source. We share the linux size-gate
        // job's rust-cache (same shared-key, same runner OS), so this is a
        // near-instant cache hit in the common case — no upload/download of
        // the binary required.
        .add_step(steps::rustup_show())
        .add_step(install_sccache_direnv())
        .add_step(verify_sccache())
        .add_step(rust_cache_readonly("linux-cargo"))
        .add_step(load_direnv())
        .add_step(steps::cargo_fetch())
        .add_step(
            Step::new("Build cargo-size-gate")
                .run("cargo build -p cargo-size-gate")
                .working_directory(WD),
        )
        .add_step(
            Step::new("Show sccache stats")
                .run("sccache --show-stats")
                .if_condition(always()),
        )
        .add_step(compose)
        .add_step(
            Step::new("Post PR comment")
                .uses("thollander", "actions-comment-pull-request", "v3.0.1")
                .if_condition(Expression::new("github.event_name == 'pull_request'"))
                .add_with(("file-path", "unified-report.md"))
                .add_with(("comment-tag", "size-gate")),
        )
        .add_step(
            Step::new("Print report + write job summary").run(indoc! {r#"
                # Print to the workflow console log (so the status is visible in
                # the job output, not just the summary tab and PR comment)...
                cat unified-report.md
                # ...and to the run's summary tab.
                cat unified-report.md >> "$GITHUB_STEP_SUMMARY"
            "#}),
        )
        .add_step(fail_step);

    steps::named_job("size-gate-report", job)
}

/// Reusable size-gate workflow.
#[must_use]
pub fn workflow() -> Workflow {
    // on.workflow_call.secrets: the two R2 sccache credentials (optional so
    // secretless runs, e.g. fork PRs, fall back to the runner-local cache).
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

    let mut workflow = vars::reusable_workflow("Size Gate (Reusable)")
        .add_event(Event::default().workflow_call(workflow_call))
        .envs(env);

    for named in [
        size_gate_linux(),
        size_gate_macos(),
        size_gate_windows(),
        size_gate_wasm(),
        size_gate_report(),
    ] {
        workflow = named.add_to(workflow);
    }

    workflow
}
