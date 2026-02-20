//! Shared pull-model traversal for places/operands/rvalues.
//!
//! This module centralizes operand evaluation order for both:
//! - bytecode emission (`emit.rs`)
//! - stack-carry safety simulation (`analysis/stack_carry.rs`)
//!
//! Keeping a single traversal avoids semantic drift between emitter and analysis.

use std::collections::{HashMap, HashSet};

use baml_compiler_mir::{
    AggregateKind, BinOp, Constant, IndexKind, Local, Operand, Place, Rvalue, UnaryOp,
};
use baml_type::Ty;

use crate::analysis::{LocalClassification, LocalDefUse};

/// What to do when pulling a local.
pub(crate) enum LocalPullAction {
    /// Local pull fully handled by the sink.
    Done,
    /// Inline this local by recursively pulling its defining rvalue.
    Inline(Rvalue),
}

/// Backend for pull-model traversal.
pub(crate) trait PullSink {
    type Error;

    fn pull_constant(&mut self, constant: &Constant) -> Result<(), Self::Error>;
    fn pull_local(&mut self, local: Local) -> Result<LocalPullAction, Self::Error>;

    fn load_field(&mut self, field: usize, name: &str) -> Result<(), Self::Error>;
    fn load_index(&mut self, kind: IndexKind) -> Result<(), Self::Error>;

    fn binary_op(&mut self, op: BinOp) -> Result<(), Self::Error>;
    fn unary_op(&mut self, op: UnaryOp) -> Result<(), Self::Error>;

    fn alloc_array(&mut self, len: usize) -> Result<(), Self::Error>;
    fn alloc_map(&mut self, len: usize) -> Result<(), Self::Error>;

    fn alloc_class_instance(&mut self, class_name: &str) -> Result<(), Self::Error>;
    fn copy_top(&mut self, offset: usize) -> Result<(), Self::Error>;
    fn store_field(&mut self, field_idx: usize, name: &str) -> Result<(), Self::Error>;

    fn alloc_enum_variant(&mut self, enum_name: &str, variant: &str) -> Result<(), Self::Error>;

    fn discriminant(&mut self) -> Result<(), Self::Error>;
    fn type_tag(&mut self) -> Result<(), Self::Error>;

    fn len_of_place(&mut self, place: &Place) -> Result<(), Self::Error>;
    fn is_type(&mut self, ty: &Ty) -> Result<(), Self::Error>;

    /// Resolve the field name for a `Place::Field { base, field }` access.
    fn resolve_field_name(&self, base: &Place, field_idx: usize) -> String;

    /// Resolve a field name given the class name directly (used for `Aggregate::Class`).
    fn class_field_name(&self, class_name: &str, field_idx: usize) -> String;
}

/// Stack-effect callbacks for statement/terminator helpers.
pub(crate) trait StackEffectSink: PullSink {
    fn store_field_value(&mut self, field: usize, name: &str) -> Result<(), Self::Error>;
    fn store_index_value(&mut self, kind: IndexKind) -> Result<(), Self::Error>;
    fn pop_values(&mut self, n: usize) -> Result<(), Self::Error>;

    fn push_watch_channel(
        &mut self,
        local: Local,
        channel_name: Option<&str>,
    ) -> Result<(), Self::Error>;
    fn watch_local(&mut self, local: Local) -> Result<(), Self::Error>;
    fn assert_top(&mut self) -> Result<(), Self::Error>;
}

/// How a local assignment statement should be emitted/evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalAssignBehavior {
    /// Skip assignment entirely (no rvalue evaluation).
    Skip,
    /// Evaluate rvalue and keep result on stack (no store).
    EvalNoStore,
    /// Evaluate rvalue and perform local store semantics.
    EvalAndStore,
}

/// How storing to a local should affect the top-of-stack value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalStoreBehavior {
    /// Store into a slot (consumes value).
    StoreSlot,
    /// Keep value on stack (phi-like stack carry).
    KeepOnStack,
    /// Discard value (virtual/dead/copy local store).
    PopValue,
}

/// Shared local-assignment classification behavior used by both emitter and simulator.
pub(crate) fn local_assign_behavior(class: LocalClassification) -> LocalAssignBehavior {
    match class {
        LocalClassification::Virtual | LocalClassification::CopyOf | LocalClassification::Dead => {
            LocalAssignBehavior::Skip
        }
        LocalClassification::PhiLike | LocalClassification::ReturnPhi => {
            LocalAssignBehavior::EvalNoStore
        }
        LocalClassification::Parameter
        | LocalClassification::Real
        | LocalClassification::CallResultImmediate => LocalAssignBehavior::EvalAndStore,
    }
}

/// Shared local-store behavior used by both emitter and simulator.
pub(crate) fn local_store_behavior(class: LocalClassification) -> LocalStoreBehavior {
    match class {
        LocalClassification::Parameter | LocalClassification::Real => LocalStoreBehavior::StoreSlot,
        LocalClassification::PhiLike
        | LocalClassification::ReturnPhi
        | LocalClassification::CallResultImmediate => LocalStoreBehavior::KeepOnStack,
        LocalClassification::Virtual | LocalClassification::CopyOf | LocalClassification::Dead => {
            LocalStoreBehavior::PopValue
        }
    }
}

/// Shared evaluation order for projection stores (`base/index -> value -> store`).
///
/// Returns `Ok(true)` when `destination` is a projection and was handled here.
/// Returns `Ok(false)` for `Place::Local(_)`.
pub(crate) fn walk_projection_store<S: StackEffectSink>(
    sink: &mut S,
    destination: &Place,
    value: &Rvalue,
) -> Result<bool, S::Error> {
    match destination {
        Place::Field { base, field } => {
            let name = sink.resolve_field_name(base, *field);
            walk_place_pull(sink, base)?;
            walk_rvalue_pull(sink, value)?;
            sink.store_field_value(*field, &name)?;
            Ok(true)
        }
        Place::Index { base, index, kind } => {
            walk_place_pull(sink, base)?;
            walk_place_pull(sink, &Place::Local(*index))?;
            walk_rvalue_pull(sink, value)?;
            sink.store_index_value(*kind)?;
            Ok(true)
        }
        Place::Local(_) => Ok(false),
    }
}

/// Shared evaluation for `Drop(place)`.
pub(crate) fn walk_drop_statement<S: StackEffectSink>(
    sink: &mut S,
    place: &Place,
) -> Result<(), S::Error> {
    walk_place_pull(sink, place)?;
    sink.pop_values(1)
}

/// Shared evaluation for `WatchOptions`.
pub(crate) fn walk_watch_options_statement<S: StackEffectSink>(
    sink: &mut S,
    local: Local,
    channel_name: Option<&str>,
    filter: &Operand,
) -> Result<(), S::Error> {
    sink.push_watch_channel(local, channel_name)?;
    walk_operand_pull(sink, filter)?;
    sink.watch_local(local)
}

/// Shared evaluation for `Assert(operand)`.
pub(crate) fn walk_assert_statement<S: StackEffectSink>(
    sink: &mut S,
    operand: &Operand,
) -> Result<(), S::Error> {
    walk_operand_pull(sink, operand)?;
    sink.assert_top()
}

/// Shared pull order for direct calls: each arg only.
pub(crate) fn walk_call_direct_args<S: PullSink>(
    sink: &mut S,
    args: &[Operand],
) -> Result<(), S::Error> {
    for arg in args {
        walk_operand_pull(sink, arg)?;
    }
    Ok(())
}

/// Shared pull order for indirect calls: `args..., callee`.
pub(crate) fn walk_call_indirect_operands<S: PullSink>(
    sink: &mut S,
    callee: &Operand,
    args: &[Operand],
) -> Result<(), S::Error> {
    walk_call_direct_args(sink, args)?;
    walk_operand_pull(sink, callee)
}

/// Resolve a call operand to a statically-known function name through
/// `Virtual`/`CopyOf` forwarding chains.
pub(crate) fn resolve_constant_function_name(
    operand: &Operand,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
) -> Option<String> {
    resolve_constant_function_name_inner(operand, classifications, def_use, &mut HashSet::new())
}

fn resolve_constant_function_name_inner(
    operand: &Operand,
    classifications: &HashMap<Local, LocalClassification>,
    def_use: &HashMap<Local, LocalDefUse>,
    visited_locals: &mut HashSet<Local>,
) -> Option<String> {
    match operand {
        Operand::Constant(Constant::Function(qn)) => Some(qn.to_runtime_string()),
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
            if !visited_locals.insert(*local) {
                return None;
            }

            match classifications.get(local).copied() {
                Some(LocalClassification::Virtual) => {
                    let def = def_use.get(local).and_then(|du| du.def.as_ref())?;
                    let Rvalue::Use(inner) = &def.rvalue else {
                        return None;
                    };
                    resolve_constant_function_name_inner(
                        inner,
                        classifications,
                        def_use,
                        visited_locals,
                    )
                }
                Some(LocalClassification::CopyOf) => {
                    let def = def_use.get(local).and_then(|du| du.def.as_ref())?;
                    let Rvalue::Use(copy_operand) = &def.rvalue else {
                        return None;
                    };
                    let source = match copy_operand {
                        Operand::Copy(Place::Local(source))
                        | Operand::Move(Place::Local(source)) => *source,
                        _ => return None,
                    };
                    if source == *local {
                        return None;
                    }
                    resolve_constant_function_name_inner(
                        &Operand::copy_local(source),
                        classifications,
                        def_use,
                        visited_locals,
                    )
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Shared pull for `Return` value place (`_0`).
pub(crate) fn walk_return_value<S: PullSink>(sink: &mut S) -> Result<(), S::Error> {
    walk_place_pull(sink, &Place::Local(Local(0)))
}

/// Shared pull for `Await` future place.
pub(crate) fn walk_await_future<S: PullSink>(sink: &mut S, future: &Place) -> Result<(), S::Error> {
    walk_place_pull(sink, future)
}

/// Walk an operand in pull order.
pub(crate) fn walk_operand_pull<S: PullSink>(
    sink: &mut S,
    operand: &Operand,
) -> Result<(), S::Error> {
    match operand {
        Operand::Copy(place) | Operand::Move(place) => walk_place_pull(sink, place),
        Operand::Constant(constant) => sink.pull_constant(constant),
    }
}

/// Walk a place read in pull order.
pub(crate) fn walk_place_pull<S: PullSink>(sink: &mut S, place: &Place) -> Result<(), S::Error> {
    match place {
        Place::Local(local) => match sink.pull_local(*local)? {
            LocalPullAction::Done => Ok(()),
            LocalPullAction::Inline(rvalue) => walk_rvalue_pull(sink, &rvalue),
        },
        Place::Field { base, field } => {
            let name = sink.resolve_field_name(base, *field);
            walk_place_pull(sink, base)?;
            sink.load_field(*field, &name)
        }
        Place::Index { base, index, kind } => {
            walk_place_pull(sink, base)?;
            walk_place_pull(sink, &Place::Local(*index))?;
            sink.load_index(*kind)
        }
    }
}

/// Walk an rvalue in pull order.
pub(crate) fn walk_rvalue_pull<S: PullSink>(sink: &mut S, rvalue: &Rvalue) -> Result<(), S::Error> {
    match rvalue {
        Rvalue::Use(operand) => walk_operand_pull(sink, operand),
        Rvalue::BinaryOp { op, left, right } => {
            walk_operand_pull(sink, left)?;
            walk_operand_pull(sink, right)?;
            sink.binary_op(*op)
        }
        Rvalue::UnaryOp { op, operand } => {
            walk_operand_pull(sink, operand)?;
            sink.unary_op(*op)
        }
        Rvalue::Array(elements) => {
            for element in elements {
                walk_operand_pull(sink, element)?;
            }
            sink.alloc_array(elements.len())
        }
        Rvalue::Map(entries) => {
            // VM `AllocMap` expects stack layout:
            // [..., v1, v2, ..., k1, k2, ...] for {(k1, v1), (k2, v2), ...}.
            for (_key, value) in entries {
                walk_operand_pull(sink, value)?;
            }
            for (key, _value) in entries {
                walk_operand_pull(sink, key)?;
            }
            sink.alloc_map(entries.len())
        }
        Rvalue::Aggregate { kind, fields } => match kind {
            AggregateKind::Array => {
                for field in fields {
                    walk_operand_pull(sink, field)?;
                }
                sink.alloc_array(fields.len())
            }
            AggregateKind::Class(class_name) => {
                sink.alloc_class_instance(class_name)?;
                for (field_idx, field_operand) in fields.iter().enumerate() {
                    let name = sink.class_field_name(class_name, field_idx);
                    sink.copy_top(0)?;
                    walk_operand_pull(sink, field_operand)?;
                    sink.store_field(field_idx, &name)?;
                }
                Ok(())
            }
            AggregateKind::EnumVariant { enum_name, variant } => {
                sink.alloc_enum_variant(enum_name, variant)
            }
        },
        Rvalue::Discriminant(place) => {
            walk_place_pull(sink, place)?;
            sink.discriminant()
        }
        Rvalue::TypeTag(place) => {
            walk_place_pull(sink, place)?;
            sink.type_tag()
        }
        Rvalue::Len(place) => sink.len_of_place(place),
        Rvalue::IsType { operand, ty } => {
            walk_operand_pull(sink, operand)?;
            sink.is_type(ty)
        }
    }
}
