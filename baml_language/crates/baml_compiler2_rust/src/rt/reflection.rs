//! Static reflected type tables and the managed carrier used at data boundaries.
use super::*;

const KINDS: &[(&str, &str)] = &[
    ("Class", "class"),
    ("Enum", "enum"),
    ("Union", "union"),
    ("Literal", "literal"),
    ("Array", "array"),
    ("Map", "map"),
    ("Interface", "interface"),
    ("Primitive", "primitive"),
    ("Function", "function"),
];

fn as_kind(name: &str) -> Option<&'static str> {
    let suffix = name.strip_prefix("reflect.Type.as_")?;
    KINDS
        .iter()
        .find_map(|(kind, method)| (*method == suffix).then_some(*kind))
}
fn view_kind(name: &str) -> Option<&str> {
    name.strip_prefix("reflect.reflect.TypeView$for$")?
        .strip_suffix(".as_type")
}
use super::equality::STRUCTURAL_EQUALS;

pub(super) fn native(name: &str) -> bool {
    baml_builtins2_codegen::supports_rt_reflection(name)
}

fn kind_class(plan: &Plan<'_>, kind: &str) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "reflect")
        .flat_map(|p| &p.classes)
        .find_map(|(path, id)| (path.to_string() == kind).then_some(id.raw()))
        .ok_or_else(|| no(format!("reflection kind {kind}")))
}

pub(super) fn prepare_native(plan: &mut Plan<'_>, name: &str) -> Gen<()> {
    plan.reflected = true;
    let extra: &[&str] = match name {
        "reflect.Type.array" => &["Array"],
        "reflect.Type.optional" | "reflect.Union.member_types" => &["Union"],
        "reflect.Type.of_value" | "reflect.Type._implements" | "reflect.Type.to_baml" => &[],
        "reflect.Function.params" => &["Function", "function.Parameter"],
        "reflect.Map._new" => &["Map"],
        "reflect.Literal.value" | "reflect.Literal.new" => &["Literal"],
        "reflect.Class.fields" => &["Class", "class.Field", "Meta"],
        "reflect.Enum.values" => &["Enum", "enum.Value", "Meta"],
        "reflect.Class.metadata" => &["Class", "Meta"],
        "reflect.Enum.metadata" => &["Enum", "Meta"],
        _ => &[""],
    };
    plan.reflect_meta |= matches!(
        name,
        "reflect.Class.fields"
            | "reflect.Enum.values"
            | "reflect.Class.metadata"
            | "reflect.Enum.metadata"
    );
    if extra != [""] {
        for kind in extra {
            plan.add(kind_class(plan, kind)?)?;
        }
        return Ok(());
    }
    let kinds: Vec<_> = if name == "reflect.Type.kind" {
        KINDS.iter().map(|(kind, _)| *kind).collect()
    } else if let Some(kind) = as_kind(name).or_else(|| view_kind(name)) {
        vec![kind]
    } else if name != "reflect.Type._to_string_impl" {
        vec![name.split('.').nth(1).unwrap()]
    } else {
        Vec::new()
    };
    for kind in kinds {
        plan.add(kind_class(plan, kind)?)?;
    }
    Ok(())
}

/// `type` values and the `reflect.Type` class that declares their methods are
/// one runtime type; implementations spelled on either apply to both.
pub(super) fn type_value_pattern(plan: &Plan<'_>, pattern: &bex_vm_types::TyTemplate) -> bool {
    match pattern {
        bex_vm_types::TyTemplate::Type => true,
        bex_vm_types::TyTemplate::Class(head, args) if args.is_empty() => kind_class(plan, "Type")
            .is_ok_and(|class| {
                matches!(&plan.program.objects[bex_vm_types::ObjectIndex::from_raw(class)],
                    Object::Class(c) if c.type_tag == head.tag())
            }),
        _ => false,
    }
}

fn reflected(repr: Repr) -> bool {
    matches!(repr, Repr::Reflected | Repr::Type(_))
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        let Some(Terminator::Call {
            callee: Operand::Constant(Constant::Function(function)),
            args,
            ..
        }) = term
        else {
            return None;
        };
        let (id, _) = plan.index.function(db, *function)?;
        native(plan.index.builtin_path(id)).then(|| {
            let name = plan.index.builtin_path(id).to_owned();
            prepare_native(plan, &name)?;
            // Constant variants passed as `unknown` need their enum's carrier tag.
            for arg in args {
                if let Operand::Constant(Constant::EnumVariant { enum_ref, .. }) = arg {
                    let head = baml_compiler2_hir_ty::layout::enum_head(db, *enum_ref);
                    let repr = plan.repr(&RuntimeTy::Enum(head))?;
                    plan.record(repr)?;
                }
            }
            Ok(registry::Reach::default())
        })
    }

    fn call(
        &self,
        e: &Emitter<'_, '_>,
        name: &str,
        types: &[Repr],
        values: &[(String, Repr)],
    ) -> Option<Gen<(String, Repr)>> {
        if matches!(name, "baml.ops.equals_equals" | STRUCTURAL_EQUALS)
            && values.iter().any(|value| reflected(value.1))
        {
            return Some(equal(e, values));
        }
        if !native(name) {
            return None;
        }
        Some((|| {
            if let Some(result) = constructed(e, name, values)? {
                return Ok(result);
            }
            if !types.is_empty() || values.len() != 1 {
                return Err(no("reflected type display arguments"));
            }
            if let Some(kind) = view_kind(name) {
                return Ok((unview(e, values[0].clone(), kind)?, Repr::Reflected));
            }
            if matches!(
                name,
                "reflect.Array.element_type"
                    | "reflect.Map.key_type"
                    | "reflect.Map.value_type"
                    | "reflect.Function.return_type"
            ) {
                let kind = name.split('.').nth(1).unwrap();
                let value = unview(e, values[0].clone(), kind)?;
                let child = usize::from(name == "reflect.Map.value_type");
                return Ok((
                    e.fallible(&format!(
                        "rt::reflection::child(&REFLECT_PROGRAM,{}, {value}, {child})",
                        e.plan.reflection_descriptor().unwrap()
                    )),
                    Repr::Reflected,
                ));
            }
            let value = e.coerce(values[0].clone(), Repr::Reflected)?;
            if name == "reflect.Type.kind" {
                let mut code =
                    format!("{{let ty={value};match rt::reflection::kind(&REFLECT_PROGRAM,ty){{");
                for (kind, _) in KINDS {
                    let class = kind_class(e.plan, kind)?;
                    let view = view(e, class, "ty")?;
                    let tagged = e.coerce((view, Repr::Ptr(class)), Repr::Tagged)?;
                    write!(code, "rt::reflection::TypeKind::{kind}=>{tagged},").unwrap();
                }
                code.push_str("}}");
                return Ok((code, Repr::Tagged));
            }
            if let Some(kind) = as_kind(name) {
                let class = kind_class(e.plan, kind)?;
                let view = view(e, class, "ty")?;
                return Ok((
                    format!(
                        "{{let ty={value};if rt::reflection::kind(&REFLECT_PROGRAM,ty)==rt::reflection::TypeKind::{kind}{{{view}}}else{{0usize}}}}"
                    ),
                    Repr::Ptr(class),
                ));
            }
            Ok((
                e.fallible(&format!(
                    "rt::reflection::text(&REFLECT_PROGRAM,{value}).map(|s|s.as_raw())"
                )),
                Repr::Str,
            ))
        })())
    }

    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if let Rvalue::CurrentPackage = value {
            // The pack is closed-world: the enclosing package is the program's
            // root, identified by its package index.
            return Some((|| {
                let class = kind_class(e.plan, "Package")?;
                let root = e.plan.program.root;
                Ok((
                    e.fallible(&format!(
                        "(||->R<usize>{{unsafe{{Ok({})}}}})()",
                        build(
                            e,
                            class,
                            &[("_inner", format!("rt::Dyn{{tag:0,payload:{root}}}"))]
                        )?
                    )),
                    Repr::Ptr(class),
                ))
            })());
        }
        if !e.plan.reflected {
            return None;
        }
        if let Rvalue::LoadType(ty) = value {
            return Some((|| {
                let runtime = ty.substitute_symbolic(&[]);
                let index = e
                    .plan
                    .reflection_types
                    .iter()
                    .position(|t| *t == runtime)
                    .ok_or_else(|| no("unregistered reflected type"))?;
                Ok((
                    e.fallible(&format!(
                        "rt::reflection::new({}, {index})",
                        e.plan.reflection_descriptor().unwrap()
                    )),
                    Repr::Type(e.plan.type_code(ty)?),
                ))
            })());
        }
        if let Rvalue::BinaryOp {
            op: BinOp::Eq | BinOp::Ne,
            left,
            right,
        } = value
        {
            if matches!(left, Operand::Constant(Constant::OmittedArg))
                || matches!(right, Operand::Constant(Constant::OmittedArg))
            {
                return None;
            }
            let left = e.operand(left);
            let right = e.operand(right);
            if left.as_ref().is_ok_and(|v| reflected(v.1))
                || right.as_ref().is_ok_and(|v| reflected(v.1))
            {
                return Some((|| {
                    let (result, repr) = equal(e, &[left?, right?])?;
                    let result = if matches!(value, Rvalue::BinaryOp { op: BinOp::Ne, .. }) {
                        format!("!({result})")
                    } else {
                        result
                    };
                    Ok((result, repr))
                })());
            }
        }
        None
    }
}

fn view(e: &Emitter<'_, '_>, class: usize, ty: &str) -> Gen<String> {
    let layout = &e.plan.layouts[&class];
    if layout.fields != [Repr::Reflected] {
        return Err(no("reflection kind layout"));
    }
    let allocation = e.fallible(&format!(
        "rt::try_alloc({}, {})",
        e.descriptors[&class], layout.bytes
    ));
    Ok(format!(
        "{{let ty={ty};let p={allocation} as *mut C{class};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f0).cast(),ty);p as usize}}"
    ))
}
fn unview(e: &Emitter<'_, '_>, value: (String, Repr), kind: &str) -> Gen<String> {
    let class = kind_class(e.plan, kind)?;
    let value = e.coerce(value, Repr::Ptr(class))?;
    let checked = e.fallible(&format!("rt::nonnull({value})"));
    Ok(format!("(*({checked} as *const C{class})).f0"))
}
/// Types built or decomposed at run time, interned by the runtime registry.
fn constructed(
    e: &Emitter<'_, '_>,
    name: &str,
    values: &[(String, Repr)],
) -> Gen<Option<(String, Repr)>> {
    let desc = e.plan.reflection_descriptor().unwrap();
    let wrap = |body: String| e.fallible(&format!("(||->R<usize>{{unsafe{{{body}}}}})()"));
    Ok(Some(match name {
        "reflect.Type.array" | "reflect.Type.optional" => {
            let ty = e.coerce(values[0].clone(), Repr::Reflected)?;
            let (function, kind) = if name.ends_with("array") {
                ("array", "Array")
            } else {
                ("optional", "Union")
            };
            let class = kind_class(e.plan, kind)?;
            let carrier = wrap(format!(
                "rt::reflection::{function}(&REFLECT_PROGRAM,{desc},{ty})"
            ));
            (view(e, class, &carrier)?, Repr::Ptr(class))
        }
        "reflect.Map._new" => {
            let key = e.coerce(values[0].clone(), Repr::Reflected)?;
            let value = e.coerce(values[1].clone(), Repr::Reflected)?;
            let class = kind_class(e.plan, "Map")?;
            let carrier = wrap(format!(
                "rt::reflection::map(&REFLECT_PROGRAM,{desc},{key},{value})"
            ));
            (view(e, class, &carrier)?, Repr::Ptr(class))
        }
        "reflect.Type._implements" => {
            let ty = e.coerce(values[0].clone(), Repr::Reflected)?;
            let other = e.coerce(values[1].clone(), Repr::Reflected)?;
            (
                e.fallible(&format!(
                    "unsafe{{rt::reflection::implements(&REFLECT_PROGRAM,{ty},{other})}}"
                )),
                Repr::Bool,
            )
        }
        "reflect.Type.to_baml" => {
            let ty = e.coerce(values[0].clone(), Repr::Reflected)?;
            (
                e.fallible(&format!(
                    "unsafe{{rt::reflection::to_baml(&REFLECT_PROGRAM,{ty})}}.map(|s|s.as_raw())"
                )),
                Repr::Str,
            )
        }
        "reflect.Class.metadata" | "reflect.Enum.metadata" => {
            let kind = name.split('.').nth(1).unwrap();
            let ty = unview(e, values[0].clone(), kind)?;
            let lookup = if kind == "Class" {
                "class_meta"
            } else {
                "enum_meta"
            };
            (
                wrap(format!(
                    "let mut cx=rt::strings::Cx::current();let m=&rt::reflection::{lookup}(&REFLECT_PROGRAM,{ty})?.meta;Ok({})",
                    meta_value(e)?
                )),
                Repr::Ptr(kind_class(e.plan, "Meta")?),
            )
        }
        "reflect.Class.fields" => {
            let ty = unview(e, values[0].clone(), "Class")?;
            let field = kind_class(e.plan, "class.Field")?;
            let list = list_repr(
                e,
                &RuntimeTy::Class(reflect_head(e, "class", "Field")?, Box::new([])),
            )?;
            let list_desc = e.lists[&list];
            let object = build(
                e,
                field,
                &[
                    ("name", "cx.string(f.name)?.as_raw()".into()),
                    (
                        "type",
                        format!("rt::reflection::intern_capture(&REFLECT_PROGRAM,{desc},f.ty)?"),
                    ),
                    ("meta", format!("{{let m=&f.meta;{}}}", meta_value(e)?)),
                    ("_owner", "rt::Dyn{tag:2,payload:0}".into()),
                ],
            )?;
            (
                wrap(format!(
                    "let mut cx=rt::strings::Cx::current();let c=rt::reflection::class_meta(&REFLECT_PROGRAM,{ty})?;let list=rt::lists::List::<rt::lists::Ptr>::new(&mut cx,{list_desc})?;for f in c.fields{{let o={object};list.push(&mut cx,rt::lists::Ptr::from_raw(o))?;}}Ok(list.as_raw())"
                )),
                list,
            )
        }
        "reflect.Enum.values" => {
            let ty = unview(e, values[0].clone(), "Enum")?;
            let value = kind_class(e.plan, "enum.Value")?;
            let list = list_repr(
                e,
                &RuntimeTy::Class(reflect_head(e, "enum", "Value")?, Box::new([])),
            )?;
            let list_desc = e.lists[&list];
            let object = build(
                e,
                value,
                &[
                    ("name", "cx.string(v.0)?.as_raw()".into()),
                    ("meta", format!("{{let m=&v.1;{}}}", meta_value(e)?)),
                ],
            )?;
            (
                wrap(format!(
                    "let mut cx=rt::strings::Cx::current();let c=rt::reflection::enum_meta(&REFLECT_PROGRAM,{ty})?;let list=rt::lists::List::<rt::lists::Ptr>::new(&mut cx,{list_desc})?;for v in c.variants{{let o={object};list.push(&mut cx,rt::lists::Ptr::from_raw(o))?;}}Ok(list.as_raw())"
                )),
                list,
            )
        }
        "reflect.Type.of_value" => {
            let value = e.coerce(values[0].clone(), Repr::Tagged)?;
            (
                wrap(format!(
                    "rt::reflection::of_value(&REFLECT_PROGRAM,{desc},CAPTURE_SHAPES,{value})"
                )),
                Repr::Reflected,
            )
        }
        "reflect.Literal.new" => {
            let value = e.coerce(values[0].clone(), Repr::Tagged)?;
            let class = kind_class(e.plan, "Literal")?;
            let carrier = wrap(format!(
                "rt::reflection::literal(&REFLECT_PROGRAM,{desc},{value})"
            ));
            (view(e, class, &carrier)?, Repr::Ptr(class))
        }
        "reflect.Literal.value" => {
            let ty = unview(e, values[0].clone(), "Literal")?;
            (
                e.fallible(&format!(
                    "rt::reflection::literal_value(&REFLECT_PROGRAM,{ty},{})",
                    e.plan.bigints.then(|| "Some(BIGINT_TAG)").unwrap_or("None")
                )),
                Repr::Tagged,
            )
        }
        "reflect.Union.member_types" => {
            let ty = unview(e, values[0].clone(), "Union")?;
            let list = list_repr(e, &RuntimeTy::Type)?;
            let list_desc = e.lists[&list];
            (
                wrap(format!(
                    "let ty={ty};let n=rt::reflection::arity(&REFLECT_PROGRAM,ty)?;let mut cx=rt::strings::Cx::current();let list=rt::lists::List::<rt::lists::Ptr>::new(&mut cx,{list_desc})?;for i in 0..n{{let t=rt::reflection::child(&REFLECT_PROGRAM,{desc},ty,i)?;list.push(&mut cx,rt::lists::Ptr::from_raw(t))?;}}Ok(list.as_raw())"
                )),
                list,
            )
        }
        "reflect.Function.params" => {
            let ty = unview(e, values[0].clone(), "Function")?;
            let class = kind_class(e.plan, "function.Parameter")?;
            let list = list_repr(e, &RuntimeTy::Class(parameter_head(e)?, Box::new([])))?;
            let list_desc = e.lists[&list];
            let layout = &e.plan.layouts[&class];
            let field = |name: &str| {
                e.plan
                    .class(class)
                    .fields
                    .iter()
                    .position(|f| f.name.as_str() == name)
                    .ok_or_else(|| no("reflect.function.Parameter field"))
            };
            let (name, ty_field, optional) = (field("name")?, field("type")?, field("optional")?);
            if layout.fields[name] != Repr::Str
                || layout.fields[ty_field] != Repr::Reflected
                || layout.fields[optional] != Repr::Bool
            {
                return Err(no("reflect.function.Parameter layout"));
            }
            (
                wrap(format!(
                    "let ty={ty};let n=rt::reflection::arity(&REFLECT_PROGRAM,ty)?;let mut cx=rt::strings::Cx::current();let list=rt::lists::List::<rt::lists::Ptr>::new(&mut cx,{list_desc})?;for i in 0..n{{let (name,optional,t)=rt::reflection::parameter(&REFLECT_PROGRAM,{desc},ty,i)?;let name=match name{{Some(n)=>cx.string(&n)?.as_raw(),None=>0usize}};let p=rt::try_alloc({}, {})? as *mut C{class};baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f{name}).cast(),name);baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f{ty_field}).cast(),t);(*p).f{optional}=u64::from(optional);list.push(&mut cx,rt::lists::Ptr::from_raw(p as usize))?;}}Ok(list.as_raw())",
                    e.descriptors[&class], layout.bytes
                )),
                list,
            )
        }
        _ => return Ok(None),
    }))
}

fn parameter_head(e: &Emitter<'_, '_>) -> Gen<baml_type::DeclName> {
    reflect_head(e, "function", "Parameter")
}

fn reflect_head(e: &Emitter<'_, '_>, namespace: &str, name: &str) -> Gen<baml_type::DeclName> {
    Ok(baml_type::DeclName::in_root(
        e.plan
            .lang
            .get(baml_base::LangPackage::Reflect)
            .ok_or_else(|| no("reflect package"))?,
        if namespace.is_empty() {
            Vec::new()
        } else {
            vec![baml_type::Name::new(namespace)]
        },
        baml_type::Name::new(name),
    ))
}

/// A block allocating `class` from per-field Rust expressions of the field's
/// storage representation. Inputs are bound before allocation so they stay
/// conservatively rooted in Rust locals.
fn build(e: &Emitter<'_, '_>, class: usize, values: &[(&str, String)]) -> Gen<String> {
    let layout = &e.plan.layouts[&class];
    let declared = &e.plan.class(class).fields;
    if declared.len() != values.len() {
        return Err(no("reflection descriptor layout"));
    }
    let mut code = String::from("{");
    let mut slots = Vec::new();
    for (name, value) in values {
        let i = declared
            .iter()
            .position(|f| f.name.as_str() == *name)
            .ok_or_else(|| no("reflection descriptor field"))?;
        write!(code, "let a{i}={value};").unwrap();
        slots.push(i);
    }
    write!(
        code,
        "let p=rt::try_alloc({}, {})? as *mut C{class};",
        e.descriptors[&class], layout.bytes
    )
    .unwrap();
    for i in slots {
        let t = layout.fields[i];
        if t.pointer() {
            write!(
                code,
                "baml_rt_gc::write_ptr(p.cast(),(&raw mut (*p).f{i}).cast(),a{i});"
            )
        } else if matches!(t, Repr::Dyn(_) | Repr::Tagged) {
            write!(code, "rt::write_dyn(p.cast(),&raw mut (*p).f{i},a{i});")
        } else if t == Repr::Bool {
            write!(code, "(*p).f{i}=u64::from(a{i});")
        } else {
            write!(code, "(*p).f{i}=a{i} as _;")
        }
        .unwrap();
    }
    code.push_str("p as usize}");
    Ok(code)
}

/// A `reflect.Meta` built from `m: &rt::reflection::Meta`, with `cx` in scope.
fn meta_value(e: &Emitter<'_, '_>) -> Gen<String> {
    let class = kind_class(e.plan, "Meta")?;
    let other = e.plan.layouts[&class].fields[e
        .plan
        .class(class)
        .fields
        .iter()
        .position(|f| f.name.as_str() == "other")
        .ok_or_else(|| no("reflect.Meta.other"))?];
    let map = e.map_type(other)?;
    let map_desc = e.lists[&other];
    let text =
        |field: &str| format!("match m.{field}{{Some(s)=>cx.string(s)?.as_raw(),None=>0usize}}");
    build(
        e,
        class,
        &[
            ("alias", text("alias")),
            ("description", text("description")),
            ("docstring", text("docstring")),
            (
                "other",
                format!(
                    "{{let map={map}::new(&mut cx,{map_desc})?;for (k,v) in m.other{{let k=cx.string(k)?;let v=cx.string(v)?;map.set(&mut cx,k,v)?;}}map.as_raw()}}"
                ),
            ),
            ("skip", "m.skip".into()),
        ],
    )
}

fn list_repr(e: &Emitter<'_, '_>, element: &RuntimeTy) -> Gen<Repr> {
    let repr = e.plan.repr(&RuntimeTy::List(Box::new(element.clone())))?;
    if !e.lists.contains_key(&repr) || e.plan.element(repr)?.slot()? != "rt::lists::Ptr" {
        return Err(no("reflected list representation"));
    }
    Ok(repr)
}

fn children(ty: &RuntimeTy) -> Vec<RuntimeTy> {
    match ty {
        RuntimeTy::List(element) => vec![*element.clone()],
        RuntimeTy::Map { key, value } => vec![*key.clone(), *value.clone()],
        RuntimeTy::Union(members) => members.to_vec(),
        RuntimeTy::Function { ret, .. } => vec![*ret.clone()],
        _ => Vec::new(),
    }
}

fn equal(e: &Emitter<'_, '_>, values: &[(String, Repr)]) -> Gen<(String, Repr)> {
    if values.len() != 2 {
        return Err(no("reflected type equality arguments"));
    }
    let left = e.coerce(values[0].clone(), Repr::Tagged)?;
    let right = e.coerce(values[1].clone(), Repr::Tagged)?;
    let tag = e
        .plan
        .reflection_descriptor()
        .ok_or_else(|| no("reflected type descriptor"))?
        + 3;
    Ok((
        format!("rt::reflection::equal(&REFLECT_PROGRAM,{tag},{left},{right})"),
        Repr::Bool,
    ))
}

/// Generic-only type tokens stay compile-time values. Ordinary parameters,
/// fields, named locals, comparisons and logs require a managed carrier.
pub(super) fn collect(
    plan: &mut Plan<'_>,
    all: &BTreeMap<usize, super::TestMir<'_>>,
    reached: &BTreeSet<usize>,
) -> Gen<()> {
    for object in reached {
        let MirFunctionKind::Bytecode(body) = &all[object].kind else {
            continue;
        };
        let type_operand = |operand: &Operand<'_>| match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                matches!(body.locals[local.0].ty, RuntimeTy::Type)
            }
            _ => false,
        };
        for block in &body.blocks {
            if let Some(Terminator::Call {
                args, ntypeargs, ..
            }) = &block.terminator
            {
                plan.reflected |= args[*ntypeargs..].iter().any(type_operand);
            }
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign {
                        value: Rvalue::BinaryOp { left, right, .. },
                        ..
                    } => plan.reflected |= type_operand(left) || type_operand(right),
                    StatementKind::Intrinsic { args, .. } => {
                        plan.reflected |= args.iter().any(type_operand)
                    }
                    _ => {}
                }
            }
        }
    }
    if !plan.reflected {
        return Ok(());
    }
    for object in reached {
        let MirFunctionKind::Bytecode(body) = &all[object].kind else {
            continue;
        };
        for statement in body.blocks.iter().flat_map(|block| &block.statements) {
            if let StatementKind::Assign {
                value: Rvalue::LoadType(ty),
                ..
            } = &statement.kind
            {
                let ty = ty.substitute_symbolic(&[]);
                baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                    .map_err(|_| no("unrealized reflected type"))?;
                if !plan.reflection_types.contains(&ty) {
                    plan.reflection_types.push(ty);
                }
            }
        }
    }
    // `Type.of_value` and `implements` answer from frozen facts; every class
    // and enum the program can hold gets an entry.
    for ty in plan.concrete_types.clone() {
        if !matches!(ty, RuntimeTy::Class(..) | RuntimeTy::Enum(..))
            || plan.reflection_types.contains(&ty)
        {
            continue;
        }
        let Ok(realized) = baml_compiler2_mir::RealizedTy::try_from(ty.clone()) else {
            continue;
        };
        if plan
            .index
            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
            .is_some()
        {
            plan.reflection_types.push(ty);
        }
    }
    let mut cursor = 0;
    while cursor < plan.reflection_types.len() {
        for child in children(&plan.reflection_types[cursor]) {
            if !plan.reflection_types.contains(&child) {
                plan.reflection_types.push(child);
            }
        }
        cursor += 1;
    }
    if plan.reflect_meta {
        for (entry, ty) in plan.reflection_types.clone().into_iter().enumerate() {
            let RuntimeTy::Class(head, _) = &ty else {
                continue;
            };
            let Some((object, _)) = plan.index.class_at(head) else {
                continue;
            };
            let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                .map_err(|_| no("unrealized reflected class"))?;
            let anchored = plan
                .index
                .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
                .ok_or_else(|| no("reflected class identity"))?
                .substitute_symbolic(&[]);
            let bex_vm_types::RuntimeTy::Class(_, args) = anchored else {
                continue;
            };
            let Object::Class(class) =
                &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(object)]
            else {
                continue;
            };
            let fields = class
                .fields
                .iter()
                .map(|f| f.field_template.substitute_symbolic(&args))
                .collect();
            plan.reflection_fields.push((entry, object, fields));
        }
    }
    Ok(())
}

pub(super) fn emit(out: &mut String, db: &dyn Db, plan: &Plan<'_>) -> Gen<()> {
    if !plan.reflected {
        return Ok(());
    }
    let facts = baml_compiler2_hir_ty::facts::Facts::new(db);
    let mut identities = Vec::<baml_type::Ty<baml_type::DeclName>>::new();
    let mut anchored_types = Vec::new();
    for ty in &plan.reflection_types {
        let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
            .map_err(|_| no("unrealized reflected type"))?;
        let anchored = plan
            .index
            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
            .ok_or_else(|| no("reflected type identity"))?;
        anchored_types.push(
            bex_vm_types::RealizedTy::try_from(anchored.substitute_symbolic(&[]))
                .map_err(|_| no("unrealized reflected type"))?,
        );
    }
    // Spelling comes from the interpreter's own renderer over the linked
    // declarations, so displayed names cannot drift from `Type.to_string`.
    let displays =
        bex_vm::telemetry::frozen::type_displays(plan.program, &anchored_types).map_err(no)?;
    let facts_vm =
        bex_vm::telemetry::frozen::reflection_facts(plan.program, &anchored_types).map_err(no)?;
    out.push_str("static REFLECT_TYPES:&[rt::reflection::StaticType]=&[\n");
    for (i, (ty, display)) in plan.reflection_types.iter().zip(&displays).enumerate() {
        let comparable = baml_type::Ty::from(ty.clone());
        let identity = identities
            .iter()
            .position(|old| baml_type::normalize::equivalent(old, &comparable, &facts))
            .unwrap_or(identities.len());
        if identity == identities.len() {
            identities.push(comparable);
        }
        let realized = baml_compiler2_mir::RealizedTy::try_from(ty.clone())
            .map_err(|_| no("unrealized reflected type"))?;
        let anchored = plan
            .index
            .anchor(&baml_compiler2_mir::TyTemplate::from(realized))
            .ok_or_else(|| no("reflected type identity"))?;
        let wire = anchored.substitute_symbolic(&[]);
        let capture = capture_codegen::capture_wire_type(plan, &wire)?;
        let kind = baml_type::type_kind::classify_type(
            &baml_compiler2_mir::RealizedTy::try_from(ty.clone())
                .map_err(|_| no("reflected kind"))?,
        );
        writeln!(out,"rt::reflection::StaticType{{ty:&{capture},display:{display:?},identity:{identity},kind:rt::reflection::TypeKind::{kind:?},implements:&{:?},to_baml:{:?}}},",facts_vm.implements[i],facts_vm.to_baml[i]).unwrap();
    }
    out.push_str("];\nstatic REFLECT_PROGRAM:rt::reflection::Program=rt::reflection::Program{types:REFLECT_TYPES,enums:&[");
    for &e in &plan.enums {
        let Object::Enum(enumeration) =
            &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(e)]
        else {
            continue;
        };
        let variants = enumeration
            .variants
            .iter()
            .map(|v| format!("{:?}", v.name.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        write!(
            out,
            "({},&[{variants}],CAPTURE_ENUM_{e}_TAG),",
            enumeration.type_tag.as_i64()
        )
        .unwrap();
    }
    out.push_str("],classes:&[");
    let meta = |alias: &Option<String>,
                description: &Option<String>,
                docstring: &Option<String>,
                other: Vec<(&String, &String)>,
                skip: bool| {
        format!(
            "rt::reflection::Meta{{alias:{alias:?},description:{description:?},docstring:{docstring:?},other:&{other:?},skip:{skip}}}"
        )
    };
    for (entry, object, types) in &plan.reflection_fields {
        let Object::Class(class) =
            &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(*object)]
        else {
            continue;
        };
        let mut fields = String::new();
        for (field, ty) in class.fields.iter().zip(types) {
            write!(
                fields,
                "rt::reflection::FieldMeta{{name:{:?},ty:&{},meta:{}}},",
                field.name,
                capture_codegen::capture_wire_type(plan, ty)?,
                meta(
                    &field.alias,
                    &field.description,
                    &field.docstring,
                    field.other.iter().collect(),
                    field.skip
                )
            )
            .unwrap();
        }
        write!(
            out,
            "rt::reflection::ClassMeta{{entry:{entry},fields:&[{fields}],meta:{}}},",
            meta(
                &class.alias,
                &class.description,
                &class.docstring,
                class.other.iter().collect(),
                false
            )
        )
        .unwrap();
    }
    out.push_str("],enum_meta:&[");
    if plan.reflect_meta {
        let mut seen = BTreeSet::new();
        for ty in &plan.reflection_types {
            let RuntimeTy::Enum(head) = ty else { continue };
            let Some(e) = plan.index.enum_at(head) else {
                continue;
            };
            if !seen.insert(e) {
                continue;
            }
            let Object::Enum(enumeration) =
                &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(e)]
            else {
                continue;
            };
            let variants = enumeration
                .variants
                .iter()
                .map(|v| {
                    format!(
                        "({:?},{})",
                        v.name,
                        meta(
                            &v.alias,
                            &v.description,
                            &v.docstring,
                            v.other.iter().collect(),
                            v.skip
                        )
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            write!(
                out,
                "rt::reflection::EnumMeta{{tag:{},variants:&[{variants}],meta:{}}},",
                enumeration.type_tag.as_i64(),
                meta(
                    &enumeration.alias,
                    &enumeration.description,
                    &enumeration.docstring,
                    enumeration.other.iter().collect(),
                    false
                )
            )
            .unwrap();
        }
    }
    out.push_str("]};\n");
    Ok(())
}
