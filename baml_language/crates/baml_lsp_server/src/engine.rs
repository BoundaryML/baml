//! Playground engine lifecycle: one runtime per workspace root.
//!
//! The salvaged shape of the old `bex_project` runtime state, rebuilt on the
//! owner/snapshot model. The old source gate is gone; its job — linearizing
//! revision reads against engine commits — is done by running every commit
//! and every validated read *on the owner thread* (an [`baml_lsp::OwnerEvent`]
//! `Call`), where the authoritative [`SourceRevision`] cannot move
//! underneath. This module therefore never touches the database: callers
//! hand it the revision they read on the owner, and the compiled program
//! they emitted on a snapshot.
//!
//! Pipeline (wired by the playground host):
//!
//! ```text
//! source change (NotificationHook) ──debounce──▶ owner Call: revision + snapshot
//!        snapshot job (diagnostics lane): check errors → emit Program
//!        tokio blocking task: construct candidate (runs `$init`)
//!        owner Call: [`ProjectRuntime::commit_if_current`] (revision fenced)
//! ```
//!
//! A superseded candidate changes nothing; a panicked build simply never
//! commits (there is no poisoned/`Broken` state — the 2.2 lock rules apply:
//! `parking_lot` leaf locks, never held across queries or `.await`s).

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use baml_compiler2_visualization::control_flow::ControlFlowGraph;
use baml_lsp::SourceRevision;
use bex_project::{BexEngine, Handle, RuntimeError};
use parking_lot::Mutex;

/// The engine a winning commit installed, plus the identity it answers for.
#[derive(Clone)]
pub struct InstalledEngine {
    /// The owner revision the engine's program was emitted at.
    pub source_revision: SourceRevision,
    /// Monotonic per-root engine identity; only a winning commit consumes
    /// one. Runs and overlays key on it.
    pub generation: u64,
    pub engine: Arc<BexEngine>,
}

/// An engine built from a compiled program, not yet installed. `$init` has
/// already run (candidate-locally); the profiling lifecycle stays inactive
/// until the candidate wins a commit.
pub struct EngineCandidate {
    pub source_revision: SourceRevision,
    engine: BexEngine,
}

impl EngineCandidate {
    /// Take the engine out without committing it. For hosts that want a
    /// throwaway engine (tests probing the platform), never the rebuild
    /// path — a committed engine must go through
    /// [`ProjectRuntime::commit_if_current`], which is what activates
    /// profiling and fences the revision.
    pub fn into_engine(self) -> BexEngine {
        self.engine
    }
}

/// Proof of a winning conditional commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitReceipt {
    pub source_revision: SourceRevision,
    pub generation: u64,
}

/// Result of [`ProjectRuntime::commit_if_current`].
pub enum CommitOutcome {
    Committed {
        receipt: CommitReceipt,
        /// The engine this commit replaced, handed back for the caller to
        /// shut down. Commits are owner-thread work and the owner is not a
        /// tokio worker, so the runtime cannot spawn the drain itself — see
        /// [`spawn_engine_shutdown`].
        retired: Option<Arc<BexEngine>>,
    },
    /// The source revision advanced past the candidate; nothing changed and
    /// the candidate was dropped quietly.
    Superseded { current_revision: SourceRevision },
}

/// Why a run snapshot could not be produced. There is no `Busy`/`Broken`
/// axis anymore: reads are owner-linearized and panics never poison state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrepareRunError {
    /// No engine yet, or the installed engine predates the current source.
    /// Runs never silently use a last-known-good engine.
    #[error("engine is not current with the latest sources; wait for the rebuild")]
    NeedsCurrentBuild,
}

/// Coherent snapshot for launching a function run.
#[derive(Clone)]
pub struct RunSnapshot {
    pub generation: u64,
    pub engine: Arc<BexEngine>,
}

/// Ticket for one test-collection attempt, captured atomically.
pub struct CollectionTicket {
    pub generation: u64,
    collection_epoch: u64,
    pub engine: Arc<BexEngine>,
    pub cancel: sys_types::CancellationToken,
}

/// Coherent lease for test-registry work (running a test, expanding a set),
/// captured in one owner-linearized transaction.
pub struct RegistryLease {
    pub generation: u64,
    /// Epoch the leased registry was collected under. Emission fencing
    /// compares this too: a same-generation re-collection replaces the
    /// registry object, and results computed against the old one are stale.
    collection_epoch: u64,
    pub engine: Arc<BexEngine>,
    pub handle: Handle,
    pub cancel: sys_types::CancellationToken,
    /// One mutation owner per installed registry: expansions mutate the
    /// registry heap object in place, so they serialize on this.
    pub expansion_gate: Arc<tokio::sync::Mutex<()>>,
}

/// Why a registry lease could not be produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RegistryLeaseError {
    /// The requested generation is not the installed one, or the installed
    /// engine no longer matches current source.
    #[error("engine is not current with the latest sources; wait for the rebuild")]
    NeedsCurrentBuild,
    /// Collection has not produced a registry for this generation yet.
    #[error("tests have not been collected for this build yet")]
    NoRegistry,
    /// Collection completed and the project has no tests.
    #[error("the project has no tests")]
    NoTests,
}

/// Coherent runtime identity for one workspace root. All fields swap
/// together under one lock.
struct RuntimeState {
    installed: Option<InstalledEngine>,
    /// Allocator for engine generations; only a winning commit consumes one.
    next_generation: u64,
    /// Cancels project-derived work (test collection, expansion) when source
    /// moves or a commit supersedes it. Never cancels run-owned tokens.
    derived_cancel: sys_types::CancellationToken,
    /// Fences test-collection installs so two collections on one engine
    /// generation cannot complete out of order.
    collection_epoch: u64,
    registry: Option<InstalledRegistry>,
}

/// The installed test registry plus the identity it was collected under.
struct InstalledRegistry {
    generation: u64,
    collection_epoch: u64,
    /// `Some` when the project has tests; `None` when collection completed
    /// and found none (`$init_test` absent). Distinguished from "not yet
    /// collected", which is `RuntimeState::registry == None`.
    handle: Option<Handle>,
    /// Serializes expansion mutations: the registry heap object has exactly
    /// one mutation owner.
    expansion_gate: Arc<tokio::sync::Mutex<()>>,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            installed: None,
            next_generation: 1,
            derived_cancel: sys_types::CancellationToken::new(),
            collection_epoch: 0,
            registry: None,
        }
    }

    /// Cancel derived work and hand out a fresh token.
    fn supersede_derived(&mut self) {
        self.derived_cancel.cancel();
        self.derived_cancel = sys_types::CancellationToken::new();
    }
}

/// How many `(generation, function)` overlay graphs one runtime retains.
///
/// Graphs are built lazily for the function a run actually executes instead
/// of eagerly for every function on every compile — fully-inlined graphs
/// grow with call-site fan-out, so eager per-compile snapshots dominated
/// LSP memory.
const RETAINED_RUN_CFGS: usize = 64;

/// Overlay control-flow graphs pinned per engine generation, so a run's
/// span overlays stay resolvable after later recompiles retire the engine
/// the run launched on.
#[derive(Default)]
struct RunCfgCache {
    order: VecDeque<(u64, String)>,
    graphs: HashMap<(u64, String), Arc<ControlFlowGraph>>,
}

impl RunCfgCache {
    fn insert(&mut self, generation: u64, function_name: &str, graph: Arc<ControlFlowGraph>) {
        let key = (generation, function_name.to_string());
        if self.graphs.insert(key.clone(), graph).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > RETAINED_RUN_CFGS {
            if let Some(evicted) = self.order.pop_front() {
                self.graphs.remove(&evicted);
            }
        }
    }

    fn graph(&self, generation: u64, function_name: &str) -> Option<Arc<ControlFlowGraph>> {
        self.graphs
            .get(&(generation, function_name.to_string()))
            .cloned()
    }
}

/// The engine lifecycle for one workspace root.
pub struct ProjectRuntime {
    state: Mutex<RuntimeState>,
    run_cfgs: Mutex<RunCfgCache>,
}

impl Default for ProjectRuntime {
    fn default() -> Self {
        Self {
            state: Mutex::new(RuntimeState::new()),
            run_cfgs: Mutex::new(RunCfgCache::default()),
        }
    }
}

impl ProjectRuntime {
    /// Atomically install `candidate` iff its revision still matches
    /// `current_revision` — which the caller read on the owner thread in the
    /// same continuation that calls this, making commit and mutation
    /// linearized without any source gate.
    ///
    /// The retired engine comes back in the outcome for the caller to drain;
    /// the runtime's derived work (collection, expansion) is superseded
    /// atomically with the swap.
    pub fn commit_if_current(
        &self,
        candidate: EngineCandidate,
        current_revision: SourceRevision,
    ) -> CommitOutcome {
        if candidate.source_revision != current_revision {
            return CommitOutcome::Superseded { current_revision };
        }

        // The revision comparison won: activate the candidate's profiling
        // lifecycle now — before the engine becomes reachable.
        candidate.engine.activate_profiling();
        let engine = Arc::new(candidate.engine);

        let (receipt, retired) = {
            let mut state = self.state.lock();
            let generation = state.next_generation;
            state.next_generation += 1;
            let retired = state
                .installed
                .replace(InstalledEngine {
                    source_revision: candidate.source_revision,
                    generation,
                    engine,
                })
                .map(|installed| installed.engine);
            // Derived work bound to the previous engine is superseded, and
            // its registry is dropped, atomically with the engine swap.
            state.supersede_derived();
            state.collection_epoch += 1;
            state.registry = None;
            (
                CommitReceipt {
                    source_revision: candidate.source_revision,
                    generation,
                },
                retired,
            )
        };
        CommitOutcome::Committed { receipt, retired }
    }

    /// A source mutation landed: derived work bound to the previous state is
    /// stale. (The installed engine stays — runs against it are refused by
    /// [`ProjectRuntime::prepare_function_run`]'s revision check, but
    /// in-flight runs keep their pinned engine.)
    pub fn on_source_changed(&self) {
        self.state.lock().supersede_derived();
    }

    /// Coherent snapshot for launching a run. `current_revision` must be
    /// read on the owner thread in the same continuation (see
    /// [`ProjectRuntime::commit_if_current`]).
    pub fn prepare_function_run(
        &self,
        current_revision: SourceRevision,
    ) -> Result<RunSnapshot, PrepareRunError> {
        let state = self.state.lock();
        let Some(installed) = state.installed.as_ref() else {
            return Err(PrepareRunError::NeedsCurrentBuild);
        };
        if installed.source_revision != current_revision {
            return Err(PrepareRunError::NeedsCurrentBuild);
        }
        Ok(RunSnapshot {
            generation: installed.generation,
            engine: Arc::clone(&installed.engine),
        })
    }

    /// The installed engine, when it is exactly `generation`. Late results
    /// from runs pinned to an older generation get `None` and must not touch
    /// the current engine.
    pub fn engine_for_generation(&self, generation: u64) -> Option<Arc<BexEngine>> {
        let state = self.state.lock();
        state
            .installed
            .as_ref()
            .filter(|installed| installed.generation == generation)
            .map(|installed| Arc::clone(&installed.engine))
    }

    // ── Test registry ────────────────────────────────────────────────────

    /// Start one test-collection attempt against the installed engine.
    ///
    /// `None` means there is nothing to collect against: no engine yet, or an
    /// engine that predates `current_revision` (collecting for a dead
    /// revision is work whose result could never be installed). The ticket
    /// captures engine, generation, cancel token, and collection epoch in one
    /// transaction; installation and every emission are fenced by that
    /// identity, so a superseded collection emits nothing.
    pub fn begin_test_collection(
        &self,
        current_revision: SourceRevision,
    ) -> Option<CollectionTicket> {
        let mut state = self.state.lock();
        let installed = state.installed.clone()?;
        if installed.source_revision != current_revision {
            return None;
        }
        state.supersede_derived();
        state.collection_epoch += 1;
        Some(CollectionTicket {
            generation: installed.generation,
            collection_epoch: state.collection_epoch,
            engine: installed.engine,
            cancel: state.derived_cancel.clone(),
        })
    }

    /// Install a collected registry iff `ticket` still matches the installed
    /// generation and collection epoch. `false` (emit nothing) is the ABA
    /// fence for a stale result.
    pub fn install_collected_registry(
        &self,
        ticket: &CollectionTicket,
        handle: Option<Handle>,
    ) -> bool {
        let mut state = self.state.lock();
        if !Self::ticket_is_current(&state, ticket) {
            return false;
        }
        state.registry = Some(InstalledRegistry {
            generation: ticket.generation,
            collection_epoch: ticket.collection_epoch,
            handle,
            expansion_gate: Arc::new(tokio::sync::Mutex::new(())),
        });
        true
    }

    /// Emission fence for collection results: `true` while the ticket's
    /// engine generation and collection epoch are still installed.
    pub fn collection_ticket_is_current(&self, ticket: &CollectionTicket) -> bool {
        Self::ticket_is_current(&self.state.lock(), ticket)
    }

    fn ticket_is_current(state: &RuntimeState, ticket: &CollectionTicket) -> bool {
        state
            .installed
            .as_ref()
            .is_some_and(|installed| installed.generation == ticket.generation)
            && state.collection_epoch == ticket.collection_epoch
    }

    /// Coherent lease for registry work: validates `generation` against the
    /// installed engine, requires that engine to match `current_revision`,
    /// and captures the registry handle plus its expansion gate in one
    /// transaction.
    pub fn lease_registry(
        &self,
        generation: u64,
        current_revision: SourceRevision,
    ) -> Result<RegistryLease, RegistryLeaseError> {
        let state = self.state.lock();
        let Some(installed) = state.installed.as_ref() else {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        };
        if installed.generation != generation || installed.source_revision != current_revision {
            return Err(RegistryLeaseError::NeedsCurrentBuild);
        }
        let Some(registry) = state.registry.as_ref() else {
            return Err(RegistryLeaseError::NoRegistry);
        };
        debug_assert_eq!(
            registry.generation, generation,
            "the registry is cleared with every engine swap, so an installed one always \
             belongs to the installed generation"
        );
        let Some(handle) = registry.handle.clone() else {
            return Err(RegistryLeaseError::NoTests);
        };
        Ok(RegistryLease {
            generation,
            collection_epoch: registry.collection_epoch,
            engine: Arc::clone(&installed.engine),
            handle,
            cancel: state.derived_cancel.clone(),
            expansion_gate: Arc::clone(&registry.expansion_gate),
        })
    }

    /// Emission fence for expansion results: `true` while the lease's engine
    /// generation is still installed *and* the leased registry object is
    /// still the installed one (a same-generation re-collection replaces the
    /// registry, making results computed against the old object stale).
    pub fn registry_lease_is_current(&self, lease: &RegistryLease) -> bool {
        let state = self.state.lock();
        state
            .installed
            .as_ref()
            .is_some_and(|installed| installed.generation == lease.generation)
            && state.registry.as_ref().is_some_and(|registry| {
                registry.generation == lease.generation
                    && registry.collection_epoch == lease.collection_epoch
            })
    }

    /// Pin `graph` as the overlay graph for `function_name` at `generation`.
    /// Called at run launch, so the run's later span overlays resolve against
    /// exactly the code it executed even after a recompile retires the engine.
    pub fn pin_overlay_graph(
        &self,
        generation: u64,
        function_name: &str,
        graph: Arc<ControlFlowGraph>,
    ) {
        self.run_cfgs
            .lock()
            .insert(generation, function_name, graph);
    }

    /// The overlay graph pinned for `(generation, function_name)`, if it is
    /// still retained. `None` means the run outlived the cache — overlays
    /// degrade to unavailable rather than resolving against newer code.
    pub fn overlay_graph(
        &self,
        generation: u64,
        function_name: &str,
    ) -> Option<Arc<ControlFlowGraph>> {
        self.run_cfgs.lock().graph(generation, function_name)
    }

    /// The installed engine's identity, for status displays.
    pub fn installed(&self) -> Option<CommitReceipt> {
        self.state
            .lock()
            .installed
            .as_ref()
            .map(|installed| CommitReceipt {
                source_revision: installed.source_revision,
                generation: installed.generation,
            })
    }
}

/// Engines per workspace root, owner-mutated (the map is a leaf lock; the
/// per-root runtimes are shared into run tasks by `Arc`).
#[derive(Default)]
pub struct RuntimeRegistry {
    runtimes: Mutex<HashMap<PathBuf, Arc<ProjectRuntime>>>,
}

impl RuntimeRegistry {
    /// The runtime for `root`, created on first use.
    pub fn runtime(&self, root: &Path) -> Arc<ProjectRuntime> {
        Arc::clone(self.runtimes.lock().entry(root.to_path_buf()).or_default())
    }

    /// The runtime for `root`, if one exists.
    pub fn existing(&self, root: &Path) -> Option<Arc<ProjectRuntime>> {
        self.runtimes.lock().get(root).cloned()
    }

    /// Apply `f` to every live runtime (source-change supersession, which
    /// is not per-root business).
    pub fn for_each(&self, f: impl Fn(&ProjectRuntime)) {
        let runtimes: Vec<Arc<ProjectRuntime>> = self.runtimes.lock().values().cloned().collect();
        for runtime in &runtimes {
            f(runtime);
        }
    }

    /// Drop the runtimes of roots that are no longer in the workspace,
    /// returning their engines for the caller to drain. In-flight runs keep
    /// their engine alive through their own `Arc`s.
    pub fn retain(&self, keep: &[PathBuf]) -> Vec<Arc<BexEngine>> {
        let mut runtimes = self.runtimes.lock();
        let removed: Vec<Arc<ProjectRuntime>> = runtimes
            .keys()
            .filter(|root| !keep.contains(root))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|root| runtimes.remove(&root))
            .collect();
        drop(runtimes);
        removed
            .into_iter()
            .filter_map(|runtime| runtime.state.lock().installed.take())
            .map(|installed| installed.engine)
            .collect()
    }
}

/// Construct an engine candidate from a compiled program. Runs `$init`
/// synchronously and candidate-locally (call from a blocking-capable
/// context); the profiling lifecycle stays inactive until the candidate wins
/// a commit. The construction incantation mirrors `bex_project::new`.
pub fn construct_engine_candidate(
    program: bex_vm_types::Program,
    sys_ops: Arc<sys_ops::SysOps>,
    source_revision: SourceRevision,
) -> Result<EngineCandidate, RuntimeError> {
    let engine = BexEngine::new_with_deferred_profiling_and_runtime_compiler(
        program,
        sys_ops,
        Vec::new(),
        Some(bex_project::runtime_compiler()),
    )
    .map_err(RuntimeError::Engine)?;
    engine.set_unhandled_spawn_error_handler(Some(Arc::new(|error| {
        let cancelled = error.cancelled;
        let error = error.into_engine_error();
        if cancelled {
            tracing::warn!("cancelled spawned task failed: {error}");
        } else {
            tracing::error!("unhandled spawned task failed: {error}");
        }
    })));
    Ok(EngineCandidate {
        source_revision,
        engine,
    })
}

/// Shut a retired engine down without blocking the caller. Call from a tokio
/// context: outside one the engine is dropped instead, which skips the
/// graceful drain (and logs a warning from the engine) though it leaks
/// nothing.
pub fn spawn_engine_shutdown(engine: Arc<BexEngine>) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            engine.shutdown().await;
        });
    } else {
        drop(engine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real engine from an empty workspace: cheap to compile, `$init` is
    /// trivial, and the state machine under test is identical to a full
    /// project's.
    fn candidate_at(revision: SourceRevision) -> EngineCandidate {
        use sys_native::SysOpsExt as _;

        let mut db = baml_db::ProjectDatabase::new();
        db.ensure_stdlib_sources();
        db.add_source_root(baml_db::SourceRootSpec {
            path: PathBuf::from("/pg-test"),
            package: baml_db::Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: baml_db::SourceRootKind::Workspace,
        })
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
        let program = db
            .get_bytecode_unchecked()
            .unwrap_or_else(|e| unreachable!("empty workspace emits: {e}"));
        construct_engine_candidate(program, Arc::new(sys_native::SysOps::native()), revision)
            .unwrap_or_else(|e| unreachable!("empty program constructs an engine: {e}"))
    }

    #[test]
    fn superseded_candidate_changes_nothing() {
        let runtime = ProjectRuntime::default();
        let outcome = runtime.commit_if_current(candidate_at(SourceRevision(1)), SourceRevision(2));
        assert!(matches!(
            outcome,
            CommitOutcome::Superseded {
                current_revision: SourceRevision(2)
            }
        ));
        assert!(runtime.installed().is_none());
        assert!(matches!(
            runtime.prepare_function_run(SourceRevision(2)),
            Err(PrepareRunError::NeedsCurrentBuild)
        ));
    }

    #[test]
    fn winning_commits_allocate_generations_and_prepare_validates_revision() {
        let runtime = ProjectRuntime::default();

        let CommitOutcome::Committed {
            receipt: first,
            retired,
        } = runtime.commit_if_current(candidate_at(SourceRevision(1)), SourceRevision(1))
        else {
            panic!("matching revisions must commit");
        };
        assert_eq!(first.generation, 1);
        assert!(retired.is_none(), "the first commit retires nothing");

        // Current source: a run snapshot pins the installed generation.
        let snapshot = runtime
            .prepare_function_run(SourceRevision(1))
            .expect("current engine serves runs");
        assert_eq!(snapshot.generation, 1);

        // Source moved on: the installed engine is refused, never silently
        // served as last-known-good.
        assert!(matches!(
            runtime.prepare_function_run(SourceRevision(2)),
            Err(PrepareRunError::NeedsCurrentBuild)
        ));

        // A rebuild at the new revision retires the old engine and consumes
        // the next generation.
        let CommitOutcome::Committed {
            receipt: second,
            retired,
        } = runtime.commit_if_current(candidate_at(SourceRevision(2)), SourceRevision(2))
        else {
            panic!("matching revisions must commit");
        };
        assert_eq!(second.generation, 2);
        assert!(
            retired.is_some(),
            "the replaced engine comes back for the caller to drain"
        );
        assert!(runtime.engine_for_generation(1).is_none(), "gen 1 retired");
        assert!(runtime.engine_for_generation(2).is_some());
    }

    #[test]
    fn registry_creates_per_root_and_remove_drops() {
        let registry = RuntimeRegistry::default();
        let a = PathBuf::from("/ws-a");
        let b = PathBuf::from("/ws-b");
        let runtime_a = registry.runtime(&a);
        assert!(
            Arc::ptr_eq(&runtime_a, &registry.runtime(&a)),
            "one runtime per root"
        );
        assert!(!Arc::ptr_eq(&runtime_a, &registry.runtime(&b)));

        assert!(
            registry.retain(std::slice::from_ref(&b)).is_empty(),
            "no engine to drain"
        );
        assert!(registry.existing(&a).is_none());
        assert!(registry.existing(&b).is_some(), "kept roots stay");
        // The held Arc keeps working for in-flight consumers.
        assert!(runtime_a.installed().is_none());
    }
}
