//! MIR reachability from a pack's entry points, by rapid type analysis:
//! interface dispatch reaches a method with the requested name when its
//! class is constructed in reached code, or when no one class owns it.
//! Missing a target is safe: an unreached function stays bytecode.

use std::collections::{BTreeSet, HashMap, VecDeque};

use baml_compiler2_mir::{
    AggregateKind, Constant, MirFunction, MirFunctionBody, MirFunctionKind, Operand, Rvalue,
    StatementKind, Terminator,
};

use crate::{
    index::ProgramIndex,
    operands::{walk_statement_operands, walk_terminator_operands},
};

/// What one body refers to: functions it names, method names it dispatches
/// and classes it constructs.
#[derive(Debug, Default, Clone)]
pub(crate) struct References {
    pub(crate) functions: BTreeSet<usize>,
    pub(crate) dispatches: BTreeSet<String>,
    pub(crate) classes: BTreeSet<usize>,
}

/// The references of `body` and the lambdas lowered inside it.
pub(crate) fn references<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    body: &MirFunctionBody<'db>,
    lambdas: &[MirFunction<'db>],
) -> References {
    let mut collector = Collector {
        db,
        index,
        out: References::default(),
    };
    collector.body(body, lambdas);
    collector.out
}

struct Collector<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    index: &'a ProgramIndex<'a>,
    out: References,
}

impl<'db> Collector<'_, 'db> {
    fn function(&mut self, func: baml_compiler2_hir_ty::extern_loc::FunctionRef<'db>) {
        if let Some((object, _)) = self.index.function(self.db, func) {
            self.out.functions.insert(object);
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
        self.out.dispatches.insert(method.to_owned());
    }

    fn body(&mut self, body: &MirFunctionBody<'db>, lambdas: &[MirFunction<'db>]) {
        for block in &body.blocks {
            for statement in &block.statements {
                walk_statement_operands(&statement.kind, &mut |operand| self.operand(operand));
                if let StatementKind::Assign { value, .. } = &statement.kind {
                    self.rvalue(value, lambdas);
                }
            }
            if let Some(terminator) = &block.terminator {
                self.terminator(terminator);
            }
        }
    }

    fn rvalue(&mut self, value: &Rvalue<'db>, lambdas: &[MirFunction<'db>]) {
        match value {
            Rvalue::MakeClosure { lambda_idx, .. } => {
                // Include closure bodies even if their calls cannot be resolved statically.
                if let Some(lambda) = lambdas.get(*lambda_idx) {
                    self.lambda(lambda);
                }
            }
            Rvalue::MakeBoundMethod { func, .. } | Rvalue::MakeGenericFunction { func, .. } => {
                self.function(*func);
            }
            Rvalue::MakeVirtualBoundMethod { method, .. }
            | Rvalue::MakeVirtualFunction { method, .. } => self.dispatch(method),
            Rvalue::Aggregate {
                kind: AggregateKind::Class { class, .. },
                ..
            } => {
                if let Some((object, _)) = self.index.class(self.db, *class) {
                    self.out.classes.insert(object);
                }
            }
            _ => {}
        }
    }

    fn lambda(&mut self, lambda: &MirFunction<'db>) {
        self.out
            .functions
            .extend(self.index.lambda(self.db, &lambda.identity));
        if let MirFunctionKind::Bytecode(body) = &lambda.kind {
            self.body(body, &lambda.lambdas);
        }
    }

    fn terminator(&mut self, terminator: &Terminator<'db>) {
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

/// Transitive reachability; objects absent from `references` end a path.
pub(crate) fn reachable(
    index: &ProgramIndex,
    entries: impl IntoIterator<Item = usize>,
    references: &HashMap<usize, References>,
) -> BTreeSet<usize> {
    reachable_with(entries, references, |method| {
        index
            .methods_named(method)
            .map(|object| (object, index.method_class(object)))
            .collect()
    })
}

/// [`reachable`] over `methods`: a dispatched name's candidate methods, each
/// with the one class that runs it, if one does.
fn reachable_with(
    entries: impl IntoIterator<Item = usize>,
    references: &HashMap<usize, References>,
    methods: impl Fn(&str) -> Vec<(usize, Option<usize>)>,
) -> BTreeSet<usize> {
    let mut walk = Walk {
        queue: entries.into_iter().collect(),
        ..Walk::default()
    };
    while let Some(object) = walk.queue.pop_front() {
        if !walk.reached.insert(object) {
            continue;
        }
        if let Some(refs) = references.get(&object) {
            walk.visit(refs, &methods);
        }
    }
    walk.reached
}

/// The state of [`reachable_with`]'s worklist.
#[derive(Default)]
struct Walk {
    reached: BTreeSet<usize>,
    queue: VecDeque<usize>,
    constructed: BTreeSet<usize>,
    dispatched: BTreeSet<String>,
    /// Dispatched methods whose class is not constructed yet, by class.
    waiting: HashMap<usize, Vec<usize>>,
}

impl Walk {
    fn visit(&mut self, refs: &References, methods: &impl Fn(&str) -> Vec<(usize, Option<usize>)>) {
        self.queue.extend(refs.functions.iter().copied());
        for &class in &refs.classes {
            if self.constructed.insert(class) {
                let ready = self.waiting.remove(&class).into_iter().flatten();
                self.queue.extend(ready);
            }
        }
        for name in &refs.dispatches {
            if self.dispatched.insert(name.clone()) {
                for (method, class) in methods(name) {
                    self.dispatch(method, class);
                }
            }
        }
    }

    fn dispatch(&mut self, method: usize, class: Option<usize>) {
        match class {
            Some(class) if !self.constructed.contains(&class) => {
                self.waiting.entry(class).or_default().push(method);
            }
            _ => self.queue.push_back(method),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(functions: &[usize], dispatches: &[&str], classes: &[usize]) -> References {
        References {
            functions: functions.iter().copied().collect(),
            dispatches: dispatches.iter().map(|name| (*name).to_owned()).collect(),
            classes: classes.iter().copied().collect(),
        }
    }

    #[test]
    fn reachable_is_the_closure_of_the_entries() {
        let references = HashMap::from([
            (1, refs(&[2, 3], &[], &[])),
            (2, refs(&[1], &[], &[])),
            (3, refs(&[4], &[], &[])),
            (5, refs(&[1], &[], &[])),
        ]);
        let none = |_: &str| Vec::new();
        assert_eq!(
            reachable_with([1], &references, none),
            BTreeSet::from([1, 2, 3, 4])
        );
        assert_eq!(reachable_with([4], &references, none), BTreeSet::from([4]));
        assert_eq!(reachable_with([], &references, none), BTreeSet::new());
    }

    #[test]
    fn dispatch_reaches_methods_of_constructed_classes_only() {
        // 1 dispatches `run`, implemented by 10 (class 100) and 11 (class
        // 101); 2 constructs class 101 after the dispatch is seen; 12 has no
        // single class.
        let references =
            HashMap::from([(1, refs(&[2], &["run"], &[])), (2, refs(&[], &[], &[101]))]);
        let methods = |name: &str| match name {
            "run" => vec![(10, Some(100)), (11, Some(101)), (12, None)],
            _ => Vec::new(),
        };
        assert_eq!(
            reachable_with([1], &references, methods),
            BTreeSet::from([1, 2, 11, 12])
        );
    }
}
