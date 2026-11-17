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

use std::collections::{BTreeMap, BTreeSet, HashMap, btree_map::Entry};

use baml_compiler2_hir_ty::extern_loc::ClassRef;
use baml_compiler2_mir::{
    AggregateKind, Constant, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue, StatementKind,
    Terminator, TypeTest, class_display_name,
    escape::{MASKED_PARAMS, immutable_ty},
    function_display_name,
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};
use bex_vm_types::{Object, ObjectIndex};

use crate::{
    admit::Unit,
    analysis::{Repr, Signature, null_compared, operand_ty, user_callee},
    index::ProgramIndex,
    operands::{walk_statement_operands, walk_terminator_operands},
};

/// The most stack variants one function gets; past it, callers keep
/// those arguments on the heap.
const MAX_STACK_VARIANTS: usize = 8;

/// Whether parameter mask `mask` takes parameter `i` (0-based) as a stack
/// struct. A parameter beyond the mask's width never is.
pub(crate) fn takes_param(mask: u64, i: usize) -> bool {
    i < MASKED_PARAMS && mask & (1 << i) != 0
}

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

const CAPTURED: &str = "captured by a closure";
const FIELD_DOES_NOT_FIT: &str = "a field's value does not fit its declared type";

/// Why passing an object to `callee` keeps it on the heap.
fn passed(callee: impl std::fmt::Display, how: &str) -> String {
    format!("passed to {callee}{how}")
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
    let states = Fixpoint {
        index,
        units,
        inputs,
        facts: &facts,
        states: BTreeMap::new(),
        changed: false,
    }
    .run();
    collect_plans(&facts, &states, &classes)
}

/// Why `callee` cannot take stack arguments, if it cannot.
fn callee_rejection(
    index: &ProgramIndex,
    units: &BTreeMap<usize, Unit<'_>>,
    inputs: &Inputs<'_>,
    callee: usize,
) -> Option<String> {
    let rejections = [
        (!units.contains_key(&callee), ", which is not compiled"),
        (inputs.pausing.contains(&callee), ", which may suspend"),
        (
            inputs.framed.contains(&callee),
            ", which runs in a VM frame",
        ),
        (
            !inputs.signatures.contains_key(&callee),
            ", which has no direct entry",
        ),
    ];
    let (_, how) = rejections.into_iter().find(|&(rejected, _)| rejected)?;
    Some(passed(index.display(callee), how))
}

/// Greatest fixpoint: every group the body allows starts as a stack object;
/// a group falls back to the heap when a call it reaches cannot take it, and
/// a variant dies when one of its stack parameters does.
struct Fixpoint<'a, 'db> {
    index: &'a ProgramIndex<'a>,
    units: &'a BTreeMap<usize, Unit<'db>>,
    inputs: &'a Inputs<'a>,
    facts: &'a BTreeMap<usize, Facts>,
    /// Per (function object, stack parameters), its state.
    states: BTreeMap<(usize, u64), State>,
    changed: bool,
}

impl Fixpoint<'_, '_> {
    fn run(mut self) -> BTreeMap<(usize, u64), State> {
        for (&object, facts) in self.facts {
            self.states.insert((object, 0), State::new(facts, 0));
        }
        loop {
            self.changed = false;
            let keys: Vec<(usize, u64)> = self.states.keys().copied().collect();
            for key in keys {
                self.visit(key);
            }
            if !self.changed {
                return self.states;
            }
        }
    }

    /// One pass over the calls of one body under one set of stack parameters.
    fn visit(&mut self, key: (usize, u64)) {
        let facts = &self.facts[&key.0];
        if self.states[&key].dead.is_some() {
            return;
        }
        for call in &facts.calls {
            let mask = self.states[&key].mask(call);
            if mask == 0 {
                continue;
            }
            let Some(reason) = self.call_failure(call.callee, mask) else {
                continue;
            };
            let state = self.states.get_mut(&key).expect("present");
            let roots: Vec<usize> = state.stack_args(call).map(|(_, root)| root).collect();
            for root in roots {
                state.invalid[root].get_or_insert_with(|| reason.clone());
            }
            self.changed = true;
        }
        let state = self.states.get_mut(&key).expect("present");
        if let Some(reason) = state.dead_param(facts) {
            state.dead = Some(reason);
            self.changed = true;
        }
    }

    /// Why `callee` cannot take the stack parameters `mask`, if it cannot;
    /// creates the variant it would call.
    fn call_failure(&mut self, callee: usize, mask: u64) -> Option<String> {
        if let Some(reason) = callee_rejection(self.index, self.units, self.inputs, callee) {
            return Some(reason);
        }
        let variant = (callee, mask);
        if !self.states.contains_key(&variant) {
            let variants = self.states.range((callee, 1)..=(callee, u64::MAX)).count();
            if variants >= MAX_STACK_VARIANTS {
                return Some(passed(
                    self.index.display(callee),
                    &format!(", which has its {MAX_STACK_VARIANTS} stack variants"),
                ));
            }
        }
        if let Entry::Vacant(entry) = self.states.entry(variant) {
            entry.insert(State::new(&self.facts[&callee], mask));
            self.changed = true;
        }
        let dead = self.states[&variant].dead.as_ref()?;
        Some(passed(
            self.index.display(callee),
            &format!(", whose {dead}"),
        ))
    }
}

/// The plans of the variants the functions reach, with each function's report.
fn collect_plans(
    facts: &BTreeMap<usize, Facts>,
    states: &BTreeMap<(usize, u64), State>,
    classes: &Classes,
) -> StackPlans {
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
        pending.extend(plan.calls.values().copied());
        for group in &plan.groups {
            if let Some(Ok(class)) = classes.by_object.get(&group.class) {
                plans.classes.insert(group.class, class.clone());
            }
        }
        if params == 0 {
            let sites = body_facts
                .sites
                .iter()
                .map(|site| state.site_report(site))
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
    /// How the class lives on the stack.
    fn resolve<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        class: ClassRef<'db>,
    ) -> Result<StackClass, String> {
        let Some((object, field_count)) = index.class(db, class) else {
            let class = class_display_name(db, class);
            return Err(format!("class `{class}` is not in the program"));
        };
        self.by_object
            .entry(object)
            .or_insert_with(|| stack_class(db, index, class, object, field_count))
            .clone()
    }
}

fn stack_class<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    class: ClassRef<'db>,
    object: usize,
    field_count: usize,
) -> Result<StackClass, String> {
    let name = class_display_name(db, class);
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
        _ if immutable_ty(ty) => Some(Repr::Value),
        _ => None,
    }
}

/// A class allocation site of a body.
struct Site {
    class: String,
    at: SiteAt,
}

enum SiteAt {
    /// Assigned to a local: the group it joins.
    Group(usize),
    /// Not assigned to a local, so on the heap.
    Heap(&'static str),
}

/// A direct call that may pass objects.
struct Call {
    block: usize,
    /// The compiled callee.
    callee: usize,
    /// Per value argument within the mask width, the group of a bare local.
    args: Vec<Option<usize>>,
}

/// The local a bare-local operand reads.
pub(crate) fn bare_local(operand: &Operand<'_>) -> Option<usize> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(local.0),
        _ => None,
    }
}

/// Per local, the root of its group: locals joined by copies and narrowing
/// binds.
fn copy_roots(body: &MirFunctionBody<'_>) -> Vec<usize> {
    let n = body.locals.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(destination),
                value: Rvalue::Use(source),
            } = &statement.kind
                && let Some(source) = bare_local(source)
            {
                union(&mut parent, destination.0, source);
            }
        }
        if let Some(Terminator::NarrowBind {
            source,
            destination,
            ..
        }) = &block.terminator
            && let Some(source) = bare_local(source)
        {
            union(&mut parent, destination.0, source);
        }
    }
    (0..n).map(|local| find(&mut parent, local)).collect()
}

/// The stack class a local of type `ty` holds.
fn local_class(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    classes: &mut Classes,
    ty: &RuntimeTy,
) -> Result<StackClass, String> {
    let RuntimeTy::Class(name, args) = ty else {
        return Err("may also hold a value that is not this class".into());
    };
    if !args.is_empty() {
        return Err("generic class".into());
    }
    let class = baml_compiler2_hir_ty::layout::class_ref_of(db, name)
        .ok_or_else(|| "class declared outside the source".to_owned())?;
    classes.resolve(db, index, class)
}

/// Why a terminator without its own rule keeps an object it uses on the heap.
fn opaque_reason(terminator: &Terminator<'_>) -> String {
    let reason = match terminator {
        Terminator::VirtualCall { method, .. } => return format!("interface call .{method}"),
        Terminator::SysOp { .. } => "passed to a sys-op",
        Terminator::Spawn { .. } => "spawned",
        Terminator::Await { .. } | Terminator::AwaitAny { .. } => "awaited",
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => "thrown",
        Terminator::Branch { .. } | Terminator::Switch { .. } => "branched on",
        _ => "used by a terminator",
    };
    reason.to_owned()
}

/// Why an rvalue other than a `Use` keeps the objects it reads on the heap;
/// `None` for a type test of a bare local, which reads only its tag.
fn rvalue_reason(value: &Rvalue<'_>) -> Option<&'static str> {
    Some(match value {
        Rvalue::IsType {
            operand,
            test: TypeTest::Class { args, .. },
        } if args.is_empty() && bare_local(operand).is_some() => return None,
        Rvalue::TypeTag(Place::Local(_)) => return None,
        Rvalue::Array(..) | Rvalue::Map(..) => "stored into a container",
        Rvalue::MakeClosure { .. } => CAPTURED,
        Rvalue::MakeBoundMethod { .. } | Rvalue::MakeVirtualBoundMethod { .. } => {
            "bound as a method receiver"
        }
        Rvalue::BinaryOp { .. } => "compared by identity or operated on",
        _ => "used as a value",
    })
}

/// The callee a call can pass stack objects to, or why it cannot.
fn call_target<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    terminator: &Terminator<'db>,
    name: &str,
) -> Result<usize, String> {
    let Terminator::Call {
        has_trace,
        argument_layout,
        callee,
        args,
        ntypeargs,
        ..
    } = terminator
    else {
        unreachable!("a call terminator");
    };
    let Some(object) = user_callee(db, index, callee) else {
        return Err(passed(name, ""));
    };
    let positional = argument_layout
        .as_ref()
        .is_none_or(|layout| layout.0.iter().all(Option::is_none));
    if *has_trace {
        return Err(passed(name, " with a trace"));
    }
    if !positional {
        return Err(passed(name, " by name"));
    }
    if *ntypeargs != 0 {
        return Err(format!("passed to generic {name}"));
    }
    if index.info(object).arity != args.len() {
        return Err(passed(name, " with defaulted arguments"));
    }
    Ok(object)
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
        let root = copy_roots(body);
        let n = root.len();
        let mut facts = Self {
            arity,
            root,
            class: vec![None; n],
            invalid: vec![None; n],
            allocated: vec![false; n],
            params: vec![Vec::new(); n],
            sites: Vec::new(),
            calls: Vec::new(),
        };
        facts.classify_locals(db, index, classes, body);
        facts.escape_handlers(body);
        for (b, block) in body.blocks.iter().enumerate() {
            for statement in &block.statements {
                facts.statement(db, index, classes, body, &statement.kind);
            }
            if let Some(terminator) = &block.terminator {
                facts.terminator(db, index, body, b, terminator);
            }
        }
        facts.check_reuse(body);
        facts
    }

    /// A group is a candidate when a member is of a class type; every member
    /// must be of that one class. The return local is returned; parameters
    /// join their group's.
    fn classify_locals<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        classes: &mut Classes,
        body: &MirFunctionBody<'db>,
    ) {
        let mut candidate = vec![false; self.root.len()];
        for (local, decl) in body.locals.iter().enumerate() {
            if matches!(decl.ty, RuntimeTy::Class(..)) {
                candidate[self.root[local]] = true;
            }
        }
        for (local, decl) in body.locals.iter().enumerate() {
            let group = self.root[local];
            if !candidate[group] {
                continue;
            }
            match local_class(db, index, classes, &decl.ty) {
                Err(reason) => self.escape(local, &reason),
                Ok(stack) => self.join_class(group, stack),
            }
            if decl.is_captured {
                self.escape(local, CAPTURED);
            }
            if local == 0 {
                self.escape(local, "returned");
            } else if local <= self.arity {
                self.params[group].push(local);
            }
        }
    }

    /// A member of `group` holds `stack`.
    fn join_class(&mut self, group: usize, stack: StackClass) {
        match &self.class[group] {
            Some(known) if known.object != stack.object => {
                self.escape_root(group, "holds objects of two classes");
            }
            Some(_) => {}
            None => self.class[group] = Some(stack),
        }
    }

    fn escape_handlers(&mut self, body: &MirFunctionBody<'_>) {
        for (_, landing) in body.handlers() {
            self.escape(landing.error_local.0, "caught error");
            self.escape(landing.context_local.0, "caught error");
        }
    }

    fn escape(&mut self, local: usize, reason: &str) {
        let root = self.root[local];
        self.escape_root(root, reason);
    }

    fn escape_root(&mut self, root: usize, reason: &str) {
        self.invalid[root].get_or_insert_with(|| reason.to_owned());
    }

    /// `operand` is used as a value in a way that lets an object escape.
    fn use_operand(&mut self, operand: &Operand<'_>, reason: &str) {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                self.escape(local.0, reason);
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
            Place::Local(local) => self.escape(local.0, reason),
            Place::Index { base, index, .. } => {
                self.use_place(base, "indexed");
                self.escape(index.0, "used as an index");
            }
            Place::Deref(baml_compiler2_mir::CellId::Local(local)) => {
                self.escape(local.0, "held in a cell");
            }
            Place::Capture(_) | Place::Deref(_) => {}
        }
    }

    /// `place` is written with something that is not a stack object.
    fn define(&mut self, place: &Place, reason: &str) {
        match place {
            Place::Local(local) => self.escape(local.0, reason),
            Place::Field { base, field } => {
                // A boxed result may be an int where the field says float.
                if let Place::Local(local) = base.as_ref()
                    && self.field_repr(local.0, *field) == Some(Repr::Float)
                {
                    self.escape(local.0, FIELD_DOES_NOT_FIT);
                }
                self.use_place(place, reason);
            }
            other => self.use_place(other, reason),
        }
    }

    /// How the stack class of `local`'s group holds `field`, if it has one.
    fn field_repr(&self, local: usize, field: usize) -> Option<Repr> {
        let class = self.class[self.root[local]].as_ref()?;
        class.fields.get(field).copied()
    }

    fn terminator<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        body: &MirFunctionBody<'db>,
        block: usize,
        terminator: &Terminator<'db>,
    ) {
        match terminator {
            // `classify_locals` keeps the return local on the heap.
            Terminator::Goto { .. } | Terminator::Unreachable | Terminator::Return => {}
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => self.narrow_bind(source, test, destination.0),
            Terminator::Call { .. } => self.call(db, index, body, block, terminator),
            other => self.opaque(other),
        }
    }

    /// A narrowing bind of a bare local joins its group (`copy_roots`); a
    /// class test reads only its tag.
    fn narrow_bind(&mut self, source: &Operand<'_>, test: &TypeTest, destination: usize) {
        if bare_local(source).is_none() {
            self.use_operand(source, "narrowed");
            self.escape(destination, "holds a narrowed value");
        } else if !matches!(test, TypeTest::Class { args, .. } if args.is_empty()) {
            self.use_operand(source, "type-tested against a non-class type");
        }
    }

    fn call<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        body: &MirFunctionBody<'db>,
        block: usize,
        terminator: &Terminator<'db>,
    ) {
        let Terminator::Call {
            callee,
            args,
            ntypeargs,
            destination,
            ..
        } = terminator
        else {
            unreachable!("a call terminator");
        };
        self.define(destination, "holds a call result");
        let (native_key, name) = match callee {
            Operand::Constant(Constant::Function(func)) => (
                index
                    .function(db, *func)
                    .and_then(|(_, info)| info.native_key.clone()),
                function_display_name(db, *func),
            ),
            _ => (None, "a function value".into()),
        };
        if null_compared(body, native_key.as_deref(), *ntypeargs, args).is_some() {
            // `x == null` folds: a stack object is never null.
            for arg in args.iter().filter(|arg| bare_local(arg).is_none()) {
                self.use_operand(arg, "compared");
            }
            return;
        }
        let target = call_target(db, index, terminator, &name);
        self.use_operand(callee, "called");
        let values = self.call_args(args, &name);
        match target {
            Ok(callee) => self.calls.push(Call {
                block,
                callee,
                args: values,
            }),
            Err(reason) => {
                for group in values.into_iter().flatten() {
                    self.escape_root(group, &reason);
                }
            }
        }
    }

    /// Per argument, the group of a bare local the callee could take as a
    /// stack parameter; every other argument escapes.
    fn call_args(&mut self, args: &[Operand<'_>], callee: &str) -> Vec<Option<usize>> {
        let mut values = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            let Some(local) = bare_local(arg) else {
                self.use_operand(arg, "passed");
                values.push(None);
                continue;
            };
            if i >= MASKED_PARAMS {
                let how = format!(" beyond its first {MASKED_PARAMS} parameters");
                self.escape(local, &passed(callee, &how));
                values.push(None);
                continue;
            }
            let group = self.root[local];
            if values.contains(&Some(group)) {
                self.escape(local, &format!("passed twice to {callee}"));
            }
            values.push(Some(group));
        }
        values
    }

    /// A terminator without its own rule: what it reads escapes, what it
    /// defines is not a stack object.
    fn opaque(&mut self, terminator: &Terminator<'_>) {
        let reason = opaque_reason(terminator);
        walk_terminator_operands(terminator, &mut |operand| {
            self.use_operand(operand, &reason);
        });
        match terminator {
            Terminator::Await { future, .. } => self.use_place(future, &reason),
            Terminator::Spawn { future, .. } => self.define(future, "holds a future"),
            _ => {}
        }
        if let Some((destination, _)) = crate::analysis::continuation(terminator) {
            self.define(destination, "holds a call result");
        }
        if let Terminator::ShortCircuit { destination, .. } = terminator {
            self.define(destination, "holds a short-circuit result");
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
            StatementKind::Assign {
                destination,
                value:
                    value @ Rvalue::Aggregate {
                        kind: AggregateKind::Class { .. },
                        ..
                    },
            } => self.allocation(db, index, classes, body, destination, value),
            StatementKind::Assign { destination, value } => {
                self.assign(body, kind, destination, value);
            }
            StatementKind::Drop(_) | StatementKind::Nop => {}
            StatementKind::FreshCell { local, .. } => {
                self.escape(local.0, CAPTURED);
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

    /// A class allocation `destination = C { fields }`: a site, and the
    /// group it defines.
    fn allocation<'db>(
        &mut self,
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        classes: &mut Classes,
        body: &MirFunctionBody<'db>,
        destination: &Place,
        value: &Rvalue<'db>,
    ) {
        let Rvalue::Aggregate {
            kind:
                AggregateKind::Class {
                    class,
                    type_arg_templates,
                },
            fields,
        } = value
        else {
            unreachable!("a class allocation");
        };
        let name = class_display_name(db, *class);
        let Place::Local(local) = destination else {
            self.sites.push(Site {
                class: name,
                at: SiteAt::Heap("stored into a heap object"),
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
            at: SiteAt::Group(root),
        });
        self.allocated[root] = true;
        let resolved = classes.resolve(db, index, *class);
        if let Some(reason) =
            self.allocation_reason(body, root, resolved, !type_arg_templates.is_empty(), fields)
        {
            self.escape_root(root, &reason);
        }
        for field in fields {
            self.use_operand(field, "stored into a field of another object");
        }
    }

    /// Why an allocation of `resolved` into group `root` keeps it on the heap.
    fn allocation_reason(
        &self,
        body: &MirFunctionBody<'_>,
        root: usize,
        resolved: Result<StackClass, String>,
        generic: bool,
        fields: &[Operand<'_>],
    ) -> Option<String> {
        let stack = match resolved {
            Err(reason) => return Some(reason),
            Ok(_) if generic => return Some("generic class".into()),
            Ok(stack) => stack,
        };
        if self.class[root]
            .as_ref()
            .is_some_and(|known| known.object != stack.object)
        {
            return Some("holds objects of two classes".into());
        }
        let fit = fields
            .iter()
            .zip(&stack.fields)
            .all(|(field, repr)| fits(body, field, *repr));
        (!fit).then(|| FIELD_DOES_NOT_FIT.to_owned())
    }

    /// Any assignment but a class allocation.
    fn assign<'db>(
        &mut self,
        body: &MirFunctionBody<'db>,
        kind: &StatementKind<'db>,
        destination: &Place,
        value: &Rvalue<'db>,
    ) {
        match (destination, value) {
            // Copies joined the groups.
            (Place::Local(_), Rvalue::Use(operand)) if bare_local(operand).is_some() => return,
            (Place::Local(local), _) => self.escape(local.0, "holds a value computed here"),
            (Place::Field { base, field }, _) => self.field_store(body, base, *field, value),
            (other, _) => self.use_place(other, "stored into"),
        }
        self.rvalue_uses(kind, destination, value);
    }

    /// A store into a stack object's field must fit the struct's field.
    fn field_store(
        &mut self,
        body: &MirFunctionBody<'_>,
        base: &Place,
        field: usize,
        value: &Rvalue<'_>,
    ) {
        let Place::Local(local) = base else {
            self.use_place(base, "stored through");
            return;
        };
        if let Some(repr) = self.field_repr(local.0, field)
            && !value_fits(body, value, repr)
        {
            self.escape(local.0, FIELD_DOES_NOT_FIT);
        }
    }

    /// The objects an assigned rvalue reads.
    fn rvalue_uses(&mut self, kind: &StatementKind<'_>, destination: &Place, value: &Rvalue<'_>) {
        if let Rvalue::Use(operand) = value {
            let reason = match destination {
                Place::Local(_) => "copied",
                _ => "stored into a heap object",
            };
            self.use_operand(operand, reason);
            return;
        }
        let Some(reason) = rvalue_reason(value) else {
            return;
        };
        walk_statement_operands(kind, &mut |operand| self.use_operand(operand, reason));
        walk_rvalue_places(value, &mut |place| {
            if matches!(place, Place::Local(_)) {
                self.use_place(place, "inspected");
            }
        });
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
            let exceptional = block.unwind.map_or(&[][..], |handler| &live_in[handler.0]);
            for statement in block.statements.iter().rev() {
                if let Some(root) = self.reallocated(&statement.kind, &candidates, &live) {
                    self.escape_root(
                        root,
                        "an alias of its previous object is live when it is allocated again",
                    );
                }
                baml_compiler2_mir::live_transfer(statement, &mut live, exceptional);
            }
        }
    }

    /// The candidate group `statement` allocates while another of its
    /// members is `live`.
    fn reallocated(
        &self,
        statement: &StatementKind<'_>,
        candidates: &BTreeSet<usize>,
        live: &[bool],
    ) -> Option<usize> {
        let StatementKind::Assign {
            destination: Place::Local(destination),
            value: Rvalue::Aggregate { .. },
        } = statement
        else {
            return None;
        };
        let root = self.root[destination.0];
        let aliased = (0..live.len())
            .any(|local| live[local] && local != destination.0 && self.root[local] == root);
        (candidates.contains(&root) && aliased).then_some(root)
    }
}

/// Whether `value`'s result can be stored into a field of `repr`.
fn value_fits(body: &MirFunctionBody<'_>, value: &Rvalue<'_>, repr: Repr) -> bool {
    match value {
        Rvalue::Use(operand) => fits(body, operand, repr),
        _ => repr != Repr::Float || float_result(body, value),
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
    /// Why this variant cannot exist.
    dead: Option<String>,
    /// The parameters this variant takes as stack objects.
    params: u64,
}

impl State {
    fn new(facts: &Facts, params: u64) -> Self {
        let invalid = (0..facts.root.len())
            .map(|root| {
                facts.invalid[root]
                    .clone()
                    .or_else(|| param_reason(facts, params, root).map(str::to_owned))
            })
            .collect();
        // A stack parameter must not be a parameter beyond the arity.
        let dead = (facts.arity.min(MASKED_PARAMS)..MASKED_PARAMS)
            .any(|i| takes_param(params, i))
            .then(|| "no such parameter".to_owned());
        Self {
            invalid,
            dead,
            params,
        }
    }

    /// Whether the group of `root` is a stack object.
    fn stack(&self, root: usize) -> bool {
        self.invalid[root].is_none()
    }

    /// Why a stack parameter of this variant is not a stack object, if one
    /// is not.
    fn dead_param(&self, facts: &Facts) -> Option<String> {
        (0..facts.arity.min(MASKED_PARAMS)).find_map(|i| {
            let reason = self.invalid[facts.root[i + 1]].as_ref();
            let reason = reason.filter(|_| takes_param(self.params, i))?;
            Some(format!("parameter {} escapes: {reason}", i + 1))
        })
    }

    /// The (argument index, group) of each stack object `call` passes.
    fn stack_args<'a>(&'a self, call: &'a Call) -> impl Iterator<Item = (usize, usize)> + 'a {
        call.args
            .iter()
            .enumerate()
            .filter_map(|(i, root)| root.filter(|&root| self.stack(root)).map(|root| (i, root)))
    }

    /// The stack parameters of the variant `call` calls; `0` for none.
    fn mask(&self, call: &Call) -> u64 {
        self.stack_args(call)
            .filter(|&(i, _)| i < MASKED_PARAMS)
            .fold(0, |mask, (i, _)| mask | (1 << i))
    }

    fn site_report(&self, site: &Site) -> SiteReport {
        let heap_because = match &site.at {
            SiteAt::Group(root) => self.invalid[*root].clone(),
            SiteAt::Heap(reason) => Some((*reason).to_owned()),
        };
        SiteReport {
            class: site.class.clone(),
            heap_because,
        }
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
                plan.groups.push(Group {
                    class: facts.class[root].as_ref().expect("a stack class").object,
                    storage: match facts.params[root].first() {
                        Some(&param) => format!("(*arg{})", param - 1),
                        None => format!("s{id}"),
                    },
                    allocated: facts.params[root].is_empty(),
                    members: Vec::new(),
                });
                id
            });
            plan.groups[id].members.push(local);
            plan.local_group[local] = Some(id);
        }
        for call in &facts.calls {
            let mask = self.mask(call);
            if mask != 0 {
                plan.calls.insert(call.block, (call.callee, mask));
            }
        }
        plan
    }
}

/// Why group `root` cannot be a stack object under stack parameters
/// `params`, whatever its uses.
fn param_reason(facts: &Facts, params: u64, root: usize) -> Option<&'static str> {
    if facts.root[root] == root && facts.class[root].is_none() {
        return Some("not a stack class");
    }
    let held = facts.params[root].len();
    let stack = facts.params[root]
        .iter()
        .filter(|&&local| takes_param(params, local - 1))
        .count();
    match (held - stack, stack, facts.allocated[root]) {
        (1.., _, _) => Some("may hold a parameter"),
        (0, 2.., _) => Some("may hold either of two parameters"),
        (0, 1, true) => Some("may hold a parameter or a new object"),
        (0, 0, false) => Some("never allocated here"),
        _ => None,
    }
}
