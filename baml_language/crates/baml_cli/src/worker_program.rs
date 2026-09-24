//! Where the worker's program comes from (contract section 9.5).
//!
//! A program is identified by its program hash: the SHA-256 of the
//! Borsh-encoded `Program`, the value a snapshot header carries. The worker
//! obtains the program in exactly one place, [`WorkerArgs::load_engine`]:
//!
//! 1. When the hash is known (`--program-hash`, or the snapshot header of a
//!    `--resume`) and `--program-store` is given, the worker reads the entry
//!    from the store. The store verifies the bytes against the hash, so a
//!    worker never runs a program other than the one that was asked for.
//! 2. Otherwise, or when the store has no usable entry, the worker compiles
//!    `--project`. A compile that does not produce the expected hash is an
//!    error. The compiled program is put into the store.
//! 3. With neither, the run fails with `program <hash> is not in the store`.
//!
//! Nothing but the bytes defines the identity. The optimization level, the
//! compiler build, and the sources all change the bytes, so they change the
//! hash, and a mismatch in any of them surfaces as a hash mismatch.
//!
//! This file is a child module of `worker_command`, so that it can use the
//! worker's private state without widening its visibility.

use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::{Context as _, Result, anyhow};
use baml_db::baml_compiler_diagnostics::Severity;
use bex_engine::BexEngine;
use bex_program_store::{Lookup, ProgramHash, ProgramStore};
use bex_vm_types::Program;
use serde_json::{Value as Json, json};

use super::{
    ProgramIdentity, WorkerArgs, WorkerState, build_engine, diagnostic, ms_since, opt_level,
};

/// How the worker obtained its program (`resumed.stats.program_source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgramSource {
    Store,
    Compile,
}

impl ProgramSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::Compile => "compile",
        }
    }
}

/// Time spent in each step of the program load, in milliseconds. A step that
/// did not run is `None`.
#[derive(Clone, Copy, Debug, Default)]
struct LoadTimings {
    /// Read the store entry and verify its hash.
    store_read_ms: Option<f64>,
    /// Borsh-decode the program.
    decode_ms: Option<f64>,
    /// Compile the project and Borsh-encode the program.
    compile_ms: Option<f64>,
    /// Put the compiled program into the store.
    store_put_ms: Option<f64>,
    /// Build the engine from the program.
    engine_ms: Option<f64>,
}

pub(super) struct LoadedEngine {
    pub(super) engine: BexEngine,
    hash: [u8; 32],
    source: ProgramSource,
    timings: LoadTimings,
}

impl LoadedEngine {
    pub(super) fn stats(&self) -> ProgramStats {
        ProgramStats(json!({
            "program_source": self.source.as_str(),
            "program_store_read_ms": self.timings.store_read_ms,
            "program_decode_ms": self.timings.decode_ms,
            "program_compile_ms": self.timings.compile_ms,
            "program_store_put_ms": self.timings.store_put_ms,
            "program_engine_ms": self.timings.engine_ms,
        }))
    }
}

/// Temporary files of the program store older than this belong to a writer
/// that died.
const STALE_TEMP_FILE_AGE: std::time::Duration = std::time::Duration::from_secs(3600);

/// Fields that join `resumed.stats` and the `hello` of a start. The contract
/// defines `program_source`. The `program_*_ms` fields break `program_load_ms`
/// down, and a reader that does not know them ignores them.
pub(super) struct ProgramStats(Json);

impl ProgramStats {
    /// `{ "stats": <stats plus these fields> }`, the fields of `resumed`.
    pub(super) fn added_to(&self, mut stats: Json) -> Json {
        merge(&mut stats, &self.0);
        json!({ "stats": stats })
    }
}

/// Lowercase hex of a program hash, as events report it.
pub(super) fn hex(hash: &[u8; 32]) -> String {
    ProgramHash::from_bytes(*hash).to_hex()
}

/// Copy the fields of the object `from` into the object `into`.
fn merge(into: &mut Json, from: &Json) {
    if let (Some(into), Some(from)) = (into.as_object_mut(), from.as_object()) {
        for (key, value) in from {
            into.insert(key.clone(), value.clone());
        }
    }
}

impl WorkerArgs {
    fn open_program_store(&self) -> Result<Option<ProgramStore>> {
        self.program_store
            .as_ref()
            .map(|dir| ProgramStore::open(dir, bex_engine::durable::runtime_build()))
            .transpose()
            .map_err(|error| anyhow!("cannot use --program-store: {error}"))
    }

    /// Load the engine of a `--start` and emit `hello`, which carries the
    /// program hash. `hello` is emitted when the load fails too, so that it
    /// stays the first event of every worker.
    pub(super) fn load_engine_for_start(&self, state: &Arc<WorkerState>) -> Result<LoadedEngine> {
        let started = Instant::now();
        let requested = self
            .program_hash
            .as_deref()
            .map(|text| text.parse::<ProgramHash>())
            .transpose()
            .map_err(|error| anyhow!("--program-hash: {error}"));
        // The hash of the loaded program, or the requested one when the load
        // failed, or `null` when neither is known.
        let mut reported = requested.as_ref().ok().copied().flatten();
        let loaded = requested
            .and_then(|requested| self.load_engine(state, requested.map(|hash| *hash.as_bytes())));
        if let Ok(loaded) = &loaded {
            reported = Some(ProgramHash::from_bytes(loaded.hash));
        }

        let mut hello = json!({
            "mode": "start",
            "function": state.run_info.function,
            "durable": state.run_info.durable,
            "program_hash": reported.map(|hash| hash.to_hex()),
            "runtime_build": bex_engine::durable::runtime_build(),
        });
        if let Ok(loaded) = &loaded {
            merge(&mut hello, &loaded.stats().0);
            merge(&mut hello, &json!({ "program_load_ms": ms_since(started) }));
        }
        state.events.emit("hello", hello);
        loaded
    }

    /// The single place where the worker obtains its program and builds the
    /// engine. `expected` is the hash the program must have, when it is known.
    pub(super) fn load_engine(
        &self,
        state: &Arc<WorkerState>,
        expected: Option<[u8; 32]>,
    ) -> Result<LoadedEngine> {
        let store = self.open_program_store()?;
        let mut timings = LoadTimings::default();

        if let (Some(store), Some(expected)) = (&store, expected)
            && let Some(program) = read_from_store(store, expected, &mut timings)
        {
            return finish(
                state,
                program.0,
                expected,
                program.1,
                ProgramSource::Store,
                timings,
            );
        }

        // Without a store the worker behaves as before the store existed and
        // compiles the current directory when `--project` is absent.
        let project = self
            .from
            .clone()
            .or_else(|| store.is_none().then(|| PathBuf::from(".")));
        let Some(project) = project else {
            match expected {
                Some(expected) => anyhow::bail!("program {} is not in the store", hex(&expected)),
                None => anyhow::bail!("--start needs --project or --program-hash"),
            }
        };

        let compile_started = Instant::now();
        let program = compile_project(&project, state.run_info.opt_level)?;
        let bytes = borsh::to_vec(&program).context("the program does not serialize")?;
        let hash = bex_engine::durable::program_hash_of_bytes(&bytes);
        timings.compile_ms = Some(ms_since(compile_started));
        if let Some(expected) = expected
            && hash != expected
        {
            anyhow::bail!(
                "the project at {} compiles to program {}, which is not the requested program {} \
                 (program hash differs: the sources, the optimization level, or the compiler are \
                 not the same)",
                project.display(),
                hex(&hash),
                hex(&expected),
            );
        }

        if let Some(store) = &store {
            let put_started = Instant::now();
            let outcome = store.put(&bytes);
            timings.store_put_ms = Some(ms_since(put_started));
            match outcome {
                Ok(outcome) => {
                    debug_assert_eq!(outcome.hash.as_bytes(), &hash);
                    // A writer that died (a worker, or a site server that
                    // stored a fetched program) leaves its temporary file
                    // behind. Nothing else collects them, so a worker that
                    // just wrote to the store does. The age protects the file
                    // of a writer that is still at work.
                    if outcome.written {
                        let removed = store.remove_stale_temp_files(STALE_TEMP_FILE_AGE);
                        if removed > 0 {
                            diagnostic(format_args!(
                                "removed {removed} stale temporary file(s) from the program store"
                            ));
                        }
                    }
                }
                // A start publishes the program: other sites and later resumes
                // will ask the store for the hash that `hello` reports. A
                // start that cannot publish fails instead of promising a
                // program that nobody can fetch.
                Err(error) if expected.is_none() => {
                    return Err(anyhow!("cannot store program {}: {error}", hex(&hash)));
                }
                // Here the store is being repaired for a run that can go on
                // without it.
                Err(error) => {
                    diagnostic(format_args!("cannot store program {}: {error}", hex(&hash)))
                }
            }
        }
        finish(
            state,
            program,
            hash,
            bytes.len(),
            ProgramSource::Compile,
            timings,
        )
    }
}

/// The verified and decoded store entry for `expected`, with the length of its
/// Borsh bytes. Every failure is reported on stderr and treated as a missing
/// entry.
fn read_from_store(
    store: &ProgramStore,
    expected: [u8; 32],
    timings: &mut LoadTimings,
) -> Option<(Program, usize)> {
    let read_started = Instant::now();
    let lookup = store.get(ProgramHash::from_bytes(expected));
    timings.store_read_ms = Some(ms_since(read_started));
    let stored = match lookup {
        Lookup::Found(stored) => stored,
        Lookup::Missing(reason) => {
            diagnostic(format_args!(
                "program {} is not usable from the store at {}: {reason}",
                hex(&expected),
                store.root().display(),
            ));
            return None;
        }
    };
    let decode_started = Instant::now();
    let decoded = borsh::from_slice::<Program>(stored.bytes());
    timings.decode_ms = Some(ms_since(decode_started));
    match decoded {
        Ok(program) => Some((program, stored.bytes().len())),
        // The bytes are the ones that were stored, and this build stored
        // them, so this is a development build whose `Program` layout changed
        // without a change of the runtime build name.
        Err(error) => {
            diagnostic(format_args!(
                "program {} in the store does not decode with this build: {error}",
                hex(&expected),
            ));
            None
        }
    }
}

fn finish(
    state: &Arc<WorkerState>,
    program: Program,
    hash: [u8; 32],
    program_bytes: usize,
    source: ProgramSource,
    mut timings: LoadTimings,
) -> Result<LoadedEngine> {
    state.set_program(ProgramIdentity {
        hash,
        bytes: program_bytes as u64,
    });
    let engine_started = Instant::now();
    let engine = build_engine(program, state)?;
    timings.engine_ms = Some(ms_since(engine_started));
    Ok(LoadedEngine {
        engine,
        hash,
        source,
        timings,
    })
}

/// Compile the project at `project_dir`.
fn compile_project(project_dir: &std::path::Path, level: u8) -> Result<Program> {
    let session = crate::project_session::ProjectSession::open(
        Some(project_dir),
        crate::project_session::CacheUse::Off,
    )?;
    if session.is_empty() {
        anyhow::bail!("no `.baml` files found in {}", session.root().display());
    }
    let errors: Vec<_> = baml_db::collect_diagnostics(&session.db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        let rendered = crate::check_command::render_project_diagnostics(&session.db, &errors);
        anyhow::bail!("compilation errors found:\n{rendered}");
    }
    baml_db::baml_compiler2_emit::generate_project_bytecode_with_opt(
        &session.db,
        session.package,
        opt_level(level),
    )
    .map_err(|e| anyhow!("compilation failed: {e:?}"))
}
