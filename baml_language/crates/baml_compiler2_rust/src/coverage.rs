//! Stack-placement coverage over a program: every allocation site of every
//! body, placed or refused with its first cause. For measuring the emitter,
//! not for compiling.
//!
//! Admission runs as [`compile_program`](crate::compile_program) runs it, so
//! the verdicts are the ones the emitter acts on: the fixpoint's final escape
//! facts, which trust a callee's summary only where the callee compiles.

use baml_base::SourceRoot;
use baml_compiler2_hir::{
    item_data::{file_functions, file_lets, is_required_interface_method},
    package::world_files,
};
use baml_compiler2_mir::{
    MirFunction, MirFunctionBody, MirFunctionKind, OptLevel, lower_function, lower_let_body,
};
use bex_vm_types::Program;

use crate::{
    admit::{Unit, admission_fixpoint, lower_reachable},
    codegen::stack::{Refusal, plan},
    index::ProgramIndex,
};

/// One body's allocation sites, in statement order, each placed (its slot)
/// or refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionPlacement {
    /// The source root the body is declared in: a user package or a
    /// standard library package.
    pub root: SourceRoot,
    /// Package-qualified name, including closure bodies.
    pub display: String,
    pub sites: Vec<Result<usize, Refusal>>,
}

/// The placement verdict of every allocation site of every bytecode body
/// the backend compiles from `entries`, as [`compile_program`] takes them.
/// With `None`, every body of the world, stdlib included, is compiled and
/// reported. With entries, only the bodies declared in `root` are reported:
/// the other roots are compiled only as far as the entries reach them. A
/// body's verdicts do not depend on which other bodies compile beyond the
/// callees it reaches, so every function of `root` as an entry gives
/// `root`'s verdicts as `None` does, at the cost of its reachable callees.
///
/// [`compile_program`]: crate::compile_program
pub fn placement_coverage(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entries: Option<&[usize]>,
) -> Result<Vec<FunctionPlacement>, String> {
    crate::analysis::validate_builtin_metadata()?;
    let index = ProgramIndex::new(db, root, program);
    let mut lowered = lower_reachable(db, root, program, &index, opt, entries);
    admission_fixpoint(db, &index, opt, &mut lowered.units, &mut lowered.rejected);
    let walker = Walker {
        db,
        index: &index,
        units: &lowered.units,
        rejected: &lowered.rejected,
        opt,
    };
    let mut out = Vec::new();
    for &file in world_files(db, root) {
        let file_root = file.source_root(db);
        if entries.is_some() && file_root != root {
            continue;
        }
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let Ok(mir) = lower_function(db, function, opt) else {
                continue;
            };
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            let object = index.declared(db, function);
            let name = mir.identity.display_name(db);
            walker.body(
                file_root,
                object,
                name,
                mir.arity,
                body,
                &mir.lambdas,
                &mut out,
            );
        }
        for &binding in file_lets(db, file) {
            let Ok(Some((body, lambdas))) = lower_let_body(db, binding, opt) else {
                continue;
            };
            let name =
                bex_vm_types::init_let_helper_name(&baml_compiler2_mir::definition_display_name(
                    db,
                    baml_compiler2_hir::contributions::Definition::Let(binding),
                ));
            let object = index.let_helper(db, binding);
            walker.body(file_root, object, name, 0, &body, &lambdas, &mut out);
        }
    }
    Ok(out)
}

/// The function objects of every function declared in `root`'s files: the
/// entries that compile all of `root` and what it reaches.
pub fn declared_functions(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
) -> Vec<usize> {
    let index = ProgramIndex::new(db, root, program);
    root.files(db)
        .iter()
        .flat_map(|&file| file_functions(db, file).iter().copied())
        .filter_map(|function| index.declared(db, function))
        .collect()
}

struct Walker<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    index: &'a ProgramIndex<'a>,
    units: &'a std::collections::BTreeMap<usize, Unit<'db>>,
    rejected: &'a [(String, crate::Rejection)],
    opt: OptLevel,
}

impl<'db> Walker<'_, 'db> {
    /// The verdicts of `body` and, recursively, its lambdas.
    #[allow(clippy::too_many_arguments)]
    fn body(
        &self,
        root: SourceRoot,
        object: Option<usize>,
        display: String,
        arity: usize,
        body: &MirFunctionBody<'db>,
        lambdas: &[MirFunction<'db>],
        out: &mut Vec<FunctionPlacement>,
    ) {
        let sites = match object.and_then(|object| self.units.get(&object)) {
            Some(unit) => plan(self.db, self.index, &unit.body, &unit.analysis.escape)
                .1
                .into_iter()
                .map(|verdict| verdict.outcome)
                .collect(),
            None => {
                let reason = self
                    .rejected
                    .iter()
                    .find(|(name, _)| *name == display)
                    .map_or_else(
                        || "not a unit of the program".to_string(),
                        |(_, rejection)| rejection.to_string(),
                    );
                let facts =
                    baml_compiler2_mir::escape::analyze(self.db, root, body, arity, self.opt);
                facts
                    .sites()
                    .map(|_| Err(Refusal::NotCompiled(reason.clone())))
                    .collect()
            }
        };
        out.push(FunctionPlacement {
            root,
            display,
            sites,
        });
        for lambda in lambdas {
            let MirFunctionKind::Bytecode(body) = &lambda.kind else {
                continue;
            };
            let object = self.index.lambda(self.db, &lambda.identity);
            self.body(
                root,
                object,
                lambda.identity.display_name(self.db),
                lambda.arity,
                body,
                &lambda.lambdas,
                out,
            );
        }
    }
}
