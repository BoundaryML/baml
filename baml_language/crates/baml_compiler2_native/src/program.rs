//! The functions a program's entry reaches through direct calls, each
//! lowered to optimized MIR and checked against the native subset.

use std::collections::{HashMap, VecDeque};

use baml_compiler2_hir::loc::FunctionLoc;
use baml_compiler2_mir::{MirFunction, lower_function};

use crate::{
    Blocker,
    check::{OPT, check_function},
};

pub(crate) struct ProgramFunction<'db> {
    pub(crate) loc: FunctionLoc<'db>,
    pub(crate) name: String,
    /// `None` when lowering failed; the failure is then the one blocker.
    pub(crate) mir: Option<&'db MirFunction<'db>>,
    pub(crate) blockers: Vec<Blocker>,
}

/// Reachable functions in breadth-first order; the entry is first.
pub(crate) struct Program<'db> {
    pub(crate) functions: Vec<ProgramFunction<'db>>,
    pub(crate) index: HashMap<FunctionLoc<'db>, usize>,
}

pub(crate) fn collect<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    entry: FunctionLoc<'db>,
) -> Program<'db> {
    let mut program = Program {
        functions: Vec::new(),
        index: HashMap::new(),
    };
    let mut queue = VecDeque::from([entry]);
    while let Some(loc) = queue.pop_front() {
        if program.index.contains_key(&loc) {
            continue;
        }
        program.index.insert(loc, program.functions.len());
        let function = match lower_function(db, loc, OPT) {
            Ok(mir) => {
                let checked = check_function(db, mir);
                queue.extend(checked.callees);
                ProgramFunction {
                    loc,
                    name: mir.identity.display_name(db),
                    mir: Some(mir),
                    blockers: checked.blockers,
                }
            }
            Err(error) => ProgramFunction {
                loc,
                name: baml_compiler2_mir::definition_display_name(
                    db,
                    baml_compiler2_hir::contributions::Definition::Function(loc),
                ),
                mir: None,
                blockers: vec![Blocker::new("MIR lowering failure", error.to_string())],
            },
        };
        program.functions.push(function);
    }
    program
}
