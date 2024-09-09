use assert_cmd::prelude::*;

use anyhow::Result;
use indoc::indoc;
use std::{any, path::PathBuf, process::Command};

#[derive(Debug, Eq, PartialEq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
pub struct Harness {
    pub test_dir: PathBuf,
}

impl Harness {
    pub fn new<S: AsRef<str>>(test_name: S) -> Result<Self> {
        if std::env::var("OPENAI_API_KEY").is_err() || std::env::var("ANTHROPIC_API_KEY").is_err() {
            anyhow::bail!(indoc! {"
                Integration tests using tests/harness.rs require OPENAI_API_KEY and ANTHROPIC_API_KEY.
                
                You can skip these using 'cargo test --lib', or run these specifically using 'cargo test --test test_cli'.
            "});
        }

        // Run the CLI in /tmp/baml-runtime-test-harness/test_name.
        // Re-create it on test start, purging its contents. (We deliberately do NOT clean up after ourselves, to enable debugging of a failed test.)
        let test_dir = std::env::temp_dir()
            .join("baml-runtime-test-harness")
            .join(test_name.as_ref());
        std::fs::create_dir_all(&test_dir)?;
        std::fs::remove_dir_all(&test_dir)?;
        std::fs::create_dir(&test_dir)?;

        Ok(Self { test_dir })
    }

    // data_path is relative to repository root
    pub fn run_cli<S: AsRef<str>>(&self, args: S) -> Result<Command> {
        let args = args.as_ref();

        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME"))?;

        cmd.args(args.split_ascii_whitespace());
        cmd.current_dir(&self.test_dir);
        // cmd.env("RUST_BACKTRACE", "1");
        cmd.env("BAML_LOG", "debug,jsonish=info");

        Ok(cmd)
    }
}
