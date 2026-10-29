//! Escape to stack: class instances that provably never leave the function
//! become typed Rust locals instead of heap objects.
//!
//! An allocation `C { ... }` of a class `C` is a *stack object* when, on every
//! path:
//!
//! 1. `C` is not generic, has no `cleanup` finalizer, and every field is a
//!    scalar or immutable value (`int`, `float`, `bool`, `string`, `bigint`,
//!    enums, literals, `null`, and unions of those). Class, array and map
//!    fields disqualify the class.
//! 2. It is never stored into a heap location: no field, element or map
//!    store, no closure capture or cell, no return, no `spawn`, no throw.
//! 3. It is passed only to compiled direct callees whose parameter is itself
//!    a stack object of the callee (this module's summary, a fixpoint over
//!    the program: the callee's body obeys these rules for that parameter),
//!    which never suspend, run without a VM frame, and take it positionally,
//!    or to `==` against `null`, which folds. No builtin takes one.
//! 4. It is never compared (`==` runs the equality driver; BAML has no
//!    `===`, and identity shows only through by-reference builtins such as
//!    `Array.includes`, which rule 3 already excludes) except against
//!    `null`, nor reflected, printed, logged, encoded, or dispatched through
//!    an interface: any use not listed here keeps it on the heap.
//! 5. A stack object lives across suspensions freely: the caller spills its
//!    heap `Value` fields to frame slots and reloads them, like any other live
//!    heap value. A callee that receives one never suspends (rule 3), so a
//!    stack parameter is never live across a suspension.
//!
//! Locals joined by copies form a *group*, all of class `C`, with one storage
//! (`s{group}`, or the `&mut` parameter of a stack variant). A group is a
//! stack object when it holds only allocations of this body (or only its one
//! stack parameter) and, wherever one of its allocations runs again, no other
//! local of the group is live: the old object is dead when its storage is
//! reused.
//!
//! A method or function called with stack objects gets a variant per set of
//! stack parameters (`{symbol}_stack{mask}`), the same body with those
//! parameters as `&mut` structs. It runs in a frame record like the typed
//! direct entry; when the VM requires a VM frame, the caller materializes the
//! object, makes the boxed call and copies the fields back. Telemetry that
//! records the call sees a materialized copy.
//!
//! v1 excludes: class-typed fields (even of stack objects), array and map
//! fields, generic classes, objects passed to builtins, callees that may
//! suspend (including every recursive function, which polls at its entry),
//! and locals that may hold either a stack object or anything else (`C |
//! null`, a parameter, a call result). Everything else stays a heap object
//! with no change in the generated code.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use baml_compiler2_hir_ty::extern_loc::ClassRef;
use baml_compiler2_mir::{
    AggregateKind, Constant, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue, StatementKind,
    Terminator, TypeTest, class_display_name, function_display_name,
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};
use bex_vm_types::{Object, ObjectIndex};

use crate::{
    admit::Unit,
    analysis::{EQUALS_EQUALS, Repr, Signature, operand_ty, user_callee},
    index::ProgramIndex,
    operands::{walk_statement_operands, walk_terminator_operands},
};

/// A class whose instances can live in a Rust struct.
#[derive(Debug, Clone)]
pub(crate) struct StackClass {
    /// The class's object index in the program.
    pub(crate) object: usize,
    /// The generated struct's name.
    pub(crate) name: String,
    /// Per field slot, how the struct holds it.
    pub(crate) fields: Vec<Repr>,
    /// The type tag every instance of it reports.
    pub(crate) tag: i64,
}

/// One stack object of a body: the locals that hold it and where it lives.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub(crate) class: usize,
    /// The Rust place of the struct: `s{n}`, or `(*arg{i})` for a stack
    /// parameter.
    pub(crate) storage: String,
    /// Whether it is allocated here (`s{n}`), not a parameter.
    pub(crate) allocated: bool,
    pub(crate) members: Vec<usize>,
}

/// How one compiled body (a function, or one of its stack variants) holds
/// its stack objects.
#[derive(Debug, Clone, Default)]
pub(crate) struct BodyPlan {
    /// Per local, its group.
    pub(crate) local_group: Vec<Option<usize>>,
    pub(crate) groups: Vec<Group>,
    /// Per block whose call passes stack objects, the callee's variant.
    pub(crate) calls: HashMap<usize, (usize, u64)>,
}

impl BodyPlan {
    pub(crate) fn group(&self, local: usize) -> Option<&Group> {
        self.groups.get((*self.local_group.get(local)?)?)
    }
}

/// Every stack object of a program.
#[derive(Debug, Default)]
pub(crate) struct StackPlans {
    /// Per (function object, stack parameters), its plan; parameters `0` is
    /// the function itself, every other entry a variant some plan calls.
    pub(crate) bodies: BTreeMap<(usize, u64), BodyPlan>,
    /// Classes some plan holds on the stack, by object.
    pub(crate) classes: BTreeMap<usize, StackClass>,
    /// Per function, its allocation sites.
    pub(crate) reports: BTreeMap<usize, Vec<SiteReport>>,
}

/// One class allocation site of a function, for the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteReport {
    /// The class allocated.
    pub class: String,
    /// `None` when it became a stack object; else why it stays on the heap.
    pub heap_because: Option<String>,
}

/// Per function, its class allocation sites and which became stack objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackReport {
    /// Package-qualified function name.
    pub function: String,
    pub sites: Vec<SiteReport>,
}

impl StackReport {
    /// How many sites became stack objects.
    pub fn stack(&self) -> usize {
        self.sites
            .iter()
            .filter(|site| site.heap_because.is_none())
            .count()
    }
}

/// Program facts the plans depend on, from the admission round.
pub(crate) struct Inputs<'a> {
    pub(crate) pausing: &'a BTreeSet<usize>,
    pub(crate) framed: &'a BTreeSet<usize>,
    pub(crate) signatures: &'a HashMap<usize, Signature>,
}

/// Plan the stack objects of every unit.
pub(crate) fn plan<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &BTreeMap<usize, Unit<'db>>,
    inputs: &Inputs<'_>,
) -> StackPlans {
    let mut classes = Classes::default();
    let facts: BTreeMap<usize, Facts> = units
        .iter()
        .map(|(&object, unit)| {
            let facts = Facts::new(db, index, &mut classes, &unit.body, unit.analysis.arity);
            (object, facts)
        })
        .collect();
    // Why each callee cannot take stack arguments, if it cannot.
    let callable = |callee: usize| -> Result<(), String> {
        let name = index.display(callee);
        if !units.contains_key(&callee) {
            return Err(format!("passed to {name}, which is not compiled"));
        }
        if inputs.pausing.contains(&callee) {
            return Err(format!("passed to {name}, which may suspend"));
        }
        if inputs.framed.contains(&callee) {
            return Err(format!("passed to {name}, which runs in a VM frame"));
        }
        if !inputs.signatures.contains_key(&callee) {
            return Err(format!("passed to {name}, which has no direct entry"));
        }
        Ok(())
    };
    // Greatest fixpoint: every group the body allows starts as a stack
    // object; a group falls back to the heap when a call it reaches cannot
    // take it, and a variant dies when one of its stack parameters does.
    let mut states: BTreeMap<(usize, u64), State> = BTreeMap::new();
    for (&object, facts) in &facts {
        states.insert((object, 0), State::new(facts, 0));
    }
    loop {
        let mut changed = false;
        let keys: Vec<(usize, u64)> = states.keys().copied().collect();
        for key in keys {
            let (object, _) = key;
            let body_facts = &facts[&object];
            if states[&key].dead.is_some() {
                continue;
            }
            for call in &body_facts.calls {
                let state = &states[&key];
                let roots: Vec<(usize, usize)> = call
                    .args
                    .iter()
                    .enumerate()
                    .filter_map(|(i, root)| root.filter(|&root| state.stack(root)).map(|r| (i, r)))
                    .collect();
                if roots.is_empty() {
                    continue;
                }
                let mask = roots.iter().fold(0u64, |mask, &(i, _)| mask | (1 << i));
                let failure = match &call.callee {
                    Err(reason) => Some(reason.clone()),
                    Ok(callee) => match callable(*callee) {
                        Err(reason) => Some(reason),
                        Ok(()) => {
                            let variant = (*callee, mask);
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                states.entry(variant)
                            {
                                entry.insert(State::new(&facts[callee], mask));
                                changed = true;
                            }
                            states[&variant].dead.as_ref().map(|reason| {
                                format!("passed to {}, whose {reason}", index.display(*callee))
                            })
                        }
                    },
                };
                if let Some(reason) = failure {
                    let state = states.get_mut(&key).expect("present");
                    for &(_, root) in &roots {
                        state.invalid[root].get_or_insert_with(|| reason.clone());
                    }
                    changed = true;
                }
            }
            let state = states.get_mut(&key).expect("present");
            if state.dead.is_none()
                && let Some(reason) = state.dead_param(body_facts)
            {
                state.dead = Some(reason);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // Keep the variants the functions reach.
    let mut plans = StackPlans::default();
    let mut pending: Vec<(usize, u64)> = facts.keys().map(|&object| (object, 0)).collect();
    while let Some(key) = pending.pop() {
        if plans.bodies.contains_key(&key) {
            continue;
        }
        let (object, params) = key;
        let body_facts = &facts[&object];
        let state = &states[&key];
        let plan = state.plan(body_facts);
        for &(callee, mask) in plan.calls.values() {
            pending.push((callee, mask));
        }
        for group in &plan.groups {
            if let Some(Ok(class)) = classes.by_object.get(&group.class) {
                plans.classes.insert(group.class, class.clone());
            }
        }
        if params == 0 {
            let sites = body_facts
                .sites
                .iter()
                .map(|site| SiteReport {
                    class: site.class.clone(),
                    heap_because: match site.root {
                        Some(root) if state.stack(root) => None,
                        Some(root) => Some(
                            state.invalid[root]
                                .clone()
                                .unwrap_or_else(|| "not a stack object".into()),
                        ),
                        None => Some(site.direct.clone().unwrap_or_default()),
                    },
                })
                .collect();
            plans.reports.insert(object, sites);
        }
        plans.bodies.insert(key, plan);
    }
    plans
}

/// The stack classes resolved so far, by class object.
#[derive(Default)]
struct Classes {
    by_object: HashMap<usize, Result<StackClass, String>>,
}

impl Classes {
    /// The class's object and how it lives on the stack.
    fn resolve<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        class: ClassRef<'db>,
    ) -> (Option<usize>, Result<StackClass, String>) {
        let Some((object, _)) = index.class(db, class) else {
            let name = class_display_name(db, class);
            return (None, Err(format!("class `{name}` is not in the program")));
        };
        let resolved = self
            .by_object
            .entry(object)
            .or_insert_with(|| stack_class(db, index, class))
            .clone();
        (Some(object), resolved)
    }
}

fn stack_class<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    class: ClassRef<'db>,
) -> Result<StackClass, String> {
    let name = class_display_name(db, class);
    let Some((object, field_count)) = index.class(db, class) else {
        return Err(format!("class `{name}` is not in the program"));
    };
    let Object::Class(decl) = &index.program.objects[ObjectIndex::from_raw(object)] else {
        return Err(format!("`{name}` is not a class object"));
    };
    if decl.cleanup.is_some() {
        return Err(format!("`{name}` has a cleanup finalizer"));
    }
    if decl.generic_param_count != 0 {
        return Err(format!("`{name}` is generic"));
    }
    let declared = baml_compiler2_hir_ty::layout::class_fields(db, class);
    if declared.len() != field_count {
        return Err(format!(
            "`{name}` has fields the program lays out differently"
        ));
    }
    let mut fields = Vec::with_capacity(declared.len());
    for (field, ty) in &declared {
        let Some(repr) = field_repr(ty) else {
            return Err(format!("`{name}.{field}` holds a mutable value"));
        };
        fields.push(repr);
    }
    let symbol: String = name
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    Ok(StackClass {
        object,
        name: format!("{symbol}_stack_{object}"),
        fields,
        tag: index.class_tag(object),
    })
}

/// How a stack struct holds a field of type `ty`; `None` for a field that
/// can hold an object with identity (a class, array, map or function).
fn field_repr(ty: &baml_type::Ty) -> Option<Repr> {
    use baml_type::Ty;
    match ty {
        Ty::Int => Some(Repr::Int),
        Ty::Float => Some(Repr::Float),
        Ty::Bool => Some(Repr::Bool),
        _ if immutable(ty) => Some(Repr::Value),
        _ => None,
    }
}

fn immutable(ty: &baml_type::Ty) -> bool {
    use baml_type::Ty;
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(..)
        | Ty::Enum(_)
        | Ty::EnumVariant(..) => true,
        Ty::Union(members) => members.iter().all(immutable),
        _ => false,
    }
}

/// A class allocation site of a body.
struct Site {
    class: String,
    /// The group its destination local joins; `None` when it is not
    /// assigned to a local.
    root: Option<usize>,
    /// Why a site with no group stays on the heap.
    direct: Option<String>,
}

/// A direct call that may pass objects.
struct Call {
    block: usize,
    /// The compiled callee, or why the call cannot take a stack object.
    callee: Result<usize, String>,
    /// Per value argument, the group of a bare local.
    args: Vec<Option<usize>>,
}

/// What one body allows of each group, whatever its callees do.
struct Facts {
    arity: usize,
    /// Per local, its group's root.
    root: Vec<usize>,
    /// Per root, its stack class when every member holds that class.
    class: Vec<Option<StackClass>>,
    /// Per root, why the body itself keeps it on the heap.
    invalid: Vec<Option<String>>,
    /// Per root, whether some allocation site defines it.
    allocated: Vec<bool>,
    /// Per root, the parameters (locals `1..=arity`) it holds.
    params: Vec<Vec<usize>>,
    sites: Vec<Site>,
    calls: Vec<Call>,
}

impl Facts {
    fn new<'db>(
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        classes: &mut Classes,
        body: &MirFunctionBody<'db>,
        arity: usize,
    ) -> Self {
        let n = body.locals.len();
        let mut parent: Vec<usize> = (0..n).collect();
        // Copies join groups.
        for block in &body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(destination),
                    value:
                        Rvalue::Use(
                            Operand::Copy(Place::Local(source))
                            | Operand::Move(Place::Local(source)),
                        ),
                } = &statement.kind
                {
                    union(&mut parent, destination.0, source.0);
                }
            }
            if let Some(Terminator::NarrowBind {
                source: Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source)),
                destination,
                ..
            }) = &block.terminator
            {
                union(&mut parent, destination.0, source.0);
            }
        }
        let root: Vec<usize> = (0..n).map(|local| find(&mut parent, local)).collect();
        let mut facts = Self {
            arity,
            root: root.clone(),
            class: vec![None; n],
            invalid: vec![None; n],
            allocated: vec![false; n],
            params: vec![Vec::new(); n],
            sites: Vec::new(),
            calls: Vec::new(),
        };
        // A group is a candidate when a member is of a class type; every
        // member must be of that one class.
        let mut class_of: Vec<Option<usize>> = vec![None; n];
        let mut candidate = vec![false; n];
        for (local, decl) in body.locals.iter().enumerate() {
            if matches!(decl.ty, RuntimeTy::Class(..)) {
                candidate[root[local]] = true;
            }
        }
        for (local, decl) in body.locals.iter().enumerate() {
            let group = root[local];
            if !candidate[group] {
                continue;
            }
            let reason = match &decl.ty {
                RuntimeTy::Class(name, args) if args.is_empty() => {
                    match baml_compiler2_hir_ty::layout::class_ref_of(db, name) {
                        None => Some("class declared outside the source".into()),
                        Some(class) => match classes.resolve(db, index, class) {
                            (None, Err(reason)) => Some(reason),
                            (Some(object), resolved) => match class_of[group] {
                                Some(known) if known != object => {
                                    Some("holds objects of two classes".into())
                                }
                                Some(_) => None,
                                None => {
                                    class_of[group] = Some(object);
                                    match resolved {
                                        Ok(stack) => {
                                            facts.class[group] = Some(stack);
                                            None
                                        }
                                        Err(reason) => Some(reason),
                                    }
                                }
                            },
                            (None, Ok(_)) => unreachable!("a resolved class has an object"),
                        },
                    }
                }
                RuntimeTy::Class(..) => Some("generic class".into()),
                _ => Some("may also hold a value that is not this class".into()),
            };
            if let Some(reason) = reason {
                facts.escape(local, reason);
            }
            if decl.is_captured {
                facts.escape(local, "captured by a closure".into());
            }
            if local == 0 {
                facts.escape(local, "returned".into());
            } else if local <= arity {
                facts.params[group].push(local);
            }
        }
        for (_, landing) in body.handlers() {
            facts.escape(landing.error_local.0, "caught error".into());
            facts.escape(landing.context_local.0, "caught error".into());
        }
        let builtins_equals = |callee: &Operand<'db>| match callee {
            Operand::Constant(Constant::Function(func)) => index
                .function(db, *func)
                .is_some_and(|(_, info)| info.native_key.as_deref() == Some(EQUALS_EQUALS)),
            _ => false,
        };
        for (b, block) in body.blocks.iter().enumerate() {
            for statement in &block.statements {
                facts.statement(db, index, classes, body, &statement.kind);
            }
            let Some(terminator) = &block.terminator else {
                continue;
            };
            match terminator {
                Terminator::Goto { .. } | Terminator::Unreachable => {}
                Terminator::Return => facts.escape(0, "returned".into()),
                Terminator::NarrowBind {
                    source,
                    test,
                    destination,
                    ..
                } => match source {
                    Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_)) => {
                        if !matches!(test, TypeTest::Class { args, .. } if args.is_empty()) {
                            facts.use_operand(source, "type-tested against a non-class type");
                        }
                    }
                    _ => {
                        facts.use_operand(source, "narrowed");
                        facts.escape(destination.0, "holds a narrowed value".into());
                    }
                },
                Terminator::Call {
                    has_trace,
                    argument_layout,
                    callee,
                    args,
                    ntypeargs,
                    destination,
                    ..
                } => {
                    facts.define(destination, "holds a call result");
                    let null_side = args.len() == 2
                        && *ntypeargs == 0
                        && builtins_equals(callee)
                        && args
                            .iter()
                            .any(|arg| operand_ty(body, arg).is_some_and(|ty| ty.is_null()));
                    if null_side {
                        // `x == null` folds: a stack object is never null.
                        for arg in args {
                            if !matches!(
                                arg,
                                Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_))
                            ) {
                                facts.use_operand(arg, "compared");
                            }
                        }
                        continue;
                    }
                    let name = match callee {
                        Operand::Constant(Constant::Function(func)) => {
                            function_display_name(db, *func)
                        }
                        _ => "a function value".into(),
                    };
                    let positional = argument_layout
                        .as_ref()
                        .is_none_or(|layout| layout.0.iter().all(Option::is_none));
                    let target = match user_callee(db, index, callee) {
                        None => Err(format!("passed to {name}")),
                        Some(_) if *has_trace => Err(format!("passed to {name} with a trace")),
                        Some(_) if !positional => Err(format!("passed to {name} by name")),
                        Some(_) if *ntypeargs != 0 => Err(format!("passed to generic {name}")),
                        Some(object) if index.info(object).arity == args.len() => Ok(object),
                        Some(_) => Err(format!("passed to {name} with defaulted arguments")),
                    };
                    facts.use_operand(callee, "called");
                    let mut seen = Vec::new();
                    let mut values = Vec::with_capacity(args.len());
                    for arg in args {
                        match arg {
                            Operand::Copy(Place::Local(local))
                            | Operand::Move(Place::Local(local)) => {
                                let group = facts.root[local.0];
                                if seen.contains(&group) {
                                    facts.escape(local.0, format!("passed twice to {name}"));
                                }
                                seen.push(group);
                                values.push(Some(group));
                            }
                            other => {
                                facts.use_operand(other, "passed");
                                values.push(None);
                            }
                        }
                    }
                    match &target {
                        Ok(_) => facts.calls.push(Call {
                            block: b,
                            callee: target,
                            args: values,
                        }),
                        Err(reason) => {
                            for group in values.into_iter().flatten() {
                                facts.escape_root(group, reason.clone());
                            }
                        }
                    }
                }
                other => {
                    let reason = match other {
                        Terminator::VirtualCall { method, .. } => {
                            format!("interface call .{method}")
                        }
                        Terminator::SysOp { .. } => "passed to a sys-op".into(),
                        Terminator::Spawn { .. } => "spawned".into(),
                        Terminator::Await { .. } | Terminator::AwaitAny { .. } => "awaited".into(),
                        Terminator::Throw { .. } | Terminator::Rethrow { .. } => "thrown".into(),
                        Terminator::Branch { .. } | Terminator::Switch { .. } => {
                            "branched on".into()
                        }
                        _ => "used by a terminator".into(),
                    };
                    walk_terminator_operands(other, &mut |operand| {
                        facts.use_operand(operand, &reason);
                    });
                    match other {
                        Terminator::Await { future, .. } => facts.use_place(future, &reason),
                        Terminator::Spawn { future, .. } => facts.define(future, "holds a future"),
                        _ => {}
                    }
                    if let Some((destination, _)) = crate::analysis::continuation(other) {
                        facts.define(destination, "holds a call result");
                    }
                    if let Terminator::ShortCircuit { destination, .. } = other {
                        facts.define(destination, "holds a short-circuit result");
                    }
                }
            }
        }
        facts.check_reuse(body);
        facts
    }

    fn escape(&mut self, local: usize, reason: String) {
        let root = self.root[local];
        self.escape_root(root, reason);
    }

    fn escape_root(&mut self, root: usize, reason: String) {
        self.invalid[root].get_or_insert(reason);
    }

    /// `operand` is used as a value in a way that lets an object escape.
    fn use_operand(&mut self, operand: &Operand<'_>, reason: &str) {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                self.escape(local.0, reason.to_string());
            }
            Operand::Copy(place) | Operand::Move(place) => self.use_place(place, reason),
            Operand::Constant(_) => {}
        }
    }

    /// The locals within `place`: a field's base is read in place, anything
    /// else escapes.
    fn use_place(&mut self, place: &Place, reason: &str) {
        match place {
            Place::Field { base, .. } => match base.as_ref() {
                Place::Local(_) => {}
                other => self.use_place(other, reason),
            },
            Place::Local(local) => self.escape(local.0, reason.to_string()),
            Place::Index { base, index, .. } => {
                self.use_place(base, "indexed");
                self.escape(index.0, "used as an index".into());
            }
            Place::Deref(baml_compiler2_mir::CellId::Local(local)) => {
                self.escape(local.0, "held in a cell".into());
            }
            Place::Capture(_) | Place::Deref(_) => {}
        }
    }

    /// `place` is written with something that is not a stack object.
    fn define(&mut self, place: &Place, reason: &str) {
        match place {
            Place::Local(local) => self.escape(local.0, reason.to_string()),
            Place::Field { base, field } => {
                // A boxed result may be an int where the field says float.
                if let Place::Local(local) = base.as_ref() {
                    let root = self.root[local.0];
                    if self.class[root]
                        .as_ref()
                        .is_some_and(|class| class.fields.get(*field) == Some(&Repr::Float))
                    {
                        self.escape_root(
                            root,
                            "a field's value does not fit its declared type".into(),
                        );
                    }
                }
                self.use_place(place, reason);
            }
            other => self.use_place(other, reason),
        }
    }

    fn statement<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        classes: &mut Classes,
        body: &MirFunctionBody<'db>,
        kind: &StatementKind<'db>,
    ) {
        match kind {
            StatementKind::Assign { destination, value } => {
                if let Rvalue::Aggregate {
                    kind:
                        AggregateKind::Class {
                            class,
                            type_arg_templates,
                        },
                    fields,
                } = value
                {
                    let name = class_display_name(db, *class);
                    let Place::Local(local) = destination else {
                        self.sites.push(Site {
                            class: name,
                            root: None,
                            direct: Some("stored into a heap object".into()),
                        });
                        for field in fields {
                            self.use_operand(field, "stored into a field");
                        }
                        self.use_place(destination, "stored");
                        return;
                    };
                    let root = self.root[local.0];
                    self.sites.push(Site {
                        class: name,
                        root: Some(root),
                        direct: None,
                    });
                    self.allocated[root] = true;
                    match classes.resolve(db, index, *class).1 {
                        Err(reason) => self.escape_root(root, reason),
                        Ok(_) if !type_arg_templates.is_empty() => {
                            self.escape_root(root, "generic class".into());
                        }
                        Ok(stack) => {
                            if self.class[root]
                                .as_ref()
                                .is_some_and(|known| known.object != stack.object)
                            {
                                self.escape_root(root, "holds objects of two classes".into());
                            }
                            for (field, repr) in fields.iter().zip(&stack.fields) {
                                if !fits(body, field, *repr) {
                                    self.escape_root(
                                        root,
                                        "a field's value does not fit its declared type".into(),
                                    );
                                }
                            }
                        }
                    }
                    for field in fields {
                        self.use_operand(field, "stored into a field of another object");
                    }
                    return;
                }
                match (destination, value) {
                    // Copies joined the groups.
                    (
                        Place::Local(_),
                        Rvalue::Use(
                            Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_)),
                        ),
                    ) => return,
                    (Place::Local(local), _) => {
                        self.escape(local.0, "holds a value computed here".into());
                    }
                    (Place::Field { base, field }, _) => {
                        if let Place::Local(local) = base.as_ref() {
                            // A store into a stack object's field must fit
                            // the struct's field.
                            let root = self.root[local.0];
                            if let (Some(class), Rvalue::Use(operand)) = (&self.class[root], value)
                                && let Some(&repr) = class.fields.get(*field)
                                && !fits(body, operand, repr)
                            {
                                self.escape_root(
                                    root,
                                    "a field's value does not fit its declared type".into(),
                                );
                            } else if let (Some(class), false) =
                                (&self.class[root], matches!(value, Rvalue::Use(_)))
                                && class.fields.get(*field) == Some(&Repr::Float)
                                && !float_result(body, value)
                            {
                                self.escape_root(
                                    root,
                                    "a field's value does not fit its declared type".into(),
                                );
                            }
                        } else {
                            self.use_place(base, "stored through");
                        }
                    }
                    (other, _) => self.use_place(other, "stored into"),
                }
                match value {
                    Rvalue::IsType {
                        operand: Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_)),
                        test: TypeTest::Class { args, .. },
                    } if args.is_empty() => {}
                    Rvalue::TypeTag(Place::Local(_)) => {}
                    Rvalue::Use(operand) => {
                        let reason = match destination {
                            Place::Local(_) => "copied",
                            _ => "stored into a heap object",
                        };
                        self.use_operand(operand, reason);
                    }
                    Rvalue::Array(..) | Rvalue::Map(..) => {
                        walk_statement_operands(kind, &mut |operand| {
                            self.use_operand(operand, "stored into a container");
                        });
                    }
                    Rvalue::MakeClosure { .. } => walk_statement_operands(kind, &mut |operand| {
                        self.use_operand(operand, "captured by a closure");
                    }),
                    Rvalue::MakeBoundMethod { .. } | Rvalue::MakeVirtualBoundMethod { .. } => {
                        walk_statement_operands(kind, &mut |operand| {
                            self.use_operand(operand, "bound as a method receiver");
                        });
                    }
                    Rvalue::BinaryOp { .. } => walk_statement_operands(kind, &mut |operand| {
                        self.use_operand(operand, "compared by identity or operated on");
                    }),
                    _ => {
                        walk_statement_operands(kind, &mut |operand| {
                            self.use_operand(operand, "used as a value");
                        });
                        walk_rvalue_places(value, &mut |place| {
                            if matches!(place, Place::Local(_)) {
                                self.use_place(place, "inspected");
                            }
                        });
                    }
                }
            }
            StatementKind::Drop(_) | StatementKind::Nop => {}
            StatementKind::FreshCell { local, .. } => {
                self.escape(local.0, "captured by a closure".into());
            }
            StatementKind::Intrinsic { op, .. } => {
                let reason = match op {
                    baml_compiler2_mir::IntrinsicOp::Log(_) => "logged",
                    _ => "passed to a trace hook",
                };
                walk_statement_operands(kind, &mut |operand| self.use_operand(operand, reason));
            }
            StatementKind::VirtualFieldStore { .. } => {
                walk_statement_operands(kind, &mut |operand| {
                    self.use_operand(operand, "accessed through an interface");
                });
            }
        }
    }

    /// An allocation reuses its group's storage: every other member must be
    /// dead when it runs, or a live alias would see the new object.
    fn check_reuse(&mut self, body: &MirFunctionBody<'_>) {
        let candidates: BTreeSet<usize> = (0..self.root.len())
            .filter(|&root| self.allocated[root] && self.invalid[root].is_none())
            .collect();
        if candidates.is_empty() {
            return;
        }
        let live_in = baml_compiler2_mir::live_in_precise(body);
        for block in &body.blocks {
            let mut live = live_out(body, block, &live_in);
            let exceptional: Vec<bool> = block.unwind.map_or_else(
                || vec![false; self.root.len()],
                |handler| live_in[handler.0].clone(),
            );
            for statement in block.statements.iter().rev() {
                if let StatementKind::Assign {
                    destination: Place::Local(destination),
                    value: Rvalue::Aggregate { .. },
                } = &statement.kind
                {
                    let root = self.root[destination.0];
                    if candidates.contains(&root)
                        && (0..live.len()).any(|local| {
                            live[local] && local != destination.0 && self.root[local] == root
                        })
                    {
                        self.escape_root(
                            root,
                            "an alias of its previous object is live when it is allocated again"
                                .into(),
                        );
                    }
                }
                transfer(&statement.kind, &mut live);
                for (live, exceptional) in live.iter_mut().zip(&exceptional) {
                    *live |= *exceptional;
                }
            }
        }
    }
}

/// Whether `operand`'s value can be held in a field of `repr` as the
/// generator converts it.
fn fits(body: &MirFunctionBody<'_>, operand: &Operand<'_>, repr: Repr) -> bool {
    let from = match operand {
        Operand::Copy(Place::Local(_)) | Operand::Move(Place::Local(_)) | Operand::Constant(_) => {
            operand_ty(body, operand).map_or(Repr::Value, |ty| Repr::of(&ty))
        }
        _ => Repr::Value,
    };
    match repr {
        Repr::Value => true,
        // A float field may hold a value the VM keeps as an int; only a
        // float converts without a check.
        Repr::Float => from == Repr::Float,
        Repr::Int | Repr::Bool => from == repr || from == Repr::Value,
    }
}

/// Whether `value` computes a float the generator holds unboxed.
fn float_result(body: &MirFunctionBody<'_>, value: &Rvalue<'_>) -> bool {
    match value {
        Rvalue::BinaryOp { left, right, .. } => {
            fits(body, left, Repr::Float) && fits(body, right, Repr::Float)
        }
        Rvalue::UnaryOp { operand, .. } => fits(body, operand, Repr::Float),
        _ => false,
    }
}

/// Over-approximate liveness after a block's last statement: what its
/// successors and handler read, and everything its terminator mentions.
fn live_out(
    body: &MirFunctionBody<'_>,
    block: &baml_compiler2_mir::BasicBlock<'_>,
    live_in: &[Vec<bool>],
) -> Vec<bool> {
    let mut live = vec![false; body.locals.len()];
    if let Some(terminator) = &block.terminator {
        for successor in terminator.successors() {
            for (live, &into) in live.iter_mut().zip(&live_in[successor.0]) {
                *live |= into;
            }
        }
        let mut reads = Resources::default();
        walk_terminator_operands(terminator, &mut |operand| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        });
        match terminator {
            Terminator::Return => live[0] = true,
            Terminator::NarrowBind { destination, .. } => live[destination.0] = true,
            _ => {}
        }
        if let Some((destination, _)) = crate::analysis::continuation(terminator) {
            place_reads(destination, &mut reads);
        }
        for local in reads.locals {
            live[local.0] = true;
        }
    }
    if let Some(handler) = block.unwind {
        for (live, &into) in live.iter_mut().zip(&live_in[handler.0]) {
            *live |= into;
        }
    }
    live
}

/// Liveness across one statement, backwards: a local store kills, every
/// mentioned local is read.
fn transfer(statement: &StatementKind<'_>, live: &mut [bool]) {
    let mut reads = Resources::default();
    match statement {
        StatementKind::Assign { destination, value } => {
            match destination {
                Place::Local(local) => live[local.0] = false,
                other => place_reads(other, &mut reads),
            }
            walk_rvalue_places(value, &mut |place| place_reads(place, &mut reads));
        }
        StatementKind::Drop(place) => place_reads(place, &mut reads),
        StatementKind::FreshCell { local, .. } => {
            live[local.0] = true;
        }
        _ => walk_statement_operands(statement, &mut |operand| {
            walk_operand_places(operand, &mut |place| place_reads(place, &mut reads));
        }),
    }
    for local in reads.locals {
        live[local.0] = true;
    }
}

fn find(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (a, b) = (find(parent, a), find(parent, b));
    if a != b {
        parent[b.max(a)] = a.min(b);
    }
}

/// Which groups of one body are stack objects, under one set of stack
/// parameters.
struct State {
    /// Per root, why it stays on the heap.
    invalid: Vec<Option<String>>,
    /// Per root, whether it is a candidate at all.
    candidate: Vec<bool>,
    /// Why this variant cannot exist.
    dead: Option<String>,
    /// The parameters this variant takes as stack objects.
    params: u64,
}

impl State {
    fn new(facts: &Facts, params: u64) -> Self {
        let n = facts.root.len();
        let mut invalid = facts.invalid.clone();
        let candidate: Vec<bool> = (0..n)
            .map(|root| facts.root[root] == root && facts.class[root].is_some())
            .collect();
        for root in 0..n {
            if !candidate[root] && facts.root[root] == root {
                invalid[root].get_or_insert_with(|| "not a stack class".into());
            }
            let stack_params: Vec<usize> = facts.params[root]
                .iter()
                .copied()
                .filter(|&local| params & (1 << (local - 1)) != 0)
                .collect();
            if facts.params[root].len() > stack_params.len() {
                invalid[root].get_or_insert_with(|| "may hold a parameter".into());
            } else if stack_params.len() > 1 {
                invalid[root].get_or_insert_with(|| "may hold either of two parameters".into());
            } else if stack_params.len() == 1 && facts.allocated[root] {
                invalid[root].get_or_insert_with(|| "may hold a parameter or a new object".into());
            } else if stack_params.is_empty() && !facts.allocated[root] {
                invalid[root].get_or_insert_with(|| "never allocated here".into());
            }
        }
        let mut state = Self {
            params,
            invalid,
            candidate,
            dead: None,
        };
        // A stack parameter must not be a parameter beyond the arity.
        for i in 0..64 {
            if params & (1 << i) != 0 && i >= facts.arity {
                state.dead = Some("no such parameter".into());
            }
        }
        state
    }

    fn stack(&self, root: usize) -> bool {
        self.candidate.get(root).copied().unwrap_or(false) && self.invalid[root].is_none()
    }

    /// Why a stack parameter of this variant is not a stack object, if one
    /// is not.
    fn dead_param(&self, facts: &Facts) -> Option<String> {
        (0..facts.arity.min(64)).find_map(|i| {
            let root = facts.root[i + 1];
            (self.params & (1 << i) != 0 && !self.stack(root)).then(|| {
                format!(
                    "parameter {} escapes: {}",
                    i + 1,
                    self.invalid[root]
                        .as_deref()
                        .unwrap_or("not a stack object")
                )
            })
        })
    }

    fn plan(&self, facts: &Facts) -> BodyPlan {
        let n = facts.root.len();
        let mut plan = BodyPlan {
            local_group: vec![None; n],
            ..BodyPlan::default()
        };
        let mut ids: HashMap<usize, usize> = HashMap::new();
        for local in 0..n {
            let root = facts.root[local];
            if !self.stack(root) {
                continue;
            }
            let id = *ids.entry(root).or_insert_with(|| {
                let id = plan.groups.len();
                let class = facts.class[root].as_ref().expect("candidate").object;
                let storage = match facts.params[root].first() {
                    Some(&param) => format!("(*arg{})", param - 1),
                    None => format!("s{id}"),
                };
                plan.groups.push(Group {
                    class,
                    storage,
                    allocated: facts.params[root].is_empty(),
                    members: Vec::new(),
                });
                id
            });
            plan.groups[id].members.push(local);
            plan.local_group[local] = Some(id);
        }
        for call in &facts.calls {
            let Ok(callee) = call.callee else { continue };
            let mask = call
                .args
                .iter()
                .enumerate()
                .filter(|(_, root)| root.is_some_and(|root| self.stack(root)))
                .fold(0u64, |mask, (i, _)| mask | (1 << i));
            if mask != 0 {
                plan.calls.insert(call.block, (callee, mask));
            }
        }
        plan
    }
}
