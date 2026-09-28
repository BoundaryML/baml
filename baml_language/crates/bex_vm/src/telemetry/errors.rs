//! Exception-path evidence kept by one VM thread. Nothing here runs on a
//! successful call, return or await.
//!
//! Landing notes remember which raise put an error into which handler slot.
//! They hold stack indexes and IDs, never heap values, so GC ignores them.
//! A rethrow is linked to an earlier raise only through these notes: equal
//! values alone never link two raises.
//!
//! A note proves nothing once its handler is finished. Lookups therefore take
//! each frame's handlers whose body covers the frame's current PC, from the
//! compiled handler-context table, and accept a note only while its handler
//! is one of them and no later landing in that frame left its body. An
//! active handler that cannot be accounted for by a valid note, but whose
//! error slot holds the value, makes the answer unresolved.
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
    /// Absolute stack index of the handler's error slot.
    slot: usize,
    raise: TelemetryId,
    origin: OriginEvidence,
    /// Landing order within this thread's run.
    seq: u64,
}

/// One live frame as an origin lookup sees it.
pub(crate) struct FrameScope<'a> {
    pub(crate) depth: usize,
    pub(crate) function: Option<FunctionId>,
    /// Handlers whose body covers the frame's current PC.
    pub(crate) active: Vec<ActiveHandler>,
    /// Whether a PC lies in a handler's body, in this frame's function:
    /// `in_body(handler_pc, pc)`.
    pub(crate) in_body: &'a dyn Fn(usize, usize) -> bool,
}

/// A handler that is running in its frame right now.
pub(crate) struct ActiveHandler {
    pub(crate) handler_pc: usize,
    /// Absolute stack slots its error is stored into on landing.
    pub(crate) error_slots: Vec<usize>,
    /// Its body may store into its error slot: a note cannot vouch for what
    /// the slot holds now.
    pub(crate) slot_written: bool,
}

/// Allocated on this thread's first raise with evidence enabled.
#[derive(Debug, Default)]
pub(crate) struct ErrorBook {
    landings: Vec<Landing>,
    next_seq: u64,
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
        self.next_seq = 0;
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

    /// Unwinding stored a raise's error in a handler's slot. A newer landing
    /// replaces an older note for the same handler in the same function and
    /// frame depth: control cannot re-enter a handler while in its body.
    #[allow(clippy::too_many_arguments, reason = "one flat note")]
    pub(crate) fn land(
        &mut self,
        depth: usize,
        function: Option<FunctionId>,
        handler_pc: usize,
        slot: usize,
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
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.landings.push(Landing {
            depth,
            function,
            handler_pc,
            slot,
            raise,
            origin,
            seq,
        });
    }

    pub(crate) fn set_escaped(&mut self, link: FutureErrorLink) {
        self.escaped = Some(link);
    }

    pub(crate) fn take_escaped(&mut self) -> Option<FutureErrorLink> {
        self.escaped.take()
    }

    /// The origin of a value held by active handlers of `frames`. `holds`
    /// compares a live stack slot with the value.
    ///
    /// A note counts only while its handler is active in its frame and no
    /// later landing in the same frame depth and function has a handler
    /// outside its body: a sibling catch, an enclosing catch, or the same
    /// frame reused by a later call. Finished catches keep their notes and
    /// slots, so neither alone is evidence.
    pub(crate) fn lookup(
        &self,
        frames: &[FrameScope<'_>],
        holds: impl Fn(usize) -> bool,
    ) -> LandingMatch {
        if self.evicted {
            return LandingMatch::Unresolved(UnresolvedOrigin::LandingsEvicted);
        }
        let mut matched: Vec<&Landing> = Vec::new();
        let mut unaccounted = false;
        for frame in frames {
            let notes: Vec<&Landing> = self
                .landings
                .iter()
                .filter(|note| note.depth == frame.depth && note.function == frame.function)
                .collect();
            for active in &frame.active {
                let valid = notes
                    .iter()
                    .filter(|note| note.handler_pc == active.handler_pc)
                    .max_by_key(|note| note.seq)
                    .filter(|note| {
                        !active.slot_written
                            && !notes.iter().any(|later| {
                                later.seq > note.seq
                                    && !(frame.in_body)(note.handler_pc, later.handler_pc)
                            })
                    });
                match valid {
                    Some(note) if holds(note.slot) => matched.push(note),
                    Some(_) => {}
                    // Running, but no note vouches for its slot: an error that
                    // reached it without a recorded landing, or a slot its body
                    // can overwrite.
                    None => unaccounted |= active.error_slots.iter().any(|slot| holds(*slot)),
                }
            }
        }
        if unaccounted {
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

    // Handlers are named by their entry PC, as compiled. A's body is
    // [100, 200); B's [150, 180) is nested in it, so B's entry lies in A's
    // body; C's [300, 400) is a sibling. Error slots are 5, 6, 7.
    const A: usize = 100;
    const B: usize = 150;
    const C: usize = 300;

    fn in_body(handler: usize, pc: usize) -> bool {
        match handler {
            A => (100..200).contains(&pc),
            B => (150..180).contains(&pc),
            C => (300..400).contains(&pc),
            _ => false,
        }
    }

    fn slot_of(handler: usize) -> usize {
        match handler {
            A => 5,
            B => 6,
            _ => 7,
        }
    }

    /// Frame at `depth` running at `pc`, with `written` handlers that may
    /// overwrite their own error slot.
    fn frame(
        depth: usize,
        function: Option<FunctionId>,
        pc: usize,
        written: &[usize],
    ) -> FrameScope<'static> {
        FrameScope {
            depth,
            function,
            active: [A, B, C]
                .into_iter()
                .filter(|handler| in_body(*handler, pc))
                .map(|handler_pc| ActiveHandler {
                    handler_pc,
                    error_slots: vec![slot_of(handler_pc)],
                    slot_written: written.contains(&handler_pc),
                })
                .collect(),
            in_body: &in_body,
        }
    }

    #[test]
    fn simultaneously_live_handlers_holding_one_value_are_ambiguous() {
        let f = Some(function());
        let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, a, OriginEvidence::Known(a));
        // Thrown from inside A's body, caught by its nested handler B.
        book.land(1, f, B, 6, b, OriginEvidence::Known(b));
        let inner = [frame(1, f, 160, &[])];
        assert_eq!(book.lookup(&inner, |_| true), LandingMatch::Ambiguous(2));
        // Only a slot that still holds the value counts.
        assert_eq!(
            book.lookup(&inner, |slot| slot == 6),
            LandingMatch::Origin(OriginEvidence::Known(b), Some(b))
        );
        // Back in A after the nested catch: only A is running.
        assert_eq!(
            book.lookup(&[frame(1, f, 120, &[])], |_| true),
            LandingMatch::Origin(OriginEvidence::Known(a), Some(a))
        );
        // Another frame depth or function sees none of these notes.
        assert_eq!(
            book.lookup(&[frame(2, f, 160, &[])], |slot| slot == 99),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
    }

    #[test]
    fn a_finished_handler_proves_nothing() {
        let f = Some(function());
        let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, a, OriginEvidence::Known(a));
        book.land(1, f, C, 7, b, OriginEvidence::Known(b));
        // Inside the later sibling handler: the first is finished.
        assert_eq!(
            book.lookup(&[frame(1, f, 350, &[])], |_| true),
            LandingMatch::Origin(OriginEvidence::Known(b), Some(b))
        );
        // Outside every handler: nothing is running, even if slots still
        // hold an equal value.
        assert_eq!(
            book.lookup(&[frame(1, f, 250, &[])], |_| true),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
    }

    #[test]
    fn a_later_landing_outside_a_handler_retires_its_note() {
        let f = Some(function());
        let (old, pad) = (allocate_telemetry_id(), allocate_telemetry_id());
        let mut book = ErrorBook::default();
        // A call lands in A and returns. A later call at the same depth lands
        // in C (a defer pad), then control reaches A's body without landing
        // there: the old note is stale.
        book.land(1, f, A, 5, old, OriginEvidence::Known(old));
        book.land(1, f, C, 7, pad, OriginEvidence::Known(pad));
        assert_eq!(
            book.lookup(&[frame(1, f, 120, &[])], |slot| slot == 5),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
        // Re-landing in A replaces the note and proves again.
        let fresh = allocate_telemetry_id();
        book.land(1, f, A, 5, fresh, OriginEvidence::Known(fresh));
        assert_eq!(
            book.lookup(&[frame(1, f, 120, &[])], |slot| slot == 5),
            LandingMatch::Origin(OriginEvidence::Known(fresh), Some(fresh))
        );
    }

    #[test]
    fn a_handler_that_writes_its_error_slot_proves_nothing() {
        let f = Some(function());
        let a = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        book.land(1, f, A, 5, a, OriginEvidence::Known(a));
        assert_eq!(
            book.lookup(&[frame(1, f, 120, &[A])], |slot| slot == 5),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
        // The same slot not holding the value: nothing to account for.
        assert_eq!(
            book.lookup(&[frame(1, f, 120, &[A])], |_| false),
            LandingMatch::Unresolved(UnresolvedOrigin::NoLanding)
        );
    }

    #[test]
    fn one_origin_through_several_frames_is_proven_without_a_single_previous_raise() {
        let f = Some(function());
        let origin = allocate_telemetry_id();
        let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
        let mut book = ErrorBook::default();
        book.land(0, f, A, 5, a, OriginEvidence::Known(origin));
        book.land(1, f, A, 5 + 64, b, OriginEvidence::Known(origin));
        let frames = [frame(0, f, 120, &[]), frame(1, f, 120, &[])];
        assert_eq!(
            book.lookup(&frames, |slot| slot == 5 || slot == 5 + 64),
            LandingMatch::Origin(OriginEvidence::Known(origin), None)
        );
        // A shallower landing drops deeper notes; pruning drops popped frames.
        book.land(0, f, C, 7, a, OriginEvidence::Known(a));
        assert_eq!(book.landings.len(), 2);
        book.prune(0);
        assert!(book.landings.is_empty());
    }

    #[test]
    fn eviction_makes_lookups_unresolved_until_cleared() {
        let f = Some(function());
        let raise = allocate_telemetry_id();
        let mut book = ErrorBook::default();
        for depth in 0..=MAX_LANDINGS {
            book.land(depth, f, A, 5, raise, OriginEvidence::Known(raise));
        }
        assert_eq!(
            book.lookup(&[frame(0, f, 120, &[])], |_| true),
            LandingMatch::Unresolved(UnresolvedOrigin::LandingsEvicted)
        );
        book.clear();
        book.land(0, f, A, 5, raise, OriginEvidence::Known(raise));
        assert_eq!(
            book.lookup(&[frame(0, f, 120, &[])], |slot| slot == 5),
            LandingMatch::Origin(OriginEvidence::Known(raise), Some(raise))
        );
    }
}
