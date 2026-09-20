//! `baml-cli program-store`: move programs into and out of a program store.
//!
//! A site server of the durable functions proof of concept serves
//! `GET /api/programs/:hash` and fetches programs from other sites (contract
//! section 9.5 of `documents/durable-poc-contracts.md`). This command gives it
//! both halves without a second implementation of the entry format, of the
//! hash check, or of the atomic write: every reader and writer of a store
//! goes through `bex_program_store`.
//!
//! stdout carries exactly one JSON object. The exit code is `0` when the
//! object describes the requested outcome and `1` when it is `{ "error" }`.
//!
//! - `export <hash>` prints `{ hash, runtime_build, program_base64 }`, the
//!   body of `GET /api/programs/:hash`.
//! - `import` reads one such object from stdin, verifies the hash, stores the
//!   program, and prints `{ hash, written, usable }`.
//! - `has <hash>` prints `{ hash, usable, reason }`.
//!
//! `usable` means that a worker of this runtime build can run the program
//! from the store. An entry that another runtime build stored is kept, and it
//! is not usable.

use std::{
    io::{Read as _, Write as _},
    path::PathBuf,
};

use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use bex_program_store::{Lookup, ProgramHash, ProgramStore};
use clap::{Args, Subcommand};
use serde_json::{Value as Json, json};

#[derive(Args, Debug)]
pub struct ProgramStoreArgs {
    /// The store directory.
    #[arg(long, value_name = "DIR")]
    pub store: PathBuf,

    #[command(subcommand)]
    pub command: ProgramStoreCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProgramStoreCommand {
    /// Print `{ hash, runtime_build, program_base64 }` for a stored program.
    Export { hash: String },
    /// Read `{ hash, runtime_build, program_base64 }` from stdin and store it.
    Import,
    /// Print whether a worker of this build can run the program from the store.
    Has { hash: String },
}

impl ProgramStoreArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let (body, code) = match self.execute() {
            Ok(body) => (body, crate::ExitCode::Success),
            // `TargetError` is the variant that maps to exit code 1.
            Err(error) => (
                json!({ "error": format!("{error:#}") }),
                crate::ExitCode::TargetError,
            ),
        };
        // A closed stdout is the reader's decision and not an error here.
        let _ = writeln!(std::io::stdout().lock(), "{body}");
        Ok(code)
    }

    fn execute(&self) -> Result<Json> {
        let store = ProgramStore::open(&self.store, bex_engine::durable::runtime_build())?;
        match &self.command {
            ProgramStoreCommand::Export { hash } => {
                let hash: ProgramHash = hash.parse()?;
                // Prefer the entry as this build vouches for it. An entry of
                // another build is still forwarded, under that build's name,
                // so that a site can pass on what it cannot run itself.
                let (program, runtime_build) = match store.get(hash) {
                    Lookup::Found(program) => (program, store.runtime_build().to_string()),
                    Lookup::Missing(_) => match store.get_any_build(hash) {
                        Lookup::Found(program) => {
                            let build = program.written_by().to_string();
                            (program, build)
                        }
                        Lookup::Missing(reason) => {
                            return Err(anyhow!("program {hash} is not in the store: {reason}"));
                        }
                    },
                };
                Ok(json!({
                    "hash": hash.to_hex(),
                    "runtime_build": runtime_build,
                    "program_base64": base64::engine::general_purpose::STANDARD.encode(program.bytes()),
                }))
            }
            ProgramStoreCommand::Import => {
                let mut text = String::new();
                std::io::stdin()
                    .lock()
                    .read_to_string(&mut text)
                    .context("cannot read stdin")?;
                let body: Json = serde_json::from_str(&text).context("stdin is not JSON")?;
                let field = |name: &str| {
                    body[name]
                        .as_str()
                        .ok_or_else(|| anyhow!("the field `{name}` is missing or not a string"))
                };
                let hash: ProgramHash = field("hash")?.parse()?;
                let program = base64::engine::general_purpose::STANDARD
                    .decode(field("program_base64")?)
                    .context("`program_base64` is not Base64")?;
                let outcome = store.put_received(hash, &program, field("runtime_build")?)?;
                Ok(json!({
                    "hash": hash.to_hex(),
                    "written": outcome.written,
                    "usable": matches!(store.get(hash), Lookup::Found(_)),
                }))
            }
            ProgramStoreCommand::Has { hash } => {
                let hash: ProgramHash = hash.parse()?;
                Ok(match store.get(hash) {
                    Lookup::Found(_) => {
                        json!({ "hash": hash.to_hex(), "usable": true, "reason": null })
                    }
                    Lookup::Missing(reason) => {
                        json!({ "hash": hash.to_hex(), "usable": false, "reason": reason.to_string() })
                    }
                })
            }
        }
    }
}
