//! Conservative MIR reachability from a pack's entry points.
//! Interface dispatch reaches every method with the requested name.

use std::collections::{BTreeSet, HashMap, VecDeque};

use baml_compiler2_mir::{
    Constant, MirFunction, MirFunctionBody, MirFunctionKind, Operand, Rvalue, StatementKind,
    Terminator,
};

use crate::{
    index::ProgramIndex,
    operands::{walk_statement_operands, walk_terminator_operands},
};

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
    index: &'a ProgramIndex<'a>,
    out: BTreeSet<usize>,
}

impl<'db> Collector<'_, 'db> {
    fn function(&mut self, func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>) {
        if let Some((object, _)) = self.index.function(self.db, func) {
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

    fn dispatch(&mut self, method: &str) {
        self.out.extend(self.index.methods_named(method));
    }

    fn body(&mut self, body: &MirFunctionBody<'db>, lambdas: &[MirFunction<'db>]) {
        for block in &body.blocks {
            for statement in &block.statements {
                walk_statement_operands(&statement.kind, &mut |operand| self.operand(operand));
                if let StatementKind::Assign { value, .. } = &statement.kind {
                    match value {
                        Rvalue::MakeClosure { lambda_idx, .. } => {
                            // Include closure bodies even if their calls cannot be resolved statically.
                            if let Some(lambda) = lambdas.get(*lambda_idx) {
                                self.out
                                    .extend(self.index.lambda(self.db, &lambda.identity));
                                if let MirFunctionKind::Bytecode(body) = &lambda.kind {
                                    self.body(body, &lambda.lambdas);
                                }
                            }
                        }
                        Rvalue::MakeBoundMethod { func, .. }
                        | Rvalue::MakeGenericFunction { func, .. } => self.function(*func),
                        Rvalue::MakeVirtualBoundMethod { method, .. }
                        | Rvalue::MakeVirtualFunction { method, .. } => self.dispatch(method),
                        _ => {}
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                if let Terminator::VirtualCall { method, .. } = terminator {
                    self.dispatch(method);
                }
                // Rethrow's context is VM exception state, not a callable reference.
                if let Terminator::Rethrow { value, .. } = terminator {
                    self.operand(value);
                } else {
                    walk_terminator_operands(terminator, &mut |operand| self.operand(operand));
                }
            }
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
