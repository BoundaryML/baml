//! Reusable Webview Tests workflow -> `webview-tests.reusable.yaml`.
//!
//! Reproduces `.github/workflows/webview-tests.reusable.yaml`: a `workflow_call`
//! reusable workflow with three independent Linux jobs (`typecheck`,
//! `unit-tests`, `browser-tests`) that share an identical 6-step setup prefix
//! (checkout, setup-node2, wasm rust toolchain, rust-cache `linux-wasm`, wasm
//! `cargo fetch`, mise `cargo:wasm-pack`).
//!
//! NOTE: webview_tests is the ONLY module with NO top-level env block — do
//! NOT call `vars::standard_env` here.

use gh_workflow::{Event, Level, Permissions, Step, Workflow, WorkflowCall};

use crate::workflows::{runners, steps, vars};

/// Reusable webview-tests workflow.
#[must_use]
pub fn workflow() -> Workflow {
    vars::reusable_workflow("Webview Tests (Reusable)")
        .add_event(Event::default().workflow_call(WorkflowCall::default()))
        .permissions(Permissions::default().contents(Level::Read))
        .concurrency(vars::concurrency_named("webview-tests"))
        .add_job("typecheck", typecheck())
        .add_job("unit-tests", unit_tests())
        .add_job("browser-tests", browser_tests())
}

/// Steps 1–6, identical across all three jobs: blacksmith checkout, setup-node2,
/// wasm rust toolchain, read-only `linux-wasm` rust-cache, wasm `cargo fetch`,
/// and the `cargo:wasm-pack` mise install.
fn add_shared_prefix(job: gh_workflow::Job) -> gh_workflow::Job {
    job.add_step(steps::checkout_blacksmith("Checkout"))
        .add_step(steps::setup_node2())
        .add_step(steps::rustup_show_wasm())
        .add_step(steps::rust_cache().shared_key("linux-wasm").save_if("false").build())
        .add_step(steps::cargo_fetch_wasm())
        .add_step(steps::setup_mise("cargo:wasm-pack"))
}

/// Build the WASM bundle consumed by app-vscode-webview (`pnpm --filter
/// pkg-playground build:wasm`, wd `typescript2`). Common to all three jobs.
fn build_wasm() -> Step<gh_workflow::Run> {
    Step::new("Build WASM")
        .run("pnpm --filter pkg-playground build:wasm")
        .working_directory("typescript2")
}

/// Job `typecheck` — "Typecheck".
fn typecheck() -> gh_workflow::Job {
    add_shared_prefix(
        gh_workflow::Job::default()
            .name("Typecheck")
            .runs_on(runners::BLACKSMITH_4VCPU)
            .timeout_minutes(15u32),
    )
    .add_step(build_wasm())
    .add_step(
        Step::new("Generate proto types")
            .run("pnpm --filter @b/pkg-proto generate")
            .working_directory("typescript2"),
    )
    .add_step(
        Step::new("Typecheck pkg-proto")
            .run("pnpm --filter @b/pkg-proto typecheck")
            .working_directory("typescript2"),
    )
    .add_step(
        Step::new("Typecheck app-vscode-webview")
            .run("pnpm --filter app-vscode-webview typecheck")
            .working_directory("typescript2"),
    )
}

/// Job `unit-tests` — "Unit Tests (jsdom)".
fn unit_tests() -> gh_workflow::Job {
    add_shared_prefix(
        gh_workflow::Job::default()
            .name("Unit Tests (jsdom)")
            .runs_on(runners::BLACKSMITH_4VCPU)
            .timeout_minutes(15u32),
    )
    .add_step(build_wasm())
    .add_step(
        Step::new("Run unit tests")
            .run("pnpm --filter app-vscode-webview test:unit:run")
            .working_directory("typescript2"),
    )
}

/// Job `browser-tests` — "Browser Tests (Playwright)".
fn browser_tests() -> gh_workflow::Job {
    add_shared_prefix(
        gh_workflow::Job::default()
            .name("Browser Tests (Playwright)")
            .runs_on(runners::BLACKSMITH_4VCPU)
            .timeout_minutes(20u32),
    )
    .add_step(build_wasm())
    .add_step(
        Step::new("Install Playwright browsers")
            .run("npx playwright install chromium")
            .working_directory("typescript2/app-vscode-webview"),
    )
    .add_step(
        Step::new("Run browser tests")
            .run("pnpm --filter app-vscode-webview test:browser:run")
            .working_directory("typescript2"),
    )
}
