//! Operand traversal shared by admission analyses.

use baml_compiler2_mir::{Operand, Rvalue, StatementKind, Terminator};

pub(crate) fn walk_statement_operands<'a, 'db>(
    statement: &'a StatementKind<'db>,
    f: &mut impl FnMut(&'a Operand<'db>),
) {
    match statement {
        StatementKind::Assign { value, .. } => walk_rvalue_operands(value, f),
        StatementKind::Intrinsic { args, .. } => args.iter().for_each(f),
        StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            f(receiver);
            f(value);
        }
        StatementKind::Drop(_) | StatementKind::FreshCell { .. } | StatementKind::Nop => {}
    }
}

fn walk_rvalue_operands<'a, 'db>(value: &'a Rvalue<'db>, f: &mut impl FnMut(&'a Operand<'db>)) {
    match value {
        Rvalue::Use(x)
        | Rvalue::UnaryOp { operand: x, .. }
        | Rvalue::IsType { operand: x, .. }
        | Rvalue::IsTypeTag { operand: x, .. }
        | Rvalue::VirtualFieldAccess { receiver: x, .. }
        | Rvalue::MakeGenericFunctionFromValue { value: x, .. }
        | Rvalue::MakeBoundMethod { receiver: x, .. }
        | Rvalue::MakeVirtualBoundMethod { receiver: x, .. }
        | Rvalue::SatisfiesBound { ty: x, .. } => f(x),
        Rvalue::BinaryOp { left, right, .. } => {
            f(left);
            f(right);
        }
        Rvalue::Array(_, elements)
        | Rvalue::Aggregate {
            fields: elements, ..
        }
        | Rvalue::MakeClosure {
            captures: elements, ..
        }
        | Rvalue::MakeVirtualFunction {
            type_args: elements,
            ..
        } => elements.iter().for_each(f),
        Rvalue::Map(_, _, entries) => {
            for (key, value) in entries {
                f(key);
                f(value);
            }
        }
        Rvalue::TraceHookSettings { .. }
        | Rvalue::Uint8Array(_)
        | Rvalue::Discriminant(_)
        | Rvalue::TypeTag(_)
        | Rvalue::Len(_)
        | Rvalue::LoadType(_)
        | Rvalue::MakeGenericFunction { .. }
        | Rvalue::CurrentPackage => {}
    }
}

pub(crate) fn walk_terminator_operands<'a, 'db>(
    terminator: &'a Terminator<'db>,
    f: &mut impl FnMut(&'a Operand<'db>),
) {
    match terminator {
        Terminator::Call { callee, args, .. } | Terminator::SysOp { callee, args, .. } => {
            f(callee);
            args.iter().for_each(f);
        }
        Terminator::VirtualCall { args, .. } => args.iter().for_each(f),
        Terminator::Branch { condition: x, .. }
        | Terminator::NarrowBind { source: x, .. }
        | Terminator::Switch {
            discriminant: x, ..
        }
        | Terminator::Spawn { plan: x, .. }
        | Terminator::AwaitAny { futures: x, .. }
        | Terminator::Throw { value: x, .. }
        | Terminator::ShortCircuit { operand: x, .. } => f(x),
        Terminator::Rethrow { value, context } => {
            f(value);
            f(context);
        }
        Terminator::Goto { .. }
        | Terminator::Return
        | Terminator::Unreachable
        | Terminator::Await { .. } => {}
    }
}
