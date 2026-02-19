//! Shared pull-model traversal for places/operands/rvalues.
//!
//! This module centralizes operand evaluation order for both:
//! - bytecode emission (`emit.rs`)
//! - stack-carry safety simulation (`analysis/stack_carry.rs`)
//!
//! Keeping a single traversal avoids semantic drift between emitter and analysis.

use baml_compiler_mir::{
    AggregateKind, BinOp, Constant, IndexKind, Local, Operand, Place, Rvalue, UnaryOp,
};
use baml_type::Ty;

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

    fn load_field(&mut self, field: usize) -> Result<(), Self::Error>;
    fn load_index(&mut self, kind: IndexKind) -> Result<(), Self::Error>;

    fn binary_op(&mut self, op: BinOp) -> Result<(), Self::Error>;
    fn unary_op(&mut self, op: UnaryOp) -> Result<(), Self::Error>;

    fn alloc_array(&mut self, len: usize) -> Result<(), Self::Error>;
    fn alloc_map(&mut self, len: usize) -> Result<(), Self::Error>;

    fn alloc_class_instance(&mut self, class_name: &str) -> Result<(), Self::Error>;
    fn copy_top(&mut self, offset: usize) -> Result<(), Self::Error>;
    fn store_field(&mut self, field_idx: usize) -> Result<(), Self::Error>;

    fn alloc_enum_variant(&mut self, enum_name: &str, variant: &str) -> Result<(), Self::Error>;

    fn discriminant(&mut self) -> Result<(), Self::Error>;
    fn type_tag(&mut self) -> Result<(), Self::Error>;

    fn len(&mut self) -> Result<(), Self::Error>;
    fn is_type(&mut self, ty: &Ty) -> Result<(), Self::Error>;
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
            walk_place_pull(sink, base)?;
            sink.load_field(*field)
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
            // VM expects values first, then keys.
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
                    sink.copy_top(0)?;
                    walk_operand_pull(sink, field_operand)?;
                    sink.store_field(field_idx)?;
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
        Rvalue::Len(place) => {
            walk_place_pull(sink, place)?;
            sink.len()
        }
        Rvalue::IsType { operand, ty } => {
            walk_operand_pull(sink, operand)?;
            sink.is_type(ty)
        }
    }
}
