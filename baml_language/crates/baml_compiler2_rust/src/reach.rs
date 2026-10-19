//! Conservative MIR reachability from a pack's entry points.
//! Interface dispatch reaches every method with the requested name.

use std::collections::{BTreeSet, HashMap, VecDeque};

use baml_compiler2_mir::{
    Constant, MirFunction, MirFunctionBody, MirFunctionKind, Operand, Rvalue, StatementKind,
    Terminator,
};

use crate::admit::ProgramIndex;

/// The function objects `body` (and the lambdas lowered inside it) refer to.
pub(crate) fn references<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    body: &MirFunctionBody<'db>,
    lambdas: &[MirFunction<'db>],
) -> BTreeSet<usize> {
    let mut collector = Collector {
        db,
        index,
        out: BTreeSet::new(),
    };
    collector.body(body, lambdas);
    collector.out
}

struct Collector<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    index: &'a ProgramIndex,
    out: BTreeSet<usize>,
}

impl<'db> Collector<'_, 'db> {
    fn function(&mut self, func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>) {
        if let Ok(Some((object, _))) = self.index.function(self.db, func) {
            self.out.insert(object);
        }
    }

    fn operand(&mut self, operand: &Operand<'db>) {
        if let Operand::Constant(
            Constant::Function(func) | Constant::GenericFunction { func, .. },
        ) = operand
        {
            self.function(*func);
        }
    }

    fn operands(&mut self, operands: &[Operand<'db>]) {
        for operand in operands {
            self.operand(operand);
        }
    }

    fn dispatch(&mut self, method: &str) {
        self.out.extend(self.index.methods_named(method));
    }

    fn body(&mut self, body: &MirFunctionBody<'db>, lambdas: &[MirFunction<'db>]) {
        for block in &body.blocks {
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign { value, .. } => self.rvalue(value, lambdas),
                    StatementKind::Intrinsic { args, .. } => self.operands(args),
                    StatementKind::VirtualFieldStore {
                        receiver, value, ..
                    } => {
                        self.operand(receiver);
                        self.operand(value);
                    }
                    StatementKind::Drop(_)
                    | StatementKind::FreshCell { .. }
                    | StatementKind::Nop => {}
                }
            }
            match &block.terminator {
                Some(
                    Terminator::Call { callee, args, .. } | Terminator::SysOp { callee, args, .. },
                ) => {
                    self.operand(callee);
                    self.operands(args);
                }
                Some(Terminator::VirtualCall { method, args, .. }) => {
                    self.dispatch(method);
                    self.operands(args);
                }
                Some(
                    Terminator::Branch { condition: x, .. }
                    | Terminator::NarrowBind { source: x, .. }
                    | Terminator::Switch {
                        discriminant: x, ..
                    }
                    | Terminator::Spawn { plan: x, .. }
                    | Terminator::AwaitAny { futures: x, .. }
                    | Terminator::Throw { value: x, .. }
                    | Terminator::Rethrow { value: x, .. }
                    | Terminator::ThrowIfPanic { value: x, .. }
                    | Terminator::ShortCircuit { operand: x, .. },
                ) => self.operand(x),
                Some(
                    Terminator::Goto { .. }
                    | Terminator::Return
                    | Terminator::Unreachable
                    | Terminator::Await { .. },
                )
                | None => {}
            }
        }
    }

    fn rvalue(&mut self, value: &Rvalue<'db>, lambdas: &[MirFunction<'db>]) {
        match value {
            Rvalue::Use(x)
            | Rvalue::UnaryOp { operand: x, .. }
            | Rvalue::IsType { operand: x, .. }
            | Rvalue::IsTypeTag { operand: x, .. }
            | Rvalue::VirtualFieldAccess { receiver: x, .. }
            | Rvalue::MakeGenericFunctionFromValue { value: x, .. } => self.operand(x),
            Rvalue::BinaryOp { left, right, .. } => {
                self.operand(left);
                self.operand(right);
            }
            Rvalue::Array(_, elements)
            | Rvalue::Aggregate {
                fields: elements, ..
            } => self.operands(elements),
            Rvalue::Map(_, _, entries) => {
                for (key, value) in entries {
                    self.operand(key);
                    self.operand(value);
                }
            }
            // Include closure bodies even when their calls cannot be resolved statically.
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                self.operands(captures);
                if let Some(lambda) = lambdas.get(*lambda_idx) {
                    if let Ok(Some(object)) = self.index.lambda(self.db, &lambda.identity) {
                        self.out.insert(object);
                    }
                    if let MirFunctionKind::Bytecode(body) = &lambda.kind {
                        self.body(body, &lambda.lambdas);
                    }
                }
            }
            Rvalue::MakeBoundMethod { func, receiver } => {
                self.function(*func);
                self.operand(receiver);
            }
            Rvalue::MakeGenericFunction { func, .. } => self.function(*func),
            Rvalue::MakeVirtualBoundMethod {
                method, receiver, ..
            } => {
                self.dispatch(method);
                self.operand(receiver);
            }
            Rvalue::MakeVirtualFunction {
                method, type_args, ..
            } => {
                self.dispatch(method);
                self.operands(type_args);
            }
            Rvalue::SatisfiesBound { ty, .. } => self.operand(ty),
            Rvalue::TraceHookSettings { .. }
            | Rvalue::Uint8Array(_)
            | Rvalue::Discriminant(_)
            | Rvalue::TypeTag(_)
            | Rvalue::Len(_)
            | Rvalue::LoadType(_)
            | Rvalue::CurrentPackage => {}
        }
    }
}

/// Transitive reachability; objects absent from `references` end a path.
pub(crate) fn reachable(
    entries: impl IntoIterator<Item = usize>,
    references: &HashMap<usize, BTreeSet<usize>>,
) -> BTreeSet<usize> {
    let mut reached = BTreeSet::new();
    let mut queue: VecDeque<usize> = entries.into_iter().collect();
    while let Some(object) = queue.pop_front() {
        if !reached.insert(object) {
            continue;
        }
        if let Some(callees) = references.get(&object) {
            queue.extend(callees.iter().copied());
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachable_is_the_closure_of_the_entries() {
        let references = HashMap::from([
            (1, BTreeSet::from([2, 3])),
            (2, BTreeSet::from([1])),
            (3, BTreeSet::from([4])),
            (5, BTreeSet::from([1])),
        ]);
        assert_eq!(reachable([1], &references), BTreeSet::from([1, 2, 3, 4]));
        assert_eq!(reachable([4], &references), BTreeSet::from([4]));
        assert_eq!(reachable([], &references), BTreeSet::new());
    }
}
