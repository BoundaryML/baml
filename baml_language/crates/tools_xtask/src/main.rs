//! `cargo xtask` — workspace task automation.
//!
//! Currently exposes a single subcommand, `workflows`, which generates the
//! `.github/workflows/*.yaml` files from typed Rust generators so the CI
//! configuration is code-reviewed, refactorable, and drift-checked.

// This is a binary crate whose `workflows::{steps,vars,runners}` modules form
// an internal "library" of helpers consumed by the per-workflow generator
// modules. The full helper surface is intentionally `pub` (so every generator
// module can use it) even though, until all generators are filled in, some
// helpers are not yet referenced. Both lints fire on that scaffolding and are
// not actionable here.
#![allow(unreachable_pub, dead_code)]
// `FluentBuilder::when_none` takes `&Option<T>` deliberately (mirrors zed's
// xtask helper surface that the generator modules are written against), so the
// `ref_option` pedantic lint does not apply.
#![allow(clippy::ref_option)]
// Doc comments here reference YAML keys, env-var names, runner labels and shell
// tokens (e.g. CARGO_INCREMENTAL, BAML_SCCACHE_R2_*, webview_tests) that are not
// Rust items; backticking all of them adds noise without value.
#![allow(clippy::doc_markdown)]

mod tasks;
mod workflows;

use clap::Parser;

#[derive(clap::Parser)]
#[command(name = "cargo-xtask", bin_name = "cargo xtask")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Generate .github/workflows YAML from the Rust generators.
    Workflows(WorkflowsArgs),
}

#[derive(clap::Args, Default)]
struct WorkflowsArgs {
    /// Verify committed YAML matches the generators instead of writing.
    #[arg(long)]
    check: bool,
}

/// Tolerate the cargo plugin convention: `cargo xtask workflows` invokes the
/// binary as `cargo-xtask xtask workflows`, so strip a leading `xtask` token.
/// Direct `cargo run -p cargo-xtask -- workflows` (no `xtask` token) also works.
fn normalized_args() -> Vec<String> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("xtask") {
        args.remove(1);
    }
    args
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse_from(normalized_args());
    match args.command {
        Command::Workflows(a) => tasks::workflows::run(tasks::workflows::RunArgs { check: a.check }),
    }
}
