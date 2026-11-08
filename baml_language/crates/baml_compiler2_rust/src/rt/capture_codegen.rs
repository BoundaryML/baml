//! Freeze canonical declaration metadata at compile time. Generated packs only
//! carry owned CAS blobs; the VM dependency belongs to this compiler, not them.
use super::*;

pub(super) fn emit(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    after_lists: usize,
) -> Gen<String> {
    let named = |name: &str| {
        plan.program
            .packages
            .iter()
            .find_map(|package| {
                package
                    .classes
                    .iter()
                    .chain(package.enums.iter())
                    .find_map(|(path, index)| {
                        (path.under(&package.name).to_string() == name).then_some(index.raw())
                    })
            })
            .ok_or_else(|| no(format!("capture declaration {name}")))
    };
    let runtime = [
        "baml.panics.IntegerOverflow",
        "baml.panics.NegativeBitShift",
        "baml.panics.Cancelled",
        "baml.panics.StackOverflow",
        "baml.panics.AllocFailure",
        "baml.panics.IndexOutOfBounds",
        "baml.json.ParseError",
        "baml.json.DecodeError",
        "baml.panics.DivisionByZero",
    ]
    .map(named)
    .into_iter()
    .collect::<Gen<Vec<_>>>()?;
    let naming = named("baml.json.Naming")?;
    // Empty callable lists can mention classes never allocated by the entry.
    // Their type captures still require the declaration, without a heap layout.
    let mut signature_declarations = BTreeSet::new();
    for signature in plan.closure_lists.values() {
        signature.visit_heads(&mut |object| {
            signature_declarations.insert(*object);
        });
    }
    let mut extra = BTreeSet::new();
    for frame in plan.function_frames.values() {
        for (_, ty) in frame {
            ty.visit_heads(&mut |h| {
                if let Some(t) = plan.index.anchor(&baml_compiler2_mir::TyTemplate::Class(
                    h.clone(),
                    Box::new([]),
                )) {
                    if let bex_vm_types::TyTemplate::Class(head, _) = t {
                        for (i, o) in plan.program.objects.iter().enumerate() {
                            if matches!(o,Object::Class(c) if c.type_tag==head.tag())
                                || matches!(o,Object::Enum(e) if e.type_tag==head.tag())
                            {
                                extra.insert(i);
                            }
                        }
                    }
                }
            });
        }
    }
    // A class argument need not have a heap layout of its own: Phantom<C>
    // still captures C's identity even if no C object is ever constructed.
    for (_, args, _) in &plan.class_instances {
        for ty in args {
            ty.visit_heads(&mut |head| {
                for (i, object) in plan.program.objects.iter().enumerate() {
                    if matches!(object, Object::Class(c) if c.type_tag == head.tag())
                        || matches!(object, Object::Enum(e) if e.type_tag == head.tag())
                    {
                        extra.insert(i);
                    }
                }
            });
        }
    }
    let objects: BTreeSet<_> = runtime
        .iter()
        .copied()
        .chain(descriptors.keys().map(|c| plan.class_origin(*c)))
        .chain(signature_declarations)
        .chain([naming])
        .chain(plan.enums.iter().copied())
        .chain(extra)
        .collect();
    let objects: Vec<_> = objects.into_iter().collect();
    let declarations =
        bex_vm::telemetry::frozen::declarations(plan.program, &objects).map_err(no)?;
    let mut out = String::new();
    let mut groups = BTreeMap::new();
    for (&object, declaration) in objects.iter().zip(&declarations) {
        let group = emit_group(&declaration.definition.group, &mut groups, &mut out);
        let name = borsh::to_vec(&declaration.name).expect("declaration name");
        writeln!(&mut out,"fn CAPTURE_D{object}()->&'static rt::telemetry::capture::Declaration{{static VALUE:std::sync::OnceLock<rt::telemetry::capture::Declaration>=std::sync::OnceLock::new();VALUE.get_or_init(||rt::telemetry::capture::Declaration::from_bytes(&{name:?},{},rt::telemetry::types::Definition{{group:CAPTURE_G{group}(),member:{}}},{}))}}",declaration.tag.as_i64(),declaration.definition.member,declaration.is_enum).unwrap();
    }
    for &(origin, ref _args, id) in &plan.class_instances {
        if descriptors.contains_key(&id) {
            writeln!(&mut out,"fn CAPTURE_D{id}()->&'static rt::telemetry::capture::Declaration{{CAPTURE_D{origin}()}}").unwrap();
        }
    }
    out.push_str("static CAPTURE_SHAPES:&[rt::telemetry::capture::Shape]=&[rt::telemetry::capture::Shape::String,\n");
    for desc in 1..after_lists {
        if matches!(desc, 10 | 11) {
            out.push_str("rt::telemetry::capture::Shape::String,\n");
            continue;
        }
        if matches!(desc, 12 | 13) {
            out.push_str(if desc == 12 {
                "rt::telemetry::capture::Shape::RustType,\n"
            } else {
                "rt::telemetry::capture::Shape::Uint8Array,\n"
            });
            continue;
        }
        let class = if desc < layout::rt_descriptors() {
            Some(runtime[desc - 1])
        } else {
            descriptors
                .iter()
                .find_map(|(&object, &index)| (index == desc).then_some(object))
        };
        let shape = if plan.bigint_descriptor() == Some(desc) {
            "Bigint".to_owned()
        } else if plan.future_descriptor() == Some(desc) {
            "Future".to_owned()
        } else if plan
            .closure_descriptors()
            .is_some_and(|(_, closure)| desc == closure)
        {
            "Closure".to_owned()
        } else if let Some(class) = class {
            if super::tasks::class_is(plan, class, "future.Future") {
                "Future".to_owned()
            } else if let Some((_, args, _)) =
                plan.class_instances.iter().find(|(_, _, id)| *id == class)
            {
                let args = args
                    .iter()
                    .map(|t| capture_wire_type(plan, t))
                    .collect::<Gen<Vec<_>>>()?
                    .join(",");
                format!(
                    "GenericClass{{declaration:CAPTURE_D{class},fields:DESCRIPTORS[{desc}].fields,args:&[{args}]}}"
                )
            } else {
                format!("Class{{declaration:CAPTURE_D{class},fields:DESCRIPTORS[{desc}].fields}}")
            }
        } else if let Some((&repr, _)) = lists.iter().find(|(_, index)| **index == desc) {
            if let Repr::MapStrPtr(c) = repr {
                format!(
                    "MapStringPtr{{tag:{},value:&rt::telemetry::capture::Type::Class(CAPTURE_D{c})}}",
                    descriptors[&c] + 3
                )
            } else if repr == Repr::MapStrInt {
                "MapStringInt".to_owned()
            } else {
                let element = Emitter::element(repr)?;
                let (tag, ty) = match element {
                    Repr::Int => (0, "Int".to_owned()),
                    Repr::Str => (3, "String".to_owned()),
                    Repr::Ptr(class) => {
                        (descriptors[&class] + 3, format!("Class(CAPTURE_D{class})"))
                    }
                    Repr::ListStr => (
                        lists[&Repr::ListStr] + 3,
                        "List(&rt::telemetry::capture::Type::String)".to_owned(),
                    ),
                    Repr::Future => {
                        let Repr::ListFuture(id) = repr else {
                            unreachable!()
                        };
                        let full = super::closures::capture_type(&plan.closure_lists[&id])?;
                        (
                            plan.future_descriptor().unwrap() + 3,
                            full.strip_prefix("rt::telemetry::capture::Type::")
                                .unwrap()
                                .to_owned(),
                        )
                    }
                    Repr::Closure => {
                        let Repr::ListClosure(id) = repr else {
                            unreachable!()
                        };
                        let full = super::closures::capture_type(&plan.closure_lists[&id])?;
                        (
                            plan.closure_descriptors().unwrap().1 + 3,
                            full.strip_prefix("rt::telemetry::capture::Type::")
                                .unwrap()
                                .to_owned(),
                        )
                    }
                    _ => return Err(no("capture list type")),
                };
                format!(
                    "List{{growing:{},tag:{tag},element:&rt::telemetry::capture::Type::{ty}}}",
                    repr.growing()
                )
            }
        } else {
            "Unsupported".to_owned()
        };
        writeln!(&mut out, "rt::telemetry::capture::Shape::{shape},").unwrap();
    }
    let declaration = &declarations[objects.iter().position(|&object| object == naming).unwrap()];
    writeln!(
        &mut out,
        "rt::telemetry::capture::Shape::Enum{{declaration:CAPTURE_D{naming},variants:&{:?}}},",
        declaration.variants
    )
    .unwrap();
    for e in &plan.enums {
        let d = &declarations[objects.iter().position(|i| i == e).unwrap()];
        writeln!(
            &mut out,
            "rt::telemetry::capture::Shape::Enum{{declaration:CAPTURE_D{e},variants:&{:?}}},",
            d.variants
        )
        .unwrap();
    }
    writeln!(
        &mut out,
        "];\nconst CAPTURE_NAMING_TAG:u64={};",
        after_lists + 3
    )
    .unwrap();
    for (i, e) in plan.enums.iter().enumerate() {
        writeln!(
            &mut out,
            "const CAPTURE_ENUM_{e}_TAG:u64={};",
            after_lists + 4 + i
        )
        .unwrap();
    }
    out.push_str("static CAPTURE_METADATA:rt::telemetry::capture::Metadata=rt::telemetry::capture::Metadata{shapes:CAPTURE_SHAPES,naming_tag:CAPTURE_NAMING_TAG};\n");
    Ok(out)
}
fn emit_group(
    group: &btel_types::DefinitionBlob,
    groups: &mut BTreeMap<[u8; 16], usize>,
    out: &mut String,
) -> usize {
    if let Some(&index) = groups.get(&group.id()) {
        return index;
    }
    let children = group
        .children()
        .iter()
        .map(|child| emit_group(child, groups, out))
        .collect::<Vec<_>>();
    let index = groups.len();
    groups.insert(group.id(), index);
    let children = children
        .into_iter()
        .map(|child| format!("CAPTURE_G{child}()"))
        .collect::<Vec<_>>()
        .join(",");
    writeln!(out,"fn CAPTURE_G{index}()->std::sync::Arc<rt::telemetry::types::DefinitionBlob>{{static VALUE:std::sync::OnceLock<std::sync::Arc<rt::telemetry::types::DefinitionBlob>>=std::sync::OnceLock::new();VALUE.get_or_init(||std::sync::Arc::new(rt::telemetry::types::DefinitionBlob::new({:?},vec!{:?}.into_boxed_slice(),vec![{children}].into_boxed_slice()))).clone()}}",group.id(),group.bytes()).unwrap();
    index
}

pub(super) fn capture_type(plan: &Plan<'_>, ty: &RuntimeTy) -> Gen<String> {
    let t = baml_compiler2_mir::TyTemplate::from(
        baml_compiler2_mir::RealizedTy::try_from(ty.clone())
            .map_err(|_| no("unrealized capture type"))?,
    );
    let anchored = plan
        .index
        .anchor(&t)
        .ok_or_else(|| no("capture type head"))?;
    capture_wire_type(plan, &anchored.substitute_symbolic(&[]))
}
fn capture_wire_type(plan: &Plan<'_>, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
    use bex_vm_types::RuntimeTy as T;
    let member = match ty {
        T::Bigint => "Bigint".into(),
        T::Int => "Int".into(),
        T::Literal(lit, f) => match lit {
            baml_type::Literal::Int(v) => {
                format!("LiteralInt({v},rt::telemetry::capture::Freshness::{f:?})")
            }
            baml_type::Literal::Bool(v) => {
                format!("LiteralBool({v},rt::telemetry::capture::Freshness::{f:?})")
            }
            baml_type::Literal::String(v) => format!(
                "LiteralString({:?},rt::telemetry::capture::Freshness::{f:?})",
                v.as_str()
            ),
            baml_type::Literal::Float(v) => {
                format!("LiteralFloat({v:?},rt::telemetry::capture::Freshness::{f:?})")
            }
            _ => return Err(no("bigint literal capture type")),
        },
        T::Float => "Float".into(),
        T::Bool => "Bool".into(),
        T::Null => "Null".into(),
        T::Unknown => "Unknown".into(),
        T::Never => "Never".into(),
        T::String => "String".into(),
        T::Uint8Array => "Uint8Array".into(),
        T::RustType => "RustType".into(),
        T::Class(head, args) => {
            let c = plan.class_tag(head.tag())?;
            if args.is_empty() {
                format!("Class(CAPTURE_D{c})")
            } else {
                format!(
                    "GenericClass(CAPTURE_D{c},&[{}])",
                    args.iter()
                        .map(|t| capture_wire_type(plan, t))
                        .collect::<Gen<Vec<_>>>()?
                        .join(",")
                )
            }
        }
        T::TypeAlias(head) | T::Interface(head, _, _) => {
            let name = plan
                .program
                .packages
                .iter()
                .find_map(|p| {
                    p.type_aliases
                        .iter()
                        .chain(p.interfaces.iter())
                        .find_map(|(path, index)| {
                            let tag = match &plan.program.objects[*index] {
                                Object::TypeAlias(a) => a.type_tag,
                                Object::Interface(i) => i.type_tag,
                                _ => return None,
                            };
                            (tag == head.tag())
                                .then(|| baml_type::DeclarationName::Declared(path.under(&p.name)))
                        })
                })
                .ok_or_else(|| no("named capture identity"))?;
            let bytes = borsh::to_vec(&name).expect("type name");
            if let T::Interface(_, args, assoc) = ty {
                format!(
                    "Interface({},&{bytes:?},&[{}],&[{}])",
                    head.tag().as_i64(),
                    args.iter()
                        .map(|t| capture_wire_type(plan, t))
                        .collect::<Gen<Vec<_>>>()?
                        .join(","),
                    assoc
                        .iter()
                        .map(|(n, t)| Ok(format!(
                            "({:?},{})",
                            n.as_str(),
                            capture_wire_type(plan, t)?
                        )))
                        .collect::<Gen<Vec<_>>>()?
                        .join(",")
                )
            } else {
                format!("Alias({},&{bytes:?})", head.tag().as_i64())
            }
        }
        T::EnumVariant(head, variant) => {
            let e = plan
                .program
                .objects
                .iter()
                .enumerate()
                .find_map(|(i, o)| {
                    matches!(o,Object::Enum(e) if e.type_tag==head.tag()).then_some(i)
                })
                .ok_or_else(|| no("capture enum variant"))?;
            format!("EnumVariant(CAPTURE_D{e},{:?})", variant.as_str())
        }
        T::Enum(head) => {
            let e = plan
                .program
                .objects
                .iter()
                .enumerate()
                .find_map(|(i, o)| {
                    matches!(o,Object::Enum(e) if e.type_tag==head.tag()).then_some(i)
                })
                .ok_or_else(|| no("capture enum"))?;
            format!("Enum(CAPTURE_D{e})")
        }
        T::List(t) => format!("List(&{})", capture_wire_type(plan, t)?),
        T::Map { key, value } => format!(
            "Map(&{},&{})",
            capture_wire_type(plan, key)?,
            capture_wire_type(plan, value)?
        ),
        T::Union(m) => format!(
            "Union(&[{}])",
            m.iter()
                .map(|t| capture_wire_type(plan, t))
                .collect::<Gen<Vec<_>>>()?
                .join(",")
        ),
        _ => return Err(no(format!("capture type {ty:?}"))),
    };
    Ok(format!("rt::telemetry::capture::Type::{member}"))
}
