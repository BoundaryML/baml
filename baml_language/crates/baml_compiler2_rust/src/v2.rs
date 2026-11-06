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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repr {
    Int,
    Bool,
    Ptr(usize),
    Null,
}
impl Repr {
    fn rust(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Bool => "bool",
            _ => "usize",
        }
    }
    fn zero(self) -> &'static str {
        if self == Self::Bool { "false" } else { "0" }
    }
    fn pointer(self) -> bool {
        matches!(self, Self::Ptr(_))
    }
}
struct Layout {
    fields: Vec<Repr>,
    offsets: Vec<usize>,
    bytes: usize,
    pointers: usize,
}
struct Plan<'a> {
    index: ProgramIndex<'a>,
    program: &'a Program,
    layouts: BTreeMap<usize, Layout>,
}
impl Plan<'_> {
    fn repr(&self, ty: &RuntimeTy) -> Gen<Repr> {
        Ok(match ty {
            RuntimeTy::Int | RuntimeTy::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            RuntimeTy::Bool | RuntimeTy::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            RuntimeTy::Null => Repr::Null,
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
        for pointer in [true, false] {
            for (i, repr) in fields.iter().enumerate() {
                if repr.pointer() == pointer {
                    offsets[i] = offset;
                    offset += 8;
                }
            }
        }
        let children: Vec<_> = fields
            .iter()
            .filter_map(|r| if let Repr::Ptr(c) = r { Some(*c) } else { None })
            .collect();
        self.layouts.insert(
            object,
            Layout {
                pointers: children.len(),
                fields,
                offsets,
                bytes: offset,
            },
        );
        for child in children {
            self.add(child)?;
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
        index: ProgramIndex::new(db, root, program),
        program,
        layouts: BTreeMap::new(),
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
        for local in &body.locals {
            if local.is_captured {
                return Err(no("captured local"));
            }
            if let Repr::Ptr(c) = plan.repr(&local.ty)? {
                plan.add(c)?;
            }
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
                    if plan.index.display(callee) != "baml.ops.equals_equals" {
                        queue.push_back(callee);
                    }
                }
                Some(Terminator::Goto { .. } | Terminator::Branch { .. } | Terminator::Return) => {}
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
    let descriptors: BTreeMap<_, _> = plan
        .layouts
        .keys()
        .enumerate()
        .map(|(i, c)| (*c, i + rt_descriptors()))
        .collect();
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,\n");
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
                    } else {
                        "Scalar"
                    },
                    match layout.fields[i] {
                        Repr::Ptr(c) => descriptors[&c],
                        Repr::Int => descriptors.len() + rt_descriptors(),
                        Repr::Bool => descriptors.len() + rt_descriptors() + 1,
                        Repr::Null => unreachable!("null fields are rejected"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(&mut out,"Descriptor{{kind:Kind::Class,size:{},ptr_slots:(8,{}),tagged_slots:(0,0),elem:None,type_id:{},fields:&[{fields}],finalizer:None}},",layout.bytes,layout.pointers,descriptors[&object]).unwrap();
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
            all: &all,
            sites: Vec::new(),
            site: 0,
        };
        out.push_str(&emitter.function(mir.arity)?);
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
    5
}
struct Emitter<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    body: &'a MirFunctionBody<'db>,
    object: usize,
    descriptors: &'a BTreeMap<usize, usize>,
    all: &'a BTreeMap<usize, &'db MirFunction<'db>>,
    sites: Vec<usize>,
    site: usize,
}
impl Emitter<'_, '_> {
    fn local(&self, i: usize) -> Gen<Repr> {
        self.plan.repr(&self.body.locals[i].ty)
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
            _ => Err(no(format!("place {p:?}"))),
        }
    }
    fn operand(&self, o: &Operand<'_>) -> Gen<(String, Repr)> {
        match o {
            Operand::Copy(p) | Operand::Move(p) => self.place(p),
            Operand::Constant(Constant::Int(i)) => Ok((format!("{i}i64"), Repr::Int)),
            Operand::Constant(Constant::Bool(b)) => Ok((b.to_string(), Repr::Bool)),
            Operand::Constant(Constant::Null) => Ok(("0usize".into(), Repr::Null)),
            Operand::Constant(_) => Err(no(format!("operand {o:?}"))),
        }
    }
    fn assign(&self, p: &Place, value: (String, Repr)) -> Gen<String> {
        let (dst, repr) = self.place(p)?;
        let (src, source) = value;
        if repr != source && !(repr.pointer() && source == Repr::Null) {
            return Err(no(format!("conversion {source:?} to {repr:?}")));
        }
        match p {
            Place::Local(_) => Ok(format!("{dst}={src};\n")),
            Place::Field { base, field } => {
                let (obj, Repr::Ptr(c)) = self.place(base)? else {
                    return Err(no("field store"));
                };
                let obj = self.fallible(&format!("rt::nonnull({obj})"));
                let slot = format!("&raw mut (*({obj} as *mut C{c})).f{field}");
                Ok(if repr.pointer() {
                    format!("bex_v2_heap::write_ptr({obj} as *mut u8,{slot},{src});\n")
                } else {
                    format!("({slot}).write({src} as _);\n")
                })
            }
            _ => Err(no("store")),
        }
    }
    fn value(&self, r: &Rvalue<'_>) -> Gen<(String, Repr)> {
        match r {
            Rvalue::Use(o) => self.operand(o),
            Rvalue::BinaryOp { op, left, right } => {
                let (l, lt) = self.operand(left)?;
                let (r, rt) = self.operand(right)?;
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
                    let (v, t) = self.operand(v)?;
                    if t != layout.fields[i] && !(t == Repr::Null && layout.fields[i].pointer()) {
                        return Err(no("class field conversion"));
                    }
                    write!(&mut init, "let a{i}={v};").unwrap();
                }
                write!(
                    &mut init,
                    "let p=rt::alloc({}, {}) as *mut C{c};",
                    self.descriptors[&c], layout.bytes
                )
                .unwrap();
                for (i, t) in layout.fields.iter().enumerate() {
                    if t.pointer() {
                        write!(
                            &mut init,
                            "bex_v2_heap::write_ptr(p.cast(),(&raw mut (*p).f{i}).cast(),a{i});"
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
    fn function(&mut self, arity: usize) -> Gen<String> {
        let cfg = Cfg {
            entry: self.body.entry.0,
            unwinds: vec![None; self.body.blocks.len()],
            flows: self
                .body
                .blocks
                .iter()
                .map(|b| match b.terminator.as_ref().unwrap() {
                    Terminator::Goto { target } | Terminator::Call { target, .. } => {
                        Ok(Flow::Goto(target.0))
                    }
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => Ok(Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
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
        let mut code = format!(
            "unsafe fn f{}({params})->R<{}>{{unsafe{{rt::observe(&O{});rt::check_stack().map_err(|e|e.at(&S{},0))?;\n",
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
            if matches!(b.terminator, Some(Terminator::Branch { .. })) {
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
                    out.push_str(&self.assign(destination, self.value(value)?)?);
                }
                StatementKind::Nop => {}
                _ => return Err(no(format!("statement {:?}", s.kind))),
            }
        }
        self.mark(pcs.terminator);
        match block.terminator.as_ref().unwrap() {
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
                let expression = if self.plan.index.display(c) == "baml.ops.equals_equals" {
                    if values.len() != 2 || !values.iter().any(|(_, r)| *r == Repr::Null) {
                        return Err(no("equality other than null test"));
                    }
                    if !values.iter().all(|(_, r)| r.pointer() || *r == Repr::Null) {
                        return Err(no("non-pointer null test"));
                    }
                    (format!("({}=={})", values[0].0, values[1].0), Repr::Bool)
                } else {
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
                    let mir = self.all[&c];
                    let MirFunctionKind::Bytecode(body) = &mir.kind else {
                        return Err(no("builtin"));
                    };
                    if values.len() != mir.arity {
                        return Err(no("call arity"));
                    }
                    for (i, (_, ty)) in values.iter().enumerate() {
                        let expected = self.plan.repr(&body.locals[i + 1].ty)?;
                        if *ty != expected && !(*ty == Repr::Null && expected.pointer()) {
                            return Err(no("call type conversion"));
                        }
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
            Terminator::Return | Terminator::Goto { .. } => {}
            _ => return Err(no("terminator")),
        }
        Ok(out)
    }
}
