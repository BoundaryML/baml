//! Exception-path evidence kept by one VM thread. Nothing here runs on a
//! successful call, return or await.
//!
//! Landing notes remember which raise put an error into which handler, by the
//! stack slot that holds the error's `baml.errors.Context`. They hold stack
//! indexes and IDs, never heap values, so GC ignores them. A rethrow is linked
//! to an earlier raise only through these notes, by the context object it
//! carries: equal values alone never link two raises.
//!
//! A context object is created once per error occurrence and travels with the
//! error when it is rethrown, so every live note whose slot holds a rethrow's
//! context belongs to that occurrence. A note stops counting when its frame
//! ends, and never counts if an instruction of its function can overwrite its
//! context slot: the slot might then hold a context that did not land there.
use btel_records::{FutureErrorLink, OriginEvidence, UnresolvedOrigin};
use btel_types::{FunctionId, TelemetryId};

/// Upper bound on live notes. Once exceeded, lookups stay unresolved until
/// the next top-level run clears the notes: a dropped note could otherwise
/// leave a stale one as the only match.
const MAX_LANDINGS: usize = 1024;

#[derive(Clone, Copy, Debug)]
struct Landing {
    depth: usize,
    function: Option<FunctionId>,
    handler_pc: usize,
    /// Absolute stack index of the handler's context slot.
    context_slot: usize,
    /// No instruction of the function stores into the context slot, so while
    /// the frame lives it holds what the landing stored.
    trusted: bool,
    raise: TelemetryId,
    origin: OriginEvidence,
}

/// Allocated on this thread's first raise with evidence enabled.
#[derive(Debug, Default)]
pub(crate) struct ErrorBook {
    landings: Vec<Landing>,
    evicted: bool,
    /// The raise that most recently escaped this thread; cleared by the next.
    escaped: Option<FutureErrorLink>,
}

/// A lookup's result before it becomes a raise origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LandingMatch {
    /// One origin; `previous` when exactly one landing matched.
    Origin(OriginEvidence, Option<TelemetryId>),
    Ambiguous(u32),
    Unresolved(UnresolvedOrigin),
}

impl ErrorBook {
    /// Forget everything: a fresh top-level run starts with no frames.
    pub(crate) fn clear(&mut self) {
        self.landings.clear();
        self.evicted = false;
        self.escaped = None;
    }

    /// Drop notes of frames that no longer exist.
    pub(crate) fn prune(&mut self, live_frames: usize) {
        self.landings.retain(|landing| landing.depth < live_frames);
    }

    pub(crate) fn begin_raise(&mut self, live_frames: usize) {
        self.escaped = None;
        self.prune(live_frames);
    }

    /// Unwinding stored a raise's error in a handler of frame `depth`, and its
    /// context in `context_slot`. A newer landing replaces an older note for
    /// the same handler in the same function and frame depth, and drops the
    /// notes of deeper frames.
    #[allow(clippy::too_many_arguments, reason = "one flat note")]
    pub(crate) fn land(
        &mut self,
        depth: usize,
        function: Option<FunctionId>,
        handler_pc: usize,
        context_slot: usize,
        trusted: bool,
        raise: TelemetryId,
        origin: OriginEvidence,
    ) {
        self.landings.retain(|landing| {
            landing.depth < depth
                || (landing.depth == depth
                    && landing.function == function
                    && landing.handler_pc != handler_pc)
        });
        if self.landings.len() >= MAX_LANDINGS {
            self.landings.remove(0);
            self.evicted = true;
        }
        self.landings.push(Landing {
            depth,
            function,
            handler_pc,
            context_slot,
            trusted,
            raise,
            origin,
        });
    }

    pub(crate) fn set_escaped(&mut self, link: FutureErrorLink) {
        self.escaped = Some(link);
    }

    pub(crate) fn take_escaped(&mut self) -> Option<FutureErrorLink> {
        self.escaped.take()
    }

    /// The origin of the error whose context a rethrow carries. `frames` are
    /// the live bytecode frames, as `(depth, function)`; `holds` tells whether
    /// a live stack slot holds that context object.
    pub(crate) fn lookup(
        &self,
        frames: &[(usize, Option<FunctionId>)],
        holds: impl Fn(usize) -> bool,
    ) -> LandingMatch {
        if self.evicted {
            return LandingMatch::Unresolved(UnresolvedOrigin::LandingsEvicted);
        }
        let matched: Vec<&Landing> = self
            .landings
            .iter()
            .filter(|note| frames.contains(&(note.depth, note.function)))
            .filter(|note| holds(note.context_slot))
            .collect();
        // A slot an instruction can write may hold a context that never
        // landed there, so its note cannot vouch for the match.
        if matched.iter().any(|note| !note.trusted) {
            return LandingMatch::Unresolved(UnresolvedOrigin::NoLanding);
        }
        let mut origins: Vec<OriginEvidence> = Vec::new();
        for note in &matched {
            if !origins.contains(&note.origin) {
                origins.push(note.origin);
            }
        }
        match (origins.as_slice(), matched.as_slice()) {
            ([], _) => LandingMatch::Unresolved(UnresolvedOrigin::NoLanding),
            ([origin], [only]) => LandingMatch::Origin(*origin, Some(only.raise)),
            ([origin], _) => LandingMatch::Origin(*origin, None),
            (many, _) => LandingMatch::Ambiguous(u32::try_from(many.len()).unwrap_or(u32::MAX)),
        }
    }
}

#[cfg(test)]
mod tests {
    use btel_types::allocate_telemetry_id;

    use super::*;

    fn function() -> FunctionId {
        btel_types::FunctionIdAllocator::default()
            .allocate()
            .unwrap()
    }

    // Handlers are named by their entry PC; context slots are 5, 6 and 7.
    const A: usize = 100;
    const B: usize = 150;
    const C: usize = 300;

    #[test]
    fn a_rethrow_is_linked_by_the_context_it_carries() {
        let f = Some(function());
        let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, true, a, OriginEvidence::Known(a));
        // Thrown from inside A's body, caught by its nested handler B: both
        // hold an equal error, but each holds its own context.
        book.land(1, f, B, 6, true, b, OriginEvidence::Known(b));
        let live = [(1, f)];
        assert_eq!(
            book.lookup(&live, |slot| slot == 5),
            LandingMatch::Origin(OriginEvidence::Known(a), Some(a))
        );
        assert_eq!(
            book.lookup(&live, |slot| slot == 6),
            LandingMatch::Origin(OriginEvidence::Known(b), Some(b))
        );
        // A context no live slot holds was never landed here.
        assert_eq!(
            book.lookup(&live, |_| false),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
    }

    #[test]
    fn a_rethrow_caught_again_keeps_its_origin() {
        let f = Some(function());
        let origin = allocate_telemetry_id();
        let again = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        book.land(0, f, A, 5, true, origin, OriginEvidence::Known(origin));
        // Rethrown and caught in the caller: its context moved with it.
        book.land(1, f, B, 6 + 64, true, again, OriginEvidence::Known(origin));
        let live = [(0, f), (1, f)];
        assert_eq!(
            book.lookup(&live, |slot| slot == 5 || slot == 6 + 64),
            LandingMatch::Origin(OriginEvidence::Known(origin), None)
        );
    }

    #[test]
    fn a_note_counts_only_while_its_frame_lives() {
        let ids = btel_types::FunctionIdAllocator::default();
        let (f, g) = (ids.allocate().ok(), ids.allocate().ok());
        assert_ne!(f, g);
        let a = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, true, a, OriginEvidence::Known(a));
        // Another function now runs at that depth.
        assert_eq!(
            book.lookup(&[(1, g)], |slot| slot == 5),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
        // A shallower landing drops deeper notes; pruning drops popped frames.
        book.land(0, f, C, 7, true, a, OriginEvidence::Known(a));
        assert_eq!(book.landings.len(), 1);
        book.prune(0);
        assert!(book.landings.is_empty());
    }

    #[test]
    fn a_context_slot_an_instruction_writes_proves_nothing() {
        let f = Some(function());
        let a = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, false, a, OriginEvidence::Known(a));
        assert_eq!(
            book.lookup(&[(1, f)], |slot| slot == 5),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
    }

    #[test]
    fn eviction_makes_lookups_unresolved_until_cleared() {
        let f = Some(function());
        let raise = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        for depth in 0..=MAX_LANDINGS {
            book.land(depth, f, A, 5, true, raise, OriginEvidence::Known(raise));
        }
        assert_eq!(
            book.lookup(&[(0, f)], |_| true),
            LandingMatch::Unresolved(UnresolvedOrigin::LandingsEvicted)
        );
        book.clear();
        book.land(0, f, A, 5, true, raise, OriginEvidence::Known(raise));
        assert_eq!(
            book.lookup(&[(0, f)], |slot| slot == 5),
            LandingMatch::Origin(OriginEvidence::Known(raise), Some(raise))
        );
    }
}
