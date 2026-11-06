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
    ListStr,
    ListListStr,
    Type(usize),
    ListPtr(usize),
    Dyn(usize),
    Tagged,
    MapStrInt,
    Iter,
    Naming,
    Null,
}
impl Repr {
    fn rust(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Bool => "bool",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn",
            _ => "usize",
        }
    }
    fn zero(self) -> &'static str {
        match self {
            Self::Bool => "false",
            Self::Dyn(_) | Self::Tagged => "rt::Dyn{tag:2,payload:0}",
            _ => "0",
        }
    }
    fn growing(self) -> bool {
        matches!(self, Self::ListStr | Self::ListListStr)
    }
    fn slot(self) -> Gen<&'static str> {
        match self {
            Self::Str => Ok("rt::strings::Str"),
            Self::ListStr => Ok("rt::lists::List<rt::strings::Str>"),
            _ => Err(no("growing list element")),
        }
    }
    fn pointer(self) -> bool {
        matches!(
            self,
            Self::Str
                | Self::Ptr(_)
                | Self::ListInt
                | Self::ListPtr(_)
                | Self::ListStr
                | Self::ListListStr
                | Self::MapStrInt
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
    json_types: Vec<Vec<usize>>,
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
    fn record_ty(&mut self, ty: &RuntimeTy) -> Gen<()> {
        if let RuntimeTy::Union(members) = ty {
            for member in members {
                self.record_ty(member)?;
            }
        } else if !matches!(ty, RuntimeTy::Function { .. }) {
            self.record(self.repr(ty)?)?;
        }
        Ok(())
    }
    fn record_field_ty(&mut self, ty: &bex_vm_types::RuntimeTy) -> Gen<()> {
        if let bex_vm_types::RuntimeTy::Union(members) = ty {
            for member in members {
                self.record_field_ty(member)?;
            }
        } else {
            self.record(self.field_repr(ty)?)?;
        }
        Ok(())
    }
    fn record(&mut self, repr: Repr) -> Gen<()> {
        match repr {
            Repr::Ptr(c) => self.add(c)?,
            Repr::ListPtr(c) => {
                self.lists.insert(repr);
                self.add(c)?;
            }
            Repr::MapStrInt => {
                self.lists.insert(repr);
                self.record(Repr::ListInt)?;
            }
            Repr::ListInt | Repr::ListStr => {
                self.lists.insert(repr);
            }
            Repr::ListListStr => {
                self.lists.insert(repr);
                self.record(Repr::ListStr)?;
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
    fn type_code(&self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<usize> {
        use baml_compiler2_mir::TyTemplate as T;
        match ty {
            T::Int => Ok(0),
            T::String => Ok(1),
            T::List(t) if matches!(t.as_ref(), T::String) => Ok(2),
            T::Class(_, _) | T::Union(_) => {
                let classes = self.json_class_types(ty)?;
                self.json_types
                    .iter()
                    .position(|c| *c == classes)
                    .map(|i| i + 3)
                    .ok_or_else(|| no("unregistered JSON specialization"))
            }
            _ => Err(no(format!("type argument {ty:?}"))),
        }
    }
    fn json_class_types(&self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<Vec<usize>> {
        use baml_compiler2_mir::TyTemplate as T;
        let members = if let T::Union(members) = ty {
            members.as_ref()
        } else {
            std::slice::from_ref(ty)
        };
        members
            .iter()
            .map(|m| match m {
                T::Class(head, args) if args.is_empty() => self
                    .index
                    .class_at(head)
                    .map(|c| c.0)
                    .ok_or_else(|| no("JSON class")),
                _ => Err(no("JSON union member")),
            })
            .collect()
    }
    fn local_repr(&self, body: &MirFunctionBody<'_>, i: usize, depth: usize) -> Gen<Repr> {
        let definitions: Vec<_> = body
            .blocks
            .iter()
            .flat_map(|b| &b.statements)
            .filter_map(|s| match &s.kind {
                StatementKind::Assign {
                    destination: Place::Local(l),
                    value,
                } if l.0 == i => Some(value),
                _ => None,
            })
            .collect();
        if definitions.iter().any(|v| matches!(v, Rvalue::LoadType(_))) {
            if i == 0
                || body.locals[i].name.is_some()
                || body.blocks.iter().any(|b| {
                    b.terminator
                        .as_ref()
                        .and_then(crate::analysis::continuation)
                        .is_some_and(|(p, _)| matches!(p,Place::Local(l) if l.0 == i))
                })
            {
                return Err(no("runtime type value"));
            }

            let mut code = None;
            for value in definitions {
                let Rvalue::LoadType(ty) = value else {
                    return Err(no("dynamic type storage"));
                };
                let next = self.type_code(ty)?;
                if code.is_some_and(|c| c != next) {
                    return Err(no("dynamic type argument"));
                }
                code = Some(next);
            }
            return Ok(Repr::Type(code.unwrap()));
        }
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
            RuntimeTy::Unknown => Repr::Tagged,
            RuntimeTy::Map { key, value }
                if matches!(key.as_ref(), RuntimeTy::String)
                    && matches!(value.as_ref(), RuntimeTy::Int) =>
            {
                Repr::MapStrInt
            }
            RuntimeTy::Null => Repr::Null,
            RuntimeTy::List(element) => match element.as_ref() {
                RuntimeTy::Int => Repr::ListInt,
                RuntimeTy::String => Repr::ListStr,
                RuntimeTy::List(t) if matches!(t.as_ref(), RuntimeTy::String) => Repr::ListListStr,
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
            RuntimeTy::Enum(head)
                if Some(head.root()) == self.lang.get(baml_base::LangPackage::Baml)
                    && head.namespace().as_slice() == ["json"]
                    && head.name().as_str() == "Naming" =>
            {
                Repr::Naming
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
                if repr.pointer() { repr } else { Repr::Tagged }
            }
            RuntimeTy::Union(members) => {
                for member in members {
                    self.repr(member)?;
                }
                Repr::Tagged
            }
            _ => return Err(no(format!("type {ty:?}"))),
        })
    }
    fn field_repr(&self, ty: &bex_vm_types::RuntimeTy) -> Gen<Repr> {
        use bex_vm_types::RuntimeTy as T;
        Ok(match ty {
            T::Null => Repr::Null,
            T::Int | T::Literal(baml_type::Literal::Int(_), _) => Repr::Int,
            T::Bool | T::Literal(baml_type::Literal::Bool(_), _) => Repr::Bool,
            T::String | T::Literal(baml_type::Literal::String(_), _) => Repr::Str,
            T::Map { key, value }
                if matches!(key.as_ref(), T::String) && matches!(value.as_ref(), T::Int) =>
            {
                Repr::MapStrInt
            }
            T::List(element) => match element.as_ref() {
                T::Int => Repr::ListInt,
                T::String => Repr::ListStr,
                T::List(t) if matches!(t.as_ref(), T::String) => Repr::ListListStr,
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
                if repr.pointer() { repr } else { Repr::Tagged }
            }
            T::Union(members) => {
                for member in members {
                    self.field_repr(member)?;
                }
                Repr::Tagged
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
        // Runtime-produced exception descriptors are reserved. Phase one supports
        // wildcard panic exclusion and user-defined typed catches; never emit a
        // second, incompatible layout/tag for a builtin exception class.
        if self.program.packages.iter().any(|package| {
            package.name == "baml"
                && package
                    .objects
                    .contains(&u32::try_from(object).expect("object index"))
        }) && (class.path.to_string().starts_with("panics.")
            || matches!(
                class.path.to_string().as_str(),
                "json.ParseError" | "json.DecodeError"
            ))
        {
            return Err(no("typed builtin exception class"));
        }
        if class.cleanup.is_some() || class.generic_param_count != 0 {
            return Err(no("generic class or cleanup"));
        }
        let fields = class
            .fields
            .iter()
            .map(|f| self.field_repr(&f.field_type))
            .collect::<Gen<Vec<_>>>()?;
        if fields.contains(&Repr::Null) {
            return Err(no("null-only field"));
        }
        let mut offsets = vec![0; fields.len()];
        let mut offset = 8;
        for group in 0..3 {
            for (i, repr) in fields.iter().enumerate() {
                let g = if repr.pointer() {
                    0
                } else if matches!(repr, Repr::Dyn(_) | Repr::Tagged) {
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
        let field_types: Vec<_> = class.fields.iter().map(|f| f.field_type.clone()).collect();
        self.layouts.insert(
            object,
            Layout {
                pointers: fields.iter().filter(|r| r.pointer()).count(),
                tagged: fields
                    .iter()
                    .filter(|r| matches!(r, Repr::Dyn(_) | Repr::Tagged))
                    .count(),
                fields,
                offsets,
                bytes: offset,
            },
        );
        for ty in &field_types {
            self.record_field_ty(ty)?;
        }
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
        json_types: Vec::new(),
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
    let mut json_decode = None;
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
        for statement in body.blocks.iter().flat_map(|b| &b.statements) {
            let unsupported_hook = match &statement.kind {
                StatementKind::Intrinsic {
                    op: baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(mode),
                    ..
                } => *mode != baml_compiler2_mir::BuiltinTraceHook::Hidden,
                StatementKind::Intrinsic {
                    op: baml_compiler2_mir::IntrinsicOp::ApplyTraceHook,
                    ..
                }
                | StatementKind::Assign {
                    value: Rvalue::TraceHookSettings { .. },
                    ..
                } => true,
                _ => false,
            };
            if unsupported_hook {
                return Err(no("declaration hooks"));
            }

            if let StatementKind::Assign {
                value:
                    Rvalue::LoadType(
                        ty @ (baml_compiler2_mir::TyTemplate::Union(_)
                        | baml_compiler2_mir::TyTemplate::Class(..)),
                    ),
                ..
            } = &statement.kind
            {
                let classes = plan.json_class_types(ty)?;
                for &c in &classes {
                    plan.add(c)?;
                }
                if !plan.json_types.contains(&classes) {
                    plan.json_types.push(classes);
                }
            }
        }
        let iters = crate::analysis::array_iterators(db, body, mir.arity);
        for (i, local) in body.locals.iter().enumerate() {
            if local.is_captured {
                return Err(no("captured local"));
            }
            if iters.contains(&i) {
                continue;
            }
            let repr = plan.local_repr(body, i, 0)?;
            if !matches!(repr, Repr::Type(_)) {
                plan.record_ty(&local.ty)?;
            }
            plan.record(repr)?;
        }
        for block in &body.blocks {
            if block.shielded {
                return Err(no("defer/shielding"));
            }
            if let Some(term) = &block.terminator
                && (crate::analysis::array_iter_walk(db, body, mir.arity, term, &iters).is_some()
                    || crate::analysis::array_next_call(db, term, &iters).is_some())
            {
                continue;
            }
            match &block.terminator {
                Some(Terminator::Call {
                    callee: Operand::Constant(Constant::Function(f)),
                    ntypeargs,
                    has_trace: false,
                    ..
                }) => {
                    let (callee, _) = plan
                        .index
                        .function(db, *f)
                        .ok_or_else(|| no("unlinked call"))?;
                    let name = plan.index.display(callee);
                    if name == "baml.json.decode" {
                        json_decode = Some(callee);
                    } else if baml_builtins2_codegen::supports_v2(name) {
                        builtins.insert(name.to_owned());
                    } else if *ntypeargs != 0 {
                        return Err(no("generic user call"));
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
                    | Terminator::Return
                    | Terminator::Throw { .. }
                    | Terminator::Rethrow { .. },
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
        .scan(descriptors.len() + rt_descriptors(), |next, r| {
            let id = *next;
            *next += if *r == Repr::MapStrInt {
                3
            } else if r.growing() {
                2
            } else {
                1
            };
            Some((*r, id))
        })
        .collect();
    let after_lists = lists
        .iter()
        .map(|(repr, id)| {
            id + if *repr == Repr::MapStrInt {
                3
            } else if repr.growing() {
                2
            } else {
                1
            }
        })
        .max()
        .unwrap_or(rt_descriptors() + descriptors.len());
    out.push_str("static DESCRIPTORS: &[Descriptor] = &[rt::STRING_DESC,rt::OVERFLOW_DESC,rt::SHIFT_DESC,rt::CANCEL_DESC,rt::STACK_DESC,rt::ALLOC_DESC,rt::INDEX_DESC,rt::json::PARSE_DESC,rt::json::DECODE_DESC,rt::DIV_DESC,\n");
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
                    } else if matches!(layout.fields[i], Repr::Dyn(_) | Repr::Tagged) {
                        "Tagged"
                    } else {
                        "Scalar"
                    },
                    match layout.fields[i] {
                        Repr::Ptr(c) => descriptors[&c] + 3,
                        r @ (Repr::ListInt
                        | Repr::ListPtr(_)
                        | Repr::ListStr
                        | Repr::ListListStr
                        | Repr::MapStrInt) => lists[&r] + 3,
                        Repr::Tagged => after_lists + 3,
                        Repr::Dyn(i) => after_lists + i + 4,
                        Repr::Str => 3,
                        Repr::Int => 0,
                        Repr::Bool => 1,
                        Repr::Iter | Repr::Naming | Repr::Null | Repr::Type(_) =>
                            unreachable!("null/type fields are rejected"),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        writeln!(&mut out,"Descriptor{{kind:Kind::Class,size:{},ptr_slots:(8,{}),tagged_slots:({},{}),elem:None,type_id:{},fields:&[{fields}],finalizer:None}},",layout.bytes,layout.pointers,8+layout.pointers*8,layout.tagged,descriptors[&object]+3).unwrap();
    }
    for (repr, desc) in &lists {
        if *repr == Repr::MapStrInt {
            for i in 0..3 {
                writeln!(&mut out, "rt::maps::descriptors({desc})[{i}],").unwrap();
            }
            continue;
        }
        if repr.growing() {
            let slot = Emitter::element(*repr)?.slot()?;
            writeln!(
                &mut out,
                "rt::lists::descriptor({}),rt::lists::buffer_descriptor::<{slot}>({}),",
                desc + 3,
                desc + 4
            )
            .unwrap();
            continue;
        }
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
    if let Some(callee) = json_decode {
        if plan.json_types.is_empty() {
            return Err(no("missing JSON specialization"));
        }
        for (specialization, json_classes) in plan.json_types.iter().enumerate() {
            let code = specialization + 3;
            for aliased in [false, true] {
                let mode = usize::from(aliased);
                let schema_name = format!("JSON_SCHEMA_{code}_{mode}");
                writeln!(&mut out, "static {schema_name}:&[rt::json::Schema]=&[").unwrap();
                for c in json_classes {
                    let Object::Class(class) =
                        &program.objects[bex_vm_types::ObjectIndex::from_raw(*c)]
                    else {
                        unreachable!()
                    };
                    for package in &program.packages {
                        for rules in package.impl_rules.values() {
                            for rule in rules {
                                if matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Class(head,_) if head.tag()==class.type_tag)
                                {
                                    return Err(no(
                                        "JSON class with custom interface implementations",
                                    ));
                                }
                            }
                        }
                    }
                    let name = class.path.to_string();
                    let layout = &plan.layouts[c];
                    write!(
                        &mut out,
                        "rt::json::Schema{{name:{name:?},desc:{},bytes:{},fields:&[",
                        descriptors[c], layout.bytes
                    )
                    .unwrap();
                    for (i, field) in class.fields.iter().enumerate() {
                        use bex_vm_types::RuntimeTy as T;
                        let ty = match &field.field_type {
                            T::String => "rt::json::Ty::String".into(),
                            T::Int => "rt::json::Ty::Int".into(),
                            T::Literal(baml_type::Literal::String(value), _) => {
                                format!("rt::json::Ty::Literal({:?})", value.as_str())
                            }
                            T::Union(m)
                                if m.len() == 2
                                    && m.iter().any(|t| matches!(t, T::Null))
                                    && m.iter().any(|t| matches!(t, T::Int)) =>
                            {
                                "rt::json::Ty::OptionalInt".into()
                            }
                            T::Union(m) => {
                                let members = m
                                    .iter()
                                    .map(|t| match t {
                                        T::Literal(baml_type::Literal::String(value), _) => {
                                            Ok(format!("{:?}", value.as_str()))
                                        }
                                        _ => Err(no("JSON field union")),
                                    })
                                    .collect::<Gen<Vec<_>>>()?;
                                format!("rt::json::Ty::Literals(&[{}])", members.join(","))
                            }
                            _ => return Err(no("JSON field type")),
                        };
                        write!(
                            &mut out,
                            "rt::json::SchemaField{{name:{:?},offset:{},ty:{ty},skip:{}}},",
                            if aliased {
                                field.alias.as_ref().unwrap_or(&field.name)
                            } else {
                                &field.name
                            },
                            layout.offsets[i],
                            aliased && field.skip
                        )
                        .unwrap();
                    }
                    out.push_str("]},\n");
                }
                out.push_str("];\n");
                let fallback = aliased
                    && json_classes.iter().any(|c| {
                        let Object::Class(class) =
                            &program.objects[bex_vm_types::ObjectIndex::from_raw(*c)]
                        else {
                            unreachable!()
                        };
                        class.fields.iter().any(|f| f.skip)
                    });
                if fallback {
                    writeln!(&mut out,"// Tree fallback: aliased @skip empty-value construction.\nunsafe fn json_specialized_{code}_{mode}(cx:&mut rt::strings::Cx,s:usize)->R<rt::Dyn>{{unsafe{{rt::json::decode(cx,rt::strings::Str::from_raw(s),{schema_name})}}}}").unwrap();
                } else {
                    // A per-specialization field matcher and fixed stack slots replace the
                    // intermediate JSON tree. Candidate checks preserve declaration-order
                    // ties and prefer the class with the most matching input keys.
                    let mut names = BTreeMap::new();
                    for c in json_classes {
                        let Object::Class(class) =
                            &program.objects[bex_vm_types::ObjectIndex::from_raw(*c)]
                        else {
                            unreachable!()
                        };
                        for field in &class.fields {
                            let next = names.len();
                            names
                                .entry(if aliased {
                                    field.alias.as_ref().unwrap_or(&field.name).clone()
                                } else {
                                    field.name.clone()
                                })
                                .or_insert(next);
                        }
                    }
                    writeln!(&mut out,"unsafe fn json_specialized_{code}_{mode}(cx:&mut rt::strings::Cx,s:usize)->R<rt::Dyn>{{let read_cx=unsafe{{rt::strings::Cx::current()}};let text=unsafe{{rt::strings::Str::from_raw(s)}};let source=text.as_str(&read_cx);let mut input=match rt::json::Stream::<{}>::scan(source,|key|match key{{",names.len()).unwrap();
                    for (name, index) in &names {
                        writeln!(&mut out, "{name:?}=>Some({index}),").unwrap();
                    }
                    out.push_str("_=>None}){Ok(input)=>input,Err(_)=>return Err(rt::json::syntax_error(cx,source))};let mut best=None;let mut score=0usize;\n");
                    let mut indices = Vec::new();
                    for (candidate, c) in json_classes.iter().enumerate() {
                        let Object::Class(class) =
                            &program.objects[bex_vm_types::ObjectIndex::from_raw(*c)]
                        else {
                            unreachable!()
                        };
                        let fields = class
                            .fields
                            .iter()
                            .map(|f| {
                                names[if aliased {
                                    f.alias.as_ref().unwrap_or(&f.name)
                                } else {
                                    &f.name
                                }]
                            })
                            .collect::<Vec<_>>();
                        let checks = fields
                    .iter()
                    .enumerate()
                    .map(|(i, slot)| {
                        format!("input.matches({slot},{schema_name}[{candidate}].fields[{i}].ty)")
                    })
                    .collect::<Vec<_>>()
                    .join(" && ");
                        let count = fields
                            .iter()
                            .copied()
                            .collect::<BTreeSet<_>>()
                            .iter()
                            .map(|slot| format!("input.present({slot})"))
                            .collect::<Vec<_>>()
                            .join("+");
                        writeln!(&mut out,"if input.is_object() && ({}){{let n={};if best.is_none()||n>score{{best=Some({candidate});score=n;}}}}",if checks.is_empty(){"true"}else{&checks},if count.is_empty(){"0"}else{&count}).unwrap();
                        indices.push(fields);
                    }
                    out.push_str("match best{\n");
                    for (candidate, c) in json_classes.iter().enumerate() {
                        writeln!(
                            &mut out,
                            "Some({candidate})=>{{let p=unsafe{{rt::try_alloc({},{})}}?;",
                            descriptors[c], plan.layouts[c].bytes
                        )
                        .unwrap();
                        for (i, slot) in indices[candidate].iter().enumerate() {
                            writeln!(&mut out,"unsafe{{input.store(cx,p,{}, {slot}, {schema_name}[{candidate}].fields[{i}].ty)}}?;",plan.layouts[c].offsets[i]).unwrap();
                        }
                        writeln!(
                            &mut out,
                            "Ok(rt::Dyn{{tag:{},payload:p as u64}})}},",
                            descriptors[c] + 3
                        )
                        .unwrap();
                    }
                    let indices = indices
                        .iter()
                        .map(|i| format!("&{i:?}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    writeln!(
                        &mut out,
                        "_=>Err(input.failure(cx,{schema_name},&[{indices}]))}}}}"
                    )
                    .unwrap();
                }
                let mir = all[&callee];
                let MirFunctionKind::Bytecode(body) = &mir.kind else {
                    return Err(no("JSON wrapper body"));
                };
                let mut line = None;
                for block in &body.blocks {
                    if let Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Function(f)),
                        ..
                    }) = &block.terminator
                        && let Some((target, _)) = plan.index.function(db, *f)
                        && plan.index.display(target) == "baml.json._decode"
                    {
                        line = plan.index.positions(callee).block_pcs[block.id.0]
                            .terminator
                            .map(|pc| plan.index.positions(callee).source_line_for_pc(pc));
                    }
                }
                let line = line.ok_or_else(|| no("JSON wrapper source line"))?;
                if specialization == 0 && !aliased {
                    writeln!(&mut out,"static O{callee}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);static S{callee}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}]}};",plan.index.info(callee).source_file,plan.index.display(callee)).unwrap();
                }
                if !aliased {
                    writeln!(&mut out,"unsafe fn f{callee}_json{code}(s:usize,naming:usize)->R<rt::Dyn>{{unsafe{{rt::observe(&O{callee});rt::check_stack().map_err(|e|e.at(&S{callee},0))?;if naming==0{{json_specialized_{code}_0(&mut rt::strings::Cx::current(),s)}}else{{json_specialized_{code}_1(&mut rt::strings::Cx::current(),s)}}}}}}").unwrap();
                }
            }
        }
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
    let telemetry_sites = std::cell::RefCell::new(vec![(None, 0usize, entry)]);
    for &object in &reached {
        let mir = all[&object];
        let MirFunctionKind::Bytecode(body) = &mir.kind else {
            unreachable!()
        };
        let iters = crate::analysis::array_iterators(db, body, mir.arity);
        let iter_arrays = body
            .blocks
            .iter()
            .filter_map(|b| {
                b.terminator
                    .as_ref()
                    .and_then(|t| crate::analysis::array_iter_walk(db, body, mir.arity, t, &iters))
            })
            .collect();
        let mut emitter = Emitter {
            db,
            plan: &plan,
            body,
            object,
            descriptors: &descriptors,
            lists: &lists,
            all: &all,
            iters,
            iter_arrays,
            arity: mir.arity,
            block: 0,
            unwind_jumps: Vec::new(),
            sites: Vec::new(),
            telemetry_sites: &telemetry_sites,
            site: 0,
        };
        out.push_str(&emitter.function(mir.arity, small_program && !recursive(&calls, object))?);
    }
    // The pack host formats an integer through stringify and _aliased(Declared).
    // Preserve these logical calls in the typed host specialization too.
    let host_function = |name: &str| {
        program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| {
                (matches!(o, Object::Function(_)) && plan.index.display(i) == name).then_some(i)
            })
            .ok_or_else(|| no(format!("host function {name}")))
    };
    let stringify = host_function("baml.json.stringify")?;
    let aliased = host_function("baml.json._aliased")?;
    let MirFunctionKind::Bytecode(host_body) = &all[&stringify].kind else {
        return Err(no("host stringify body"));
    };
    let alias_block = host_body
        .blocks
        .iter()
        .find(|b| {
            matches!(&b.terminator,
        Some(Terminator::Call { callee:Operand::Constant(Constant::Function(f)), .. })
        if plan.index.function(db,*f).is_some_and(|(id,_)|id==aliased))
        })
        .ok_or_else(|| no("host naming call"))?;
    let bytecode = plan.index.positions(stringify);
    let alias_pc = bytecode
        .lower_to_compact_with_offsets()
        .offset_of(bytecode.block_pcs[alias_block.id.0].terminator.unwrap())
        .unwrap();
    let format_site = telemetry_sites.borrow().len();
    telemetry_sites
        .borrow_mut()
        .extend([(None, 0, stringify), (Some(stringify), alias_pc, aliased)]);
    let mut observed = reached.clone();
    for id in [stringify, aliased] {
        if observed.insert(id) {
            writeln!(
                &mut out,
                "static O{id}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);"
            )
            .unwrap();
        }
    }
    let function_ids: BTreeMap<_, _> = observed
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();
    out.push_str("static TELEMETRY_FUNCTIONS:&[rt::telemetry::Function]=&[\n");
    let allocator = btel_types::FunctionIdAllocator::default();
    for &id in &observed {
        let mut info = plan.index.info(id).clone();
        info.bytecode.compact = Some(info.bytecode.lower_to_compact());
        let label = program
            .packages
            .iter()
            .find(|p| {
                p.objects
                    .contains(&u32::try_from(id).expect("function index"))
            })
            .map(|p| baml_base::Name::new(&p.name));
        let metadata = info.telemetry_metadata(allocator.allocate().unwrap(), label.as_ref());
        let metadata = telemetry_metadata_source(&metadata);
        let hidden = info.trace_passthrough
            || matches!(&all[&id].kind,
            MirFunctionKind::Bytecode(body) if body.blocks.iter().flat_map(|b|&b.statements).any(|s|matches!(&s.kind,
                StatementKind::Intrinsic {op:baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(baml_compiler2_mir::BuiltinTraceHook::Hidden),..})));

        writeln!(&mut out,"rt::telemetry::Function{{metadata:|function_id|{{use rt::telemetry::types::*;{metadata}}},word:&O{id},policy:rt::telemetry::Policy::NONE,hidden:{hidden}}},").unwrap();
    }
    out.push_str("];\nstatic TELEMETRY_SITES:&[rt::telemetry::Site]=&[\n");
    for &(caller, pc, callee) in telemetry_sites.borrow().iter() {
        writeln!(
            &mut out,
            "rt::telemetry::Site{{caller:{:?},pc:{pc},callee:{}}},",
            caller.map(|id| function_ids[&id]),
            function_ids[&callee]
        )
        .unwrap();
    }
    out.push_str("];\n");
    let builtin_init = if builtins.is_empty() {
        ""
    } else {
        "init_builtin_observation();"
    };
    write!(&mut out,"fn entry()->R<i64>{{unsafe{{f{entry}(0)}}}}\nfn main()->std::process::ExitCode{{{builtin_init}rt::driver_observed(DESCRIPTORS,entry,TELEMETRY_FUNCTIONS,TELEMETRY_SITES,Some({format_site}))}}\n").unwrap();
    Ok(out)
}
fn rt_descriptors() -> usize {
    10
}
struct Emitter<'a, 'db> {
    db: &'db dyn Db,
    plan: &'a Plan<'a>,
    body: &'a MirFunctionBody<'db>,
    object: usize,
    descriptors: &'a BTreeMap<usize, usize>,
    lists: &'a BTreeMap<Repr, usize>,
    all: &'a BTreeMap<usize, &'db MirFunction<'db>>,
    iters: BTreeSet<usize>,
    iter_arrays: BTreeMap<usize, usize>,
    arity: usize,
    block: usize,
    unwind_jumps: Vec<String>,
    sites: Vec<usize>,
    telemetry_sites: &'a std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
    site: usize,
}
impl Emitter<'_, '_> {
    fn telemetry_site(&self, callee: usize) -> usize {
        let bytecode = self.plan.index.positions(self.object);
        let instruction = bytecode.block_pcs[self.block]
            .terminator
            .expect("call source position");
        let pc = bytecode
            .lower_to_compact_with_offsets()
            .offset_of(instruction)
            .expect("compact site");
        let mut sites = self.telemetry_sites.borrow_mut();
        let id = sites.len();
        sites.push((Some(self.object), pc, callee));
        id
    }
    fn local(&self, i: usize) -> Gen<Repr> {
        if self
            .body
            .blocks
            .iter()
            .any(|b| b.landing.is_some_and(|l| l.error_local.0 == i))
        {
            return Ok(Repr::Tagged);
        }
        if self.iters.contains(&i) {
            return Ok(Repr::Iter);
        }
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
    fn raise(&self, error: &str) -> String {
        if let Some(handler) = self.body.blocks[self.block].unwind {
            let landing = self.body.blocks[handler.0]
                .landing
                .expect("verified handler landing");
            format!(
                "{{let error={error};if error.value==0{{return Err(error);}}let rooted=rt::CaughtError::new(error);let p=rooted.value();l{}=rt::Dyn{{tag:(*(p as *const bex_v2_heap::Header)).desc as u64+3,payload:p as u64}};caught{}=Some(rooted);{}}}",
                landing.error_local.0, handler.0, self.unwind_jumps[self.block]
            )
        } else {
            format!("return Err({error});")
        }
    }
    fn fallible(&self, code: &str) -> String {
        let error = format!("e.at(&S{},{})", self.object, self.site);
        format!(
            "match ({code}){{Ok(v)=>v,Err(e)=>{{{}}}}}",
            self.raise(&error)
        )
    }
    fn tag(&self, repr: Repr) -> Gen<usize> {
        match repr {
            Repr::Int => Ok(0),
            Repr::Bool => Ok(1),
            Repr::Null => Ok(2),
            Repr::Str => Ok(3),
            Repr::Ptr(c) => Ok(self.descriptors[&c] + 3),
            r @ (Repr::ListInt
            | Repr::ListStr
            | Repr::ListListStr
            | Repr::ListPtr(_)
            | Repr::MapStrInt) => Ok(self.lists[&r] + 3),
            _ => Err(no("static tag")),
        }
    }
    fn null_test(value: &(String, Repr)) -> String {
        if value.1 == Repr::Tagged {
            format!("({}).tag==2", value.0)
        } else if value.1.pointer() || value.1 == Repr::Null {
            format!("({})==0", value.0)
        } else {
            "false".into()
        }
    }
    fn coerce(&self, value: (String, Repr), target: Repr) -> Gen<String> {
        let (value, source) = value;
        if source == target || (source == Repr::Null && target.pointer()) {
            return Ok(value);
        }
        if target == Repr::Tagged {
            let tag = self.tag(source)?;
            let test = if source.pointer() {
                "if v==0 {2} else {TAG}".replace("TAG", &tag.to_string())
            } else {
                tag.to_string()
            };
            return Ok(format!(
                "{{let v={value};rt::Dyn{{tag:{test},payload:v as u64}}}}"
            ));
        }
        if source == Repr::Tagged {
            let tag = self.tag(target)?;
            let bits = self.fallible(&format!("rt::untag({value},{tag},{})", target.pointer()));
            return Ok(if target == Repr::Bool {
                format!("({bits}!=0)")
            } else {
                format!("({bits} as {})", target.rust())
            });
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
            Repr::ListStr => Ok(Repr::Str),
            Repr::ListListStr => Ok(Repr::ListStr),
            Repr::ListPtr(c) => Ok(Repr::Ptr(c)),
            _ => Err(no("indexed non-list")),
        }
    }
    fn place(&self, p: &Place) -> Gen<(String, Repr)> {
        match p {
            Place::Local(l) => Ok((format!("l{}", l.0), self.local(l.0)?)),
            Place::Field { base, field } => {
                let (base, repr) = self.place(base)?;
                if repr == Repr::Tagged {
                    let mut expression = format!("{{let value={base};match value.tag{{");
                    for (&class, layout) in &self.plan.layouts {
                        let Some(&field_repr) = layout.fields.get(*field) else {
                            continue;
                        };
                        let raw = format!("(*(value.payload as *const C{class})).f{field}");
                        let raw = if field_repr == Repr::Bool {
                            format!("({raw}!=0)")
                        } else {
                            raw
                        };
                        let tagged = self.coerce((raw, field_repr), Repr::Tagged)?;
                        write!(
                            &mut expression,
                            "{}=>{tagged},",
                            self.descriptors[&class] + 3
                        )
                        .unwrap();
                    }
                    let fail=self.fallible("Err::<rt::Dyn,_>(rt::Error::internal(\"invalid typed field access\".into()))");
                    write!(&mut expression, "_=>{fail}}}}}").unwrap();
                    return Ok((expression, Repr::Tagged));
                }
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
                if repr.growing() {
                    let slot = element.slot()?;
                    return Ok((self.fallible(&format!("rt::lists::List::<{slot}>::from_raw({base}).get(&rt::strings::Cx::current(),l{}).map(|v|v.as_raw())", index.0)), element));
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
            Operand::Constant(Constant::EnumVariant { enum_ref, index }) => {
                let head = baml_compiler2_hir_ty::layout::enum_head(self.db, *enum_ref);
                if self.plan.repr(&RuntimeTy::Enum(head))? != Repr::Naming || *index > 1 {
                    return Err(no("enum constant"));
                }
                Ok((format!("{index}usize"), Repr::Naming))
            }
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
                } else if matches!(repr, Repr::Dyn(_) | Repr::Tagged) {
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
                let (obj, container) = self.place(base)?;
                if container.growing() {
                    return Err(no("growing list indexed assignment"));
                }
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
            Rvalue::IsTypeTag {
                operand: Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)),
                tag,
            } if *tag == baml_type::typetag::PANIC => {
                let handler = self
                    .body
                    .blocks
                    .iter()
                    .find(|b| b.landing.is_some_and(|l| l.error_local == *local))
                    .ok_or_else(|| no("panic test outside catch"))?
                    .id
                    .0;
                Ok((
                    format!(
                        "caught{handler}.as_ref().expect(\"missing caught error\").error().class.starts_with(\"baml.panics.\")"
                    ),
                    Repr::Bool,
                ))
            }
            Rvalue::IsType { operand, test } => {
                let value = self.coerce(self.operand(operand)?, Repr::Tagged)?;
                if let baml_compiler2_mir::TypeTest::Template(
                    baml_compiler2_mir::TyTemplate::Literal(baml_type::Literal::String(literal), _),
                ) = test
                {
                    return Ok((
                        format!(
                            "{{let v={value};v.tag==3 && rt::strings::Str::from_raw(v.payload as usize).as_str(&rt::strings::Cx::current())=={:?}}}",
                            literal.as_str()
                        ),
                        Repr::Bool,
                    ));
                }
                let tag = match test {
                    baml_compiler2_mir::TypeTest::Class { class, args } if args.is_empty() => {
                        let head = baml_compiler2_hir_ty::layout::class_head(self.db, *class);
                        if Some(head.root()) == self.plan.lang.get(baml_base::LangPackage::Baml)
                            && head.namespace().as_slice() == ["iter"]
                            && head.name().as_str() == "Done"
                        {
                            "u64::MAX".to_owned()
                        } else {
                            let (c, _) = self
                                .plan
                                .index
                                .class(self.db, *class)
                                .ok_or_else(|| no("type-test class"))?;
                            self.descriptors
                                .get(&c)
                                .ok_or_else(|| no("unplanned type-test class"))?
                                .checked_add(3)
                                .unwrap()
                                .to_string()
                        }
                    }
                    _ => return Err(no("type test")),
                };
                Ok((format!("({value}).tag=={tag}"), Repr::Bool))
            }
            Rvalue::Map(_, _, entries) if expected == Repr::MapStrInt => {
                if !entries.is_empty() {
                    return Err(no("nonempty map literal"));
                }
                let desc = self.lists[&Repr::MapStrInt];
                Ok((self.fallible(&format!("rt::maps::StringIntMap::new(&mut rt::strings::Cx::current(),{desc}).map(|m|m.as_raw())")),Repr::MapStrInt))
            }
            Rvalue::Len(place) => {
                let (value, repr) = self.place(place)?;
                let element = Self::element(repr)?;
                let expr = if repr.growing() {
                    format!(
                        "rt::lists::List::<{}>::from_raw({value}).len(&rt::strings::Cx::current())",
                        element.slot()?
                    )
                } else {
                    format!(
                        "*(({} + 8) as *const i64)",
                        self.fallible(&format!("rt::nonnull({value})"))
                    )
                };
                Ok((expr, Repr::Int))
            }
            Rvalue::LoadType(ty) => Ok(("0usize".into(), Repr::Type(self.plan.type_code(ty)?))),
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
                if expected.growing() {
                    let slot = element.slot()?;
                    let new = self.fallible(&format!(
                        "rt::lists::List::<{slot}>::new(&mut rt::strings::Cx::current(),{desc})"
                    ));
                    write!(&mut code, "let p={new};").unwrap();
                    for i in 0..fields.len() {
                        let push = self.fallible(&format!(
                            "p.push(&mut rt::strings::Cx::current(),<{slot}>::from_raw(a{i}))"
                        ));
                        write!(&mut code, "{push};").unwrap();
                    }
                    code.push_str("p.as_raw()}");
                    return Ok((code, expected));
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
                    BinOp::Div => Some("div"),
                    BinOp::Mod => Some("rem"),
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
                    } else if matches!(t, Repr::Dyn(_) | Repr::Tagged) {
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
            unwinds: self
                .body
                .blocks
                .iter()
                .map(|b| b.unwind.map(|h| h.0))
                .collect(),
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
                    Terminator::Return | Terminator::Throw { .. } | Terminator::Rethrow { .. } => {
                        Ok(Flow::Exit)
                    }
                    _ => Err(no("control flow")),
                })
                .collect::<Gen<_>>()?,
        };
        let structured = structurize(&cfg).map_err(no)?;
        self.unwind_jumps = structured
            .unwind_jumps
            .iter()
            .map(|jumps| {
                jumps
                    .iter()
                    .map(|jump| match jump {
                        Stmt::Break(l) => Ok(format!("break 'b{l};")),
                        Stmt::BreakLoop(l) => Ok(format!("break 'l{l};")),
                        Stmt::Continue(l) => Ok(format!("continue 'l{l};")),
                        _ => Err(no("unwind jump shape")),
                    })
                    .collect::<Gen<Vec<_>>>()
                    .map(|s| s.join(""))
            })
            .collect::<Gen<_>>()?;

        let positions = self.plan.index.positions(self.object);
        if positions.block_pcs.len() != self.body.blocks.len() {
            return Err(no("missing/mismatched MIR source positions"));
        }
        let params = (1..=arity)
            .map(|i| Ok(format!("mut l{i}:{}", self.local(i)?.rust())))
            .collect::<Gen<Vec<_>>>()?
            .join(",");
        let attribute = if inline { "#[inline(always)]\n" } else { "" };
        let object = self.object;
        let returned = self.local(0)?.rust();
        let arguments = (1..=arity)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut code = format!(
            "#[inline(always)] unsafe fn f{object}(telemetry_site:usize,{params})->R<{returned}>{{unsafe{{if !rt::telemetry::enabled(&O{object}){{return f{object}_body({arguments});}}let frame=rt::telemetry::enter_observed(telemetry_site);let mut result=f{object}_body({arguments});frame.finish(&mut result);result}}}}\n{attribute}unsafe fn f{object}_body({params})->R<{returned}>{{unsafe{{rt::check_stack().map_err(|e|e.at(&S{object},0))?;\n"
        );
        for i in 0..self.body.locals.len() {
            if i > 0 && i <= arity {
                continue;
            }
            let t = self.local(i)?;
            writeln!(&mut code, "let mut l{i}:{}={};", t.rust(), t.zero()).unwrap();
        }
        for iter in &self.iters {
            writeln!(&mut code, "let mut iter_index{iter}=0i64;").unwrap();
        }
        for b in &self.body.blocks {
            if b.landing.is_some() {
                writeln!(
                    &mut code,
                    "let mut caught{}:Option<rt::CaughtError>=None;",
                    b.id.0
                )
                .unwrap();
            }

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
        self.block = b;
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
        let term = block.terminator.as_ref().unwrap();
        if let Some((iter, array)) =
            crate::analysis::array_iter_walk(self.db, self.body, self.arity, term, &self.iters)
        {
            writeln!(&mut out, "l{iter}=l{array};iter_index{iter}=0;").unwrap();
            return Ok(out);
        }
        if let Some(iter) = crate::analysis::array_next_call(self.db, term, &self.iters) {
            let array = self.iter_arrays[&iter];
            let repr = self.local(array)?;
            let element = Self::element(repr)?;
            let (len, get) = if repr.growing() {
                let slot = element.slot()?;
                let list = format!("rt::lists::List::<{slot}>::from_raw(l{iter})");
                (format!("{list}.len(&rt::strings::Cx::current())"),self.fallible(&format!("{list}.get(&rt::strings::Cx::current(),iter_index{iter}).map(|v|v.as_raw())")))
            } else {
                (
                    format!("*((l{iter}+8) as *const i64)"),
                    format!(
                        "*((l{iter}+16+iter_index{iter} as usize*8) as *const {})",
                        element.rust()
                    ),
                )
            };
            let value = self.coerce((get, element), Repr::Tagged)?;
            let (destination, _) = crate::analysis::continuation(term).unwrap();
            let expr = format!(
                "if iter_index{iter}>={len}{{rt::Dyn{{tag:u64::MAX,payload:0}}}}else{{let value={value};iter_index{iter}+=1;value}}"
            );
            out.push_str(&self.assign(destination, (expr, Repr::Tagged))?);
            return Ok(out);
        }
        match term {
            Terminator::Throw { value } => {
                let (value, Repr::Ptr(c)) = self.operand(value)? else {
                    return Err(no("non-class throw"));
                };
                let Object::Class(class) =
                    &self.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(c)]
                else {
                    unreachable!()
                };
                if self.plan.layouts[&c].fields.iter().any(|r| *r != Repr::Int) {
                    return Err(no("throw class with non-int fields"));
                }
                let package = self
                    .plan
                    .program
                    .packages
                    .iter()
                    .find(|p| p.objects.contains(&u32::try_from(c).expect("object index")))
                    .ok_or_else(|| no("throw class package"))?;
                let name = format!("{}.{}", package.name, class.path);
                let fields = class
                    .fields
                    .iter()
                    .map(|f| format!("{}: {{}}", f.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let display = format!("{name} {{{{{fields}}}}}");
                let values = (0..class.fields.len())
                    .map(|i| format!("(*(p as *const C{c})).f{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let error = format!(
                    "Box::new(rt::Error{{value:p,class:{name:?},message:String::new(),frames:Vec::new(),display_value:Some(format!({display:?},{values}))}}).at(&S{},{})",
                    self.object, self.site
                );
                writeln!(&mut out, "{{let p={value};{}}}", self.raise(&error)).unwrap();
            }
            Terminator::Rethrow {
                context: Operand::Copy(Place::Local(context)) | Operand::Move(Place::Local(context)),
                ..
            } => {
                let handler = self
                    .body
                    .blocks
                    .iter()
                    .find(|b| b.landing.is_some_and(|l| l.context_local == *context))
                    .ok_or_else(|| no("rethrow context"))?
                    .id
                    .0;
                out.push_str(&self.raise(&format!(
                    "caught{handler}.take().expect(\"missing caught error\").rethrow()"
                )));
            }

            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let (v, t) = self.operand(operand)?;
                if matches!(kind, baml_compiler2_mir::ShortCircuitKind::Coalesce) {
                    let short = format!("!({})", Self::null_test(&(v.clone(), t)));
                    let store = self.assign(destination, (v, t))?;
                    writeln!(&mut out, "c{b}={short};if c{b}{{{store}}}").unwrap();
                    return Ok(out);
                }
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
                ntypeargs,
                argument_layout,
                ..
            } => {
                let (c, info) = self
                    .plan
                    .index
                    .function(self.db, *f)
                    .ok_or_else(|| no("call"))?;
                if self.plan.index.display(c) == "baml.json.decode" {
                    let Repr::Type(code) = self.operand(&args[0])?.1 else {
                        return Err(no("JSON type argument"));
                    };
                    if *ntypeargs != 1 || args.len() != 3 || code < 3 {
                        return Err(no("JSON decode specialization/naming"));
                    }
                    let naming = if matches!(args[2], Operand::Constant(Constant::OmittedArg)) {
                        "0usize".to_owned()
                    } else {
                        self.coerce(self.operand(&args[2])?, Repr::Naming)?
                    };
                    let text = self.coerce(self.operand(&args[1])?, Repr::Str)?;
                    let (destination, _) = crate::analysis::continuation(term).unwrap();
                    out.push_str(&self.assign(
                        destination,
                        (
                            self.fallible(&format!("f{c}_json{code}({text},{naming})")),
                            Repr::Tagged,
                        ),
                    )?);
                    return Ok(out);
                }
                let types = args[..*ntypeargs]
                    .iter()
                    .map(|a| self.operand(a).map(|v| v.1))
                    .collect::<Gen<Vec<_>>>()?;
                let mut values = args[*ntypeargs..]
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
                let expression = if matches!(
                    name,
                    "baml.Array.push" | "baml.Array.join" | "baml.Array.length"
                ) {
                    let repr = values.first().ok_or_else(|| no("list receiver"))?.1;
                    if !repr.growing() {
                        return Err(no("growing list builtin receiver"));
                    }
                    let element = Self::element(repr)?;
                    let code = if element == Repr::Str { 1 } else { 2 };
                    if types != [Repr::Type(code)] {
                        return Err(no("list builtin type arguments"));
                    }
                    let slot = element.slot()?;
                    let list = format!("rt::lists::List::<{slot}>::from_raw({})", values[0].0);
                    match name {
                        "baml.Array.length" if values.len() == 1 => (format!("v2_array_length(&rt::strings::Cx::current(),{list})"), Repr::Int),
                        "baml.Array.push" if values.len() == 2 && values[1].1 == element => (self.fallible(&format!("v2_array_push(&mut rt::strings::Cx::current(),{list},<{slot}>::from_raw({}))",values[1].0)), Repr::Int),
                        "baml.Array.join" if values.len() == 2 && element == Repr::Str && values[1].1 == Repr::Str => (self.fallible(&format!("v2_array_join(&mut rt::strings::Cx::current(),{list},rt::strings::Str::from_raw({})).map(|s|s.as_raw())",values[1].0)), Repr::Str),
                        _ => return Err(no("list builtin arguments")),
                    }
                } else if matches!(name, "baml.Map.set" | "baml.Map.values") {
                    if !(types == [Repr::Type(1), Repr::Type(0)]
                        || (types.is_empty() && name == "baml.Map.set"))
                        || values.first().is_none_or(|v| v.1 != Repr::MapStrInt)
                    {
                        return Err(no("map builtin arguments"));
                    }
                    let map = format!("rt::maps::StringIntMap::from_raw({})", values[0].0);
                    if name == "baml.Map.set"
                        && values.len() == 3
                        && values[1].1 == Repr::Str
                        && values[2].1 == Repr::Int
                    {
                        (self.fallible(&format!("v2_map_set(&mut rt::strings::Cx::current(),{map},rt::strings::Str::from_raw({}),{})",values[1].0,values[2].0)),Repr::Tagged)
                    } else if name == "baml.Map.values" && values.len() == 1 {
                        (
                            self.fallible(&format!(
                                "v2_map_values(&mut rt::strings::Cx::current(),{map},{})",
                                self.lists[&Repr::ListInt]
                            )),
                            Repr::ListInt,
                        )
                    } else {
                        return Err(no("map builtin signature"));
                    }
                } else if name == "baml._to_string_default"
                    && types == [Repr::Type(0)]
                    && values.len() == 1
                    && values[0].1 == Repr::Int
                {
                    (self.fallible(&format!("v2_int_to_string(&mut rt::strings::Cx::current(),{}).map(|s|s.as_raw())",values[0].0)),Repr::Str)
                } else if name == "baml.String.to_code_points"
                    && types.is_empty()
                    && values.len() == 1
                    && values[0].1 == Repr::Str
                {
                    (self.fallible(&format!("v2_code_points(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw({}),{})",values[0].0,self.lists[&Repr::ListInt])),Repr::ListInt)
                } else if !types.is_empty() {
                    return Err(no("generic call"));
                } else if name == "baml.String.char_count" {
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
                    if values.len() != 2 {
                        return Err(no("equality arity"));
                    }
                    if let Some(other) = values.iter().find(|v| v.1 != Repr::Null)
                        && values.iter().any(|v| v.1 == Repr::Null)
                    {
                        (Self::null_test(other), Repr::Bool)
                    } else if values.iter().any(|v| v.1 == Repr::Str) {
                        let a = self.coerce(values[0].clone(), Repr::Tagged)?;
                        let b = self.coerce(values[1].clone(), Repr::Tagged)?;
                        (
                            format!(
                                "{{let a={a};let b={b};a.tag==3 && b.tag==3 && rt::strings::Str::from_raw(a.payload as usize).as_str(&rt::strings::Cx::current())==rt::strings::Str::from_raw(b.payload as usize).as_str(&rt::strings::Cx::current())}}"
                            ),
                            Repr::Bool,
                        )
                    } else {
                        return Err(no("equality other than primitive string or null test"));
                    }
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
                            "f{c}({}, {})",
                            self.telemetry_site(c),
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
                        "{}=>f{callee}({},{arguments}),",
                        self.descriptors[&class] + 3,
                        self.telemetry_site(callee)
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

fn telemetry_metadata_source(m: &btel_types::FunctionMetadata) -> String {
    fn string(s: &str) -> String {
        format!("{s:?}.into()")
    }
    fn optional(s: Option<&str>) -> String {
        s.map_or_else(|| "None".into(), |s| format!("Some({})", string(s)))
    }
    fn key(k: Option<&btel_types::DefinitionKey>) -> String {
        k.map_or_else(
            || "None".into(),
            |k| format!("Some(DefinitionKey({}))", string(&k.0)),
        )
    }
    let span = m.source_span.as_ref().map_or_else(
        || "None".into(),
        |s| {
            format!(
                "Some(SourceSpan{{file_id:{},start:{},end:{}}})",
                s.file_id, s.start, s.end
            )
        },
    );
    let args = m.argument_layout.as_ref().map_or_else(
        || "None".into(),
        |a| {
            format!(
                "Some(ArgumentLayout{{slots:vec![{}]}})",
                a.slots
                    .iter()
                    .map(|s| format!(
                        "ArgumentSlot{{name:{},receiver:{}}}",
                        optional(s.name.as_deref()),
                        s.receiver
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
    );
    let map = m.source_map.as_ref().map_or_else(
        || "None".into(),
        |s| {
            format!(
                "Some(SourceMap{{code_bytes:{},entries:vec![{}]}})",
                s.code_bytes,
                s.entries
                    .iter()
                    .map(|e| format!(
                        "SourceMapEntry{{pc:{},file_id:{},start:{},end:{},line:{}}}",
                        e.pc, e.file_id, e.start, e.end, e.line
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
    );
    format!(
        "FunctionMetadata{{function_id,fqn:{},display_name:{},source_file:{},source_span:{span},kind:RuntimeFunctionKind::Bytecode,origin:RuntimeFunctionOrigin::{:?},owner_type:{},parent_function:{},lambda_path:{},definition_key:{},package_name:{},namespace:vec![{}],argument_layout:{args},source_map:{map},model_invoke:{}}}",
        string(&m.fqn),
        string(&m.display_name),
        optional(m.source_file.as_deref()),
        m.origin,
        key(m.owner_type.as_ref()),
        key(m.parent_function.as_ref()),
        optional(m.lambda_path.as_deref()),
        key(m.definition_key.as_ref()),
        optional(m.package_name.as_deref()),
        m.namespace
            .iter()
            .map(|s| string(s))
            .collect::<Vec<_>>()
            .join(","),
        m.model_invoke
    )
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
