//! The functions a program's entry reaches, through calls and through the
//! function values it makes, each lowered to optimized MIR and checked
//! against the native subset.

use std::collections::{HashMap, VecDeque};

use baml_compiler2_hir::loc::FunctionLoc;
use baml_compiler2_mir::{MirFunction, lower_function};

use crate::{
    Blocker,
    check::{OPT, check_function},
    types::{NativeTy, TypeArg},
};

/// How a compiled function is entered.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Entry {
    /// By a call that names it: its parameters, in order.
    Direct,
    /// Through a function value: the closure first, then its parameters.
    Value,
    /// Through a bound method value: the closure first, then the
    /// parameters after the receiver, which the closure holds.
    Bound,
}

/// One compiled function: a declared function or a lambda inside one,
/// entered one way.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct FnKey<'db> {
    pub(crate) loc: FunctionLoc<'db>,
    /// The lambda's position in its parent's `lambdas`, parent by parent
    /// from `loc`; empty for `loc` itself.
    pub(crate) lambdas: Vec<usize>,
    pub(crate) entry: Entry,
    /// The type arguments of the function's frame this copy is compiled
    /// for; a lambda has its parent's.
    pub(crate) arguments: Vec<TypeArg<'db>>,
}

impl<'db> FnKey<'db> {
    pub(crate) fn new(loc: FunctionLoc<'db>, entry: Entry) -> Self {
        Self {
            loc,
            lambdas: Vec::new(),
            entry,
            arguments: Vec::new(),
        }
    }

    pub(crate) fn with_arguments(mut self, arguments: Vec<TypeArg<'db>>) -> Self {
        self.arguments = arguments;
        self
    }

    /// Lambda `index` of this function. A lambda is only ever entered
    /// through its closure.
    pub(crate) fn lambda(&self, index: usize) -> Self {
        let mut lambdas = self.lambdas.clone();
        lambdas.push(index);
        Self {
            loc: self.loc,
            lambdas,
            entry: Entry::Value,
            arguments: self.arguments.clone(),
        }
    }

    pub(crate) fn mir(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
    ) -> Result<&'db MirFunction<'db>, Blocker> {
        let mut mir = lower_function(db, self.loc, OPT)
            .as_ref()
            .map_err(|error| Blocker::new("MIR lowering failure", error.to_string()))?;
        for index in &self.lambdas {
            mir = mir
                .lambdas
                .get(*index)
                .ok_or_else(|| Blocker::new("closure of a lambda that does not exist", ""))?;
        }
        Ok(mir)
    }
}

/// A function some other needs compiled, and what its captured cells hold.
pub(crate) struct Needed<'db> {
    pub(crate) key: FnKey<'db>,
    pub(crate) captures: Vec<NativeTy<'db>>,
}

pub(crate) struct ProgramFunction<'db> {
    pub(crate) key: FnKey<'db>,
    pub(crate) name: String,
    /// `None` when lowering failed; the failure is then the one blocker.
    pub(crate) mir: Option<&'db MirFunction<'db>>,
    /// What the cells a lambda captures hold, in capture order.
    pub(crate) captures: Vec<NativeTy<'db>>,
    pub(crate) blockers: Vec<Blocker>,
}

/// Reachable functions in breadth-first order; the entry is first.
pub(crate) struct Program<'db> {
    pub(crate) functions: Vec<ProgramFunction<'db>>,
    pub(crate) index: HashMap<FnKey<'db>, usize>,
}

pub(crate) fn collect<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    entry: FunctionLoc<'db>,
) -> Program<'db> {
    let mut program = Program {
        functions: Vec::new(),
        index: HashMap::new(),
    };
    let mut queue = VecDeque::from([Needed {
        key: FnKey::new(entry, Entry::Direct),
        captures: Vec::new(),
    }]);
    while let Some(Needed { key, captures }) = queue.pop_front() {
        if let Some(&known) = program.index.get(&key) {
            // Every closure of one lambda captures the same cells.
            let known = &mut program.functions[known];
            if known.captures != captures && known.blockers.is_empty() {
                known.blockers.push(Blocker::new(
                    "closures of one lambda that capture different types",
                    "",
                ));
            }
            continue;
        }
        program.index.insert(key.clone(), program.functions.len());
        let function = match key.mir(db) {
            Ok(mir) => {
                let checked = check_function(db, &key, mir, &captures);
                queue.extend(checked.needed);
                let mut name = mir.identity.display_name(db);
                if !key.arguments.is_empty() {
                    let arguments: Vec<&str> = key
                        .arguments
                        .iter()
                        .map(|argument| argument.as_ref().map_or("_", crate::types::describe))
                        .collect();
                    name = format!("{name}<{}>", arguments.join(", "));
                }
                ProgramFunction {
                    name: match key.entry {
                        Entry::Direct => name,
                        _ if !key.lambdas.is_empty() => name,
                        Entry::Value => format!("{name} as a value"),
                        Entry::Bound => format!("{name} as a bound method"),
                    },
                    key,
                    mir: Some(mir),
                    captures,
                    blockers: checked.blockers,
                }
            }
            Err(blocker) => ProgramFunction {
                name: baml_compiler2_mir::definition_display_name(
                    db,
                    baml_compiler2_hir::contributions::Definition::Function(key.loc),
                ),
                key,
                mir: None,
                captures,
                blockers: vec![blocker],
            },
        };
        program.functions.push(function);
    }
    program
}
