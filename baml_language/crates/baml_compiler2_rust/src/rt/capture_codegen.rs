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
        "baml.panics.MapKeyNotFound",
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
    let objects: BTreeSet<_> = runtime
        .iter()
        .copied()
        .chain(descriptors.keys().copied())
        .chain(signature_declarations)
        .chain([naming])
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
    out.push_str("static CAPTURE_SHAPES:&[rt::telemetry::capture::Shape]=&[rt::telemetry::capture::Shape::String,\n");
    for desc in 1..after_lists {
        let class = if desc < layout::rt_descriptors() {
            Some(runtime[desc - 1])
        } else {
            descriptors
                .iter()
                .find_map(|(&object, &index)| (index == desc).then_some(object))
        };
        let shape = if plan
            .closure_descriptors()
            .is_some_and(|(_, closure)| desc == closure)
        {
            "Closure".to_owned()
        } else if let Some(class) = class {
            format!("Class{{declaration:CAPTURE_D{class},fields:DESCRIPTORS[{desc}].fields}}")
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
    writeln!(&mut out,"rt::telemetry::capture::Shape::Enum{{declaration:CAPTURE_D{naming},variants:&{:?}}},\n];\nconst CAPTURE_NAMING_TAG:u64={};\nstatic CAPTURE_METADATA:rt::telemetry::capture::Metadata=rt::telemetry::capture::Metadata{{shapes:CAPTURE_SHAPES,naming_tag:CAPTURE_NAMING_TAG}};",declaration.variants,after_lists+3).unwrap();
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

pub(super) fn capture_type(plan: &Plan<'_>, repr: Repr) -> Gen<String> {
    let prefix = "rt::telemetry::capture::Type::";
    Ok(match repr {
        Repr::Int => format!("{prefix}Int"),
        Repr::Float => format!("{prefix}Float"),
        Repr::Bool => format!("{prefix}Bool"),
        Repr::Str => format!("{prefix}String"),
        Repr::Null => format!("{prefix}Null"),
        Repr::Tagged | Repr::Dyn(_) => format!("{prefix}Unknown"),
        Repr::Ptr(c) => format!("{prefix}Class(CAPTURE_D{c})"),
        Repr::Map(_) | Repr::MapStrInt => {
            let (key, value) = map_types(plan, repr)?;
            format!("{prefix}Map(&{key},&{value})")
        }
        other => format!(
            "{prefix}List(&{})",
            capture_type(plan, Emitter::element(other)?)?
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
        Repr::Closure => (plan.closure_descriptors().unwrap().1 + 3, format!("{value}.as_raw()")),
        Repr::Ptr(c) => (descriptors[&c] + 3, format!("{value}.as_raw()")),
        r if r.pointer() => (lists[&r] + 3, format!("{value}.as_raw()")),
        _ => return Err(no("map capture slot")),
    };
    let tag = if repr.pointer() { format!("if raw==0{{2}}else{{{tag}}}") } else {tag.to_string()};
    Ok(format!("{{let raw={raw};rt::Dyn{{tag:{tag},payload:raw as u64}}}}"))
}

fn map_types(plan: &Plan<'_>, repr: Repr) -> Gen<(String, String)> {
    if let Repr::Map(id) = repr {
        let types = plan.map_types.borrow();
        Ok((types[id].key_type.clone(), types[id].value_type.clone()))
    } else {
        let (key, value) = plan.map_parts(repr)?;
        Ok((capture_type(plan, key)?, capture_type(plan, value)?))
    }
}
/// Preserve declared map types: equal storage layouts can have different
/// nullable/union element types in the recorder's canonical value encoding.
pub(super) fn declared_type(plan: &Plan<'_>, ty: &RuntimeTy) -> Gen<String> {
    match ty {
        RuntimeTy::Function { .. } => super::closures::capture_type(&plan.closure_type(ty)?.1),
        RuntimeTy::Literal(value, freshness) => literal_type(value, *freshness),
        RuntimeTy::Union(members) => {
            let members = members
                .iter()
                .map(|m| declared_type(plan, m))
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            Ok(format!("rt::telemetry::capture::Type::Union(&[{members}])"))
        }
        RuntimeTy::List(element) => Ok(format!(
            "rt::telemetry::capture::Type::List(&{})",
            declared_type(plan, element)?
        )),
        _ => capture_type(plan, plan.repr(ty)?),
    }
}
pub(super) fn declared_field_type(plan: &Plan<'_>, ty: &bex_vm_types::RuntimeTy) -> Gen<String> {
    match ty {
        bex_vm_types::RuntimeTy::Function { .. } => super::closures::capture_type(&plan.closure_field_type(ty)?.1),
        bex_vm_types::RuntimeTy::Literal(value, freshness) => literal_type(value, *freshness),
        bex_vm_types::RuntimeTy::Union(members) => {
            let members = members
                .iter()
                .map(|m| declared_field_type(plan, m))
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            Ok(format!("rt::telemetry::capture::Type::Union(&[{members}])"))
        }
        bex_vm_types::RuntimeTy::List(element) => Ok(format!(
            "rt::telemetry::capture::Type::List(&{})",
            declared_field_type(plan, element)?
        )),
        _ => capture_type(plan, plan.field_repr(ty)?),
    }
}

fn literal_type(literal: &baml_type::Literal, freshness: baml_type::Freshness) -> Gen<String> {
    let fresh = freshness == baml_type::Freshness::Fresh;
    Ok(match literal {
        baml_type::Literal::Int(value) => {
            format!("rt::telemetry::capture::Type::IntLiteral({value},{fresh})")
        }
        baml_type::Literal::Bool(value) => {
            format!("rt::telemetry::capture::Type::BoolLiteral({value},{fresh})")
        }
        baml_type::Literal::String(value) => {
            format!("rt::telemetry::capture::Type::StringLiteral({value:?},{fresh})")
        }
        _ => return Err(no("map literal type")),
    })
}
