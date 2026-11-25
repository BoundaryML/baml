//! The functions a program's entry reaches, through calls and through the
//! function values it makes, each lowered to optimized MIR and checked
//! against the native subset.

use std::collections::{HashMap, VecDeque};

use baml_compiler2_hir::loc::FunctionLoc;
use baml_compiler2_hir_ty::extern_loc::ClassRef;
use baml_compiler2_mir::{MirFunction, lower_function};

use crate::{
    Blocker,
    check::{Dispatched, OPT, check_function, function_signature, identified_arguments},
    dispatch::{self, Slot, Target},
    types::{NativeTy, Signature, TypeArg, assignable, type_argument},
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
    dispatched: Vec<Dispatched<'db>>,
    constructed: Vec<NativeTy<'db>>,
    /// Whether it has the runtime compare two values.
    compares: bool,
}

/// One interface method that some call dispatches when it runs.
pub(crate) struct SlotInfo<'db> {
    pub(crate) slot: Slot,
    /// Its column in the method table. Methods no one type has both of
    /// share a column.
    pub(crate) column: usize,
    /// The parameters after the receiver, and the result, of every
    /// implementation.
    pub(crate) signature: Signature<'db>,
    /// Whether some type leaves the method to the language's own
    /// `to_string`, for which its row holds no code.
    pub(crate) falls_back: bool,
}

/// The code a type has for a dispatched method.
#[derive(Clone)]
pub(crate) enum Code<'db> {
    /// A function of the program.
    Function(FnKey<'db>),
    /// An `async` native of the runtime, entered through its stub, which
    /// is called as a function of the program is.
    Native(&'static crate::natives::Native),
}

/// The signature the stub of `native` is entered with by a dispatching
/// call, without its receiver, when the method table can hold it: the
/// table's entry passes the stub what a call of the native would, but for
/// the description of a static type, which a dispatching call has none of.
fn dispatched_native<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    native: &'static crate::natives::Native,
) -> Option<Signature<'db>> {
    use crate::natives::Ty;
    let (signature, flags) = native.callable().ok()?;
    let fits = flags.is_async
        && native.type_args == 0
        && !signature
            .params
            .iter()
            .any(|ty| matches!(ty, Ty::Typed | Ty::Null));
    let (_, params) = signature.params.split_first()?;
    fits.then(|| Signature {
        params: params.iter().map(|ty| ty.native(db, &[])).collect(),
        result: signature.result.native(db, &[]),
    })
}

/// Reachable functions in breadth-first order; the entry is first.
pub(crate) struct Program<'db> {
    pub(crate) functions: Vec<ProgramFunction<'db>>,
    pub(crate) index: HashMap<FnKey<'db>, usize>,
    pub(crate) slots: Vec<SlotInfo<'db>>,
    /// The method table's rows: for a type, the function behind each slot
    /// (by position in `slots`) it implements in source.
    pub(crate) rows: Vec<(NativeTy<'db>, Vec<(usize, Code<'db>)>)>,
    /// Every type an interface value can hold: strings, the boxed scalars,
    /// and each enum and class instantiation the program makes values of.
    pub(crate) types: Vec<NativeTy<'db>>,
    /// A row is `8 << column_shift` bytes: the columns, rounded up to a
    /// power of two.
    pub(crate) column_shift: u8,
    /// The classes the program makes values of that have a `to_json` or a
    /// `from_json` of their own, with those functions: what `baml.json`
    /// calls when it writes or reads one.
    pub(crate) hooks: Vec<(ClassRef<'db>, crate::json::Hooks<FnKey<'db>>)>,
    /// The classes the program makes values of that have an `eq` of their
    /// own, with it: what the runtime calls when it compares two instances
    /// it meets inside other values.
    pub(crate) equals: Vec<(ClassRef<'db>, FnKey<'db>)>,
    /// The generic classes and the enums the program makes values of that
    /// have an `eq` of their own, which the runtime has no way to call: a
    /// program that has one cannot ask the runtime to compare two values.
    pub(crate) equals_unlisted: Vec<String>,
}

/// One interface method some call dispatches when it runs, at one set of
/// the method's own type arguments.
struct DispatchedMethod<'db> {
    slot: Slot,
    interface: baml_compiler2_mir::TyTemplateInterface,
    method: String,
    type_arguments: Vec<TypeArg<'db>>,
}

/// A type as a row of the method table is for it: instantiations of a
/// class that share a type id are one.
fn row_type<'db>(ty: &NativeTy<'db>) -> NativeTy<'db> {
    match ty {
        NativeTy::Class(class, arguments) => NativeTy::Class(
            *class,
            arguments.iter().cloned().map(type_argument).collect(),
        ),
        other => other.clone(),
    }
}

pub(crate) fn collect<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    entry: FunctionLoc<'db>,
) -> Program<'db> {
    let mut program = Program {
        functions: Vec::new(),
        index: HashMap::new(),
        slots: Vec::new(),
        rows: Vec::new(),
        types: Vec::new(),
        column_shift: 0,
        hooks: Vec::new(),
        equals: Vec::new(),
        equals_unlisted: Vec::new(),
    };
    let mut queue = VecDeque::from([Needed {
        key: FnKey::new(entry, Entry::Direct),
        captures: Vec::new(),
    }]);
    // Each dispatched method, and what it runs on each type an interface
    // value can hold. An implementation is itself a function to compile,
    // which can make more types and dispatch more methods.
    let mut calls: Vec<DispatchedMethod<'db>> = Vec::new();
    let mut resolved: Vec<(usize, NativeTy<'db>, Option<Target<'db>>)> = Vec::new();
    let mut unsupported = Vec::new();
    // The classes whose own `to_json` and `from_json` have been looked for.
    let mut hooked: Vec<ClassRef<'db>> = Vec::new();
    loop {
        visit(db, &mut program, &mut queue);
        let mut types = vec![
            NativeTy::String,
            NativeTy::Int,
            NativeTy::Float,
            NativeTy::Bool,
        ];
        for function in &program.functions {
            for ty in function.constructed.iter().map(row_type) {
                if !types.contains(&ty) {
                    types.push(ty);
                }
            }
            for call in &function.dispatched {
                if !calls.iter().any(|known| known.slot == call.slot) {
                    calls.push(DispatchedMethod {
                        slot: call.slot.clone(),
                        interface: call.interface.clone(),
                        method: call.method.clone(),
                        type_arguments: call.type_arguments.clone(),
                    });
                }
            }
        }
        for (index, call) in calls.iter().enumerate() {
            for ty in &types {
                if resolved
                    .iter()
                    .any(|(slot, known, _)| *slot == index && known == ty)
                {
                    continue;
                }
                let target = dispatch::resolve(
                    db,
                    ty,
                    &call.interface,
                    &call.method,
                    &[],
                    &call.type_arguments,
                )
                .unwrap_or_else(|blocker| {
                    unsupported.push(blocker);
                    None
                });
                if let Some(Target::Function(key)) = &target {
                    queue.push_back(Needed {
                        key: key.clone(),
                        captures: Vec::new(),
                    });
                }
                resolved.push((index, ty.clone(), target));
            }
        }
        // A class's own `to_json` and `from_json` are functions to compile
        // too: the runtime calls them, where no call in the program does.
        for ty in &types {
            let unlisted = match ty {
                NativeTy::Class(class, arguments) if !arguments.is_empty() => {
                    crate::json::hook_functions(db, *class)
                        .1
                        .map(|_| crate::json::full_class_name(db, *class))
                }
                NativeTy::Enum(enumeration) => crate::json::enum_eq(db, *enumeration).map(|_| {
                    baml_compiler2_hir_ty::layout::enum_head(db, *enumeration)
                        .name()
                        .to_string()
                }),
                _ => None,
            };
            if let Some(name) = unlisted
                && !program.equals_unlisted.contains(&name)
            {
                program.equals_unlisted.push(name);
            }
            let NativeTy::Class(class, arguments) = ty else {
                continue;
            };
            if !arguments.is_empty() {
                continue;
            }
            if hooked.contains(class) {
                continue;
            }
            hooked.push(*class);
            let (functions, eq) = crate::json::hook_functions(db, *class);
            let functions = functions
                .map(|function| function.map(|function| FnKey::new(function, Entry::Direct)));
            let eq = eq.map(|function| FnKey::new(function, Entry::Direct));
            if let Some(eq) = &eq {
                program.equals.push((*class, eq.clone()));
            }
            for key in functions.iter().flatten().chain(&eq) {
                queue.push_back(Needed {
                    key: key.clone(),
                    captures: Vec::new(),
                });
            }
            if functions.iter().any(Option::is_some) {
                program.hooks.push((*class, functions));
            }
        }
        if queue.is_empty() {
            program.types = types;
            break;
        }
    }
    unsupported.extend(lay_out_methods(db, &mut program, &calls, &resolved));
    unsupported.extend(check_hooks(db, &program));
    // The runtime's walk of two values compares what it meets of these by
    // variant or by field, which is not what their `eq` says.
    if program.functions.iter().any(|function| function.compares) {
        unsupported.extend(program.equals_unlisted.iter().map(|name| {
            Blocker::new(
                "comparing values in a program with a type whose own eq the runtime cannot call",
                name.clone(),
            )
        }));
    }
    if let Some(entry) = program.functions.first_mut() {
        entry.blockers.extend(unsupported);
    }
    program
}

/// What keeps the runtime from calling a class's own `to_json` or
/// `from_json`: it is entered with one value held as a union holds it, and
/// returns one held the same way.
fn check_hooks<'db>(db: &'db dyn baml_compiler2_mir::Db, program: &Program<'db>) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    for (class, functions) in &program.hooks {
        for (which, key) in functions.iter().enumerate() {
            let Some(key) = key else { continue };
            // Reported as its own blocked function.
            let Ok(signature) = function_signature(db, key) else {
                continue;
            };
            let own = NativeTy::Class(*class, Vec::new());
            let (takes, returns) = if which == 0 {
                (
                    signature.params[..] == [own.clone()],
                    signature.result.holds_any(),
                )
            } else {
                (
                    matches!(&signature.params[..], [json] if json.holds_any()),
                    signature.result == own,
                )
            };
            if !takes || !returns || identified_arguments(db, key).contains(&true) {
                blockers.push(Blocker::new(
                    "to_json or from_json with a layout the runtime cannot call",
                    crate::types::describe(&own),
                ));
            }
        }
    }
    // An `eq` is entered with two instances and answers with a `bool`.
    for (class, key) in &program.equals {
        let Ok(signature) = function_signature(db, key) else {
            continue;
        };
        let own = NativeTy::Class(*class, Vec::new());
        if signature.params[..] != [own.clone(), own.clone()]
            || signature.result != NativeTy::Bool
            || identified_arguments(db, key).contains(&true)
        {
            blockers.push(Blocker::new(
                "eq with a layout the runtime cannot call",
                crate::types::describe(&own),
            ));
        }
    }
    blockers
}

/// Give each dispatched method its signature and its column, each type its
/// row, and check every dispatching call against the signature.
fn lay_out_methods<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    program: &mut Program<'db>,
    calls: &[DispatchedMethod<'db>],
    resolved: &[(usize, NativeTy<'db>, Option<Target<'db>>)],
) -> Vec<Blocker> {
    let mut blockers = Vec::new();
    let mut implementors: Vec<Vec<&NativeTy<'db>>> = Vec::new();
    for (index, DispatchedMethod { slot, method, .. }) in calls.iter().enumerate() {
        let unsupported =
            |detail: &str| Blocker::new("interface method call", format!("{method}: {detail}"));
        let mut signature = None;
        let mut falls_back = false;
        let mut with_code = Vec::new();
        for (_, ty, target) in resolved.iter().filter(|(slot, ..)| *slot == index) {
            let code = match target {
                Some(Target::Function(key)) => Code::Function(key.clone()),
                Some(Target::ToString) => {
                    falls_back = true;
                    continue;
                }
                // An array's header does not say what its elements are.
                Some(Target::Sort(_)) => {
                    blockers.push(unsupported("of an array only known when it runs"));
                    continue;
                }
                Some(Target::Native(native)) => Code::Native(native),
                None => continue,
            };
            let found = match &code {
                Code::Function(key) => {
                    if identified_arguments(db, key).contains(&true) {
                        blockers.push(unsupported("a default body that tests its type argument"));
                        continue;
                    }
                    let Ok(mut found) = function_signature(db, key) else {
                        // Reported as its own blocked function.
                        continue;
                    };
                    if found.params.is_empty() {
                        blockers.push(unsupported("without a receiver"));
                        continue;
                    }
                    found.params.remove(0);
                    found
                }
                // Only the stub of an `async` native is called as the
                // table's functions are.
                Code::Native(native) => match dispatched_native(db, native) {
                    Some(found) => found,
                    None => {
                        blockers.push(unsupported(
                            "implemented in Rust for a type only known when it runs",
                        ));
                        continue;
                    }
                },
            };
            match &signature {
                None => signature = Some(found),
                Some(known) if !same_registers(known, &found) => {
                    blockers.push(unsupported("implementations with different layouts"));
                    continue;
                }
                Some(_) => {}
            }
            with_code.push(ty);
            match program.rows.iter_mut().find(|(row, _)| row == ty) {
                Some((_, methods)) => methods.push((index, code)),
                None => program.rows.push((ty.clone(), vec![(index, code)])),
            }
        }
        let to_string = Slot::to_string_signature();
        if falls_back && signature.as_ref().is_some_and(|known| *known != to_string) {
            blockers.push(unsupported("implementations with different layouts"));
        }
        // Methods share a column when no type has code for both.
        let column = (0..=program.slots.len())
            .find(|column| {
                !program
                    .slots
                    .iter()
                    .zip(&implementors)
                    .any(|(other, theirs)| {
                        other.column == *column && theirs.iter().any(|ty| with_code.contains(ty))
                    })
            })
            .unwrap_or(0);
        implementors.push(with_code);
        program.slots.push(SlotInfo {
            slot: slot.clone(),
            column,
            signature: signature.unwrap_or(to_string),
            falls_back,
        });
    }
    let columns = program.slots.iter().map(|slot| slot.column + 1).max();
    program.column_shift = columns.map_or(0, |columns| {
        u8::try_from(columns.next_power_of_two().trailing_zeros()).unwrap_or(u8::MAX)
    });
    for function in &mut program.functions {
        for call in &function.dispatched {
            let Some(slot) = program.slots.iter().find(|slot| slot.slot == call.slot) else {
                continue;
            };
            let fits = slot.signature.params.len() == call.arguments.len()
                && slot
                    .signature
                    .params
                    .iter()
                    .zip(&call.arguments)
                    .all(|(expected, found)| assignable(expected, found))
                && assignable(&call.result, &slot.signature.result);
            if !fits {
                function.blockers.push(Blocker::new(
                    "call that does not match its callee's layout",
                    call.method.clone(),
                ));
            }
        }
    }
    blockers
}

/// Whether a call written for one signature can enter code compiled for
/// the other through the method table: each argument and the result is the
/// same type, or a reference either way, which the type checker has let
/// through.
fn same_registers(left: &Signature<'_>, right: &Signature<'_>) -> bool {
    let alike = |left: &NativeTy<'_>, right: &NativeTy<'_>| {
        let reference = |ty: &NativeTy<'_>| ty.is_ref() && !matches!(ty, NativeTy::Cell(_));
        left == right || (reference(left) && reference(right))
    };
    left.params.len() == right.params.len()
        && left
            .params
            .iter()
            .zip(&right.params)
            .all(|(left, right)| alike(left, right))
        && alike(&left.result, &right.result)
}

/// Check and record every function waiting in `queue`, and those they need.
fn visit<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    program: &mut Program<'db>,
    queue: &mut VecDeque<Needed<'db>>,
) {
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
                    dispatched: checked.dispatched,
                    constructed: checked.constructed,
                    compares: checked.compares,
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
                dispatched: Vec::new(),
                constructed: Vec::new(),
                compares: false,
            },
        };
        program.functions.push(function);
    }
}
