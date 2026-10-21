//! Dense active-epoch calling-context aggregation.
//!
//! The counter and reference types are shared with evidence code on every
//! target; the active-epoch engine is consumer-side and wasm32 has no consumer.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use rustc_hash::FxHashMap as HashMap;

use super::{
    ContextKey, ContextTuple, EdgeKind, MemoryDenied, Owner, ProfilerMemoryGovernor, Reservation,
    ReservationClass,
};
use crate::{
    ids::{FunctionId, ProgramId},
    prof::record::{CallSiteSourceSpan, FunctionEndStatus},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParentContextRef {
    Root,
    Local(u32),
    External(ContextKey),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CctCounters {
    pub invocations_started: u64,
    pub spans_selected: u64,
    pub completed_ok: u64,
    pub completed_error: u64,
    pub completed_cancelled: u64,
    pub completed_exit: u64,
    pub inclusive_ns: u128,
    pub direct_call_child_inclusive_ns: u128,
    pub await_ns: u128,
    pub await_count: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CounterHealth {
    pub counter_saturated: bool,
    pub await_counter_saturated: bool,
    pub self_time_underflow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedTiming {
    pub self_ns: u128,
    pub complete: bool,
}

impl CctCounters {
    #[must_use]
    pub fn derived_timing(self, timing_complete: bool) -> DerivedTiming {
        let deducted = self
            .direct_call_child_inclusive_ns
            .saturating_add(self.await_ns);
        DerivedTiming {
            self_ns: self.inclusive_ns.saturating_sub(deducted),
            complete: timing_complete && deducted <= self.inclusive_ns,
        }
    }

    fn start(&mut self, selected: bool, health: &mut CounterHealth) {
        saturating_add_u64(
            &mut self.invocations_started,
            1,
            &mut health.counter_saturated,
        );
        if selected {
            saturating_add_u64(&mut self.spans_selected, 1, &mut health.counter_saturated);
        }
    }

    fn end(
        &mut self,
        status: FunctionEndStatus,
        inclusive_ns: u64,
        await_ns: u64,
        await_count: u32,
        health: &mut CounterHealth,
    ) {
        let completed = match status {
            FunctionEndStatus::Ok => &mut self.completed_ok,
            FunctionEndStatus::Errored => &mut self.completed_error,
            FunctionEndStatus::Cancelled => &mut self.completed_cancelled,
            FunctionEndStatus::Exited => &mut self.completed_exit,
        };
        saturating_add_u64(completed, 1, &mut health.counter_saturated);
        saturating_add_u128(
            &mut self.inclusive_ns,
            u128::from(inclusive_ns),
            &mut health.counter_saturated,
        );
        saturating_add_u128(
            &mut self.await_ns,
            u128::from(await_ns),
            &mut health.await_counter_saturated,
        );
        saturating_add_u64(
            &mut self.await_count,
            u64::from(await_count),
            &mut health.await_counter_saturated,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverflowReason {
    ContextMemoryUnavailableAfterDrain,
    InvalidParentContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextRef {
    Normal(ContextKey),
    Overflow {
        reason: OverflowReason,
        edge_kind: EdgeKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContextAdmission {
    Normal {
        local_id: u32,
        context_ref: ContextRef,
    },
    Overflow {
        context_ref: ContextRef,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextDelta {
    pub key: ContextKey,
    pub tuple: Option<ContextTuple>,
    pub counters: CctCounters,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverflowDelta {
    pub reason: OverflowReason,
    pub edge_kind: EdgeKind,
    pub counters: CctCounters,
}

#[derive(Debug)]
pub struct SealedCctEpoch {
    pub contexts: Vec<ContextDelta>,
    pub overflow: Vec<OverflowDelta>,
    pub health: CounterHealth,
    reservations: Vec<Reservation>,
}

impl SealedCctEpoch {
    /// Governed bytes carried by this sealed epoch (transferred to the stream
    /// writer on hand-off).
    #[must_use]
    pub fn accounted_bytes(&self) -> u64 {
        self.reservations
            .iter()
            .map(Reservation::accounted_bytes)
            .fold(0u64, u64::saturating_add)
    }

    /// Merges another sealed epoch of the same execution into this one:
    /// contexts merged by `ContextKey` (tuples must be identical within one
    /// execution — mismatch keeps the first), counters added saturating,
    /// `CounterHealth` OR-ed, overflow added by `(reason, edge)`. Both inputs
    /// are key-sorted (`seal` sorts) and the result stays sorted.
    pub(crate) fn merge(&mut self, other: SealedCctEpoch) {
        let mut merged = Vec::with_capacity(self.contexts.len() + other.contexts.len());
        let mut left = std::mem::take(&mut self.contexts).into_iter().peekable();
        let mut right = other.contexts.into_iter().peekable();
        while let (Some(a), Some(b)) = (left.peek(), right.peek()) {
            match a.key.0.cmp(&b.key.0) {
                std::cmp::Ordering::Less => merged.push(left.next().expect("peeked")),
                std::cmp::Ordering::Greater => merged.push(right.next().expect("peeked")),
                std::cmp::Ordering::Equal => {
                    let mut a = left.next().expect("peeked");
                    let b = right.next().expect("peeked");
                    debug_assert!(
                        a.tuple.is_none() || b.tuple.is_none() || a.tuple == b.tuple,
                        "one execution cannot define one ContextKey two ways"
                    );
                    if a.tuple.is_none() {
                        a.tuple = b.tuple;
                    }
                    add_delta_counters(&mut a.counters, b.counters, &mut self.health);
                    merged.push(a);
                }
            }
        }
        merged.extend(left);
        merged.extend(right);
        self.contexts = merged;
        for delta in other.overflow {
            match self.overflow.iter_mut().find(|existing| {
                existing.reason == delta.reason && existing.edge_kind == delta.edge_kind
            }) {
                Some(existing) => {
                    add_delta_counters(&mut existing.counters, delta.counters, &mut self.health);
                }
                None => self.overflow.push(delta),
            }
        }
        self.health.counter_saturated |= other.health.counter_saturated;
        self.health.await_counter_saturated |= other.health.await_counter_saturated;
        self.health.self_time_underflow |= other.health.self_time_underflow;
        self.reservations.extend(other.reservations);
    }
}

fn add_delta_counters(target: &mut CctCounters, delta: CctCounters, health: &mut CounterHealth) {
    saturating_add_u64(
        &mut target.invocations_started,
        delta.invocations_started,
        &mut health.counter_saturated,
    );
    saturating_add_u64(
        &mut target.spans_selected,
        delta.spans_selected,
        &mut health.counter_saturated,
    );
    saturating_add_u64(
        &mut target.completed_ok,
        delta.completed_ok,
        &mut health.counter_saturated,
    );
    saturating_add_u64(
        &mut target.completed_error,
        delta.completed_error,
        &mut health.counter_saturated,
    );
    saturating_add_u64(
        &mut target.completed_cancelled,
        delta.completed_cancelled,
        &mut health.counter_saturated,
    );
    saturating_add_u64(
        &mut target.completed_exit,
        delta.completed_exit,
        &mut health.counter_saturated,
    );
    saturating_add_u128(
        &mut target.inclusive_ns,
        delta.inclusive_ns,
        &mut health.counter_saturated,
    );
    saturating_add_u128(
        &mut target.direct_call_child_inclusive_ns,
        delta.direct_call_child_inclusive_ns,
        &mut health.counter_saturated,
    );
    saturating_add_u128(
        &mut target.await_ns,
        delta.await_ns,
        &mut health.await_counter_saturated,
    );
    saturating_add_u64(
        &mut target.await_count,
        delta.await_count,
        &mut health.await_counter_saturated,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct LookupKey {
    parent_key: Option<ContextKey>,
    function_id: FunctionId,
    call_site: Option<CallSiteKey>,
    edge_kind: EdgeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CallSiteKey {
    file_id: u32,
    start_offset: u32,
    end_offset: u32,
    line: u32,
}

impl From<CallSiteSourceSpan> for CallSiteKey {
    fn from(value: CallSiteSourceSpan) -> Self {
        Self {
            file_id: value.file_id,
            start_offset: value.start_offset,
            end_offset: value.end_offset,
            line: value.line,
        }
    }
}

#[derive(Debug)]
struct ActiveContext {
    key: ContextKey,
    tuple: ContextTuple,
    parent: ParentContextRef,
    counters: CctCounters,
}

#[derive(Debug)]
pub(crate) struct ActiveCctEpoch {
    program_id: ProgramId,
    context_charge_bytes: u64,
    lookup: HashMap<LookupKey, u32>,
    contexts: Vec<ActiveContext>,
    external_deltas: HashMap<ContextKey, CctCounters>,
    overflow: [[CctCounters; 3]; 2],
    health: CounterHealth,
    reservation: Option<Reservation>,
}

impl ActiveCctEpoch {
    #[must_use]
    pub(crate) fn new(program_id: ProgramId, context_charge_bytes: u64) -> Self {
        Self {
            program_id,
            context_charge_bytes,
            lookup: HashMap::default(),
            contexts: Vec::new(),
            external_deltas: HashMap::default(),
            overflow: [[CctCounters::default(); 3]; 2],
            health: CounterHealth::default(),
            reservation: None,
        }
    }

    pub(crate) fn record_start(
        &mut self,
        parent: ParentContextRef,
        function_id: FunctionId,
        call_site: Option<CallSiteSourceSpan>,
        edge_kind: EdgeKind,
        selected: bool,
        memory: &ProfilerMemoryGovernor,
    ) -> ContextAdmission {
        if let ParentContextRef::External(parent_key) = parent
            && !self.external_deltas.contains_key(&parent_key)
        {
            if self.reserve_context(memory).is_err() {
                return self.record_overflow(
                    OverflowReason::ContextMemoryUnavailableAfterDrain,
                    edge_kind,
                    selected,
                );
            }
            if self.external_deltas.try_reserve(1).is_err() {
                return self.record_overflow(
                    OverflowReason::ContextMemoryUnavailableAfterDrain,
                    edge_kind,
                    selected,
                );
            }
            self.external_deltas
                .insert(parent_key, CctCounters::default());
        }
        let Some(parent_key) = self.parent_key(parent) else {
            return self.record_overflow(OverflowReason::InvalidParentContext, edge_kind, selected);
        };
        let lookup_key = LookupKey {
            parent_key,
            function_id,
            call_site: call_site.map(Into::into),
            edge_kind,
        };
        if let Some(local_id) = self.lookup.get(&lookup_key).copied() {
            self.contexts[local_id as usize]
                .counters
                .start(selected, &mut self.health);
            return ContextAdmission::Normal {
                local_id,
                context_ref: ContextRef::Normal(self.contexts[local_id as usize].key),
            };
        }

        if self.contexts.len() >= u32::MAX as usize {
            self.health.counter_saturated = true;
            return self.record_overflow(
                OverflowReason::ContextMemoryUnavailableAfterDrain,
                edge_kind,
                selected,
            );
        }
        if self.reserve_context(memory).is_err() {
            return self.record_overflow(
                OverflowReason::ContextMemoryUnavailableAfterDrain,
                edge_kind,
                selected,
            );
        }
        if self.contexts.try_reserve(1).is_err() || self.lookup.try_reserve(1).is_err() {
            return self.record_overflow(
                OverflowReason::ContextMemoryUnavailableAfterDrain,
                edge_kind,
                selected,
            );
        }

        let tuple = ContextTuple {
            program_id: self.program_id,
            parent_context_key: parent_key,
            function_id,
            call_site,
            edge_kind,
        };
        let key = ContextKey::for_tuple(&tuple);
        let local_id = u32::try_from(self.contexts.len()).expect("context count checked above");
        let mut counters = CctCounters::default();
        counters.start(selected, &mut self.health);
        self.contexts.push(ActiveContext {
            key,
            tuple,
            parent,
            counters,
        });
        self.lookup.insert(lookup_key, local_id);
        ContextAdmission::Normal {
            local_id,
            context_ref: ContextRef::Normal(key),
        }
    }

    pub(crate) fn record_end(
        &mut self,
        context: ContextAdmission,
        status: FunctionEndStatus,
        inclusive_ns: u64,
        await_ns: u64,
        await_count: u32,
    ) {
        match context {
            ContextAdmission::Normal { local_id, .. } => {
                let Some(active) = self.contexts.get_mut(local_id as usize) else {
                    self.health.counter_saturated = true;
                    return;
                };
                let edge_kind = active.tuple.edge_kind;
                let parent = active.parent;
                active.counters.end(
                    status,
                    inclusive_ns,
                    await_ns,
                    await_count,
                    &mut self.health,
                );
                if edge_kind == EdgeKind::Call {
                    self.add_direct_child(parent, inclusive_ns);
                }
            }
            ContextAdmission::Overflow { context_ref, .. } => {
                let ContextRef::Overflow {
                    reason, edge_kind, ..
                } = context_ref
                else {
                    return;
                };
                let counters = &mut self.overflow[reason_index(reason)][edge_index(edge_kind)];
                counters.end(
                    status,
                    inclusive_ns,
                    await_ns,
                    await_count,
                    &mut self.health,
                );
            }
        }
    }

    /// Records an end delta by durable key. The direct decoder uses this for
    /// every normal call so a call that crosses an epoch rollover never
    /// targets a reused dense local id. Same-epoch start/end rows merge by
    /// `ContextKey` exactly like cross-segment deltas.
    pub(crate) fn record_external_end(
        &mut self,
        key: ContextKey,
        status: FunctionEndStatus,
        inclusive_ns: u64,
        await_ns: u64,
        await_count: u32,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<(), MemoryDenied> {
        if !self.external_deltas.contains_key(&key) {
            self.reserve_context(memory)?;
            self.external_deltas.insert(key, CctCounters::default());
        }
        self.external_deltas
            .get_mut(&key)
            .expect("external delta was inserted")
            .end(
                status,
                inclusive_ns,
                await_ns,
                await_count,
                &mut self.health,
            );
        Ok(())
    }

    pub(crate) fn record_external_direct_child(
        &mut self,
        parent: ContextKey,
        inclusive_ns: u64,
        memory: &ProfilerMemoryGovernor,
    ) -> Result<(), MemoryDenied> {
        if !self.external_deltas.contains_key(&parent) {
            self.reserve_context(memory)?;
            self.external_deltas.insert(parent, CctCounters::default());
        }
        saturating_add_u128(
            &mut self
                .external_deltas
                .get_mut(&parent)
                .expect("external parent delta was inserted")
                .direct_call_child_inclusive_ns,
            u128::from(inclusive_ns),
            &mut self.health.counter_saturated,
        );
        Ok(())
    }

    #[must_use]
    pub(crate) fn context_key(&self, local_id: u32) -> Option<ContextKey> {
        self.contexts
            .get(local_id as usize)
            .map(|context| context.key)
    }

    #[must_use]
    pub(crate) fn cardinality(&self) -> usize {
        self.contexts.len()
    }

    /// Whether sealing this epoch would produce any context or overflow
    /// delta.
    #[must_use]
    pub(crate) fn has_content(&self) -> bool {
        !self.contexts.is_empty()
            || !self.external_deltas.is_empty()
            || self
                .overflow
                .iter()
                .flatten()
                .any(|counters| *counters != CctCounters::default())
    }

    #[must_use]
    pub(crate) fn seal(mut self) -> SealedCctEpoch {
        let mut contexts: Vec<_> = self
            .contexts
            .drain(..)
            .map(|context| ContextDelta {
                key: context.key,
                tuple: Some(context.tuple),
                counters: context.counters,
            })
            .chain(
                self.external_deltas
                    .drain()
                    .map(|(key, counters)| ContextDelta {
                        key,
                        tuple: None,
                        counters,
                    }),
            )
            .collect();
        contexts.sort_unstable_by_key(|context| context.key.0);
        let mut overflow = Vec::with_capacity(6);
        for reason in [
            OverflowReason::ContextMemoryUnavailableAfterDrain,
            OverflowReason::InvalidParentContext,
        ] {
            for edge_kind in [EdgeKind::Root, EdgeKind::Call, EdgeKind::Spawn] {
                let counters = self.overflow[reason_index(reason)][edge_index(edge_kind)];
                if counters != CctCounters::default() {
                    overflow.push(OverflowDelta {
                        reason,
                        edge_kind,
                        counters,
                    });
                }
            }
        }
        SealedCctEpoch {
            contexts,
            overflow,
            health: self.health,
            reservations: self.reservation.take().into_iter().collect(),
        }
    }

    fn reserve_context(&mut self, memory: &ProfilerMemoryGovernor) -> Result<(), MemoryDenied> {
        match &mut self.reservation {
            Some(reservation) => reservation.try_grow(self.context_charge_bytes),
            None => {
                self.reservation = Some(memory.try_reserve(
                    ReservationClass::General,
                    Owner::Population,
                    self.context_charge_bytes,
                )?);
                Ok(())
            }
        }
    }

    // The outer option distinguishes an invalid local reference; the inner
    // option distinguishes the root from a keyed parent.
    #[allow(clippy::option_option)]
    fn parent_key(&self, parent: ParentContextRef) -> Option<Option<ContextKey>> {
        match parent {
            ParentContextRef::Root => Some(None),
            ParentContextRef::Local(local_id) => self.context_key(local_id).map(Some),
            ParentContextRef::External(key) => Some(Some(key)),
        }
    }

    fn record_overflow(
        &mut self,
        reason: OverflowReason,
        edge_kind: EdgeKind,
        selected: bool,
    ) -> ContextAdmission {
        self.overflow[reason_index(reason)][edge_index(edge_kind)]
            .start(selected, &mut self.health);
        ContextAdmission::Overflow {
            context_ref: ContextRef::Overflow { reason, edge_kind },
        }
    }

    fn add_direct_child(&mut self, parent: ParentContextRef, inclusive_ns: u64) {
        match parent {
            ParentContextRef::Root => {}
            ParentContextRef::Local(local_id) => {
                if let Some(parent) = self.contexts.get_mut(local_id as usize) {
                    saturating_add_u128(
                        &mut parent.counters.direct_call_child_inclusive_ns,
                        u128::from(inclusive_ns),
                        &mut self.health.counter_saturated,
                    );
                }
            }
            ParentContextRef::External(key) => {
                let counters = self.external_deltas.entry(key).or_default();
                saturating_add_u128(
                    &mut counters.direct_call_child_inclusive_ns,
                    u128::from(inclusive_ns),
                    &mut self.health.counter_saturated,
                );
            }
        }
    }
}

fn reason_index(reason: OverflowReason) -> usize {
    match reason {
        OverflowReason::ContextMemoryUnavailableAfterDrain => 0,
        OverflowReason::InvalidParentContext => 1,
    }
}

fn edge_index(edge: EdgeKind) -> usize {
    edge as usize
}

fn saturating_add_u64(target: &mut u64, value: u64, saturated: &mut bool) {
    match target.checked_add(value) {
        Some(sum) => *target = sum,
        None => {
            *target = u64::MAX;
            *saturated = true;
        }
    }
}

fn saturating_add_u128(target: &mut u128, value: u128, saturated: &mut bool) {
    match target.checked_add(value) {
        Some(sum) => *target = sum,
        None => {
            *target = u128::MAX;
            *saturated = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prof::backend::{MeasuredLayouts, ProfilerSizingPolicy};

    fn harness() -> (ActiveCctEpoch, ProfilerMemoryGovernor) {
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
        let epoch = ActiveCctEpoch::new(
            ProgramId([9; 16]),
            MeasuredLayouts::V1.population_item_min_bytes,
        );
        (epoch, memory)
    }

    fn site(start_offset: u32) -> CallSiteSourceSpan {
        CallSiteSourceSpan {
            file_id: 1,
            start_offset,
            end_offset: start_offset + 1,
            line: 1,
        }
    }

    #[test]
    fn repeated_path_changes_counters_not_cardinality() {
        let (mut epoch, memory) = harness();
        for _ in 0..10_000 {
            let admission = epoch.record_start(
                ParentContextRef::Root,
                FunctionId(7),
                Some(site(10)),
                EdgeKind::Root,
                false,
                &memory,
            );
            epoch.record_end(admission, FunctionEndStatus::Ok, 100, 0, 0);
        }
        assert_eq!(epoch.cardinality(), 1);
        let sealed = epoch.seal();
        assert_eq!(sealed.contexts[0].counters.invocations_started, 10_000);
        assert_eq!(sealed.contexts[0].counters.completed_ok, 10_000);
    }

    #[test]
    fn call_sites_and_edges_are_distinct() {
        let (mut epoch, memory) = harness();
        for (offset, edge) in [
            (10, EdgeKind::Call),
            (11, EdgeKind::Call),
            (10, EdgeKind::Spawn),
        ] {
            epoch.record_start(
                ParentContextRef::Root,
                FunctionId(7),
                Some(site(offset)),
                edge,
                false,
                &memory,
            );
        }
        assert_eq!(epoch.cardinality(), 3);
    }

    #[test]
    fn synchronous_child_is_subtracted_but_spawn_is_not() {
        let (mut epoch, memory) = harness();
        let parent = epoch.record_start(
            ParentContextRef::Root,
            FunctionId(1),
            None,
            EdgeKind::Root,
            false,
            &memory,
        );
        let ContextAdmission::Normal { local_id, .. } = parent else {
            panic!("parent must be normal")
        };
        let child = epoch.record_start(
            ParentContextRef::Local(local_id),
            FunctionId(2),
            Some(site(20)),
            EdgeKind::Call,
            false,
            &memory,
        );
        epoch.record_end(child, FunctionEndStatus::Ok, 30, 5, 1);
        let spawned = epoch.record_start(
            ParentContextRef::Local(local_id),
            FunctionId(2),
            Some(site(21)),
            EdgeKind::Spawn,
            false,
            &memory,
        );
        epoch.record_end(spawned, FunctionEndStatus::Ok, 1000, 0, 0);
        epoch.record_end(parent, FunctionEndStatus::Ok, 100, 10, 2);
        let sealed = epoch.seal();
        let parent = sealed
            .contexts
            .iter()
            .find(|context| {
                context
                    .tuple
                    .is_some_and(|tuple| tuple.function_id == FunctionId(1))
            })
            .unwrap();
        assert_eq!(parent.counters.direct_call_child_inclusive_ns, 30);
        assert_eq!(parent.counters.derived_timing(true).self_ns, 60);
    }

    #[test]
    fn invalid_parent_uses_preallocated_overflow() {
        let (mut epoch, memory) = harness();
        let admission = epoch.record_start(
            ParentContextRef::Local(99),
            FunctionId(4),
            None,
            EdgeKind::Call,
            true,
            &memory,
        );
        assert!(matches!(admission, ContextAdmission::Overflow { .. }));
        epoch.record_end(admission, FunctionEndStatus::Errored, 50, 10, 1);
        let sealed = epoch.seal();
        assert!(sealed.contexts.is_empty());
        assert_eq!(sealed.overflow.len(), 1);
        assert_eq!(sealed.overflow[0].counters.spans_selected, 1);
    }

    #[test]
    fn external_parent_collects_mergeable_direct_child_delta() {
        let (mut epoch, memory) = harness();
        let parent_key = ContextKey([8; 32]);
        let child = epoch.record_start(
            ParentContextRef::External(parent_key),
            FunctionId(5),
            None,
            EdgeKind::Call,
            false,
            &memory,
        );
        epoch.record_end(child, FunctionEndStatus::Ok, 44, 0, 0);
        let sealed = epoch.seal();
        let external = sealed
            .contexts
            .iter()
            .find(|context| context.key == parent_key)
            .unwrap();
        assert!(external.tuple.is_none());
        assert_eq!(external.counters.direct_call_child_inclusive_ns, 44);
    }
}
