//! What a MIR instruction reads and writes, as sets of *resources*.
//!
//! A resource is a storage location a body can observe: a frame slot holding
//! a value, the cell a captured binding lives in, a frame type-argument slot,
//! a heap field or element, or the order of observable effects. Every pass that
//! moves, duplicates, or discards an evaluation asks the same two questions:
//! what does this rvalue read, and what does this instruction write.
//! This module is the one place that answers them, so the answers cannot drift
//! between the MIR optimizer and the emitter.
//!
//! The heap model is field-sensitive and alias-insensitive: a store to field
//! `n` of any object clobbers a read of field `n` of any object, and a call
//! clobbers every heap location at once. Field sensitivity applies only
//! between concrete accesses, whose index is a slot in a statically known
//! class layout, so two of them alias exactly when they name the same slot of
//! the same object. An interface field access resolves its slot at run time
//! — the class backing it may keep it under any name and index — so it is
//! every heap location at once, on both the read and the write side. Cells
//! are heap locations: a closure sharing one, or another task, may write it at
//! any call, await, or spawn, so nothing this frame does or does not do to a
//! cell says anything about its stability across one of those.

use std::collections::{HashMap, HashSet};

use baml_type::{Literal, RuntimeTy};

pub use crate::ir::CellId;
use crate::{
    AggregateKind, BinOp, Constant, IntrinsicOp, Local, MirFunctionBody, Operand, Place, Rvalue,
    StatementKind, Terminator, UnaryOp,
};

/// A set of resources, read by an evaluation or written by an instruction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resources {
    /// Frame slots holding values (registers).
    pub locals: HashSet<Local>,
    /// Cells, through a captured local or a capture slot.
    pub cells: HashSet<CellId>,
    /// Frame type-argument slots: the `TypeArgRef` leaves of a template, which
    /// a `BindType` rebinds.
    pub type_slots: HashSet<u32>,
    /// Heap fields by index, on any object.
    pub fields: HashSet<usize>,
    /// Container elements and lengths, of any container.
    pub elements: bool,
    /// Every heap location at once: every field, element, and cell. Read by
    /// an access this module cannot name (an interface field resolved at run
    /// time); written by anything that may run arbitrary code.
    pub heap: bool,
    /// The order of observable effects. Read by an evaluation that can trap,
    /// because the trap is itself an event; written by every observable
    /// instruction.
    pub order: bool,
}

impl Resources {
    pub fn is_empty(&self) -> bool {
        self.locals.is_empty()
            && self.cells.is_empty()
            && self.type_slots.is_empty()
            && self.fields.is_empty()
            && !self.elements
            && !self.heap
            && !self.order
    }

    pub fn extend(&mut self, other: &Resources) {
        self.locals.extend(other.locals.iter().copied());
        self.cells.extend(other.cells.iter().copied());
        self.type_slots.extend(other.type_slots.iter().copied());
        self.fields.extend(other.fields.iter().copied());
        self.elements |= other.elements;
        self.heap |= other.heap;
        self.order |= other.order;
    }

    /// Whether the set names any heap location.
    fn touches_heap(&self) -> bool {
        self.heap || self.elements || !self.fields.is_empty() || !self.cells.is_empty()
    }

    /// Whether some resource is in both sets. Symmetric: `heap` on either side
    /// meets any heap location on the other.
    pub fn intersects(&self, other: &Resources) -> bool {
        (self.heap && other.touches_heap())
            || (other.heap && self.touches_heap())
            || (self.elements && other.elements)
            || (self.order && other.order)
            || self.fields.iter().any(|field| other.fields.contains(field))
            || self.cells.iter().any(|cell| other.cells.contains(cell))
            || self.locals.iter().any(|local| other.locals.contains(local))
            || self
                .type_slots
                .iter()
                .any(|slot| other.type_slots.contains(slot))
    }
}

/// How writes to frame slots are judged.
#[derive(Clone, Copy, Debug)]
pub struct ClobberModel {
    /// Whether a register write is an observable event. A slot is invisible
    /// outside its frame, so only a catch handler in the same frame can see
    /// one slot's value after a trap elsewhere in the frame; a body with no
    /// handler has no observer of them.
    pub registers_observed: bool,
}

impl ClobberModel {
    pub fn for_body(body: &MirFunctionBody<'_>) -> Self {
        Self {
            registers_observed: !body.catch_regions.is_empty(),
        }
    }
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

/// Walk every place an operand names, calling `f` for each.
pub fn walk_operand_places<'a>(operand: &'a Operand<'_>, f: &mut impl FnMut(&'a Place)) {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => f(place),
        Operand::Constant(_) => {}
    }
}

/// Walk every place an rvalue reads, calling `f` for each. The one traversal
/// every reader of an rvalue's operands goes through, so no two can disagree
/// about which operands evaluating it touches.
pub fn walk_rvalue_places<'a>(rvalue: &'a Rvalue<'_>, f: &mut impl FnMut(&'a Place)) {
    match rvalue {
        Rvalue::Use(operand) => walk_operand_places(operand, f),
        Rvalue::BinaryOp { left, right, .. } => {
            walk_operand_places(left, f);
            walk_operand_places(right, f);
        }
        Rvalue::UnaryOp { operand, .. } => walk_operand_places(operand, f),
        Rvalue::Array(_, elements) => {
            for elem in elements {
                walk_operand_places(elem, f);
            }
        }
        Rvalue::Uint8Array(_) => {}
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                walk_operand_places(key, f);
                walk_operand_places(value, f);
            }
        }
        Rvalue::Aggregate { fields, .. } => {
            for field in fields {
                walk_operand_places(field, f);
            }
        }
        Rvalue::Discriminant(place) | Rvalue::TypeTag(place) | Rvalue::Len(place) => f(place),
        Rvalue::IsType { operand, .. } | Rvalue::IsTypeTag { operand, .. } => {
            walk_operand_places(operand, f);
        }
        Rvalue::MakeClosure { captures, .. } => {
            for cap in captures {
                walk_operand_places(cap, f);
            }
        }
        Rvalue::MakeBoundMethod { receiver, .. }
        | Rvalue::MakeVirtualBoundMethod { receiver, .. }
        | Rvalue::VirtualFieldAccess { receiver, .. } => {
            walk_operand_places(receiver, f);
        }
        Rvalue::MakeVirtualFunction { type_args, .. } => {
            for arg in type_args {
                walk_operand_places(arg, f);
            }
        }
        Rvalue::LoadType(_) | Rvalue::CurrentPackage(_) | Rvalue::MakeGenericFunction { .. } => {
            // No place operands — the templates are compile-time data.
        }
        Rvalue::MakeGenericFunctionFromValue { value, .. } => {
            walk_operand_places(value, f);
        }
    }
}

/// Walk every frame type-arg slot an rvalue reads — the `TypeArgRef` leaves of
/// each template it carries — calling `f` for each.
///
/// Matched exhaustively on purpose, like [`rvalue_can_trap`]: a template
/// carried by a new variant must be listed here, or a rebinding between its
/// definition and its use goes unseen.
pub fn walk_rvalue_type_slots(rvalue: &Rvalue<'_>, f: &mut impl FnMut(u32)) {
    match rvalue {
        Rvalue::Array(element, _) => element.for_each_type_arg_ref(f),
        Rvalue::Map(key, value, _) => {
            key.for_each_type_arg_ref(f);
            value.for_each_type_arg_ref(f);
        }
        Rvalue::Aggregate { kind, .. } => match kind {
            AggregateKind::Class {
                type_arg_templates, ..
            } => {
                for template in type_arg_templates {
                    template.for_each_type_arg_ref(f);
                }
            }
            AggregateKind::Array | AggregateKind::EnumVariant { .. } => {}
        },
        Rvalue::IsType { ty_template, .. } => ty_template.for_each_type_arg_ref(f),
        Rvalue::MakeClosure {
            type_arg_templates, ..
        }
        | Rvalue::MakeGenericFunction {
            type_arg_templates, ..
        }
        | Rvalue::MakeGenericFunctionFromValue {
            type_arg_templates, ..
        } => {
            for template in type_arg_templates {
                template.for_each_type_arg_ref(f);
            }
        }
        Rvalue::MakeVirtualBoundMethod {
            iface, type_args, ..
        } => {
            iface.for_each_type_arg_ref(f);
            for template in type_args {
                template.for_each_type_arg_ref(f);
            }
        }
        Rvalue::MakeVirtualFunction { self_ty, iface, .. } => {
            self_ty.for_each_type_arg_ref(f);
            iface.for_each_type_arg_ref(f);
        }
        Rvalue::VirtualFieldAccess { iface, .. } => iface.for_each_type_arg_ref(f),
        Rvalue::LoadType(template) => template.for_each_type_arg_ref(f),
        Rvalue::Use(_)
        | Rvalue::BinaryOp { .. }
        | Rvalue::UnaryOp { .. }
        | Rvalue::Uint8Array(_)
        | Rvalue::Len(_)
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::IsTypeTag { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::CurrentPackage(_) => {}
    }
}

/// Walk every frame type-arg slot a statement reads, calling `f` for each.
pub fn walk_statement_type_slots(kind: &StatementKind<'_>, f: &mut impl FnMut(u32)) {
    match kind {
        StatementKind::Assign { value, .. } => walk_rvalue_type_slots(value, f),
        StatementKind::VirtualFieldStore { iface, .. } => iface.for_each_type_arg_ref(f),
        // A `BindType` operand is a type *value* in a local, not a slot read.
        StatementKind::Drop(_)
        | StatementKind::FreshCell { .. }
        | StatementKind::Intrinsic { .. }
        | StatementKind::Nop => {}
    }
}

/// Walk every frame type-arg slot a terminator reads, calling `f` for each.
pub fn walk_terminator_type_slots(terminator: &Terminator<'_>, f: &mut impl FnMut(u32)) {
    match terminator {
        Terminator::NarrowBind { ty_template, .. } => ty_template.for_each_type_arg_ref(f),
        Terminator::VirtualCall { iface, .. } => iface.for_each_type_arg_ref(f),
        Terminator::Spawn { future_ty, .. } => {
            future_ty.returns.for_each_type_arg_ref(f);
            future_ty.throws.for_each_type_arg_ref(f);
        }
        // Call type arguments are `LoadType` temps, read where they are defined.
        Terminator::Goto { .. }
        | Terminator::Branch { .. }
        | Terminator::Switch { .. }
        | Terminator::Return
        | Terminator::Call { .. }
        | Terminator::Unreachable
        | Terminator::SysOp { .. }
        | Terminator::Await { .. }
        | Terminator::AwaitAny { .. }
        | Terminator::Throw { .. }
        | Terminator::Rethrow { .. }
        | Terminator::ThrowIfPanic { .. }
        | Terminator::ShortCircuit { .. } => {}
    }
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// The cell a place reads or writes through, if it goes through one.
pub fn place_cell(place: &Place) -> Option<CellId> {
    match place {
        Place::Deref(cell) => Some(*cell),
        Place::Field { base, .. } | Place::Index { base, .. } => place_cell(base),
        Place::Local(_) | Place::Capture(_) => None,
    }
}

/// Whether evaluating an rvalue reads through a cell anywhere.
pub fn rvalue_reads_cell(rvalue: &Rvalue<'_>) -> bool {
    let mut reads = false;
    walk_rvalue_places(rvalue, &mut |place| reads |= place_cell(place).is_some());
    reads
}

/// The cells a body shares with the tasks it spawns: every cell a spawned
/// closure captures, and every cell a closure stored in one of those cells
/// captures in turn (a spawned body that calls a captured closure reaches
/// that closure's cells).
///
/// Computed from MIR alone. A spawn's closure operand is followed through
/// single-definition copies to its `MakeClosure`; a cell holding a closure is
/// followed through every store into it. Emit's arithmetic specialization
/// declines on these cells: another task may write them at any point.
pub fn spawn_shared_cells<'a, 'db>(body: &'a MirFunctionBody<'db>) -> HashSet<CellId> {
    struct Defs<'a, 'db> {
        local_defs: HashMap<Local, Vec<&'a Rvalue<'db>>>,
        cell_stores: HashMap<CellId, Vec<&'a Rvalue<'db>>>,
    }
    struct Walk<'a, 'db> {
        pending: Vec<&'a Rvalue<'db>>,
        followed_locals: HashSet<Local>,
        followed_cells: HashSet<CellId>,
    }
    impl<'a, 'db> Walk<'a, 'db> {
        // Reach the closure value an operand holds: a local's single
        // definition, or the stores into the cell the operand reads through.
        fn follow_operand(&mut self, defs: &Defs<'a, 'db>, operand: &Operand<'db>) {
            let (Operand::Copy(place) | Operand::Move(place)) = operand else {
                return;
            };
            match place_cell(place) {
                Some(cell) => self.follow_cell(defs, cell),
                None => {
                    if let Place::Local(local) = place
                        && self.followed_locals.insert(*local)
                        && let Some([def]) = defs.local_defs.get(local).map(Vec::as_slice)
                    {
                        self.pending.push(def);
                    }
                }
            }
        }

        fn follow_cell(&mut self, defs: &Defs<'a, 'db>, cell: CellId) {
            if self.followed_cells.insert(cell) {
                self.pending
                    .extend(defs.cell_stores.get(&cell).into_iter().flatten());
            }
        }
    }

    let mut local_defs: HashMap<Local, Vec<&'a Rvalue<'db>>> = HashMap::new();
    let mut cell_stores: HashMap<CellId, Vec<&'a Rvalue<'db>>> = HashMap::new();
    for block in &body.blocks {
        for stmt in &block.statements {
            let StatementKind::Assign { destination, value } = &stmt.kind else {
                continue;
            };
            match destination {
                Place::Local(local) => local_defs.entry(*local).or_default().push(value),
                Place::Deref(cell) => cell_stores.entry(*cell).or_default().push(value),
                Place::Capture(_) | Place::Field { .. } | Place::Index { .. } => {}
            }
        }
    }

    let defs = Defs {
        local_defs,
        cell_stores,
    };
    let mut walk = Walk {
        pending: Vec::new(),
        followed_locals: HashSet::new(),
        followed_cells: HashSet::new(),
    };
    for block in &body.blocks {
        if let Some(Terminator::Spawn { closure, .. }) = &block.terminator {
            walk.follow_operand(&defs, closure);
        }
    }

    let mut shared = HashSet::new();
    while let Some(rvalue) = walk.pending.pop() {
        match rvalue {
            Rvalue::MakeClosure { captures, .. } => {
                for capture in captures {
                    let (Operand::Copy(place) | Operand::Move(place)) = capture else {
                        continue;
                    };
                    let cell = CellId::try_from(place.clone())
                        .unwrap_or_else(|_| unreachable!("a capture operand is a cell pointer"));
                    shared.insert(cell);
                    // The closure a shared cell holds is reachable from the task.
                    walk.follow_cell(&defs, cell);
                }
            }
            Rvalue::Use(operand) => walk.follow_operand(&defs, operand),
            _ => {}
        }
    }
    shared
}

/// Record the resources reading a place touches.
///
/// A bare local names its slot, which for a captured local holds the cell
/// pointer; the value behind it is `Deref`, which names the cell and the slot
/// the pointer was loaded from. A bare capture is the pointer in the closure's
/// capture array, which nothing writes.
pub fn place_reads(place: &Place, out: &mut Resources) {
    match place {
        Place::Local(local) => {
            out.locals.insert(*local);
        }
        Place::Capture(_) => {}
        Place::Deref(cell) => {
            out.cells.insert(*cell);
            out.locals.extend(cell.local());
        }
        Place::Field { base, field } => {
            out.fields.insert(*field);
            place_reads(base, out);
        }
        Place::Index { base, index, .. } => {
            out.elements = true;
            out.locals.insert(*index);
            place_reads(base, out);
        }
    }
}

/// The resources evaluating an rvalue reads.
///
/// `type_slots` are collected only when `track_type_slots`: walking templates
/// is the expensive part of this function and only matters in a body that
/// rebinds a slot, and every other body would pay it for an answer that
/// cannot matter.
pub fn rvalue_reads(
    body: &MirFunctionBody<'_>,
    rvalue: &Rvalue<'_>,
    track_type_slots: bool,
) -> Resources {
    let mut out = Resources::default();
    walk_rvalue_places(rvalue, &mut |place| place_reads(place, &mut out));
    // Reads beyond the operands themselves. Exhaustive so a new variant that
    // reads the heap has to say so here.
    match rvalue {
        // A length lives with the elements: a push changes it.
        Rvalue::Len(_) => out.elements = true,
        // Some field of the receiver, resolved at run time — any field.
        Rvalue::VirtualFieldAccess { .. } => out.heap = true,
        Rvalue::Use(_)
        | Rvalue::BinaryOp { .. }
        | Rvalue::UnaryOp { .. }
        | Rvalue::Array(..)
        | Rvalue::Uint8Array(_)
        | Rvalue::Map(..)
        | Rvalue::Aggregate { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::IsType { .. }
        | Rvalue::IsTypeTag { .. }
        | Rvalue::MakeClosure { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::MakeVirtualBoundMethod { .. }
        | Rvalue::MakeVirtualFunction { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage(_)
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. } => {}
    }
    if track_type_slots {
        walk_rvalue_type_slots(rvalue, &mut |slot| {
            out.type_slots.insert(slot);
        });
    }
    if rvalue_can_trap(body, rvalue) {
        out.order = true;
    }
    out
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

/// Record the resources a store to a place writes.
fn place_writes(place: &Place, out: &mut Resources) {
    match place {
        Place::Local(local) => {
            out.locals.insert(*local);
        }
        Place::Capture(_) => unreachable!("a bare capture is a pointer nothing stores to"),
        Place::Deref(cell) => {
            out.cells.insert(*cell);
        }
        Place::Field { field, .. } => {
            out.fields.insert(*field);
            out.order = true;
        }
        Place::Index { .. } => {
            out.elements = true;
            out.order = true;
        }
    }
}

/// The resources a statement writes.
pub fn statement_clobbers(
    body: &MirFunctionBody<'_>,
    model: ClobberModel,
    kind: &StatementKind<'_>,
) -> Resources {
    let mut out = Resources::default();
    match kind {
        StatementKind::Assign { destination, value } => {
            place_writes(destination, &mut out);
            if model.registers_observed && !out.locals.is_empty() {
                out.order = true;
            }
            // The evaluation may trap, and the trap is an event.
            if rvalue_can_trap(body, value) {
                out.order = true;
            }
        }
        // A destructor runs arbitrary code.
        StatementKind::Drop(_) => {
            out.heap = true;
            out.order = true;
        }
        // The slot now points at a different cell: both change.
        StatementKind::FreshCell { local, .. } => {
            out.locals.insert(*local);
            out.cells.insert(CellId::Local(*local));
        }
        StatementKind::Intrinsic { op, .. } => match op {
            IntrinsicOp::Log(_) => out.order = true,
            IntrinsicOp::BindType(slot) => {
                out.type_slots.insert(*slot);
            }
        },
        // Some field of the receiver, resolved at run time — any field.
        StatementKind::VirtualFieldStore { .. } => {
            out.heap = true;
            out.order = true;
        }
        StatementKind::Nop => {}
    }
    out
}

/// The resources a terminator writes.
///
/// A call, await, spawn, or sys-op runs code this frame cannot see (a closure
/// holding one of its cells, another task, a host operation), so it writes
/// every heap location; a throw is an event.
pub fn terminator_clobbers(model: ClobberModel, terminator: &Terminator<'_>) -> Resources {
    let mut out = Resources::default();
    match terminator {
        Terminator::Goto { .. }
        | Terminator::Branch { .. }
        | Terminator::Switch { .. }
        | Terminator::Return
        | Terminator::Unreachable => {}
        Terminator::NarrowBind { destination, .. } => {
            out.locals.insert(*destination);
        }
        Terminator::ShortCircuit { destination, .. } => place_writes(destination, &mut out),
        Terminator::Call { destination, .. }
        | Terminator::VirtualCall { destination, .. }
        | Terminator::SysOp { destination, .. }
        | Terminator::Await { destination, .. }
        | Terminator::AwaitAny { destination, .. } => {
            place_writes(destination, &mut out);
            out.heap = true;
            out.order = true;
        }
        Terminator::Spawn { future, .. } => {
            place_writes(future, &mut out);
            out.heap = true;
            out.order = true;
        }
        Terminator::Throw { .. } | Terminator::Rethrow { .. } | Terminator::ThrowIfPanic { .. } => {
            out.order = true;
        }
    }
    if model.registers_observed && !out.locals.is_empty() {
        out.order = true;
    }
    out
}

/// The resources the VM writes when control enters `block`: a catch handler
/// receives its error (and context) bindings with no statement saying so.
pub fn block_entry_clobbers(body: &MirFunctionBody<'_>, block: crate::BlockId) -> Resources {
    let mut out = Resources::default();
    for region in &body.catch_regions {
        if region.handler == block {
            out.locals.insert(region.error_local);
            if let Some(local) = region.stack_trace_local {
                out.locals.insert(local);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Evaluation properties
// ---------------------------------------------------------------------------

/// Whether evaluating this rvalue allocates a fresh object whose identity is
/// observable (mutable containers, class instances, and callable objects), so
/// evaluating it more times than the program does is itself observable.
///
/// Matched exhaustively on purpose: a wrong `false` silently miscompiles.
pub fn rvalue_allocates_identity(rvalue: &Rvalue<'_>) -> bool {
    match rvalue {
        Rvalue::Map(..)
        | Rvalue::Array(..)
        | Rvalue::Uint8Array(_)
        | Rvalue::Aggregate { .. }
        | Rvalue::MakeClosure { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::MakeVirtualBoundMethod { .. }
        | Rvalue::MakeVirtualFunction { .. } => true,
        Rvalue::Use(_)
        | Rvalue::BinaryOp { .. }
        | Rvalue::UnaryOp { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::Len(_)
        | Rvalue::IsType { .. }
        | Rvalue::IsTypeTag { .. }
        | Rvalue::VirtualFieldAccess { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage(_) => false,
    }
}

/// Can evaluating this rvalue raise a catchable panic (`baml.panics.*`)?
///
/// A trap is an observable event: moving an evaluation that can trap past a
/// store, a call, or a handler boundary changes which effects run before the
/// trap and which handler receives it. Only arithmetic can fail, and only `/`
/// fails for every operand type. The rest are `int`-only failures — `float`
/// saturates to infinity or NaN, `bigint` grows, and `string + string` is
/// concatenation — so they ask `operand_could_be_int`. Bitwise and/or/xor and
/// the comparisons stay in range whatever the operands are.
///
/// Matched exhaustively on purpose. This is a soundness predicate, and a
/// wrong `false` miscompiles silently — so a new `Rvalue` variant must fail to
/// compile here rather than default into the infallible group.
pub fn rvalue_can_trap<'db>(body: &MirFunctionBody<'db>, rvalue: &Rvalue<'db>) -> bool {
    match rvalue {
        Rvalue::BinaryOp { op, left, right } => match op {
            // `/` rejects a zero divisor on both numeric paths — BAML throws
            // rather than yielding IEEE infinity (`OpCode::DivFloat`), so this
            // holds whatever the operands are.
            BinOp::Div => true,
            // `%` is guarded on the `int` path only; the float path yields NaN.
            BinOp::Mod | BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl | BinOp::Shr => {
                operand_could_be_int(body, left) && operand_could_be_int(body, right)
            }
            BinOp::Eq
            | BinOp::Ne
            | BinOp::Lt
            | BinOp::Le
            | BinOp::Gt
            | BinOp::Ge
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor => false,
        },
        Rvalue::UnaryOp { op, operand } => match op {
            UnaryOp::Neg => operand_could_be_int(body, operand),
            UnaryOp::Not | UnaryOp::Truthy => false,
        },
        // Reading through an index projection can raise `IndexOutOfBounds`.
        Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => place_indexes(place),
        Rvalue::Use(Operand::Constant(_)) => false,
        // Allocation can report `AllocFailure`, but that is a host resource
        // condition rather than a property of the program point, and treating
        // every allocation as a barrier would disable virtualization outright.
        Rvalue::Array(..)
        | Rvalue::Uint8Array(_)
        | Rvalue::Map(..)
        | Rvalue::Aggregate { .. }
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::Len(_)
        | Rvalue::IsType { .. }
        | Rvalue::IsTypeTag { .. }
        | Rvalue::MakeClosure { .. }
        | Rvalue::MakeBoundMethod { .. }
        | Rvalue::MakeVirtualBoundMethod { .. }
        | Rvalue::VirtualFieldAccess { .. }
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::MakeGenericFunctionFromValue { .. }
        | Rvalue::MakeVirtualFunction { .. }
        | Rvalue::LoadType(_)
        | Rvalue::CurrentPackage(_) => false,
    }
}

/// Whether a place goes through an index projection anywhere in its chain.
fn place_indexes(place: &Place) -> bool {
    match place {
        Place::Local(_) | Place::Capture(_) | Place::Deref(_) => false,
        Place::Index { .. } => true,
        Place::Field { base, .. } => place_indexes(base),
    }
}

/// Could this operand hold an `int` at runtime?
///
/// Deliberately answers `true` for anything whose runtime representation is not
/// pinned down — a union, a type variable, a value read through a projection, a
/// type family variant added later. Only a type that provably never holds an
/// `int` answers `false`.
fn operand_could_be_int<'db>(body: &MirFunctionBody<'db>, operand: &Operand<'db>) -> bool {
    match operand {
        Operand::Constant(c) => matches!(c, Constant::Int(_)),
        Operand::Copy(place) | Operand::Move(place) => match place {
            Place::Local(local) => ty_could_be_int(&body.local(*local).ty),
            // A captured local's declared type is the type of the value in
            // its cell.
            Place::Deref(CellId::Local(local)) => ty_could_be_int(&body.local(*local).ty),
            Place::Deref(CellId::Capture(_)) => true,
            // A field / index / capture read carries no type here.
            Place::Field { .. } | Place::Index { .. } | Place::Capture(_) => true,
        },
    }
}

/// See [`operand_could_be_int`]. The `_ => true` fallback keeps an unlisted or
/// newly added variant on the conservative side.
fn ty_could_be_int(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Int => true,
        RuntimeTy::Literal(lit, ..) => matches!(lit, Literal::Int(_)),
        RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Void
        | RuntimeTy::Media(..)
        | RuntimeTy::Class(..)
        | RuntimeTy::Enum(..)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::List(..)
        | RuntimeTy::Map { .. }
        | RuntimeTy::Function { .. }
        | RuntimeTy::Future(..)
        | RuntimeTy::RustType
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst => false,
        _ => true,
    }
}
