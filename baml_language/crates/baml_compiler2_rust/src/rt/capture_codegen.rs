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
        "baml.errors.InvalidArgument",
        "baml.errors.ParseError",
        "baml.panics.HostUnavailable",
        "baml.errors.Io",
        "baml.panics.MapKeyNotFound",
        "baml.json.SerializationError",
    ]
    .map(named)
    .into_iter()
    .collect::<Gen<Vec<_>>>()?;
    let naming = named("baml.json.Naming")?;
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
    // Empty callable lists can mention classes never allocated by the entry.
    // Their type captures still require the declaration, without a heap layout.
    let mut signature_declarations = BTreeSet::new();
    for signature in plan.closure_lists.values() {
        signature.visit_heads(&mut |object| {
            signature_declarations.insert(*object);
        });
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
        let class = if desc < runtime.len() {
            Some(runtime[desc - 1])
        } else if desc == 19 {
            Some(runtime[14])
        } else {
            descriptors
                .iter()
                .find_map(|(&object, &index)| (index == desc).then_some(object))
        };
        let shape = if desc == 15 {
            "Bigint".to_owned()
        } else if desc == 16 {
            "Bytes".to_owned()
        } else if desc == 17 {
            "Opaque".to_owned()
        } else if plan
            .closure_descriptors()
            .is_some_and(|(_, closure)| desc == closure)
        {
            "Closure".to_owned()
        } else if let Some(class) = class {
            if let Some((_, args, _)) = plan.class_instances.iter().find(|(_, _, id)| *id == class)
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
            if repr == Repr::MapStrInt {
                "MapStringInt".to_owned()
            } else if matches!(repr, Repr::Map(_)) {
                let (key, value) = plan.map_parts(repr)?;
                let key_slot = maps::slot_type(key, descriptors, lists)?;
                let value_slot = maps::slot_type(value, descriptors, lists)?;
                let map = format!("rt::maps::Map::<{key_slot},{value_slot}>");
                let (key_ty, value_ty) = map_types(plan, repr)?;
                let key_dyn = capture_dyn(plan, key, "key", descriptors, lists)?;
                let value_dyn = capture_dyn(plan, value, "value", descriptors, lists)?;
                format!(
                    "Map{{key:&{key_ty},value:&{value_ty},len:|p|unsafe{{{map}::from_raw(p).len()}},entry:|p,i|unsafe{{let (key,value)={map}::from_raw(p).entry(i).unwrap();({key_dyn},{value_dyn})}}}}"
                )
            } else {
                let (tag, ty) = capture_list_element(plan, descriptors, lists, repr)?;
                format!(
                    "List{{growing:true,tag:{tag},element:&rt::telemetry::capture::Type::{ty}}}"
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
        T::Bigint => "Bigint".into(),
        T::Uint8Array => "Bytes".into(),
        T::RustType => "Opaque".into(),
        T::Float => "Float".into(),
        T::Bool => "Bool".into(),
        T::Null => "Null".into(),
        T::Unknown => "Unknown".into(),
        T::Never => "Never".into(),
        T::Function {
            params,
            ret,
            throws,
        } => {
            let params = params
                .iter()
                .map(|p| {
                    Ok(format!(
                        "rt::telemetry::capture::Parameter{{name:{:?},optional:{},ty:&{}}}",
                        p.name.as_ref().map(|n| n.as_str()),
                        p.mode == baml_type::FunctionParamMode::Optional,
                        capture_wire_type(plan, &p.ty)?
                    ))
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            format!(
                "Function{{params:&[{params}],ret:&{},throws:&{}}}",
                capture_wire_type(plan, ret)?,
                capture_wire_type(plan, throws)?
            )
        }
        T::String => "String".into(),
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
pub(super) fn capture_repr_type(plan: &Plan<'_>, repr: Repr) -> Gen<String> {
    let prefix = "rt::telemetry::capture::Type::";
    Ok(match repr {
        Repr::Bigint => format!("{prefix}Bigint"),
        Repr::Bytes => format!("{prefix}Bytes"),
        Repr::Opaque => format!("{prefix}Opaque"),
        Repr::Int => format!("{prefix}Int"),
        Repr::Float => format!("{prefix}Float"),
        Repr::Bool => format!("{prefix}Bool"),
        Repr::Str => format!("{prefix}String"),
        Repr::Null => format!("{prefix}Null"),
        Repr::Closure => format!("{prefix}Unknown"),
        Repr::Enum(e) => format!("{prefix}Enum(CAPTURE_D{e})"),
        Repr::Naming => {
            let e = plan
                .program
                .packages
                .iter()
                .find_map(|p| {
                    p.enums.iter().find_map(|(path, index)| {
                        (path.under(&p.name).to_string() == "baml.json.Naming")
                            .then_some(index.raw())
                    })
                })
                .ok_or_else(|| no("naming enum declaration"))?;
            format!("{prefix}Enum(CAPTURE_D{e})")
        }
        Repr::Tagged | Repr::Dyn(_) => format!("{prefix}Unknown"),
        Repr::Ptr(c) => format!("{prefix}Class(CAPTURE_D{c})"),
        Repr::Map(_) | Repr::MapStrInt => {
            let (key, value) = map_types(plan, repr)?;
            format!("{prefix}Map(&{key},&{value})")
        }
        Repr::ListClosure(id) => format!(
            "{prefix}List(&{})",
            super::closures::capture_type(&plan.closure_lists[&id])?
        ),
        Repr::List(id) => {
            let types = plan.list_elements.borrow();
            let element = &types[id];
            let ty = if let Some(declared) = &element.declared {
                declared.clone()
            } else if element.members.is_empty() {
                capture_repr_type(plan, element.repr)?
            } else {
                let members = element
                    .members
                    .iter()
                    .map(|m| capture_repr_type(plan, *m))
                    .collect::<Gen<Vec<_>>>()?;
                if members.len() == 1 {
                    members[0].clone()
                } else {
                    format!("{prefix}Union(&[{}])", members.join(","))
                }
            };
            format!("{prefix}List(&{ty})")
        }
        other => format!(
            "{prefix}List(&{})",
            capture_repr_type(plan, plan.element(other)?)?
        ),
    })
}
pub(super) fn capture_dyn(
    plan: &Plan<'_>,
    repr: Repr,
    value: &str,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<String> {
    if matches!(repr, Repr::Tagged | Repr::Dyn(_)) {
        return Ok(value.to_owned());
    }
    let (tag, raw) = match repr {
        Repr::Float => {
            return Ok(format!(
                "rt::Dyn{{tag:rt::FLOAT_TAG,payload:({value}).to_bits()}}"
            ));
        }
        Repr::Int => (0, value.to_owned()),
        Repr::Bool => (1, value.to_owned()),
        Repr::Null => (2, "0usize".into()),
        Repr::Str => (3, format!("{value}.as_raw()")),
        Repr::Bigint => (18, format!("{value}.as_raw()")),
        Repr::Bytes => (19, format!("{value}.as_raw()")),
        Repr::Opaque => (20, format!("{value}.as_raw()")),
        Repr::Enum(e) => {
            return Ok(format!(
                "rt::Dyn{{tag:CAPTURE_ENUM_{e}_TAG,payload:{value} as u64}}"
            ));
        }
        Repr::Naming => {
            return Ok(format!(
                "rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:{value} as u64}}"
            ));
        }
        Repr::Closure => (
            plan.closure_descriptors().unwrap().1 + 3,
            format!("{value}.as_raw()"),
        ),
        Repr::Ptr(c) => (descriptors[&c] + 3, format!("{value}.as_raw()")),
        r if r.pointer() => (lists[&r] + 3, format!("{value}.as_raw()")),
        _ => return Err(no("map capture slot")),
    };
    let tag = if repr.pointer() {
        format!("if raw==0{{2}}else{{{tag}}}")
    } else {
        tag.to_string()
    };
    Ok(format!(
        "{{let raw={raw};rt::Dyn{{tag:{tag},payload:raw as u64}}}}"
    ))
}

fn map_types(plan: &Plan<'_>, repr: Repr) -> Gen<(String, String)> {
    if let Repr::Map(id) = repr {
        let types = plan.map_types.borrow();
        Ok((types[id].key_type.clone(), types[id].value_type.clone()))
    } else {
        let (key, value) = plan.map_parts(repr)?;
        Ok((
            capture_repr_type(plan, key)?,
            capture_repr_type(plan, value)?,
        ))
    }
}
pub(super) fn declared_type(plan: &Plan<'_>, ty: &RuntimeTy) -> Gen<String> {
    capture_type(plan, ty)
}
pub(super) fn declared_field_type(plan: &Plan<'_>, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
    capture_wire_type(plan, ty)
}
fn capture_element(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    repr: Repr,
) -> Gen<(usize, String)> {
    Ok(match repr {
        Repr::Int => (0, "Int".into()),
        Repr::Bool => (1, "Bool".into()),
        Repr::Float => (u32::MAX as usize - 1, "Float".into()),
        Repr::Str => (3, "String".into()),
        Repr::Bigint => (18, "Bigint".into()),
        Repr::Bytes => (19, "Bytes".into()),
        Repr::Opaque => (20, "Opaque".into()),
        r @ (Repr::Naming | Repr::Enum(_)) => (
            layout::enum_tag(plan, r, descriptors, lists),
            capture_repr_type(plan, r)?
                .strip_prefix("rt::telemetry::capture::Type::")
                .unwrap()
                .into(),
        ),
        Repr::Ptr(c) => (descriptors[&c] + 3, format!("Class(CAPTURE_D{c})")),
        Repr::Closure => (plan.closure_descriptors().unwrap().1 + 3, "Unknown".into()),
        r if r.growing() => {
            let (_, element) = capture_list_element(plan, descriptors, lists, r)?;
            (
                lists[&r] + 3,
                format!("List(&rt::telemetry::capture::Type::{element})"),
            )
        }
        r @ Repr::Map(_) => (
            lists[&r] + 3,
            capture_repr_type(plan, r)?
                .strip_prefix("rt::telemetry::capture::Type::")
                .unwrap()
                .to_owned(),
        ),
        Repr::MapStrInt => (
            lists[&Repr::MapStrInt] + 3,
            "Map(&rt::telemetry::capture::Type::String,&rt::telemetry::capture::Type::Int)".into(),
        ),
        Repr::Tagged => (usize::MAX - 1, "Unknown".into()),
        Repr::Null => (2, "Null".into()),
        _ => return Err(no("capture list element")),
    })
}

fn capture_list_element(
    plan: &Plan<'_>,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
    repr: Repr,
) -> Gen<(usize, String)> {
    if let Repr::ListClosure(id) = repr {
        let ty = super::closures::capture_type(&plan.closure_lists[&id])?;
        return Ok((
            plan.closure_descriptors().unwrap().1 + 3,
            ty.strip_prefix("rt::telemetry::capture::Type::")
                .unwrap()
                .into(),
        ));
    }
    if let Repr::List(id) = repr
        && let Some(declared) = &plan.list_elements.borrow()[id].declared
    {
        let (tag, _) = capture_element(plan, descriptors, lists, plan.element(repr)?)?;
        return Ok((
            tag,
            declared
                .strip_prefix("rt::telemetry::capture::Type::")
                .unwrap()
                .into(),
        ));
    }
    let (tag, mut ty) = capture_element(plan, descriptors, lists, plan.element(repr)?)?;
    if let Repr::List(id) = repr {
        let members = plan.list_elements.borrow()[id].members.clone();
        if !members.is_empty() {
            let types = members
                .into_iter()
                .map(|m| {
                    capture_element(plan, descriptors, lists, m)
                        .map(|(_, ty)| format!("rt::telemetry::capture::Type::{ty}"))
                })
                .collect::<Gen<Vec<_>>>()?;
            ty = if types.len() == 1 {
                types[0]
                    .strip_prefix("rt::telemetry::capture::Type::")
                    .unwrap()
                    .to_owned()
            } else {
                format!("Union(&[{}])", types.join(","))
            };
        }
    }
    Ok((tag, ty))
}

// Unions are sets for storage identity. Keep the first declaration's order
// separately for canonical type/value captures.
pub(super) fn type_identity(plan: &Plan<'_>, ty: &RuntimeTy) -> Gen<String> {
    Ok(match ty {
        RuntimeTy::Union(members) => {
            let mut members = members
                .iter()
                .map(|m| type_identity(plan, m))
                .collect::<Gen<Vec<_>>>()?;
            members.sort();
            members.dedup();
            format!("Union({})", members.join(","))
        }
        RuntimeTy::List(t) => format!("List({})", type_identity(plan, t)?),
        RuntimeTy::Map { key, value } => format!(
            "Map({},{})",
            type_identity(plan, key)?,
            type_identity(plan, value)?
        ),
        _ => declared_type(plan, ty)?,
    })
}
pub(super) fn field_type_identity(plan: &Plan<'_>, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
    use bex_vm_types::RuntimeTy as T;
    Ok(match ty {
        T::Union(members) => {
            let mut members = members
                .iter()
                .map(|m| field_type_identity(plan, m))
                .collect::<Gen<Vec<_>>>()?;
            members.sort();
            members.dedup();
            format!("Union({})", members.join(","))
        }
        T::List(t) => format!("List({})", field_type_identity(plan, t)?),
        T::Map { key, value } => format!(
            "Map({},{})",
            field_type_identity(plan, key)?,
            field_type_identity(plan, value)?
        ),
        _ => declared_field_type(plan, ty)?,
    })
}
