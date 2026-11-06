//! Closed-world typed emission. Unsupported MIR is a pack error, never bytecode.
use crate::{
    index::ProgramIndex,
    structure::{Cfg, Flow, Stmt, structurize},
};
use baml_base::SourceRoot;
use baml_compiler2_hir::{
    item_data::{file_functions, file_lets, is_required_interface_method},
    package::world_files,
};
use baml_compiler2_mir::{
    AggregateKind, BinOp, Constant, Db, MirFunction, MirFunctionBody, MirFunctionKind, Operand,
    OptLevel, Place, RuntimeTy, Rvalue, StatementKind, Terminator, UnaryOp, lower_function,
};
use bex_vm_types::{Object, Program};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

type Gen<T> = Result<T, String>;
fn no(what: impl std::fmt::Display) -> String {
    format!("unsupported in runtime v2: {what}")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Repr {
    Int,
    Bool,
    Str,
    Ptr(usize),
    ListInt,
    ListPtr(usize),
    Dyn(usize),
    Null,
}
impl Repr {
    fn rust(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Bool => "bool",
            Self::Dyn(_) => "rt::Dyn",
            _ => "usize",
        }
    }
    fn zero(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::Dyn(_) => "rt::Dyn{tag:2,payload:0}",
            _ => "0",
        }
    }
    fn pointer(self) -> bool {
        matches!(
            self,
            Self::Str | Self::Ptr(_) | Self::ListInt | Self::ListPtr(_)
        )
    }
}
struct Layout {
    fields: Vec<Repr>,
    offsets: Vec<usize>,
    bytes: usize,
    pointers: usize,
    tagged: usize,
}
struct Plan<'a> {
    lang: baml_base::LangRoots,
    index: ProgramIndex<'a>,
    program: &'a Program,
    layouts: BTreeMap<usize, Layout>,
    lists: BTreeSet<Repr>,
}
impl Plan<'_> {
    fn class_tag(&self, tag: baml_type::typetag::TypeTag) -> Gen<usize> {
        self.program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| match o {
                Object::Class(c) if c.type_tag == tag => Some(i),
                _ => None,
            })
            .ok_or_else(|| no("unlinked class tag"))
    }
    fn implementors(
        &self,
        interface: usize,
    ) -> Gen<Vec<(usize, &bex_vm_types::types::ProgramImplRule)>> {
        let Object::Interface(def) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(interface)]
        else {
            return Err(no("non-interface dispatch"));
        };
        if def.structural_default.is_some()
            || !def.args.is_empty()
            || !def.assoc.is_empty()
            || !def.requires.is_empty()
        {
            return Err(no("generic or structural interface"));
        }
        let mut result = Vec::new();
        for package in &self.program.packages {
            if let Some(rules) = package
                .impl_rules
                .get(&bex_vm_types::ObjectIndex::from_raw(interface))
            {
                for rule in rules {
                    if !rule.generic_param_bounds.is_empty()
                        || !rule.interface_args.is_empty()
                        || !rule.interface_assoc.is_empty()
                    {
                        return Err(no("generic interface implementation"));
                    }
                    let bex_vm_types::TyTemplate::Class(head, args) = &rule.for_ty_pattern else {
                        return Err(no("non-class implementation"));
                    };
                    if !args.is_empty() {
                        return Err(no("generic implementor"));
                    }
                    result.push((self.class_tag(head.tag())?, rule));
                }
            }
        }
        if result.is_empty() {
            return Err(no("interface without concrete implementations"));
        }
        Ok(result)
    }
    fn targets(&self, interface: usize, method: &str) -> Gen<Vec<(usize, usize)>> {
        self.implementors(interface)?
            .into_iter()
            .map(|(c, rule)| {
                let body = rule
                    .methods
                    .get(method)
                    .ok_or_else(|| no("adopted interface default"))?;
                if !body.frame.is_empty() {
                    return Err(no("generic dispatch frame"));
                }
                Ok((c, body.fqn.raw()))
            })
            .collect()
    }
    fn record(&mut self, repr: Repr) -> Gen<()> {
        match repr {
            Repr::Ptr(c) => self.add(c)?,
            Repr::ListPtr(c) => {
                self.lists.insert(repr);
                self.add(c)?;
            }
            Repr::ListInt => {
                self.lists.insert(repr);
            }
            Repr::Dyn(i) => {
                let classes: Vec<_> = self.implementors(i)?.iter().map(|(c, _)| *c).collect();
                for c in classes {
                    self.add(c)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    // MIR may reuse a method-valued temporary for its extracted receiver.
    // Infer only copy-assigned storage, and require every definition to agree.
    // Actual function values, cycles, parameters and call results stay rejected.
    fn local_repr(&self, body: &MirFunctionBody<'_>, i: usize, depth: usize) -> Gen<Repr> {
        if !matches!(body.locals[i].ty, RuntimeTy::Function { .. }) {
            return self.repr(&body.locals[i].ty);
        }
        if depth >= body.locals.len() || i == 0 || body.locals[i].name.is_some() {
            return Err(no("function-valued local"));
        }
        let mut inferred = None;
        for block in &body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(dst),
                    value,
                } = &statement.kind
                    && dst.0 == i
                {
                    let Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) = value else {
                        return Err(no("function-valued local definition"));
                    };
                    let repr = self.storage_repr(body, place, depth + 1)?;
                    if inferred.is_some_and(|previous| previous != repr) {
                        return Err(no("inconsistent receiver storage"));
                    }
                    inferred = Some(repr);
                }
            }
            if matches!(&block.terminator,
                Some(Terminator::Call { destination: Place::Local(dst), .. }
                    | Terminator::VirtualCall { destination: Place::Local(dst), .. }) if dst.0 == i)
            {
                return Err(no("function-valued call result"));
            }
        }
        inferred.ok_or_else(|| no("unresolved function-valued storage"))
    }
    fn storage_repr(&self, body: &MirFunctionBody<'_>, place: &Place, depth: usize) -> Gen<Repr> {
        match place {
            Place::Local(local) => self.local_repr(body, local.0, depth),
            Place::Field { base, field } => {
                let Repr::Ptr(class) = self.storage_repr(body, base, depth)? else {
                    return Err(no("receiver field on non-class"));
                };
                let Object::Class(class) =
                    &self.program.objects[bex_vm_types::ObjectIndex::from_raw(class)]
                else {
                    return Err(no("receiver class"));
                };
                self.field_repr(
                    &class
                        .fields
                        .get(*field)
                        .ok_or_else(|| no("receiver field"))?
                        .field_type,
                )
            }
            _ => Err(no("receiver storage place")),
        }
    }

    fn repr(&self, ty: &RuntimeTy) -> Gen<Repr> {
        Ok(match ty {
            RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            RuntimeTy::Bool | RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            RuntimeTy::String | RuntimeTy::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            RuntimeTy::Null => Repr::Null,
            RuntimeTy::List(element) => match element.as_ref() {
                RuntimeTy::Int => Repr::ListInt,
                RuntimeTy::Union(_) => match self.repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list element")),
                },
                _ => return Err(no("list element")),
            },
            RuntimeTy::Interface(head, args, assoc) if args.is_empty() && assoc.is_empty() => {
                Repr::Dyn(
                    self.index
                        .interface_at(head)
                        .ok_or_else(|| no("interface"))?,
                )
            }
            RuntimeTy::Class(head, args)
                if args.is_empty()
                    && head.builtin_primitive(self.lang)
                        == Some(baml_type::PrimitiveType::String) =>
            {
                Repr::Str
            }
            RuntimeTy::Class(head, args) if args.is_empty() => Repr::Ptr(
                self.index
                    .class_at(head)
                    .ok_or_else(|| no("unlinked class"))?
                    .0,
            ),
            RuntimeTy::Union(members)
                if members.len() == 2 && members.iter().any(|m| matches!(m, RuntimeTy::Null)) =>
            {
                let member = members
                    .iter()
                    .find(|m| !matches!(m, RuntimeTy::Null))
                    .unwrap();
                let repr = self.repr(member)?;
                if !repr.pointer() {
                    return Err(no("optional scalar"));
                }
                repr
            }
            _ => return Err(no(format!("type {ty:?}"))),
        })
    }
    fn field_repr(&self, ty: &bex_vm_types::RuntimeTy) -> Gen<Repr> {
        use bex_vm_types::RuntimeTy as T;
        Ok(match ty {
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            T::List(element) => match element.as_ref() {
                T::Int => Repr::ListInt,
                T::Union(_) => match self.field_repr(element)? {
                    Repr::Ptr(c) => Repr::ListPtr(c),
                    _ => return Err(no("list field element")),
                },
                _ => return Err(no("list field element")),
            },
            T::Interface(head, args, assoc) if args.is_empty() && assoc.is_empty() => Repr::Dyn(
                self.program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::Interface(d) if d.type_tag == head.tag() => Some(i),
                        _ => None,
                    })
                    .ok_or_else(|| no("interface field"))?,
            ),
            T::Class(head, args) if args.is_empty() => {
                let object = self
                    .program
                    .objects
                    .iter()
                    .enumerate()
                    .find_map(|(i, o)| match o {
                        Object::Class(c) if c.type_tag == head.tag() => Some(i),
                        _ => None,
                    })
                    .ok_or_else(|| no("unlinked field class"))?;
                Repr::Ptr(object)
            }
            T::Union(m) if m.len() == 2 && m.iter().any(|t| matches!(t, T::Null)) => {
                let repr = self.field_repr(m.iter().find(|t| !matches!(t, T::Null)).unwrap())?;
                if !repr.pointer() {
                    return Err(no("optional scalar field"));
                }
                repr
            }
            _ => return Err(no(format!("field type {ty:?}"))),
        })
    }
    fn add(&mut self, object: usize) -> Gen<()> {
        if self.layouts.contains_key(&object) {
            return Ok(());
        }
        let Object::Class(class) =
            &self.program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
        else {
            return Err(no("non-class layout"));
        };
        if class.cleanup.is_some() || class.generic_param_count != 0 {
            return Err(no("generic class or cleanup"));
        }
        let fields = class
            .fields
            .iter()
            .map(|f| self.field_repr(&f.field_type))
            .collect::<Gen<Vec<_>>>()?;
        let mut offsets = vec![0; fields.len()];
        let mut offset = 8;
        for group in 0..3 {
            for (i, repr) in fields.iter().enumerate() {
                let g = if repr.pointer() {
                    0
                } else if matches!(repr, Repr::Dyn(_)) {
                    1
                } else {
                    2
                };
                if g == group {
                    offsets[i] = offset;
                    offset += if g == 1 { 16 } else { 8 };
                }
            }
        }
        let children = fields.clone();
        self.layouts.insert(
            object,
            Layout {
                pointers: fields.iter().filter(|r| r.pointer()).count(),
                tagged: fields.iter().filter(|r| matches!(r, Repr::Dyn(_))).count(),
                fields,
                offsets,
                bytes: offset,
            },
        );
        for child in children {
            self.record(child)?;
        }
        Ok(())
    }
}

/// Emit a single parameterless integer entry, including a compatible benchmark host.
/// Layouts and direct call bodies are derived exclusively from MIR and linked metadata.
pub fn compile_v2(
    db: &dyn Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    entry: usize,
) -> Gen<String> {
    let mut plan = Plan {
        lang: baml_compiler2_hir::package::lang_roots(db),
        index: ProgramIndex::new(db, root, program),
        program,
        layouts: BTreeMap::new(),
        lists: BTreeSet::new(),
    };
    let mut all = BTreeMap::new();
    for &file in world_files(db, root) {
        if file.source_root(db).kind(db) != baml_base::SourceRootKind::Stdlib
            && !file_lets(db, file).is_empty()
        {
            return Err(no("top-level initializers"));
        }
        for &f in file_functions(db, file) {
            if is_required_interface_method(db, f) {
                continue;
            }
            let mir = lower_function(db, f, opt)
                .as_ref()
                .map_err(|e| no(format!("MIR: {e:?}")))?;
            if let Some(object) = plan.index.declared(db, f) {
                all.insert(object, mir);
            }
        }
    }
    let mut queue = VecDeque::from([entry]);
    let mut reached = BTreeSet::new();
    let mut calls: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut builtins = BTreeSet::new();
    while let Some(object) = queue.pop_front() {
        if !reached.insert(object) {
            continue;
        }
        let mir = all
            .get(&object)
            .ok_or_else(|| no(format!("missing MIR for {}", plan.index.display(object))))?;
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            return Err(no(plan.index.display(object)));
        };
        if !mir.lambdas.is_empty() {
            return Err(no("closures"));
        }
        if body.locals.len() > 1024 {
            return Err(no("more than 1024 locals in one body"));
        }
        for (i, local) in body.locals.iter().enumerate() {
            if local.is_captured {
                return Err(no("captured local"));
            }
            plan.record(plan.local_repr(body, i, 0)?)?;
        }
        for block in &body.blocks {
            if block.unwind.is_some() || block.landing.is_some() || block.shielded {
                return Err(no("catch/defer"));
            }
            match &block.terminator {
                Some(Terminator::Call {
                    callee: Operand::Constant(Constant::Function(f)),
                    ntypeargs: 0,
                    has_trace: false,
                    ..
                }) => {
                    let (callee, _) = plan
                        .index
                        .function(db, *f)
                        .ok_or_else(|| no("unlinked call"))?;
                    let name = plan.index.display(callee);
                    if baml_builtins2_codegen::supports_v2(name) {
                        builtins.insert(name.to_owned());
                    } else if name != "baml.ops.equals_equals" {
                        calls.entry(object).or_default().insert(callee);
                        queue.push_back(callee);
                    }
                }
                Some(Terminator::VirtualCall {
                    iface,
                    method,
                    ntypeargs: 0,
                    has_trace: false,
                    self_arg: 0,
                    ..
                }) if iface.generics.is_empty() => {
                    let i = plan
                        .index
                        .interface_at(&iface.name)
                        .ok_or_else(|| no("virtual interface"))?;
                    for (_, target) in plan.targets(i, method)? {
                        calls.entry(object).or_default().insert(target);
                        queue.push_back(target);
                    }
                }
                Some(
                    Terminator::Goto { .. }
                    | Terminator::Branch { .. }
                    | Terminator::ShortCircuit { .. }
                    | Terminator::Return,
                ) => {}
                other => return Err(no(format!("terminator {other:?}"))),
            }
        }
    }
    let mir = all[&entry];
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        unreachable!()
    };
    if mir.arity != 0 || plan.repr(&body.locals[0].ty)? != Repr::Int {
        return Err(no("entry must have no parameters and return int"));
    }
    let mut out = String::from(
        "#![allow(unused_mut,unused_variables,unused_assignments,unused_labels,dead_code,non_snake_case)]\nuse bex_v2_runtime::{self as rt, R};\nuse bex_v2_heap::{Descriptor,Kind,Field,SlotKind};\n",
    );
    if !builtins.is_empty() {
        let (declarations, _, _) = baml_builtins2_codegen::extract_native_builtins().map_err(no)?;
        out.push_str(&baml_builtins2_codegen::generate_v2(
            &declarations,
            &builtins,
        )?);
    }
    let descriptors: BTreeMap<_, _> = plan
        .layouts
        .keys()
        .enumerate()
        .map(|(i, c)| (*c, i + rt_descriptors()))
        .collect();
    let lists: BTreeMap<_, _> = plan
        .lists
        .iter()
        .enumerate()
        .map(|(i, r)| (*r, i + descriptors.len() + rt_descriptors()))
        .collect();
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,\n");
    for (&object, layout) in &plan.layouts {
        let Object::Class(class) = &program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
        else {
            unreachable!()
        };
        let fields = class
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                format!(
                    "Field{{name:{:?},offset:{},slot:SlotKind::{},type_id:{}}}",
                    f.name,
                    layout.offsets[i],
                    if layout.fields[i].pointer() {
                        "Pointer"
                    } else if matches!(layout.fields[i], Repr::Dyn(_)) {
                        "Tagged"
                    } else {
                        "Scalar"
                    },
                    match layout.fields[i] {
                        Repr::Ptr(c) => descriptors[&c] + 3,
                        r @ (Repr::ListInt | Repr::ListPtr(_)) => lists[&r] + 3,
                        Repr::Dyn(i) => rt_descriptors() + descriptors.len() + lists.len() + i + 3,
                        Repr::Str => 3,
                        Repr::Int => 0,
                        Repr::Bool => 1,
                        Repr::Null => unreachable!("null fields are rejected"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(&mut out,"Descriptor{{kind:Kind::Class,size:{},ptr_slots:(8,{}),tagged_slots:({},{}),elem:None,type_id:{},fields:&[{fields}],finalizer:None}},",layout.bytes,layout.pointers,8+layout.pointers*8,layout.tagged,descriptors[&object]+3).unwrap();
    }
    for (repr, desc) in &lists {
        let elem = if *repr == Repr::ListInt {
            "Scalar"
        } else {
            "Pointer"
        };
        writeln!(&mut out, "Descriptor{{kind:Kind::List,size:8,ptr_slots:(0,0),tagged_slots:(0,0),elem:Some(bex_v2_heap::Elements{{offset:16,layout:bex_v2_heap::ElementLayout::{elem}}}),type_id:{},fields:&[],finalizer:None}},", desc+3).unwrap();
    }
    out.push_str("];\n");
    for (&object, layout) in &plan.layouts {
        write!(
            &mut out,
            "#[repr(C)] struct C{object}{{header:bex_v2_heap::Header,"
        )
        .unwrap();
        let mut ordered: Vec<_> = (0..layout.fields.len()).collect();
        ordered.sort_by_key(|i| layout.offsets[*i]);
        for i in ordered {
            write!(
                &mut out,
                "f{i}:{},",
                if layout.fields[i] == Repr::Bool {
                    "u64"
                } else {
                    layout.fields[i].rust()
                }
            )
            .unwrap();
        }
        out.push_str("}\n");
    }
    // Bound forced inlining by the same total-local budget as a single body.
    // Recursive SCCs retain real frames and the normal per-entry stack guard.
    let small_program = reached
        .iter()
        .map(|id| match &all[id].kind {
            MirFunctionKind::Bytecode(body) => body.locals.len(),
            MirFunctionKind::Builtin(_) => 0,
        })
        .sum::<usize>()
        <= 1024;
    for &object in &reached {
        let mir = all[&object];
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            unreachable!()
        };
        let mut emitter = Emitter {
            db,
            plan: &plan,
            body,
            object,
            descriptors: &descriptors,
            lists: &lists,
            all: &all,
            sites: Vec::new(),
            site: 0,
        };
        out.push_str(&emitter.function(mir.arity, small_program && !recursive(&calls, object))?);
    }
    // Resolve once at startup. Opaque initialization keeps the observation
    // load/branch real under LTO without imposing an opaque call on every entry.
    let mut observation_init = String::new();
    for id in &reached {
        write!(
            &mut observation_init,
            "O{id}.store(std::hint::black_box(0),std::sync::atomic::Ordering::Relaxed);"
        )
        .unwrap();
    }
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{f{entry}()}}}}\nfn main()->std::process::ExitCode{{{observation_init}rt::driver(DESCRIPTORS,entry)}}\n").unwrap();
    Ok(out)
}
fn rt_descriptors() -> usize {
    7
}
struct Emitter<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    body: &'a MirFunctionBody<'db>,
    object: usize,
    descriptors: &'a BTreeMap<usize, usize>,
    lists: &'a BTreeMap<Repr, usize>,
    all: &'a BTreeMap<usize, &'db MirFunction<'db>>,
    sites: Vec<usize>,
    site: usize,
}
impl Emitter<'_, '_> {
    fn local(&self, i: usize) -> Gen<Repr> {
        self.plan.local_repr(self.body, i, 0)
    }
    fn mark(&mut self, pc: Option<usize>) {
        if let Some(pc) = pc {
            self.site = self.sites.len();
            self.sites.push(
                self.plan
                    .index
                    .positions(self.object)
                    .source_line_for_pc(pc),
            );
        }
    }
    fn fallible(&self, code: &str) -> String {
        format!(
            "({code}).map_err(|e|e.at(&S{},{}))?",
            self.object, self.site
        )
    }
    fn coerce(&self, value: (String, Repr), target: Repr) -> Gen<String> {
        let (value, source) = value;
        if source == target || (source == Repr::Null && target.pointer()) {
            return Ok(value);
        }
        if let (Repr::Ptr(c), Repr::Dyn(i)) = (source, target) {
            if self
                .plan
                .implementors(i)?
                .iter()
                .any(|(class, _)| *class == c)
            {
                return Ok(format!(
                    "rt::Dyn{{tag:{},payload:({value}) as u64}}",
                    self.descriptors[&c] + 3
                ));
            }
        }
        Err(no(format!("conversion {source:?} to {target:?}")))
    }
    fn element(repr: Repr) -> Gen<Repr> {
        match repr {
            Repr::ListInt => Ok(Repr::Int),
            Repr::ListPtr(c) => Ok(Repr::Ptr(c)),
            _ => Err(no("indexed non-list")),
        }
    }
    fn place(&self, p: &Place) -> Gen<(String, Repr)> {
        match p {
            Place::Local(l) => Ok((format!("l{}", l.0), self.local(l.0)?)),
            Place::Field { base, field } => {
                let (base, repr) = self.place(base)?;
                let Repr::Ptr(c) = repr else {
                    return Err(no("field on non-class"));
                };
                let layout = &self.plan.layouts[&c];
                let repr = *layout.fields.get(*field).ok_or_else(|| no("field index"))?;
                let checked = self.fallible(&format!("rt::nonnull({base})"));
                let value = format!("(*({checked} as *const C{c})).f{field}");
                Ok((
                    if repr == Repr::Bool {
                        format!("({value} != 0)")
                    } else {
                        value
                    },
                    repr,
                ))
            }
            Place::Index {
                base,
                index,
                kind: baml_compiler2_mir::IndexKind::Array,
            } => {
                let (base, repr) = self.place(base)?;
                let element = Self::element(repr)?;
                if self.local(index.0)? != Repr::Int {
                    return Err(no("non-int list index"));
                }
                let slot = self.fallible(&format!("rt::list_slot({base},l{})", index.0));
                Ok((format!("*({slot} as *const {})", element.rust()), element))
            }
            _ => Err(no(format!("place {p:?}"))),
        }
    }
    fn operand(&self, o: &Operand<'_>) -> Gen<(String, Repr)> {
        match o {
            Operand::Copy(p) | Operand::Move(p) => self.place(p),
            Operand::Constant(Constant::Int(i)) => Ok((format!("{i}i64"), Repr::Int)),
            Operand::Constant(Constant::Bool(b)) => Ok((b.to_string(), Repr::Bool)),
            Operand::Constant(Constant::String(text)) => Ok((
                self.fallible(&format!(
                    "rt::strings::Cx::current().string({:?}).map(|s|s.as_raw())",
                    text.as_str()
                )),
                Repr::Str,
            )),
            Operand::Constant(Constant::Null) => Ok(("0usize".into(), Repr::Null)),
            Operand::Constant(_) => Err(no(format!("operand {o:?}"))),
        }
    }
    fn assign(&self, p: &Place, value: (String, Repr)) -> Gen<String> {
        let (dst, repr) = self.place(p)?;
        let src = self.coerce(value, repr)?;
        match p {
            Place::Local(_) => Ok(format!("{dst}={src};\n")),
            Place::Field { base, field } => {
                let (obj, Repr::Ptr(c)) = self.place(base)? else {
                    return Err(no("field store"));
                };
                let obj = self.fallible(&format!("rt::nonnull({obj})"));
                let slot = format!("&raw mut (*(object as *mut C{c})).f{field}");
                let write = if repr.pointer() {
                    format!("bex_v2_heap::write_ptr(object as *mut u8,{slot},value);")
                } else if matches!(repr, Repr::Dyn(_)) {
                    format!("rt::write_dyn(object as *mut u8,{slot},value);")
                } else {
                    format!("({slot}).write(value as _);")
                };
                Ok(format!("{{let value={src};let object={obj};{write}}}\n"))
            }
            Place::Index {
                base,
                index,
                kind: baml_compiler2_mir::IndexKind::Array,
            } => {
                let (obj, _) = self.place(base)?;
                let slot = self.fallible(&format!("rt::list_slot(object,l{})", index.0));
                let write = if repr.pointer() {
                    "bex_v2_heap::write_ptr(object as *mut u8,slot.cast(),value);".to_owned()
                } else {
                    format!("slot.cast::<{}>().write(value);", repr.rust())
                };
                Ok(format!(
                    "{{let value={src};let object={obj};let slot={slot};{write}}}\n"
                ))
            }
            _ => Err(no("store")),
        }
    }
    fn value(&self, r: &Rvalue<'_>, expected: Repr) -> Gen<(String, Repr)> {
        match r {
            Rvalue::Use(o) => self.operand(o),
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                fields,
            }
            | Rvalue::Array(_, fields) => {
                let element = Self::element(expected)?;
                let desc = self
                    .lists
                    .get(&expected)
                    .ok_or_else(|| no("unplanned list"))?;
                let mut code = String::from("{");
                for (i, v) in fields.iter().enumerate() {
                    let v = self.coerce(self.operand(v)?, element)?;
                    write!(&mut code, "let a{i}={v};").unwrap();
                }
                let alloc =
                    self.fallible(&format!("rt::try_alloc({desc},{})", 16 + fields.len() * 8));
                write!(
                    &mut code,
                    "let p={alloc};p.add(8).cast::<usize>().write({});",
                    fields.len()
                )
                .unwrap();
                for i in 0..fields.len() {
                    if element.pointer() {
                        write!(
                            &mut code,
                            "bex_v2_heap::write_ptr(p,p.add({}).cast(),a{i});",
                            16 + i * 8
                        )
                        .unwrap();
                    } else {
                        write!(
                            &mut code,
                            "p.add({}).cast::<i64>().write(a{i});",
                            16 + i * 8
                        )
                        .unwrap();
                    }
                }
                code.push_str("p as usize}");
                Ok((code, expected))
            }
            Rvalue::ConcatStrings(parts) => {
                let values = parts
                    .iter()
                    .map(|part| {
                        let value = self.coerce(self.operand(part)?, Repr::Str)?;
                        Ok(format!("rt::strings::Str::from_raw({value})"))
                    })
                    .collect::<Gen<Vec<_>>>()?
                    .join(",");
                Ok((self.fallible(&format!("rt::strings::concat(&mut rt::strings::Cx::current(),&[{values}]).map(|s|s.as_raw())")), Repr::Str))
            }
            Rvalue::BinaryOp { op, left, right } => {
                let (l, lt) = self.operand(left)?;
                let (r, rt) = self.operand(right)?;
                if *op == BinOp::Add && lt == Repr::Str && rt == Repr::Str {
                    return Ok((self.fallible(&format!("rt::strings::concat(&mut rt::strings::Cx::current(),&[rt::strings::Str::from_raw({l}),rt::strings::Str::from_raw({r})]).map(|s|s.as_raw())")), Repr::Str));
                }
                if lt != Repr::Int || rt != Repr::Int {
                    return Err(no("non-integer binary operation"));
                }
                let helper = match op {
                    BinOp::Add => Some("add"),
                    BinOp::Sub => Some("sub"),
                    BinOp::Mul => Some("mul"),
                    BinOp::Shl => Some("shl"),
                    BinOp::Shr => Some("shr"),
                    _ => None,
                };
                if let Some(h) = helper {
                    return Ok((self.fallible(&format!("rt::{h}({l},{r})")), Repr::Int));
                }
                let ty = match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Repr::Bool
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => Repr::Int,
                    _ => return Err(no("division/remainder")),
                };
                Ok((format!("({l} {op} {r})"), ty))
            }
            Rvalue::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => {
                let (v, t) = self.operand(operand)?;
                if t != Repr::Bool {
                    return Err(no("non-bool not"));
                }
                Ok((format!("!({v})"), t))
            }
            Rvalue::Aggregate {
                kind:
                    AggregateKind::Class {
                        class,
                        type_arg_templates,
                    },
                fields,
            } if type_arg_templates.is_empty() => {
                let (c, _) = self
                    .plan
                    .index
                    .class(self.db, *class)
                    .ok_or_else(|| no("class"))?;
                let layout = &self.plan.layouts[&c];
                if fields.len() != layout.fields.len() {
                    return Err(no("partial class literal"));
                }
                let mut init = String::from("{");
                // Bind every input before allocation: roots stay in addressable Rust locals.
                for (i, v) in fields.iter().enumerate() {
                    let v = self.coerce(self.operand(v)?, layout.fields[i])?;
                    write!(&mut init, "let a{i}={v};").unwrap();
                }
                write!(
                    &mut init,
                    "let p={} as *mut C{c};",
                    self.fallible(&format!(
                        "rt::try_alloc({}, {})",
                        self.descriptors[&c], layout.bytes
                    ))
                )
                .unwrap();
                for (i, t) in layout.fields.iter().enumerate() {
                    if t.pointer() {
                        write!(
                            &mut init,
                            "bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f{i}).cast(),a{i});"
                        )
                        .unwrap();
                    } else if matches!(t, Repr::Dyn(_)) {
                        write!(
                            &mut init,
                            "rt::write_dyn(p.cast(),&raw mut (*p).f{i},a{i});"
                        )
                        .unwrap();
                    } else {
                        write!(&mut init, "(*p).f{i}=a{i} as _;").unwrap();
                    }
                }
                init.push_str("p as usize}");
                Ok((init, Repr::Ptr(c)))
            }
            _ => Err(no(format!("rvalue {r:?}"))),
        }
    }
    fn function(&mut self, arity: usize, inline: bool) -> Gen<String> {
        let cfg = Cfg {
            entry: self.body.entry.0,
            unwinds: vec![None; self.body.blocks.len()],
            flows: self
                .body
                .blocks
                .iter()
                .map(|b| match b.terminator.as_ref().unwrap() {
                    Terminator::Goto { target }
                    | Terminator::Call { target, .. }
                    | Terminator::VirtualCall { target, .. } => Ok(Flow::Goto(target.0)),
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => Ok(Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
                    }),
                    Terminator::ShortCircuit { eval_rhs, join, .. } => Ok(Flow::Branch {
                        then_block: join.0,
                        else_block: eval_rhs.0,
                    }),
                    Terminator::Return => Ok(Flow::Exit),
                    _ => Err(no("control flow")),
                })
                .collect::<Gen<_>>()?,
        };
        let structured = structurize(&cfg).map_err(no)?;
        let positions = self.plan.index.positions(self.object);
        if positions.block_pcs.len() != self.body.blocks.len() {
            return Err(no("missing/mismatched MIR source positions"));
        }
        let params = (1..=arity)
            .map(|i| Ok(format!("mut l{i}:{}", self.local(i)?.rust())))
            .collect::<Gen<Vec<_>>>()?
            .join(",");
        let attribute = if inline { "#[inline(always)]\n" } else { "" };
        let mut code = format!(
            "{attribute}unsafe fn f{}({params})->R<{}>{{unsafe{{rt::observe(&O{});rt::check_stack().map_err(|e|e.at(&S{},0))?;\n",
            self.object,
            self.local(0)?.rust(),
            self.object,
            self.object
        );
        for i in 0..self.body.locals.len() {
            if i > 0 && i <= arity {
                continue;
            }
            let t = self.local(i)?;
            writeln!(&mut code, "let mut l{i}:{}={};", t.rust(), t.zero()).unwrap();
        }
        for b in &self.body.blocks {
            if matches!(
                b.terminator,
                Some(Terminator::Branch { .. } | Terminator::ShortCircuit { .. })
            ) {
                writeln!(&mut code, "let mut c{}=false;", b.id.0).unwrap();
            }
        }
        for h in cfg.loop_headers() {
            writeln!(&mut code, "let mut poll{h}=0u32;").unwrap();
        }
        code.push_str(&self.stmts(&structured.stmts, &cfg.loop_headers())?);
        code.push_str("}}\n");
        let lines = self
            .sites
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        Ok(format!(
            "static O{}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);\nstatic S{}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{lines}]}};\n{code}",
            self.object,
            self.object,
            self.plan.index.info(self.object).source_file,
            self.plan.index.display(self.object)
        ))
    }
    fn stmts(&mut self, stmts: &[Stmt], headers: &BTreeSet<usize>) -> Gen<String> {
        let mut out = String::new();
        for s in stmts {
            match s {
                Stmt::Leaf(b) => out.push_str(&self.leaf(*b, headers)?),
                Stmt::Block { label, body } => {
                    writeln!(&mut out, "'b{label}:{{ {} }}", self.stmts(body, headers)?).unwrap();
                }
                Stmt::Loop { label, body } => writeln!(
                    &mut out,
                    "'l{label}:loop{{ {} }}",
                    self.stmts(body, headers)?
                )
                .unwrap(),
                Stmt::If {
                    block,
                    then_branch,
                    else_branch,
                } => writeln!(
                    &mut out,
                    "if c{block}{{ {} }}else{{ {} }}",
                    self.stmts(then_branch, headers)?,
                    self.stmts(else_branch, headers)?
                )
                .unwrap(),
                Stmt::Break(l) => writeln!(&mut out, "break 'b{l};").unwrap(),
                Stmt::BreakLoop(l) => writeln!(&mut out, "break 'l{l};").unwrap(),
                Stmt::Continue(l) => writeln!(&mut out, "continue 'l{l};").unwrap(),
                Stmt::Exit(_) => out.push_str("return Ok(l0);\n"),
                Stmt::Switch { .. } => return Err(no("structured switch")),
            }
        }
        Ok(out)
    }
    fn leaf(&mut self, b: usize, headers: &BTreeSet<usize>) -> Gen<String> {
        let block = &self.body.blocks[b];
        let pcs = &self.plan.index.positions(self.object).block_pcs[b];
        self.mark(pcs.start);
        let mut out = String::new();
        if headers.contains(&b) {
            writeln!(
                &mut out,
                "poll{b}=poll{b}.wrapping_add(1);if poll{b}&15==0{{{};}}",
                self.fallible("rt::poll()")
            )
            .unwrap();
        }
        for (i, s) in block.statements.iter().enumerate() {
            self.mark(pcs.statements.get(i).copied().flatten());
            match &s.kind {
                StatementKind::Assign { destination, value } => {
                    out.push_str(
                        &self
                            .assign(destination, self.value(value, self.place(destination)?.1)?)?,
                    );
                }
                StatementKind::Intrinsic {
                    op:
                        baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(
                            baml_compiler2_mir::BuiltinTraceHook::Hidden,
                        ),
                    args,
                } if args.is_empty() => {} // Hidden observation branch is in the prologue.
                StatementKind::Nop => {}
                _ => return Err(no(format!("statement {:?}", s.kind))),
            }
        }
        self.mark(pcs.terminator);
        match block.terminator.as_ref().unwrap() {
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let (v, t) = self.operand(operand)?;
                if t != Repr::Bool {
                    return Err(no("non-boolean short circuit"));
                }
                let short = match kind {
                    baml_compiler2_mir::ShortCircuitKind::And => format!("!({v})"),
                    baml_compiler2_mir::ShortCircuitKind::Or => v.clone(),
                    baml_compiler2_mir::ShortCircuitKind::Coalesce => return Err(no("coalesce")),
                };
                let store = self.assign(destination, (v, t))?;
                writeln!(&mut out, "c{b}={short};if c{b}{{{store}}}").unwrap();
            }
            Terminator::Branch { condition, .. } => {
                let (v, t) = self.operand(condition)?;
                if t != Repr::Bool {
                    return Err(no("branch condition"));
                }
                writeln!(&mut out, "c{b}={v};").unwrap();
            }
            Terminator::Call {
                callee: Operand::Constant(Constant::Function(f)),
                args,
                argument_layout,
                ..
            } => {
                let (c, info) = self
                    .plan
                    .index
                    .function(self.db, *f)
                    .ok_or_else(|| no("call"))?;
                let mut values = args
                    .iter()
                    .map(|a| self.operand(a))
                    .collect::<Gen<Vec<_>>>()?;
                if let Some(layout) = argument_layout {
                    let mapping = layout.map_to(&info.argument_layout()).map_err(no)?;
                    values = mapping
                        .into_iter()
                        .map(|i| {
                            i.map(|i| values[i].clone())
                                .ok_or_else(|| no("omitted argument"))
                        })
                        .collect::<Gen<_>>()?;
                }
                let name = self.plan.index.display(c);
                let expression = if name == "baml.String.char_count" {
                    if values.len() != 1 || values[0].1 != Repr::Str {
                        return Err(no("String.char_count arguments"));
                    }
                    (
                        format!(
                            "v2_string_char_count(&rt::strings::Cx::current(),rt::strings::Str::from_raw({}))",
                            values[0].0
                        ),
                        Repr::Int,
                    )
                } else if name == "baml.String.slice" {
                    if values.len() != 3
                        || values[0].1 != Repr::Str
                        || values[1].1 != Repr::Int
                        || values[2].1 != Repr::Int
                    {
                        return Err(no("String.slice arguments"));
                    }
                    (self.fallible(&format!("v2_string_slice(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw({}),{},{}).map(|s|s.as_raw())", values[0].0,values[1].0,values[2].0)),Repr::Str)
                } else if name == "baml.ops.equals_equals" {
                    if values.len() != 2 || !values.iter().any(|(_, r)| *r == Repr::Null) {
                        return Err(no("equality other than null test"));
                    }
                    if !values.iter().all(|(_, r)| r.pointer() || *r == Repr::Null) {
                        return Err(no("non-pointer null test"));
                    }
                    (format!("({}=={})", values[0].0, values[1].0), Repr::Bool)
                } else {
                    let mir = self.all[&c];
                    let MirFunctionKind::Bytecode(body) = &mir.kind else {
                        return Err(no("builtin"));
                    };
                    if values.len() != mir.arity {
                        return Err(no("call arity"));
                    }
                    for (i, value) in values.iter_mut().enumerate() {
                        let expected = self.plan.repr(&body.locals[i + 1].ty)?;
                        *value = (self.coerce(value.clone(), expected)?, expected);
                    }
                    (
                        self.fallible(&format!(
                            "f{c}({})",
                            values
                                .iter()
                                .map(|(v, _)| v.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        )),
                        self.plan.repr(&body.locals[0].ty)?,
                    )
                };
                let Terminator::Call { destination, .. } = block.terminator.as_ref().unwrap()
                else {
                    unreachable!()
                };
                out.push_str(&self.assign(destination, expression)?);
            }
            Terminator::VirtualCall {
                iface,
                method,
                args,
                destination,
                argument_layout,
                self_arg: 0,
                ntypeargs: 0,
                has_trace: false,
                ..
            } if iface.generics.is_empty() => {
                let interface = self
                    .plan
                    .index
                    .interface_at(&iface.name)
                    .ok_or_else(|| no("virtual interface"))?;
                let targets = self.plan.targets(interface, method)?;
                let mut expr = String::from("{");
                let mut reps = Vec::new();
                for (i, arg) in args.iter().enumerate() {
                    let value = self.operand(arg)?;
                    let (v, repr) = if i == 0 {
                        (
                            self.coerce(value, Repr::Dyn(interface))?,
                            Repr::Dyn(interface),
                        )
                    } else {
                        value
                    };
                    write!(&mut expr, "let a{i}={v};").unwrap();
                    reps.push(repr);
                }
                expr.push_str("match a0.tag{");
                let mut returned = None;
                for (class, callee) in targets {
                    let mir = self
                        .all
                        .get(&callee)
                        .ok_or_else(|| no("dispatch body missing"))?;
                    let MirFunctionKind::Bytecode(body) = &mir.kind else {
                        return Err(no("non-bytecode dispatch body"));
                    };
                    let mut values: Vec<_> = reps
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            if i == 0 {
                                ("a0.payload as usize".into(), Repr::Ptr(class))
                            } else {
                                (format!("a{i}"), *t)
                            }
                        })
                        .collect();
                    if let Some(layout) = argument_layout {
                        let mapping = layout
                            .map_to(&self.plan.index.info(callee).argument_layout())
                            .map_err(no)?;
                        values = mapping
                            .into_iter()
                            .map(|i| {
                                i.map(|i| values[i].clone())
                                    .ok_or_else(|| no("omitted dispatch argument"))
                            })
                            .collect::<Gen<_>>()?;
                    }
                    if values.len() != mir.arity {
                        return Err(no("dispatch arity"));
                    }
                    let arguments = values
                        .into_iter()
                        .enumerate()
                        .map(|(i, v)| self.coerce(v, self.plan.repr(&body.locals[i + 1].ty)?))
                        .collect::<Gen<Vec<_>>>()?
                        .join(",");
                    let ty = self.plan.repr(&body.locals[0].ty)?;
                    if returned.is_some_and(|r| r != ty) {
                        return Err(no("dispatch return conversion"));
                    }
                    returned = Some(ty);
                    write!(
                        &mut expr,
                        "{}=>f{callee}({arguments}),",
                        self.descriptors[&class] + 3
                    )
                    .unwrap();
                }
                expr.push_str(
                    "_=>Err(rt::Error::internal(\"invalid closed-world interface tag\".into()))}}",
                );
                out.push_str(&self.assign(
                    destination,
                    (
                        self.fallible(&expr),
                        returned.ok_or_else(|| no("empty dispatch"))?,
                    ),
                )?);
            }
            Terminator::Return | Terminator::Goto { .. } => {}
            _ => return Err(no("terminator")),
        }
        Ok(out)
    }
}

fn recursive(calls: &BTreeMap<usize, BTreeSet<usize>>, function: usize) -> bool {
    let mut pending: Vec<_> = calls
        .get(&function)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(callee) = pending.pop() {
        if callee == function {
            return true;
        }
        if seen.insert(callee) {
            pending.extend(calls.get(&callee).into_iter().flatten().copied());
        }
    }
    false
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    #[test]
    fn recursion_including_indirect_cycles_keeps_real_frames() {
        let calls = BTreeMap::from([
            (0, BTreeSet::from([1])),
            (1, BTreeSet::from([2])),
            (2, BTreeSet::from([1])),
            (3, BTreeSet::from([3])),
        ]);
        assert!(!recursive(&calls, 0));
        assert!(recursive(&calls, 1));
        assert!(recursive(&calls, 2));
        assert!(recursive(&calls, 3));
        assert!(!recursive(&calls, 4));
    }
}
