//! One shared setup path for every command that loads a BAML project.
//!
//! `check`, `run`, `test`, `pack`, `describe`, and `generate` all
//! need the same preamble — resolve the project, build the database, open
//! the bytecode cache, install the warm seeds — but historically each
//! command hand-rolled its own subset, and the subsets drifted: `pack`
//! shipped without any cache participation, `describe` never got the
//! warm seeds or the parallel index prime. [`ProjectSession`] is the single
//! implementation; a command states *how* it uses the cache and gets the
//! rest of the ritual identically.
//!
//! The phases stay separate on purpose:
//!
//! 1. [`ProjectSession::open`] / [`ProjectSession::open_lenient`] — resolve +
//!    read sources + build the salsa database + open the cache. No expensive
//!    compiler work.
//! 2. [`ProjectSession::try_cached_program`] — the whole-program hit for
//!    executing commands (`run`/`test`/`pack`); called *before* warm prep so
//!    a hit skips it entirely.
//! 3. [`ProjectSession::warm_prep`] — stdlib-interface seed + per-file reuse
//!    plan (throw facts, callable-throws fragments, diagnostics blobs).
//! 4. [`ProjectSession::prime`] — parallel per-file semantic-index prime for
//!    commands that query whole-package aggregates without running the check
//!    collectors (which prime internally): `describe`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use baml_project::ProjectDatabase;

use crate::{
    bytecode_cache::{CacheContext, ReusePlan},
    project_load::{
        ResolvedProject, build_db_from_sources, resolve_project_sources, resolve_standalone_file,
        validate_file_project_flags,
    },
};

/// How a command participates in the bytecode cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CacheUse {
    /// Read the warm seeds and store artifacts after a successful compile —
    /// the `run`/`check`/`pack` keyspace (`emit_test_cases: false`).
    ReadWrite,
    /// Same, in the `baml test` keyspace (`emit_test_cases: true`).
    ReadWriteTests,
    /// Consume warm seeds, never store: introspection commands
    /// (`describe`, `generate`). Shares the run/check keyspace.
    ReadOnly,
    /// No cache at all (projectless fallback, tests).
    Off,
}

impl CacheUse {
    fn emit_test_cases(self) -> bool {
        matches!(self, CacheUse::ReadWriteTests)
    }
}

/// The state of the warm-database preamble after [`ProjectSession::warm_prep`].
pub(crate) struct SessionWarmth {
    pub(crate) reuse_plan: Option<ReusePlan>,
    /// Whether the stdlib typed interface was served from the cache —
    /// storing commands skip re-writing it in that case.
    pub(crate) stdlib_interface_hit: bool,
}

/// A loaded project plus its cache handle: the shared preamble every
/// project-consuming command starts from.
pub(crate) struct ProjectSession {
    pub(crate) resolved: ResolvedProject,
    pub(crate) db: ProjectDatabase,
    pub(crate) cache: Option<CacheContext>,
}

impl ProjectSession {
    /// Strict open: manifest is validated (a build-shaped command must fail
    /// fast on a broken `baml.toml`). The session may still have zero files —
    /// commands own their empty-project error text.
    pub(crate) fn open(from: Option<&Path>, cache_use: CacheUse) -> Result<Self> {
        let resolved = resolve_project_sources(from)?;
        Ok(Self::from_resolved(resolved, cache_use))
    }

    /// Open either a discovered project or one hermetic standalone source.
    ///
    /// Standalone files deliberately skip the bytecode cache: they have no
    /// project manifest or stable project root to own cache state. The file's
    /// parent is still installed as the database root so relative paths resolve
    /// consistently with `baml run --file` and `baml pack --file`.
    pub(crate) fn open_project_or_file(
        from: Option<&Path>,
        file: Option<&Path>,
        cache_use: CacheUse,
    ) -> Result<Self> {
        validate_file_project_flags(file, from)?;
        let Some(file) = file else {
            return Self::open(from, cache_use);
        };

        let canonical = resolve_standalone_file(file)?;
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read {}", canonical.display()))?;
        let root = canonical
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        Ok(Self::from_resolved(
            ResolvedProject {
                root,
                manifest: None,
                files: vec![(canonical, content)],
            },
            CacheUse::Off,
        ))
    }

    /// Lenient open for introspection commands (`describe`): never
    /// fails on a missing or invalid `baml.toml`. Inside a project the
    /// manifest is read raw (its bytes still key the cache, unvalidated);
    /// outside any project the session is an empty database rooted at the
    /// search directory, with the cache off — introspection always has
    /// *something* to work with.
    pub(crate) fn open_lenient(from: Option<&Path>, cache_use: CacheUse) -> Result<Self> {
        match crate::project_load::resolve_project_sources_lenient(from)? {
            Some(resolved) => Ok(Self::from_resolved(resolved, cache_use)),
            None => {
                let root = crate::project_load::projectless_search_dir(from)?;
                let mut db = ProjectDatabase::new();
                db.set_project_root(&root);
                Ok(Self {
                    resolved: ResolvedProject {
                        root,
                        manifest: None,
                        files: Vec::new(),
                    },
                    db,
                    cache: None,
                })
            }
        }
    }

    fn from_resolved(resolved: ResolvedProject, cache_use: CacheUse) -> Self {
        let db = build_db_from_sources(&resolved, |_| {});
        let cache = match cache_use {
            CacheUse::Off => None,
            _ => CacheContext::open(&resolved, cache_use.emit_test_cases()),
        };
        Self {
            resolved,
            db,
            cache,
        }
    }

    /// The whole-program cache hit, for commands that execute or package the
    /// compiled program. Call before [`Self::warm_prep`]: a hit makes the
    /// warm preamble unnecessary. Gated off under `BAML_CACHE_VERIFY` so the
    /// verify tripwire always exercises the full compile path.
    pub(crate) fn try_cached_program(&self) -> Option<bex_vm_types::Program> {
        if CacheContext::verify_enabled() {
            return None;
        }
        self.cache.as_ref().and_then(CacheContext::load)
    }

    /// Seed the stdlib typed interface and prepare the per-file reuse plan —
    /// the identical warm-database setup for every cache-participating
    /// command. A no-op (all-`None`) session without a cache.
    pub(crate) fn warm_prep(&mut self) -> SessionWarmth {
        let Some(ctx) = &self.cache else {
            return SessionWarmth {
                reuse_plan: None,
                stdlib_interface_hit: false,
            };
        };
        let prep = ctx.prepare_warm_db(&mut self.db);
        SessionWarmth {
            reuse_plan: prep.reuse_plan,
            stdlib_interface_hit: prep.stdlib_interface_hit,
        }
    }

    /// Variant of [`Self::warm_prep`] for read-only introspection
    /// (`describe`): installs the stdlib-interface seed always, and
    /// the per-file throws seeds only on a **no-delta** plan — where they are
    /// byte-for-byte the stored values and the serve-time gate is a proven
    /// tautology. On a project with edits, introspection simply derives
    /// honestly (no seeds, no gate, no staleness surface); the parallel
    /// index prime keeps that fast.
    pub(crate) fn warm_prep_seeds_only(&mut self) -> SessionWarmth {
        let Some(ctx) = &self.cache else {
            return SessionWarmth {
                reuse_plan: None,
                stdlib_interface_hit: false,
            };
        };
        let stdlib_interface_hit = ctx.seed_stdlib_interface(&mut self.db);
        let reuse_plan = ctx.plan_reuse(&self.db).and_then(|mut plan| {
            if !plan.no_delta {
                return None;
            }
            self.db
                .set_seeded_throw_facts(std::mem::take(&mut plan.seeded_throw_facts));
            self.db
                .set_seeded_callable_throws(std::mem::take(&mut plan.seeded_callable_throws));
            Some(plan)
        });
        SessionWarmth {
            reuse_plan,
            stdlib_interface_hit,
        }
    }

    /// Parallel per-file semantic-index prime. Commands that query
    /// whole-package aggregates *outside* the check collectors (`describe`)
    /// call this so the aggregate fold is parallel-fed instead of a
    /// serial parse of the project. Harmless to call twice.
    pub(crate) fn prime(&self) {
        baml_project::prime_file_indexes_parallel(&self.db);
    }

    /// A fresh, un-seeded database over the same sources — the honest
    /// baseline the sampled verify oracle compares served artifacts against.
    pub(crate) fn honest_db(&self) -> ProjectDatabase {
        build_db_from_sources(&self.resolved, |_| {})
    }

    pub(crate) fn file_count(&self) -> usize {
        self.resolved.files.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.resolved.files.is_empty()
    }

    pub(crate) fn root(&self) -> &PathBuf {
        &self.resolved.root
    }

    /// Whether any source would change under `baml fmt` — the shared input
    /// for `run`/`pack`'s format advisory.
    pub(crate) fn needs_format_hint(&self) -> bool {
        self.resolved
            .files
            .iter()
            .any(|(_, source)| crate::run_command::source_needs_format_hint(source))
    }
}
