//! Step builders, the `FluentBuilder` trait, the lightweight `NamedJob`
//! wrapper, and the artifact-block job-threading helpers.
//!
//! Translate agents depend on these EXACT signatures.
//!
//! ## Naming convention
//!
//! Typed steps are renamed with the fork's `.name(...)` setter (delegated from
//! `StepValue` onto both `Step<Run>` and `Step<Use>`). The checkout helpers
//! therefore take an explicit `name: &str`. Working-directory is set with
//! `.working_directory(...)`, conditions with `.if_condition(Expression)`, ids
//! with `.id(...)`.
//!
//! ## Local composite actions / reusable `uses: ./path`
//!
//! The fork's `Step::uses(owner, repo, ver)` formats `owner/repo@ver`; there is
//! no public API for a raw `uses: ./path`. [`uses_local_step`] sets the
//! `Step.value.uses` field directly to emit `uses: <path>` verbatim.

use gh_workflow::{Expression, Input, Job, JobType, Run, Step, Use};
use indoc::indoc;

use crate::workflows::vars::WD;

// ---------------------------------------------------------------------------
// FluentBuilder
// ---------------------------------------------------------------------------

/// Imperative-conditional builder helper (copied from zed's xtask). Impl'd for
/// [`Job`], `Workflow`, [`Input`], and [`Step<T>`].
pub trait FluentBuilder {
    fn map<U>(self, f: impl FnOnce(Self) -> U) -> U
    where
        Self: Sized,
    {
        f(self)
    }

    #[must_use]
    fn when(self, cond: bool, then: impl FnOnce(Self) -> Self) -> Self
    where
        Self: Sized,
    {
        if cond {
            then(self)
        } else {
            self
        }
    }

    #[must_use]
    fn when_else(
        self,
        cond: bool,
        then: impl FnOnce(Self) -> Self,
        else_fn: impl FnOnce(Self) -> Self,
    ) -> Self
    where
        Self: Sized,
    {
        if cond {
            then(self)
        } else {
            else_fn(self)
        }
    }

    #[must_use]
    fn when_some<T>(self, opt: Option<T>, then: impl FnOnce(Self, T) -> Self) -> Self
    where
        Self: Sized,
    {
        if let Some(value) = opt {
            then(self, value)
        } else {
            self
        }
    }

    #[must_use]
    fn when_none<T>(self, opt: &Option<T>, then: impl FnOnce(Self) -> Self) -> Self
    where
        Self: Sized,
    {
        if opt.is_none() {
            then(self)
        } else {
            self
        }
    }
}

impl<J: JobType> FluentBuilder for Job<J> {}
impl FluentBuilder for gh_workflow::Workflow {}
impl FluentBuilder for Input {}
impl<T> FluentBuilder for Step<T> {}

// ---------------------------------------------------------------------------
// NamedJob
// ---------------------------------------------------------------------------

/// A job plus the string key it is registered under in `jobs:`. Job `needs`
/// and output references are wired by this name throughout the workflow
/// modules.
///
/// The job is stored type-erased behind a closure so that `RunJob` and
/// `UsesJob` jobs can be collected into a single `Vec<NamedJob>` and added to a
/// [`Workflow`] uniformly.
pub struct NamedJob {
    pub name: String,
    add: Box<dyn FnOnce(gh_workflow::Workflow) -> gh_workflow::Workflow>,
}

impl NamedJob {
    /// Register this named job onto a [`Workflow`], erasing its `JobType` so
    /// jobs of different types (`RunJob` / `UsesJob`) can be added uniformly.
    #[must_use]
    pub fn add_to(self, wf: gh_workflow::Workflow) -> gh_workflow::Workflow {
        (self.add)(wf)
    }
}

#[must_use]
pub fn named_job<J: JobType + 'static>(name: &str, job: Job<J>) -> NamedJob {
    let name = name.to_string();
    let key = name.clone();
    NamedJob { name, add: Box::new(move |wf| wf.add_job(key, job)) }
}

// ---------------------------------------------------------------------------
// Low-level local-uses helper
// ---------------------------------------------------------------------------

/// Emits `- name: <name>\n  uses: <path>` — a step that `uses:` a local path
/// (composite action or reusable workflow) verbatim. Used to implement
/// [`setup_mise`] / [`setup_node2`] and any other `./.github/actions/*` ref.
#[must_use]
pub fn uses_local_step(name: &str, path: &str) -> Step<Use> {
    // Step::new(..).uses(..) is the only way to obtain a Step<Use>; we then
    // overwrite the formatted `owner/repo@ver` with the raw local path.
    let mut step = Step::new(name).uses("local", "local", "local");
    step.value.uses = Some(path.to_string());
    step
}

// ---------------------------------------------------------------------------
// Checkout helpers
// ---------------------------------------------------------------------------

/// H1: `useblacksmith/checkout@v1` with `persist-credentials: false`, no
/// `fetch-depth`.
#[must_use]
pub fn checkout_blacksmith(name: &str) -> Step<Use> {
    Step::new(name)
        .uses("useblacksmith", "checkout", "v1")
        .add_with(("persist-credentials", false))
}

/// H1 + `fetch-depth: 0`.
#[must_use]
pub fn checkout_blacksmith_full(name: &str) -> Step<Use> {
    Step::new(name)
        .uses("useblacksmith", "checkout", "v1")
        .add_with(("persist-credentials", false))
        .add_with(("fetch-depth", 0))
}

/// H2: `actions/checkout@v6` with `persist-credentials: false`.
#[must_use]
pub fn checkout_actions(name: &str) -> Step<Use> {
    Step::new(name)
        .uses("actions", "checkout", "v6")
        .add_with(("persist-credentials", false))
}

// ---------------------------------------------------------------------------
// Rust toolchain helpers (all set working-directory = baml_language)
// ---------------------------------------------------------------------------

/// H3: name `Install Rust toolchain`, `rustup show`, wd `baml_language`.
#[must_use]
pub fn rustup_show() -> Step<Run> {
    Step::new("Install Rust toolchain")
        .run("rustup show")
        .working_directory(WD)
}

/// H3 wasm: `rustup show` + `rustup target add wasm32-unknown-unknown`.
#[must_use]
pub fn rustup_show_wasm() -> Step<Run> {
    Step::new("Install Rust toolchain")
        .run("rustup show\nrustup target add wasm32-unknown-unknown")
        .working_directory(WD)
}

/// `rustup toolchain install`, wd `baml_language` (prek / proto-sync).
#[must_use]
pub fn rustup_toolchain_install() -> Step<Run> {
    Step::new("Install Rust toolchain")
        .run("rustup toolchain install")
        .working_directory(WD)
}

// ---------------------------------------------------------------------------
// Rust cache (H4) builder
// ---------------------------------------------------------------------------

/// Builder for `Swatinem/rust-cache@v2`. `build()` always emits
/// `workspaces: "baml_language -> target"`, plus whatever knobs were set.
pub struct RustCache {
    shared_key: Option<String>,
    save_if: Option<String>,
    cache_all_crates: Option<bool>,
    cache_targets: Option<bool>,
    cache_workspace_crates: Option<bool>,
}

#[must_use]
pub fn rust_cache() -> RustCache {
    RustCache {
        shared_key: None,
        save_if: None,
        cache_all_crates: None,
        cache_targets: None,
        cache_workspace_crates: None,
    }
}

impl RustCache {
    #[must_use]
    pub fn shared_key(mut self, k: &str) -> Self {
        self.shared_key = Some(k.to_string());
        self
    }

    /// Accepts a literal like `vars::save_if_canary()` or `"false"`.
    #[must_use]
    pub fn save_if(mut self, expr: &str) -> Self {
        self.save_if = Some(expr.to_string());
        self
    }

    #[must_use]
    pub fn cache_all_crates(mut self, b: bool) -> Self {
        self.cache_all_crates = Some(b);
        self
    }

    #[must_use]
    pub fn cache_targets(mut self, b: bool) -> Self {
        self.cache_targets = Some(b);
        self
    }

    #[must_use]
    pub fn cache_workspace_crates(mut self, b: bool) -> Self {
        self.cache_workspace_crates = Some(b);
        self
    }

    #[must_use]
    pub fn build(self) -> Step<Use> {
        let mut step = Step::new("Cache Rust dependencies")
            .uses("Swatinem", "rust-cache", "v2")
            .add_with(("workspaces", "baml_language -> target"));
        if let Some(k) = self.shared_key {
            step = step.add_with(("shared-key", k));
        }
        if let Some(s) = self.save_if {
            step = step.add_with(("save-if", s));
        }
        if let Some(b) = self.cache_all_crates {
            step = step.add_with(("cache-all-crates", b));
        }
        if let Some(b) = self.cache_targets {
            step = step.add_with(("cache-targets", b));
        }
        if let Some(b) = self.cache_workspace_crates {
            step = step.add_with(("cache-workspace-crates", b));
        }
        step
    }
}

// ---------------------------------------------------------------------------
// Composite-action helpers (local `uses:`)
// ---------------------------------------------------------------------------

/// H5: `uses: ./.github/actions/setup-mise` with `install_args`, name
/// `Install mise`. Caller may rename via `.name(...)`.
#[must_use]
pub fn setup_mise(install_args: &str) -> Step<Use> {
    uses_local_step("Install mise", "./.github/actions/setup-mise")
        .add_with(("install_args", install_args))
}

/// H6: `uses: ./.github/actions/setup-node2`, name
/// `Setup Node.js for typescript2`.
#[must_use]
pub fn setup_node2() -> Step<Use> {
    uses_local_step("Setup Node.js for typescript2", "./.github/actions/setup-node2")
}

// ---------------------------------------------------------------------------
// Common run-step helpers
// ---------------------------------------------------------------------------

/// H7: `Load .envrc with direnv`.
#[must_use]
pub fn load_direnv() -> Step<Run> {
    Step::new("Load .envrc with direnv")
        .run(indoc! {r#"
            set -euo pipefail
            direnv allow
            direnv export gha >> "$GITHUB_ENV"
        "#})
        .working_directory(WD)
}

/// H8: `Fetch cargo dependencies`, `cargo fetch`, wd `baml_language`.
#[must_use]
pub fn cargo_fetch() -> Step<Run> {
    Step::new("Fetch cargo dependencies")
        .run("cargo fetch")
        .working_directory(WD)
}

/// H8 wasm: `cargo fetch --target wasm32-unknown-unknown`.
#[must_use]
pub fn cargo_fetch_wasm() -> Step<Run> {
    Step::new("Fetch cargo dependencies")
        .run("cargo fetch --target wasm32-unknown-unknown")
        .working_directory(WD)
}

/// H9: `taiki-e/install-action@v2` with `tool: cargo-nextest`.
#[must_use]
pub fn install_nextest() -> Step<Use> {
    install_action("cargo-nextest")
}

/// Generalized `taiki-e/install-action@v2` with the given `tool`
/// (e.g. `cargo-codspeed@4.7.0`, `cargo-insta,cargo-nextest`).
#[must_use]
pub fn install_action(tool: &str) -> Step<Use> {
    Step::new(format!("Install {tool}"))
        .uses("taiki-e", "install-action", "v2")
        .add_with(("tool", tool))
}

/// H12: `Verify sccache`, `sccache --version` (no working-directory).
#[must_use]
pub fn verify_sccache() -> Step<Run> {
    Step::new("Verify sccache").run("sccache --version")
}

// ---------------------------------------------------------------------------
// Upload helper
// ---------------------------------------------------------------------------

/// `actions/upload-artifact@v7` with `name` + `path`. Caller adds
/// `if-no-files-found` / `if:failure` / `retention-days` via `.add_with(..)` /
/// `.if_condition(..)`.
#[must_use]
pub fn upload_artifact(name: &str, path: &str) -> Step<Use> {
    Step::new(format!("Upload {name}"))
        .uses("actions", "upload-artifact", "v7")
        .add_with(("name", name))
        .add_with(("path", path))
}

// ---------------------------------------------------------------------------
// Artifact-block job-threading helpers (H10 / H11)
//
// These blocks mix Step<Run> and Step<Use>; to sidestep heterogeneous Vec
// typing they THREAD a Job: `job = steps::sccache_stats(job, "test-linux");`.
// ---------------------------------------------------------------------------

fn always() -> Expression {
    Expression::new("always()")
}

/// H10: appends the 4-step sccache-stats block (all `if: always()`):
/// show stats, dump json to `sccache-stats-<id>.json`, upload via
/// `actions/upload-artifact@v7` (name + path `sccache-stats-<id>`,
/// `if-no-files-found: ignore`).
#[must_use]
pub fn sccache_stats(job: Job, id: &str) -> Job {
    job.add_step(
        Step::new("Show sccache stats")
            .run("sccache --show-stats")
            .if_condition(always()),
    )
    .add_step(
        Step::new("Dump sccache stats json")
            .run(format!("sccache --show-stats --stats-format json > sccache-stats-{id}.json"))
            .if_condition(always()),
    )
    .add_step(
        upload_artifact(&format!("sccache-stats-{id}"), &format!("sccache-stats-{id}.json"))
            .add_with(("if-no-files-found", "ignore"))
            .if_condition(always()),
    )
}

/// H11: appends the 2-step cargo-timings block (all `if: always()`): rename
/// `target/cargo-timings/cargo-timing.html` -> `cargo-timing-<id>.html` (wd
/// `baml_language`) + upload `cargo-timings-<id>`.
#[must_use]
pub fn cargo_timings(job: Job, id: &str) -> Job {
    job.add_step(
        Step::new("Rename cargo timings")
            .run(format!("mv target/cargo-timings/cargo-timing.html cargo-timing-{id}.html"))
            .working_directory(WD)
            .if_condition(always()),
    )
    .add_step(
        upload_artifact(
            &format!("cargo-timings-{id}"),
            &format!("{WD}/cargo-timing-{id}.html"),
        )
        .if_condition(always()),
    )
}

/// WASM variant of [`cargo_timings`]: the rename is wrapped in an
/// `if [[ -f ... ]]` guard (see cargo-test-wasm).
#[must_use]
pub fn cargo_timings_conditional(job: Job, id: &str) -> Job {
    job.add_step(
        Step::new("Rename cargo timings")
            .run(format!(
                "if [[ -f target/cargo-timings/cargo-timing.html ]]; then \
                 mv target/cargo-timings/cargo-timing.html cargo-timing-{id}.html; fi"
            ))
            .working_directory(WD)
            .if_condition(always()),
    )
    .add_step(
        upload_artifact(
            &format!("cargo-timings-{id}"),
            &format!("{WD}/cargo-timing-{id}.html"),
        )
        .add_with(("if-no-files-found", "ignore"))
        .if_condition(always()),
    )
}
