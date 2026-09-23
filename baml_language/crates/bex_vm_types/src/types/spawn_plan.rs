//! BEP-040 spawn plans: the sealed recipe behind a `baml.spawn.Plan<T, E>`
//! value, and the state of the single-use `baml.spawn.Execution<T, E>`
//! capability one of its wrappers receives once it is launched.
//!
//! The recipe is a traced heap object of its own
//! ([`Object::SpawnPlan`](crate::Object::SpawnPlan)) because it owns live
//! closure pointers that must stay sealed at runtime: an `Object::RustData`
//! leaf cannot keep them alive, and a closure reachable through an ordinary
//! field is callable through reflection. The capability needs no object kind:
//! a `baml.spawn.Execution` instance holds the launched plan's handle and an
//! [`ExecutionState`] behind `$rust_type` fields — user code cannot mint the
//! state, a deep copy shares it, and it pins the plan it was minted for.

use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

use tokio_util::sync::CancellationToken;

use crate::{
    HeapPtr, RealizedTy,
    limit::{LimitInner, LimitSet},
};

/// Plan ids are process-global: every plan value, transformed or fresh, has
/// its own, and an execution runs only the plan it was minted for.
static NEXT_SPAWN_PLAN_ID: AtomicU64 = AtomicU64::new(0);

fn mint_plan_id() -> u64 {
    NEXT_SPAWN_PLAN_ID.fetch_add(1, Ordering::Relaxed)
}

/// The recipe behind a `baml.spawn.Plan<T, E>` value: what a launch runs and
/// how it is admitted, parented, and cancelled. Immutable — every
/// transformation yields a new plan with its own id — so launching one twice,
/// or from two tasks, shares its modifier values (limits, tokens) and nothing
/// else.
#[derive(Clone, Debug)]
pub struct SpawnPlanData {
    /// This plan value's identity; a heap copy of the handle keeps it.
    pub id: u64,
    /// The written name, if any.
    pub name: Option<bex_str::BexStr>,
    /// The sealed body: a callable `() -> T throws E`.
    pub body: HeapPtr,
    /// The body's `T`.
    pub body_returns: RealizedTy,
    /// The body's `E`.
    pub body_throws: RealizedTy,
    /// Execution wrappers in application order: the first wraps the body, the
    /// last is outermost and is what a launch invokes.
    pub layers: Box<[SpawnPlanLayer]>,
    /// Admission: every limit is taken together before the first attempt.
    pub limits: LimitSet,
    /// `Root` applied: the task's cancellation parent is the runtime root, not
    /// the spawning task.
    pub root: bool,
    /// Linked tokens: each fires the task's own.
    pub cancel: Box<[CancellationToken]>,
}

/// One execution wrapper of a plan: a callable
/// `(Execution<T, E>) -> Output throws Error`, where `T`/`E` are the types of
/// the plan beneath it.
#[derive(Clone, Debug)]
pub struct SpawnPlanLayer {
    pub wrapper: HeapPtr,
    /// The wrapper's `Output`.
    pub returns: RealizedTy,
    /// The wrapper's `Error`.
    pub throws: RealizedTy,
}

impl SpawnPlanData {
    /// The plan `Plan.new(name, body)` builds: the body alone.
    pub fn new(
        name: Option<bex_str::BexStr>,
        body: HeapPtr,
        returns: RealizedTy,
        throws: RealizedTy,
    ) -> Self {
        Self {
            id: mint_plan_id(),
            name,
            body,
            body_returns: returns,
            body_throws: throws,
            layers: Box::new([]),
            limits: LimitSet::new(),
            root: false,
            cancel: Box::new([]),
        }
    }

    /// The `T` and `E` of the plan beneath `layer` — what the `Execution`
    /// handed to that layer runs: the body's for the innermost layer, else the
    /// `Output`/`Error` of the layer below.
    pub fn types_beneath(&self, layer: usize) -> (&RealizedTy, &RealizedTy) {
        match layer.checked_sub(1) {
            None => (&self.body_returns, &self.body_throws),
            Some(below) => (&self.layers[below].returns, &self.layers[below].throws),
        }
    }

    /// The `T` and `E` of the `Future<T, E>` a launch yields: the outermost
    /// layer's `Output`/`Error`, or the body's.
    pub fn future_types(&self) -> (&RealizedTy, &RealizedTy) {
        self.types_beneath(self.layers.len())
    }

    /// This plan wrapped by `layer`, which becomes outermost and decides the
    /// launch's `Output` and `Error`.
    #[must_use]
    pub fn wrapped(&self, wrapper: HeapPtr, returns: RealizedTy, throws: RealizedTy) -> Self {
        let mut layers = self.layers.to_vec();
        layers.push(SpawnPlanLayer {
            wrapper,
            returns,
            throws,
        });
        Self {
            id: mint_plan_id(),
            layers: layers.into_boxed_slice(),
            ..self.clone()
        }
    }

    /// This plan admitted through `limit` as well; the same limit again is
    /// the same admission set.
    #[must_use]
    pub fn with_limit(&self, limit: &Arc<LimitInner>) -> Self {
        Self {
            id: mint_plan_id(),
            limits: self.limits.with(limit),
            ..self.clone()
        }
    }

    /// This plan with the runtime root as its cancellation parent.
    #[must_use]
    pub fn rooted(&self) -> Self {
        Self {
            id: mint_plan_id(),
            root: true,
            ..self.clone()
        }
    }

    /// This plan with `tokens` linked into the launch's cancellation: the
    /// launch is cancelled when any of them fires.
    #[must_use]
    pub fn with_cancel(&self, tokens: impl IntoIterator<Item = CancellationToken>) -> Self {
        let mut cancel = self.cancel.to_vec();
        cancel.extend(tokens);
        Self {
            id: mint_plan_id(),
            cancel: cancel.into_boxed_slice(),
            ..self.clone()
        }
    }

    /// Every heap object this plan keeps alive: its body and its wrappers.
    pub fn heap_refs(&self) -> impl Iterator<Item = HeapPtr> + '_ {
        std::iter::once(self.body).chain(self.layers.iter().map(|layer| layer.wrapper))
    }

    /// Rewrite every heap reference in place (GC forwarding).
    pub fn forward_heap_refs(&mut self, mut forward: impl FnMut(&mut HeapPtr)) {
        forward(&mut self.body);
        for layer in &mut self.layers {
            forward(&mut layer.wrapper);
        }
    }
}

/// Where a capability is in its life: `run` moves it from `Unused` to
/// `Running` to `Finished`; the wrapper returning moves an `Unused` one to
/// `Expired`. A run in flight when its wrapper returns — only possible by
/// handing the capability to another task — is not stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionPhase {
    Unused = 0,
    Running = 1,
    Finished = 2,
    Expired = 3,
}

impl ExecutionPhase {
    fn from_u8(phase: u8) -> Self {
        match phase {
            0 => Self::Unused,
            1 => Self::Running,
            2 => Self::Finished,
            3 => Self::Expired,
            other => unreachable!("execution phase {other} is never stored"),
        }
    }
}

/// Why `run` was refused: the BEP's runtime invariant violations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMisuse {
    /// The capability was paired with a plan it was not minted for.
    WrongPlan,
    /// `run` was already called on this capability (or a copy of it).
    RanTwice,
    /// The wrapper that received this capability has returned.
    OutsideExtent,
}

/// The state behind a `baml.spawn.Execution<T, E>` value: which layer of
/// which plan it continues, and its phase. Shared by every copy of the value,
/// so a deep copy is the same capability, not a second one.
#[derive(Debug)]
pub struct ExecutionState {
    /// The [`SpawnPlanData::id`] of the launched plan.
    plan: u64,
    /// The layer this capability was handed to; `run` continues with the
    /// layer beneath it, or the body when this is the innermost.
    layer: usize,
    phase: AtomicU8,
}

impl ExecutionState {
    /// The capability for `layer` of the plan with id `plan`, unused and
    /// inside its extent.
    pub fn new(plan: u64, layer: usize) -> Self {
        Self {
            plan,
            layer,
            phase: AtomicU8::new(ExecutionPhase::Unused as u8),
        }
    }

    pub fn layer(&self) -> usize {
        self.layer
    }

    pub fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::from_u8(self.phase.load(Ordering::Acquire))
    }

    /// `run` begins on `plan`: only the plan this state was minted for, only
    /// once, and only inside the extent.
    pub fn begin(&self, plan: &SpawnPlanData) -> Result<(), ExecutionMisuse> {
        if plan.id != self.plan {
            return Err(ExecutionMisuse::WrongPlan);
        }
        match self.phase.compare_exchange(
            ExecutionPhase::Unused as u8,
            ExecutionPhase::Running as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Ok(()),
            Err(phase) => Err(match ExecutionPhase::from_u8(phase) {
                ExecutionPhase::Running | ExecutionPhase::Finished => ExecutionMisuse::RanTwice,
                ExecutionPhase::Expired => ExecutionMisuse::OutsideExtent,
                ExecutionPhase::Unused => unreachable!("the exchange failed on the expected phase"),
            }),
        }
    }

    /// The run that `begin` started has returned or thrown.
    pub fn finish(&self) {
        let previous = self
            .phase
            .swap(ExecutionPhase::Finished as u8, Ordering::AcqRel);
        debug_assert_eq!(
            ExecutionPhase::from_u8(previous),
            ExecutionPhase::Running,
            "finish follows begin"
        );
    }

    /// The wrapper that received the capability has returned: a run that
    /// never started now never can.
    pub fn expire(&self) {
        // Only `Unused` expires; `Running`/`Finished` keep their phase so a
        // later `run` still reports "twice", not "outside".
        let _ = self.phase.compare_exchange(
            ExecutionPhase::Unused as u8,
            ExecutionPhase::Expired as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

/// The dynamic extent of one `Execution`: held while the wrapper it was
/// handed to runs, and dropped when that wrapper returns, throws, or is torn
/// down with its task — at which point an unused execution expires.
#[derive(Debug)]
pub struct ExecutionExtent(Arc<ExecutionState>);

impl ExecutionExtent {
    pub fn new(state: Arc<ExecutionState>) -> Self {
        Self(state)
    }
}

impl Drop for ExecutionExtent {
    fn drop(&mut self) {
        self.0.expire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> SpawnPlanData {
        SpawnPlanData::new(
            None,
            HeapPtr::null(),
            RealizedTy::int(),
            RealizedTy::never(),
        )
    }

    #[test]
    fn each_layer_runs_the_types_beneath_it() {
        let plan = plan().wrapped(HeapPtr::null(), RealizedTy::string(), RealizedTy::int());
        assert_eq!(
            plan.types_beneath(0),
            (&RealizedTy::int(), &RealizedTy::never()),
            "the innermost layer runs the body"
        );
        assert_eq!(
            plan.future_types(),
            (&RealizedTy::string(), &RealizedTy::int()),
            "a launch yields the outermost layer's types"
        );
    }

    #[test]
    fn a_capability_runs_once() {
        let plan = plan();
        let state = ExecutionState::new(plan.id, 0);
        assert_eq!(state.phase(), ExecutionPhase::Unused);
        assert_eq!(state.begin(&plan), Ok(()));
        assert_eq!(state.begin(&plan), Err(ExecutionMisuse::RanTwice));
        state.finish();
        assert_eq!(state.begin(&plan), Err(ExecutionMisuse::RanTwice));
        state.expire();
        assert_eq!(
            state.phase(),
            ExecutionPhase::Finished,
            "a finished run stays finished"
        );
    }

    #[test]
    fn a_capability_cannot_run_after_its_wrapper_returned() {
        let plan = plan();
        let state = ExecutionState::new(plan.id, 0);
        state.expire();
        assert_eq!(state.begin(&plan), Err(ExecutionMisuse::OutsideExtent));
    }

    #[test]
    fn a_capability_runs_only_the_plan_it_was_minted_for() {
        let launched = plan();
        let state = ExecutionState::new(launched.id, 0);
        assert_eq!(state.begin(&plan()), Err(ExecutionMisuse::WrongPlan));
        assert_eq!(
            state.begin(&launched.rooted()),
            Err(ExecutionMisuse::WrongPlan),
            "a transformed plan is a different plan"
        );
        let copy = launched.clone();
        assert_eq!(copy.id, launched.id);
        assert_eq!(
            state.begin(&copy),
            Ok(()),
            "a copied handle is the same plan"
        );
    }

    #[test]
    fn an_extent_expires_its_execution_when_dropped() {
        let plan = plan();
        let state = Arc::new(ExecutionState::new(plan.id, 0));
        drop(ExecutionExtent::new(Arc::clone(&state)));
        assert_eq!(state.begin(&plan), Err(ExecutionMisuse::OutsideExtent));
    }

    #[test]
    fn copies_share_one_capability() {
        let plan = plan();
        let state = Arc::new(ExecutionState::new(plan.id, 0));
        let copy = Arc::clone(&state);
        assert_eq!(copy.begin(&plan), Ok(()));
        assert_eq!(state.begin(&plan), Err(ExecutionMisuse::RanTwice));
    }
}
