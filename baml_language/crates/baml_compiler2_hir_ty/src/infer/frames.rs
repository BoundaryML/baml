//! The control frames of one inference run: the body owner's, and one for
//! each lambda body, `defer` body and tagged-template flatten block the
//! walk is inside. A frame owns the state a construct's body must not share
//! with the code around it - whether the current point has diverged, and
//! which loops are open - so entering one starts that state fresh and
//! leaving one resumes the enclosing frame's, by construction rather than
//! by a save/restore pair at each site.

use std::num::NonZeroUsize;

use baml_compiler2_hir::body::BodyOwnerId;
use baml_type::interned::Ty;
use rustc_hash::FxHashMap;

use super::{Diverges, FileScopeId, InferenceContext, UnitSite, flow::LoopFrame};
use crate::diagnostics::{LoopControlBarrier, ReturnContext};

/// One frame of control state; see the module doc.
pub(super) struct ControlFrame {
    pub kind: FrameKind,
    /// Whether execution can proceed past the current point of the frame.
    pub diverges: Diverges,
    /// The loops open in the frame, innermost last. A `break` or `continue`
    /// binds to the last one; with none open there is nothing it can leave
    /// (see [`InferenceContext::loop_control_escape`]).
    pub loops: Vec<LoopFrame>,
}

impl ControlFrame {
    /// The bottom frame: the body owner's, with its declared return type as
    /// the expectation every `return` written directly in the body meets,
    /// keying its expressions under the owner's scope.
    pub(super) fn owner(return_ty: Option<Ty>, scope: Option<FileScopeId>) -> ControlFrame {
        ControlFrame::open(FrameKind::Callable {
            returns: ReturnFrame {
                expected: return_ty,
                candidates: Vec::new(),
                scoped_bindings_floor: 0,
            },
            scope,
        })
    }

    fn open(kind: FrameKind) -> ControlFrame {
        ControlFrame {
            kind,
            diverges: Diverges::Maybe,
            loops: Vec::new(),
        }
    }
}

/// What opened a frame.
pub(super) enum FrameKind {
    /// The body owner (the bottom frame) or a lambda body: what a `return`
    /// leaves. `scope` is the metadata scope the frame's expressions key
    /// under: the owner's for the bottom frame, and for a lambda the one the
    /// semantic index registered for it, since it keys a lambda body's
    /// expressions under the lambda's own scope even though they share the
    /// owner's arena (`builder.rs::walk_lambda_expr`). A frame without one
    /// keys under the enclosing frame's.
    Callable {
        returns: ReturnFrame,
        scope: Option<FileScopeId>,
    },
    /// A `defer` body (BEP-042). It runs at scope exit, so a `return`, or a
    /// `break`/`continue` that binds to no loop opened inside it, would
    /// escape it and is rejected.
    Defer,
    /// A tagged template's flatten block, with the tag's body-lambda
    /// parameters (`prompt`'s `role`/`ctx`), which are name-visible inside
    /// it and shadow package items like locals. The semantic index cannot
    /// register them as bindings (the tag is a cross-file item; resolving it
    /// during the index walk would cycle), so path resolution consults the
    /// frame instead. The block is a deferred closure, so its divergence and
    /// loops are its own (a `break` inside it binds to a loop inside it
    /// only, and a `return` is rejected: it would leave the closure when
    /// the template is rendered, not the enclosing function), but it keys
    /// its expressions under the enclosing scope.
    Template {
        params: FxHashMap<baml_type::Name, Ty>,
    },
}

/// The return context of a callable frame.
#[derive(Debug, Clone, Default)]
pub(super) struct ReturnFrame {
    pub expected: Option<Ty>,
    pub candidates: Vec<Returned>,
    /// Depth of the scoped-binding stack when the frame opened. A binding at
    /// or past it belongs to this frame and closes before the frame's value
    /// is published, so a `return` typed by one is an escape.
    pub scoped_bindings_floor: usize,
}

/// Why a `break`/`continue` written at some point has no loop to take it:
/// the innermost frame has no loop open, so the statement would leave the
/// frame's construct instead.
pub(super) enum LoopControlEscape {
    /// A `defer` body (BEP-042) inside a loop.
    Defer,
    /// No loop at all, or a loop that cannot be left from inside the frame's
    /// construct (a lambda body, a template interpolation).
    OutsideLoop(LoopControlBarrier),
}

/// Why a `return` written at some point is rejected: it would leave a
/// construct that is not a function body.
pub(super) enum ReturnEscape {
    /// A `defer` body (BEP-042).
    Defer,
    /// An expression context ([`ReturnContext`]).
    Outside(ReturnContext),
}

/// A value a frame returns, with the implicit units it carries when the
/// frame expects no type: they are judged against the frame's other values
/// when it closes ([`InferenceContext::join_units`]).
#[derive(Debug, Clone)]
pub(super) struct Returned {
    pub ty: Ty,
    pub units: Vec<UnitSite>,
}

impl InferenceContext<'_> {
    /// The innermost frame.
    pub(super) fn control_frame(&self) -> &ControlFrame {
        self.frames.last()
    }

    pub(super) fn control_frame_mut(&mut self) -> &mut ControlFrame {
        self.frames.last_mut()
    }

    /// Opens a frame, with nothing diverged and no loop open in it.
    pub(super) fn enter_frame(&mut self, kind: FrameKind) {
        self.frames.push(ControlFrame::open(kind));
    }

    /// Closes the innermost frame; the enclosing frame's state resumes.
    pub(super) fn leave_frame(&mut self) {
        let outer = NonZeroUsize::new(self.frames.len() - 1).unwrap_or_else(|| {
            unreachable!("`leave_frame` pairs with `enter_frame`; the owner frame is never left")
        });
        self.frames.truncate(outer);
    }

    /// Whether execution can proceed past the current point.
    pub(super) fn diverges(&self) -> Diverges {
        self.control_frame().diverges
    }

    pub(super) fn set_diverges(&mut self, diverges: Diverges) {
        self.control_frame_mut().diverges = diverges;
    }

    /// The metadata scope expression lookups key under right now: the
    /// innermost callable frame's, down to the owner's.
    pub(super) fn current_scope(&self) -> Option<FileScopeId> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match &frame.kind {
                FrameKind::Callable {
                    scope: Some(scope), ..
                } => Some(*scope),
                FrameKind::Callable { scope: None, .. }
                | FrameKind::Defer
                | FrameKind::Template { .. } => None,
            })
    }

    /// The frame a `return` written here leaves: the innermost callable.
    pub(super) fn return_frame(&self) -> &ReturnFrame {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match &frame.kind {
                FrameKind::Callable { returns, .. } => Some(returns),
                FrameKind::Defer | FrameKind::Template { .. } => None,
            })
            .unwrap_or_else(|| unreachable!("the owner frame is a callable"))
    }

    pub(super) fn return_frame_mut(&mut self) -> &mut ReturnFrame {
        self.frames
            .iter_mut()
            .rev()
            .find_map(|frame| match &mut frame.kind {
                FrameKind::Callable { returns, .. } => Some(returns),
                FrameKind::Defer | FrameKind::Template { .. } => None,
            })
            .unwrap_or_else(|| unreachable!("the owner frame is a callable"))
    }

    /// What a `return` written here would leave other than a function body.
    /// The innermost frame decides: a `defer` body or template interpolation
    /// is left before any callable around it, a lambda's `return` is its
    /// own, and with no frame nested in the owner's the owner itself may be
    /// an expression (a global's initializer, a parameter default), whose
    /// value is its last expression.
    pub(super) fn return_escape(&self) -> Option<ReturnEscape> {
        let (_, nested) = self.frames.split_first();
        match nested.last().map(|frame| &frame.kind) {
            Some(FrameKind::Callable { .. }) => None,
            Some(FrameKind::Defer) => Some(ReturnEscape::Defer),
            Some(FrameKind::Template { .. }) => {
                Some(ReturnEscape::Outside(ReturnContext::TemplateInterpolation))
            }
            None => match self.owner {
                BodyOwnerId::Function(_) => None,
                BodyOwnerId::ParameterDefaults(_) => {
                    Some(ReturnEscape::Outside(ReturnContext::ParameterDefault))
                }
                BodyOwnerId::Let(_) => self
                    .global_initializer()
                    .map(|name| ReturnEscape::Outside(ReturnContext::GlobalInitializer { name })),
            },
        }
    }

    /// What a `break`/`continue` written here would escape, when no loop
    /// in the innermost frame can take it: with no loop open in any frame
    /// there is simply none, otherwise the innermost frame's construct is
    /// what stands between the statement and the loop around it.
    pub(super) fn loop_control_escape(&self) -> Option<LoopControlEscape> {
        let frame = self.control_frame();
        if !frame.loops.is_empty() {
            return None;
        }
        if self.frames.iter().all(|frame| frame.loops.is_empty()) {
            return Some(LoopControlEscape::OutsideLoop(LoopControlBarrier::NoLoop));
        }
        Some(match &frame.kind {
            FrameKind::Defer => LoopControlEscape::Defer,
            FrameKind::Template { .. } => {
                LoopControlEscape::OutsideLoop(LoopControlBarrier::Template)
            }
            // A loop is open in some outer frame, so this callable is a
            // lambda, not the owner.
            FrameKind::Callable { .. } => {
                LoopControlEscape::OutsideLoop(LoopControlBarrier::Lambda)
            }
        })
    }

    /// The type of a tagged-template body parameter in scope, innermost
    /// template winning.
    pub(super) fn template_param_ty(&self, name: &baml_type::Name) -> Option<Ty> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match &frame.kind {
                FrameKind::Template { params } => params.get(name),
                FrameKind::Callable { .. } | FrameKind::Defer => None,
            })
            .cloned()
    }
}
