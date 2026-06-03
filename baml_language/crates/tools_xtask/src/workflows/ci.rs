//! Orchestrator workflow -> `ci.yaml`.
//!
//! Reproduces the hand-written `.github/workflows/ci.yaml`: the
//! `determine_changes` change-detection job, the lint/metadata jobs, the
//! reusable-workflow caller jobs (cargo-tests / size-gate / wasm-pack-tests /
//! webview-tests / docs), the proto-sync + miri jobs, the split benchmark
//! jobs, the perf PR-notice job, and the failure-alert gate.

#[allow(unused_imports)]
use crate::workflows::{
    runners, steps,
    steps::{FluentBuilder, NamedJob},
    vars,
};
use gh_workflow::{
    Env, Event, Expression, Input, Job, Level, MergeGroup, MergeGroupType, Permissions, PullRequest,
    Push, Step, Workflow, WorkflowDispatch,
};

/// The `determine_changes.code`-or-main/canary gate shared by the lint /
/// metadata / reusable-caller jobs.
fn cond_code() -> Expression {
    vars::cond_code_or_main_canary()
}

/// `useblacksmith/checkout@v1` with `fetch-depth: 0`, used by the unnamed
/// checkout in `determine_changes` (no `name:`).
fn checkout_blacksmith_full_unnamed() -> Step<gh_workflow::Use> {
    let mut step = steps::checkout_blacksmith_full("Checkout");
    step.value.name = None;
    step
}

/// One `check_<id>` step in `determine_changes`: a git-diff over the given
/// pathspecs that writes `<flag>=true|false` to `$GITHUB_OUTPUT`.
fn change_check(name: &str, id: &str, flag: &str, pathspecs: &[&str]) -> Step<gh_workflow::Run> {
    use std::fmt::Write as _;
    let mut body = String::new();
    body.push_str("if git diff --quiet \"${MERGE_BASE}...HEAD\" -- \\\n");
    for spec in pathspecs {
        let _ = writeln!(body, "  '{spec}' \\");
    }
    body.push_str("; then\n");
    let _ = writeln!(body, "  echo \"{flag}=false\" >> \"$GITHUB_OUTPUT\"");
    body.push_str("else\n");
    let _ = writeln!(body, "  echo \"{flag}=true\" >> \"$GITHUB_OUTPUT\"");
    body.push_str("fi\n");
    Step::new(name)
        .run(body)
        .id(id)
        .add_env(("MERGE_BASE", "${{ steps.merge_base.outputs.sha }}"))
}

/// Job `determine_changes` — change detection + perf-gating.
fn determine_changes() -> NamedJob {
    let mut outputs = vars::job_outputs_with_key("changed", &[
        ("code", "check_code"),
        ("lexer", "check_lexer"),
        ("parser", "check_parser"),
        ("hir", "check_hir"),
        ("thir", "check_thir"),
        ("codegen", "check_codegen"),
        ("docs", "check_docs"),
        ("webview", "check_webview"),
        ("unsafe", "check_unsafe"),
        ("proto", "check_proto"),
        ("prof", "check_prof"),
    ]);
    // `run_perf` is wired to `check_perf.outputs.run` (output name != flag).
    outputs.insert(
        "run_perf".to_string(),
        vars::step_output("check_perf", "run"),
    );

    let merge_base = Step::new("Determine merge base")
        .run(
            "sha=$(git merge-base HEAD \"origin/${BASE_REF}\")\n\
             echo \"sha=${sha}\" >> \"$GITHUB_OUTPUT\"\n",
        )
        .id("merge_base")
        .add_env(("BASE_REF", "${{ github.event.pull_request.base.ref || 'canary' }}"));

    let perf = Step::new("Check perf benchmark opt-in")
        .run(CHECK_PERF_SCRIPT)
        .id("check_perf")
        .add_env(
            Env::default()
                .add("EVENT_NAME", "${{ github.event_name }}")
                .add("PR_TITLE", "${{ github.event.pull_request.title }}")
                .add("PR_BODY", "${{ github.event.pull_request.body }}")
                .add("MERGE_BASE", "${{ steps.merge_base.outputs.sha }}")
                .add("PUSH_BEFORE", "${{ github.event.before }}")
                .add("PUSH_AFTER", "${{ github.sha }}"),
        );

    let job = Job::default()
        .name("Determine changes")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .outputs(outputs)
        .add_step(checkout_blacksmith_full_unnamed())
        .add_step(merge_base)
        .add_step(change_check(
            "Check if code changed",
            "check_code",
            "changed",
            &[
                ":baml_language/**",
                ":scripts/baml-language-version",
                ":scripts/baml-wrapper-version",
                ":scripts/baml-release-manifests",
                ":scripts/baml-package-manager-artifacts",
                ":scripts/install.sh",
                ":scripts/install.ps1",
                ":packaging/aur/**",
                ":tools/pkg_boundaryml_com/**",
                ":.github/workflows/ci.yaml",
                ":.github/workflows/release-baml-language.yml",
                ":.github/workflows/build2-python-sdk.reusable.yaml",
                ":.github/workflows/build2-nodejs-sdk.reusable.yaml",
                ":.github/workflows/cargo-tests.reusable.yaml",
                ":.github/workflows/size-gate.reusable.yaml",
                ":.github/workflows/wasm-pack-tests.reusable.yaml",
            ],
        ))
        .add_step(change_check(
            "Check if lexer code changed",
            "check_lexer",
            "changed",
            &[
                ":baml_language/crates/baml_compiler_lexer/**",
                ":baml_language/crates/baml_base/**",
                ":baml_language/Cargo.toml",
                ":baml_language/Cargo.lock",
            ],
        ))
        .add_step(change_check(
            "Check if parser code changed",
            "check_parser",
            "changed",
            &[
                ":baml_language/crates/baml_compiler_parser/**",
                ":baml_language/crates/baml_compiler_syntax/**",
                ":baml_language/crates/baml_compiler_lexer/**",
                ":baml_language/crates/baml_base/**",
            ],
        ))
        .add_step(change_check(
            "Check if HIR code changed",
            "check_hir",
            "changed",
            &[
                ":baml_language/crates/baml_compiler_hir/**",
                ":baml_language/crates/baml_workspace/**",
                ":baml_language/crates/baml_compiler_parser/**",
                ":baml_language/crates/baml_base/**",
            ],
        ))
        .add_step(change_check(
            "Check if THIR code changed",
            "check_thir",
            "changed",
            &[
                ":baml_language/crates/baml_thir/**",
                ":baml_language/crates/baml_compiler_hir/**",
                ":baml_language/crates/baml_base/**",
            ],
        ))
        .add_step(change_check(
            "Check if codegen code changed",
            "check_codegen",
            "changed",
            &[
                ":baml_language/crates/baml_compiler_emit/**",
                ":baml_language/crates/baml_thir/**",
                ":baml_language/crates/baml_base/**",
            ],
        ))
        .add_step(change_check(
            "Check if docs changed",
            "check_docs",
            "changed",
            &[
                ":fern/**",
                ":typescript/apps/ask-baml-client/**",
                ":typescript/apps/sage-backend/**",
                ":.github/workflows/docs.reusable.yaml",
            ],
        ))
        .add_step(change_check(
            "Check if webview changed",
            "check_webview",
            "changed",
            &[
                ":typescript2/app-vscode-webview/**",
                ":typescript2/pkg-playground/**",
                ":typescript2/pkg-proto/**",
                ":baml_language/crates/bridge_ctypes/types/**",
                ":.github/workflows/webview-tests.reusable.yaml",
            ],
        ))
        .add_step(change_check(
            "Check if unsafe code changed",
            "check_unsafe",
            "changed",
            &[":baml_language/crates/bex_heap/**"],
        ))
        .add_step(change_check(
            "Check if proto sources changed",
            "check_proto",
            "changed",
            &[":baml_language/crates/bridge_ctypes/**/*.proto"],
        ))
        // Includes the job's own inputs (workspace manifest with the
        // loom/minstant dep versions + check-cfg list, the pinned toolchain,
        // and this workflow file) so a change that can break the loom/miri
        // build re-runs the gate. Cargo.lock is deliberately excluded: it
        // churns on every dep bump and the cargo-tests job already covers the
        // std halves of the suite.
        .add_step(change_check(
            "Check if profiling ring code changed",
            "check_prof",
            "changed",
            &[
                ":baml_language/crates/bex_events/**",
                ":baml_language/Cargo.toml",
                ":baml_language/rust-toolchain.toml",
                ":.github/workflows/ci.yaml",
            ],
        ))
        .add_step(perf);

    steps::named_job("determine_changes", job)
}

/// Job `prek` — "Pre-commit Checks".
fn prek() -> NamedJob {
    let cache = steps::rust_cache()
        .shared_key("linux-cargo")
        .save_if("false")
        .build();
    let cache_prek = Step::new("Cache prek")
        .uses("actions", "cache", "v5")
        .add_with(("path", "~/.cache/pre-commit"))
        .add_with(("key", "pre-commit-${{ hashFiles('.pre-commit-config.yaml') }}"));
    let job = Job::default()
        .name("Pre-commit Checks")
        .runs_on(runners::BLACKSMITH_16VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(cond_code())
        .timeout_minutes(20u32)
        .add_step(steps::checkout_blacksmith("Checkout Branch"))
        .add_step(steps::rustup_toolchain_install())
        .add_step(cache)
        .add_step(steps::setup_mise("cargo:prek python uv"))
        .add_step(cache_prek)
        .add_step(steps::cargo_fetch())
        .add_step(Step::new("Run prek").run(RUN_PREK_SCRIPT));
    steps::named_job("prek", job)
}

/// Job `release-metadata` — "Release Metadata".
fn release_metadata() -> NamedJob {
    let job = Job::default()
        .name("Release Metadata")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(cond_code())
        .timeout_minutes(5u32)
        .add_step(steps::checkout_blacksmith_full("Checkout"))
        .add_step(
            Step::new("Validate baml_language release metadata")
                .run("scripts/baml-language-version check"),
        )
        .add_step(
            Step::new("Validate canary/nightly version computation")
                .add_env(("BAML_LANGUAGE_VERSION_DATE", "20260522"))
                .run(VALIDATE_VERSIONS_SCRIPT),
        );
    steps::named_job("release-metadata", job)
}

/// Job `cargo-tests` — calls `cargo-tests.reusable.yaml`.
fn cargo_tests() -> NamedJob {
    let job = vars::call_reusable_inherit("./.github/workflows/cargo-tests.reusable.yaml")
        .name("Cargo Tests")
        .needs(vec!["determine_changes".to_string()])
        .cond(cond_code());
    steps::named_job("cargo-tests", job)
}

/// Job `size-gate` — calls `size-gate.reusable.yaml`.
fn size_gate() -> NamedJob {
    let job = vars::call_reusable_inherit("./.github/workflows/size-gate.reusable.yaml")
        .name("Size Gate")
        .needs(vec!["determine_changes".to_string()])
        .cond(cond_code())
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .pull_requests(Level::Write),
        );
    steps::named_job("size-gate", job)
}

/// Job `wasm-pack-tests` — calls `wasm-pack-tests.reusable.yaml`.
fn wasm_pack_tests() -> NamedJob {
    let job = vars::call_reusable_inherit("./.github/workflows/wasm-pack-tests.reusable.yaml")
        .name("WASM Pack Tests")
        .needs(vec!["determine_changes".to_string()])
        .cond(cond_code());
    steps::named_job("wasm-pack-tests", job)
}

/// Job `docs` — calls the out-of-scope `docs.reusable.yaml` (always runs; no
/// `if:`). Preserved verbatim: `with: docs_changed/is_canary`,
/// `secrets: inherit`, `permissions: write-all`.
fn docs() -> NamedJob {
    let inputs = Input::default()
        .add("docs_changed", "${{ needs.determine_changes.outputs.docs }}")
        .add("is_canary", "${{ github.ref == 'refs/heads/canary' }}");
    let job = vars::call_reusable_inherit_with("./.github/workflows/docs.reusable.yaml", inputs)
        .name("Docs")
        .needs(vec!["determine_changes".to_string()])
        .permissions(vars::permissions_write_all());
    steps::named_job("docs", job)
}

/// Job `webview-tests` — calls `webview-tests.reusable.yaml`; gates on
/// `webview`, NOT `code`.
fn webview_tests() -> NamedJob {
    let job = vars::call_reusable_inherit("./.github/workflows/webview-tests.reusable.yaml")
        .name("Webview Tests")
        .needs(vec!["determine_changes".to_string()])
        .cond(vars::cond_output_or_main_canary("webview"));
    steps::named_job("webview-tests", job)
}

/// Job `miri-tests` — "Miri (unsafe code verification)". Currently disabled
/// (`if: false`).
fn miri_tests() -> NamedJob {
    let job = Job::default()
        .name("Miri (unsafe code verification)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(Expression::new("false"))
        .timeout_minutes(25u32)
        .add_step(steps::checkout_blacksmith("Checkout"))
        .add_step(
            Step::new("Install Rust nightly and Miri")
                .run(
                    "rustup toolchain install nightly --component miri\n\
                     rustup override set nightly\n",
                )
                .working_directory(vars::WD),
        )
        .add_step(
            Step::new("Fetch cargo dependencies")
                .run("cargo +nightly fetch")
                .working_directory(vars::WD),
        )
        .add_step(
            Step::new("Run Miri tests on bex_heap")
                .run("cargo miri test -p bex_heap --lib")
                .working_directory(vars::WD),
        );
    steps::named_job("miri-tests", job)
}

/// Job `prof-concurrency` — "Profiling ring (loom + miri)". Loom + Miri
/// verification for the lock-free profiling ring (bex_events::prof), scoped to
/// the prof:: tests so it stays minutes-fast — deliberately narrower than the
/// (disabled) whole-crate miri-tests job, whose bex_heap runs were timing out.
fn prof_concurrency() -> NamedJob {
    let cache = steps::rust_cache().shared_key("prof-concurrency").build();
    let job = Job::default()
        .name("Profiling ring (loom + miri)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(Expression::new(
            "needs.determine_changes.outputs.prof == 'true'",
        ))
        .timeout_minutes(45u32)
        .add_step(steps::checkout_blacksmith("Checkout"))
        .add_step(steps::rustup_toolchain_install())
        .add_step(cache)
        .add_step(steps::cargo_fetch())
        // The model checker explores every interleaving (bounded at 3
        // preemptions, set in the test harness) of the ring's
        // producer/consumer/lifecycle protocols. The custom cfg name
        // (baml_loom, not the conventional loom) keeps the flag from
        // half-activating loom support in third-party deps (e.g. boxcar) that
        // gate on cfg(loom) but need their own loom feature enabled to compile.
        .add_step(
            Step::new("Loom model checking (bex_events::prof)")
                .run("cargo test -p bex_events --release --lib prof::")
                .working_directory(vars::WD)
                .add_env(
                    Env::default()
                        .add("RUSTFLAGS", "--cfg baml_loom")
                        .add("CARGO_TARGET_DIR", "target/loom"),
                ),
        )
        .add_step(
            Step::new("Install Rust nightly and Miri")
                .run("rustup toolchain install nightly --component miri")
                .working_directory(vars::WD),
        )
        // Miri checks the raw-pointer/UnsafeCell discipline of the same
        // scenarios on real threads. Leaked rings are by design (&'static
        // lifetime model); isolation is off for park_timeout/sleep in the
        // stress tests.
        .add_step(
            Step::new("Miri (bex_events::prof)")
                .run("cargo +nightly miri test -p bex_events --lib prof::")
                .working_directory(vars::WD)
                .add_env(("MIRIFLAGS", "-Zmiri-ignore-leaks -Zmiri-disable-isolation")),
        );
    steps::named_job("prof-concurrency", job)
}

/// Job `proto-sync` — "proto generated files sync".
fn proto_sync() -> NamedJob {
    let cache = steps::rust_cache()
        .shared_key("linux-cargo")
        .save_if("false")
        .build();
    let job = Job::default()
        .name("proto generated files sync")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(Expression::new(
            "needs.determine_changes.outputs.proto == 'true'",
        ))
        .timeout_minutes(20u32)
        .add_step(steps::checkout_blacksmith("Checkout"))
        .add_step(steps::rustup_toolchain_install())
        .add_step(cache)
        .add_step(steps::setup_mise("node npm:pnpm protoc protoc-gen-go"))
        .add_step(steps::cargo_fetch())
        .add_step(
            Step::new("Generate Rust + Python proto bindings (bridge_ctypes)")
                .run("cargo build -p bridge_ctypes")
                .working_directory(vars::WD),
        )
        .add_step(
            Step::new("Generate Go proto bindings (sdks/go/bridge_go)")
                .run("./build.sh")
                .working_directory("baml_language/sdks/go/bridge_go"),
        )
        .add_step(
            // --ignore-workspace: install from bridge_nodejs's own
            // pnpm-lock.yaml rather than the root workspace lockfile. The two
            // resolve different protobufjs-cli versions, and pbjs codegen
            // output (committed under typescript_src/proto/) must come from
            // the version this package pins.
            // --ignore-scripts: skip dependency build scripts (esbuild's
            // binary fetch, protobufjs postinstall) — none are needed for
            // pbjs/tsc/napi codegen, and pnpm errors on unapproved build
            // scripts otherwise.
            // Same flags as sdk_tests/crates/typescript_node/setup.sh.
            Step::new("Install Node SDK dependencies")
                .run("pnpm install --frozen-lockfile --ignore-workspace --ignore-scripts")
                .working_directory("baml_language/sdks/nodejs/bridge_nodejs"),
        )
        .add_step(
            Step::new("Generate Node/TypeScript proto bindings (sdks/nodejs/bridge_nodejs)")
                .run("pnpm build:debug")
                .working_directory("baml_language/sdks/nodejs/bridge_nodejs"),
        )
        .add_step(Step::new("Check generated proto files are in sync").run(PROTO_SYNC_SCRIPT));
    steps::named_job("proto-sync", job)
}

/// Job `benchmarks-build` — "benchmarks build (baml)".
fn benchmarks_build() -> NamedJob {
    let cache = steps::rust_cache()
        .shared_key("linux-arm-bench")
        .save_if(vars::save_if_canary())
        .cache_all_crates(true)
        .cache_workspace_crates(true)
        .build();
    let job = Job::default()
        .name("benchmarks build (baml)")
        .runs_on(runners::BLACKSMITH_8VCPU_ARM)
        .needs(vec!["determine_changes".to_string()])
        .cond(Expression::new(
            "needs.determine_changes.outputs.run_perf == 'true'",
        ))
        .timeout_minutes(30u32)
        .add_step(steps::checkout_actions("Checkout Branch"))
        .add_step(steps::rustup_show())
        .add_step(cache)
        .add_step(steps::install_action("cargo-codspeed@4.7.0").name("Install cargo-codspeed"))
        .add_step(steps::cargo_fetch())
        .add_step(
            Step::new("Build benchmarks (walltime)")
                .run("cargo codspeed build -p baml_tests --bench runtime_benchmark -m walltime")
                .working_directory(vars::WD),
        )
        .add_step(
            steps::upload_artifact("codspeed-walltime-benches", "baml_language/target/codspeed/walltime")
                .name("Upload prebuilt benchmark binaries")
                .add_with(("if-no-files-found", "error"))
                .add_with(("retention-days", 1)),
        )
        .add_step(
            Step::new("Pack cargo home for benchmarks-run").run(
                "tar -C \"$HOME\" --exclude='.cargo/registry/src' -cf cargo-home.tar \
                 .cargo/registry .cargo/git",
            ),
        )
        .add_step(
            steps::upload_artifact("codspeed-cargo-home", "cargo-home.tar")
                .name("Upload cargo home")
                .add_with(("if-no-files-found", "error"))
                .add_with(("retention-days", 1)),
        );
    steps::named_job("benchmarks-build", job)
}

/// Job `benchmarks-run` — "benchmarks instrumented (baml)".
fn benchmarks_run() -> NamedJob {
    let run_bench = Step::new("Run benchmarks (walltime)")
        .uses("CodSpeedHQ", "action", "v4")
        .continue_on_error(true)
        .add_env(
            Env::default()
                .add("CARGO_NET_OFFLINE", "true")
                .add("DIVAN_MAX_TIME", "2"),
        )
        .add_with(("mode", "walltime"))
        .add_with((
            "run",
            "cd baml_language && cargo codspeed run -m walltime 'vm_speedtest'",
        ))
        .add_with(("token", "${{ secrets.CODSPEED_TOKEN }}"));

    let job = Job::default()
        .name("benchmarks instrumented (baml)")
        .runs_on(runners::CODSPEED_MACRO)
        .needs(vec![
            "determine_changes".to_string(),
            "benchmarks-build".to_string(),
        ])
        .cond(Expression::new(
            "needs.determine_changes.outputs.run_perf == 'true'",
        ))
        .timeout_minutes(30u32)
        .add_step(steps::checkout_actions("Checkout Branch"))
        .add_step(steps::rustup_show())
        .add_step(
            Step::new("Download cargo home")
                .uses("actions", "download-artifact", "v7")
                .add_with(("name", "codspeed-cargo-home"))
                .add_with(("path", "/tmp/cargo-home")),
        )
        .add_step(
            Step::new("Unpack cargo home into ~/.cargo")
                .run("tar -C \"$HOME\" -xf /tmp/cargo-home/cargo-home.tar"),
        )
        .add_step(steps::install_action("cargo-codspeed@4.7.0").name("Install cargo-codspeed"))
        .add_step(
            Step::new("Download prebuilt benchmark binaries")
                .uses("actions", "download-artifact", "v7")
                .add_with(("name", "codspeed-walltime-benches"))
                .add_with(("path", "baml_language/target/codspeed/walltime")),
        )
        .add_step(
            Step::new("Restore executable bit on bench binaries")
                .run("chmod -R +x baml_language/target/codspeed/walltime"),
        )
        .add_step(run_bench);
    steps::named_job("benchmarks-run", job)
}

/// Job `perf-pr-notice` — "Perf benchmarks (PR notice)".
fn perf_pr_notice() -> NamedJob {
    let comment = Step::new("Comment perf opt-in instructions")
        .uses("actions", "github-script", "v7")
        .add_env(("RUN_PERF", "${{ needs.determine_changes.outputs.run_perf }}"))
        .add_with(("script", PERF_NOTICE_SCRIPT));
    let job = Job::default()
        .name("Perf benchmarks (PR notice)")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec!["determine_changes".to_string()])
        .cond(Expression::new(
            "github.event_name == 'pull_request' \
             && github.event.pull_request.head.repo.fork == false",
        ))
        .permissions(Permissions::default().pull_requests(Level::Write))
        .add_step(comment);
    steps::named_job("perf-pr-notice", job)
}

/// Job `ci-failure-alert` — "CI-v2 Failure Alert".
fn ci_failure_alert() -> NamedJob {
    let job = Job::default()
        .name("CI-v2 Failure Alert")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .needs(vec![
            "prek".to_string(),
            "release-metadata".to_string(),
            "cargo-tests".to_string(),
            "wasm-pack-tests".to_string(),
            "docs".to_string(),
            "webview-tests".to_string(),
            "prof-concurrency".to_string(),
            "proto-sync".to_string(),
        ])
        .cond(Expression::new("${{ failure() || cancelled() }}"))
        .add_step(Step::new("Report failure").run(FAILURE_ALERT_SCRIPT));
    steps::named_job("ci-failure-alert", job)
}

/// CI orchestrator workflow.
#[must_use]
pub fn workflow() -> Workflow {
    let jobs: Vec<NamedJob> = vec![
        determine_changes(),
        prek(),
        release_metadata(),
        cargo_tests(),
        size_gate(),
        wasm_pack_tests(),
        docs(),
        webview_tests(),
        miri_tests(),
        prof_concurrency(),
        proto_sync(),
        benchmarks_build(),
        benchmarks_run(),
        perf_pr_notice(),
        ci_failure_alert(),
    ];

    vars::standard_env(Workflow::default().name("CI - BAML Language"))
        .permissions(
            Permissions::default()
                .contents(Level::Read)
                .id_token(Level::Write)
                .pull_requests(Level::Write),
        )
        .add_event(
            Event::default()
                .push(Push::default().add_branch("main").add_branch("canary"))
                .pull_request(PullRequest::default())
                .merge_group(MergeGroup::default().add_type(MergeGroupType::ChecksRequested))
                .workflow_dispatch(WorkflowDispatch::default()),
        )
        .concurrency(vars::concurrency_ci())
        .defaults(vars::bash_defaults())
        .map(|wf| jobs.into_iter().fold(wf, |wf, nj| nj.add_to(wf)))
}

// ---------------------------------------------------------------------------
// Verbatim run scripts
// ---------------------------------------------------------------------------

const CHECK_PERF_SCRIPT: &str = r###"# Any of these, case-insensitive, in the PR title/body or a commit
# message opts a PR into a perf run. Keep in sync with the instructions
# posted by the "Perf benchmarks (PR notice)" job below.
PATTERN='RUN_CODSPEED=1|run-perf|/perf'

# After merge (push to canary/main): only run when the merge actually
# touched baml_language/. NOTE: we diff the pushed range (before..after)
# directly rather than reuse determine_changes' `code` flag — on a push
# the merge-base of HEAD and origin/<branch> collapses to HEAD, so that
# diff is always empty here. The push payload's before/after are the
# real merged range.
if [ "$EVENT_NAME" = "push" ]; then
  if [ -z "$PUSH_BEFORE" ] || [ "$PUSH_BEFORE" = "0000000000000000000000000000000000000000" ]; then
    # Branch creation / unknown previous tip — nothing to diff against,
    # so run to be safe rather than silently skip.
    echo "run=true" >> "$GITHUB_OUTPUT"
    exit 0
  fi
  if git diff --quiet "$PUSH_BEFORE" "$PUSH_AFTER" -- ':baml_language/**'; then
    echo "run=false" >> "$GITHUB_OUTPUT"
  else
    echo "run=true" >> "$GITHUB_OUTPUT"
  fi
  exit 0
fi

# Manual dispatch always runs.
if [ "$EVENT_NAME" = "workflow_dispatch" ]; then
  echo "run=true" >> "$GITHUB_OUTPUT"
  exit 0
fi

# Never run in the merge queue — it already ran (or was opted out) on
# the PR.
if [ "$EVENT_NAME" = "merge_group" ]; then
  echo "run=false" >> "$GITHUB_OUTPUT"
  exit 0
fi

# On PRs: opt in via the PR title/body or any commit message in the PR.
if printf '%s\n%s' "$PR_TITLE" "$PR_BODY" | grep -qiE "$PATTERN"; then
  echo "run=true" >> "$GITHUB_OUTPUT"
  exit 0
fi
if [ -n "$MERGE_BASE" ] && git log --format='%B' "${MERGE_BASE}..HEAD" | grep -qiE "$PATTERN"; then
  echo "run=true" >> "$GITHUB_OUTPUT"
  exit 0
fi

echo "run=false" >> "$GITHUB_OUTPUT"
"###;

const VALIDATE_VERSIONS_SCRIPT: &str = r###"set -euo pipefail
canary="$(scripts/baml-language-version show)"
test "$(scripts/baml-language-version compute --channel canary)" = "$canary"
test "$(scripts/baml-language-version compute --channel canary --pypi)" = "$canary"

nightly="$(scripts/baml-language-version compute --channel nightly)"
pypi="$(scripts/baml-language-version compute --channel nightly --pypi)"
python3 - "$canary" "$nightly" "$pypi" <<'PY'
import re
import sys

canary, nightly, pypi = sys.argv[1:]
major, minor, patch = [int(part) for part in canary.split(".")]
base = f"{major}.{minor}.{patch + 1}"
match = re.fullmatch(rf"{re.escape(base)}-nightly\.20260522\.([a-z])", nightly)
if not match:
    raise SystemExit(f"unexpected nightly version: {nightly}")

index = ord(match.group(1)) - ord("a")
expected_pypi = f"{base}.dev20260522{index:02d}"
if pypi != expected_pypi:
    raise SystemExit(f"unexpected nightly PyPI version: {pypi} != {expected_pypi}")
PY
"###;

const RUN_PREK_SCRIPT: &str = r###"echo '```console' > "$GITHUB_STEP_SUMMARY"
# Enable color output for prek and remove it for the summary
# Use --hook-stage=manual to enable slower hooks that are skipped by default
SKIP=no-commit-to-branch prek run --all-files --show-diff-on-failure --color always --hook-stage manual | \
  tee >(sed -E 's/\x1B\[([0-9]{1,2}(;[0-9]{1,2})*)?[mGK]//g' >> "$GITHUB_STEP_SUMMARY") >&1
exit_code="${PIPESTATUS[0]}"
echo '```' >> "$GITHUB_STEP_SUMMARY"
exit "$exit_code"
"###;

const PROTO_SYNC_SCRIPT: &str = r###"# Check for both modified tracked files and untracked files across every
# codegen output documented in baml_language/crates/bridge_ctypes/README.md.
PATHS=(
  baml_language/sdks/nodejs/bridge_nodejs
  baml_language/crates/bridge_ctypes
  baml_language/sdks/go/bridge_go
  baml_language/sdks/python/src/baml_core/cffi
  typescript2/pkg-proto
)
README="baml_language/crates/bridge_ctypes/README.md"
STATUS=$(git status --porcelain -- "${PATHS[@]}")
if [ -n "$STATUS" ]; then
  echo "::error::proto generated files are out of sync — consult ${README} for the regeneration commands and commit the resulting changes."
  echo ""
  echo "The following files are out of sync:"
  echo "$STATUS"
  echo ""
  echo "===== ${README} ====="
  cat "${README}"
  echo "===== end of ${README} ====="
  echo ""
  git diff -- "${PATHS[@]}"
  exit 1
fi
echo "All generated proto files are in sync."
"###;

const PERF_NOTICE_SCRIPT: &str = r#"// Hidden marker so we find & update our own comment instead of
// posting a new one on every push.
const marker = '<!-- perf-benchmarks-pr-notice -->';
const triggered = process.env.RUN_PERF === 'true';
const body = triggered
  ? [
      marker,
      '### 🏎️ Performance benchmarks are running for this PR',
      '',
      'CodSpeed perf benchmarks were triggered because this PR opted in. ' +
        'Results will appear in the CodSpeed check / dashboard once they finish.',
    ].join('\n')
  : [
      marker,
      '### ⏭️ Performance benchmarks were skipped',
      '',
      'Perf benchmarks (CodSpeed) are **opt-in** on pull requests — they no ' +
        'longer run on every push. They always run automatically after merge ' +
        'to `canary`/`main`.',
      '',
      'To run them on **this** PR, do any of the following, then push a commit ' +
        '(or re-run CI):',
      '',
      '- Add `RUN_CODSPEED=1` to the PR description, **or**',
      '- Include `run-perf` or `/perf` in the PR title or any commit message.',
    ].join('\n');

const { owner, repo } = context.repo;
const issue_number = context.issue.number;
const comments = await github.paginate(github.rest.issues.listComments, {
  owner, repo, issue_number,
});
const existing = comments.find((c) => c.body && c.body.includes(marker));
if (existing) {
  await github.rest.issues.updateComment({ owner, repo, comment_id: existing.id, body });
} else {
  await github.rest.issues.createComment({ owner, repo, issue_number, body });
}
"#;

const FAILURE_ALERT_SCRIPT: &str = r###"echo "## ❌ CI Failed" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "One or more required jobs failed or were cancelled." >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "| Job | Result |" >> $GITHUB_STEP_SUMMARY
echo "|-----|--------|" >> $GITHUB_STEP_SUMMARY
echo "| prek | ${{ needs.prek.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| release-metadata | ${{ needs.release-metadata.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| cargo-tests | ${{ needs.cargo-tests.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| wasm-pack-tests | ${{ needs.wasm-pack-tests.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| docs | ${{ needs.docs.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| webview-tests | ${{ needs.webview-tests.result }} |" >> $GITHUB_STEP_SUMMARY
# echo "| miri-tests | ${{ needs.miri-tests.result }} |" >> $GITHUB_STEP_SUMMARY  # TEMPORARILY DISABLED
echo "| prof-concurrency | ${{ needs.prof-concurrency.result }} |" >> $GITHUB_STEP_SUMMARY
echo "| proto-sync | ${{ needs.proto-sync.result }} |" >> $GITHUB_STEP_SUMMARY
echo "" >> $GITHUB_STEP_SUMMARY
echo "::error::One or more CI jobs failed!"
exit 1
"###;
