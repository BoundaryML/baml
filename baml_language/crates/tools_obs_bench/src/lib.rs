//! obs-bench — benchmark & acceptance harness for BAML observability
//! (TASK/design.md §10). Library surface so integration tests exercise the
//! same code paths as the `obs-bench` binary.

#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI harness: stdout is the row/report surface, stderr the progress log"
)]

pub mod baseline;
pub mod calibrate;
pub mod corpus;
pub mod crashfuzz;
pub mod gen_paths;
pub mod machine;
pub mod prof_stats;
pub mod replay;
pub mod report;
pub mod rows;
pub mod runner;
pub mod validate;
pub mod value_stats;
