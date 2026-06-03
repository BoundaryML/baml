//! Workflow-level helpers: shared env block, condition strings, concurrency,
//! reusable-workflow scaffolding, and typed output wiring.
//!
//! Translate agents depend on these EXACT signatures.

use std::fmt::Display;

use gh_workflow::{
    Concurrency, Defaults, Env, Expression, Input, Job, Permissions, RunDefaults, UsesJob,
    Workflow, WorkflowCallSecret,
};
use indexmap::IndexMap;

/// Canonical working-directory for cargo/rustup steps.
pub const WD: &str = "baml_language";

/// Sentinel `with:` key emitted by [`call_reusable_inherit`]. The generation
/// driver rewrites jobs carrying this key so they emit `secrets: inherit`
/// instead. The fork's `secrets` field is unreachable through the public API
/// (`Job.config` is private and `UsesJob.secrets` is `#[setters(skip)]`), so
/// `secrets: inherit` is injected as a post-serialization text transform keyed
/// on this sentinel. See `tasks::workflows::apply_secrets_inherit`.
pub const SECRETS_INHERIT_SENTINEL: &str = "__xtask_secrets_inherit__";

/// Sentinel `timeout-minutes` value emitted by jobs whose timeout is a
/// templated expression the fork's `Option<u32>` field cannot hold. The
/// generation driver rewrites `timeout-minutes: <SENTINEL>` into
/// [`TIMEOUT_MATRIX_EXPR`]. See `tasks::workflows::apply_matrix_timeout`.
pub const TIMEOUT_MATRIX_SENTINEL: u32 = 424_242;

/// Replacement text for [`TIMEOUT_MATRIX_SENTINEL`].
pub const TIMEOUT_MATRIX_EXPR: &str = "${{ matrix.timeout }}";

/// The scope keys (kebab-case, in `Permissions` serialization order) that
/// [`permissions_write_all`] sets to `write`. The generation driver collapses
/// any `permissions:` block whose children are exactly this set into the
/// `permissions: write-all` scalar, because the fork's [`Permissions`] struct
/// models only these 10 scopes and cannot express `write-all` (which also
/// grants attestations / discussions / models / repository-projects /
/// security-events). See `tasks::workflows::apply_write_all`.
pub const WRITE_ALL_SCOPES: &[&str] = &[
    "actions",
    "contents",
    "issues",
    "pull-requests",
    "deployments",
    "checks",
    "statuses",
    "packages",
    "pages",
    "id-token",
];

/// The four standard cargo env vars, applied at the workflow top level.
///
/// Used by ci / cargo_tests (+ sccache extras) / size_gate / wasm_pack, but
/// NOT webview_tests (which deliberately has no env block).
#[must_use]
pub fn standard_env(workflow: Workflow) -> Workflow {
    workflow.envs(
        Env::default()
            .add("CARGO_INCREMENTAL", 0)
            .add("CARGO_NET_RETRY", 10)
            .add("CARGO_TERM_COLOR", "always")
            .add("RUSTUP_MAX_RETRIES", 10),
    )
}

/// Returns a standalone [`Env`] with the four standard cargo vars, for callers
/// that need to extend it (e.g. cargo_tests adds the two `BAML_SCCACHE_R2_*`
/// secrets, size_gate / wasm_pack reuse it verbatim).
#[must_use]
pub fn standard_env_block() -> Env {
    Env::default()
        .add("CARGO_INCREMENTAL", 0)
        .add("CARGO_NET_RETRY", 10)
        .add("CARGO_TERM_COLOR", "always")
        .add("RUSTUP_MAX_RETRIES", 10)
}

/// `code == 'true' || main || canary`
#[must_use]
pub fn cond_code_or_main_canary() -> Expression {
    cond_output_or_main_canary("code")
}

/// Generalized: `<output> == 'true' || main || canary`.
#[must_use]
pub fn cond_output_or_main_canary(output: &str) -> Expression {
    Expression::new(format!(
        "needs.determine_changes.outputs.{output} == 'true' \
         || github.ref == 'refs/heads/main' \
         || github.ref == 'refs/heads/canary'"
    ))
}

/// `${{ github.ref == 'refs/heads/canary' }}` — for `rust_cache().save_if(...)`.
#[must_use]
pub fn save_if_canary() -> &'static str {
    "${{ github.ref == 'refs/heads/canary' }}"
}

/// Top-level CI concurrency group.
#[must_use]
pub fn concurrency_ci() -> Concurrency {
    Concurrency::default()
        .group(
            "${{ github.workflow }}-${{ github.ref_name }}-\
             ${{ github.event.pull_request.number || github.sha }}"
                .to_string(),
        )
        .cancel_in_progress(true)
}

/// `${{ github.workflow }}-${{ github.ref }}-<suffix>`, cancel-in-progress true.
#[must_use]
pub fn concurrency_named(suffix: &str) -> Concurrency {
    Concurrency::default()
        .group(format!("${{{{ github.workflow }}}}-${{{{ github.ref }}}}-{suffix}"))
        .cancel_in_progress(true)
}

/// `bash` defaults block.
#[must_use]
pub fn bash_defaults() -> Defaults {
    Defaults::default().run(RunDefaults::default().shell("bash"))
}

/// Scaffolding for the top of a reusable workflow file: a [`Workflow`] with the
/// given name and `defaults.run.shell: bash`. The caller adds
/// `on.workflow_call`, `env`, and `permissions` as needed.
#[must_use]
pub fn reusable_workflow(name: &str) -> Workflow {
    Workflow::default().name(name).defaults(bash_defaults())
}

/// Builds a [`WorkflowCallSecret`] for the `on.workflow_call.secrets` map.
#[must_use]
pub fn workflow_call_secret(description: &str, required: bool) -> WorkflowCallSecret {
    WorkflowCallSecret { description: description.to_string(), required }
}

/// A reusable-workflow CALLER job (`uses: ./<path>`) with `secrets: inherit`
/// already wired (via the [`SECRETS_INHERIT_SENTINEL`] post-serialization
/// transform). The caller chains `.needs`/`.cond`/`.permissions` and wraps the
/// result in a [`crate::workflows::steps::NamedJob`].
///
/// IMPORTANT: do NOT call `.with(..)` on the returned job — the fork's `with`
/// setter REPLACES rather than merges, which would clobber the sentinel that
/// drives the `secrets: inherit` rewrite. If the caller needs `with:` inputs,
/// use [`call_reusable_inherit_with`] and pass them up front.
#[must_use]
pub fn call_reusable_inherit(uses_local_path: &str) -> Job<UsesJob> {
    Job::default()
        .uses_local(uses_local_path.to_string())
        .with(Input::default().add(SECRETS_INHERIT_SENTINEL, true))
}

/// Like [`call_reusable_inherit`] but also sets `with:` inputs. The sentinel
/// and the caller's inputs are written together in one `with:` block (the fork
/// cannot merge into an existing `with:` after the fact). `inputs` are
/// `(key, value)` pairs whose values may be any `serde_json`-convertible scalar
/// (strings, bools, ints — ints are de-arbitrary-precision'd by the driver).
#[must_use]
pub fn call_reusable_inherit_with(
    uses_local_path: &str,
    inputs: Input,
) -> Job<UsesJob> {
    let mut with = Input::default().add(SECRETS_INHERIT_SENTINEL, true);
    for (k, v) in inputs.0 {
        with = with.add(k, v);
    }
    Job::default().uses_local(uses_local_path.to_string()).with(with)
}

/// Typed job-output reference: `Display`s to
/// `${{ needs.<job>.outputs.<name> }}`.
pub struct JobOutput {
    pub job: String,
    pub name: String,
}

impl Display for JobOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${{{{ needs.{}.outputs.{} }}}}", self.job, self.name)
    }
}

impl JobOutput {
    #[must_use]
    pub fn new(job: &str, name: &str) -> Self {
        Self { job: job.to_string(), name: name.to_string() }
    }
}

/// `${{ steps.<step_id>.outputs.<output_name> }}`.
#[must_use]
pub fn step_output(step_id: &str, output_name: &str) -> String {
    format!("${{{{ steps.{step_id}.outputs.{output_name} }}}}")
}

/// Builds a job `outputs:` map from `(output_name, step_id)` pairs, wiring each
/// to `${{ steps.<step_id>.outputs.<output_name> }}`.
#[must_use]
pub fn job_outputs(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs
        .iter()
        .map(|(output_name, step_id)| {
            ((*output_name).to_string(), step_output(step_id, output_name))
        })
        .collect()
}

/// Like [`job_outputs`], but each job-level output reads a fixed step-output
/// `key` (e.g. `changed`) rather than reusing the job-output name as the step
/// key. Used by `determine_changes`, whose `check_<flag>` steps all write the
/// `changed` key.
#[must_use]
pub fn job_outputs_with_key(key: &str, pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs
        .iter()
        .map(|(output_name, step_id)| {
            ((*output_name).to_string(), step_output(step_id, key))
        })
        .collect()
}

/// `permissions: write-all` cannot be expressed via the fork's [`Permissions`]
/// struct (it only models the per-scope map). Callers that need it (the docs
/// job) set every modeled scope to `Write`; the generation driver then collapses
/// the resulting block (which matches [`WRITE_ALL_SCOPES`]) back into the
/// `permissions: write-all` scalar so the GITHUB_TOKEN is granted write on
/// *all* scopes — including the ones the fork cannot model (attestations,
/// discussions, models, repository-projects, security-events). See
/// `tasks::workflows::apply_write_all`. Exposed here so callers have one place
/// to build it.
#[must_use]
pub fn permissions_write_all() -> Permissions {
    use gh_workflow::Level::Write;
    Permissions::default()
        .actions(Write)
        .contents(Write)
        .issues(Write)
        .pull_requests(Write)
        .deployments(Write)
        .checks(Write)
        .statuses(Write)
        .packages(Write)
        .pages(Write)
        .id_token(Write)
}
