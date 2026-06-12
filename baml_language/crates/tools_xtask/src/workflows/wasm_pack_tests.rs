//! Reusable WASM Pack Tests workflow -> `wasm-pack-tests.reusable.yaml`.
//!
//! Mirrors the hand-written `.github/workflows/wasm-pack-tests.reusable.yaml`:
//! a single-job reusable (`workflow_call`) workflow that runs the wasm-pack
//! tests for the `sys_llm` crate.

#[allow(unused_imports)]
use crate::workflows::{
    runners, steps,
    steps::{FluentBuilder, NamedJob},
    vars,
};
use gh_workflow::{Event, Job, Level, Permissions, Step, WorkflowCall};

/// Reusable wasm-pack-tests workflow (`WASM Pack Tests (Reusable)`).
#[must_use]
pub fn workflow() -> gh_workflow::Workflow {
    let job = wasm_pack_test();

    let wf = vars::standard_env(vars::reusable_workflow("WASM Pack Tests (Reusable)"))
        // on: { workflow_call: {} }
        .add_event(Event::default().workflow_call(WorkflowCall::default()))
        // permissions: { contents: read }
        .permissions(Permissions::default().contents(Level::Read))
        // concurrency: ${{ github.workflow }}-${{ github.ref }}-wasm-pack-tests
        .concurrency(vars::concurrency_named("wasm-pack-tests"));
    job.add_to(wf)
}

/// Job `wasm-pack-test` — "wasm-pack test".
fn wasm_pack_test() -> NamedJob {
    let job = Job::default()
        .name("wasm-pack test")
        .runs_on(runners::BLACKSMITH_4VCPU)
        .timeout_minutes(20u32)
        // 1. H1 blacksmith checkout (persist-credentials: false, no fetch-depth)
        .add_step(steps::checkout_blacksmith("Checkout"))
        // 2. Install Rust toolchain: rustup show + wasm target, wd baml_language
        .add_step(steps::rustup_show_wasm())
        // 3. rust-cache: shared-key linux-wasm, save-if false
        .add_step(
            steps::rust_cache()
                .shared_key("linux-wasm")
                .save_if("false")
                .build(),
        )
        // 4. Fetch cargo deps for the wasm target, wd baml_language
        .add_step(steps::cargo_fetch_wasm())
        // 5. setup-mise with install_args: cargo:wasm-pack
        .add_step(steps::setup_mise("cargo:wasm-pack"))
        // 6. Run wasm-pack tests (sys_llm), wd baml_language
        .add_step(
            Step::new("Run wasm-pack tests (sys_llm)")
                .run("wasm-pack test --node crates/sys_llm")
                .working_directory(vars::WD),
        );

    steps::named_job("wasm-pack-test", job)
}
