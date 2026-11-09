//! JSON specializations for maps, kept separate from class union decoding.
use super::*;
pub(super) fn code(repr: Repr) -> Gen<usize> {
    match repr {
        Repr::MapStrInt => Ok(2_000_000),
        Repr::Map(id) => Ok(2_000_001 + id),
        _ => Err(no("JSON map representation")),
    }
}
pub(super) fn emit(
    out: &mut String,
    cx: &ProgramEmission<'_, '_>,
    lists: &BTreeMap<Repr, usize>,
    decode: Option<usize>,
    alias: usize,
) -> Gen<()> {
    let Some(callee) = decode else { return Ok(()) };
    let plan = cx.plan;
    if plan.json_maps.is_empty() {
        return Ok(());
    }
    let MirFunctionKind::Bytecode(body) = &cx.all[&callee].kind else {
        return Err(no("JSON decode body"));
    };
    let block=body.blocks.iter().find(|b|matches!(&b.terminator,Some(Terminator::Call{callee:Operand::Constant(Constant::Function(f)),..}) if plan.index.function(cx.db,*f).is_some_and(|(id,_)|plan.index.display(id)=="baml.json._decode"))).ok_or_else(||no("JSON source location"))?;
    let bytecode = plan.index.positions(callee);
    let pc = bytecode.block_pcs[block.id.0].terminator.unwrap();
    let line = bytecode.source_line_for_pc(pc);
    let pc = bytecode
        .lower_to_compact_with_offsets()
        .offset_of(pc)
        .unwrap();
    if !plan
        .json_decode_codes
        .iter()
        .any(|c| *c >= 3 && *c < 2_000_000)
    {
        writeln!(out,"static O{callee}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);static S{callee}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}],pcs:&[{pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};",plan.index.info(callee).source_file,plan.index.display(callee)).unwrap();
    }
    for (&repr, ty) in &plan.json_maps {
        let RuntimeTy::Map { key, value } = ty else {
            unreachable!()
        };
        let (_, value_repr) = plan.map_parts(repr)?;
        let recursive = !matches!(key.as_ref(), RuntimeTy::String) || recursive_value(value);
        if recursive {
            for aliased in [false, true] {
                json_lists::emit(
                    out,
                    plan,
                    cx.descriptors,
                    lists,
                    Some(&baml_compiler2_mir::TyTemplate::from(
                        baml_compiler2_mir::RealizedTy::try_from(ty)
                            .map_err(|_| no("JSON map schema"))?,
                    )),
                    &[],
                    code(repr)?,
                    aliased,
                )?;
            }
        }
        let decoder = if recursive {
            String::new()
        } else {
            format!(
                "|cx,reader,path|{{{}}}",
                decode_value(plan, value, cx.descriptors, lists)?
            )
        };
        let code = code(repr)?;
        let tag = lists[&repr] + 3;
        let desc = lists[&repr];
        let slot = maps::slot_type(value_repr, cx.descriptors, lists)?;
        let ty = capture_codegen::capture_repr(plan, repr)?;
        let decode = if recursive {
            format!(
                "if naming==0{{json_specialized_{code}_0(&mut rt::strings::Cx::current(),s)}}else{{json_specialized_{code}_1(&mut rt::strings::Cx::current(),s)}}"
            )
        } else {
            format!(
                "rt::maps::json::decode::<{slot}>(&mut rt::strings::Cx::current(),{desc},rt::strings::Str::from_raw(s),{decoder}).map(|map|rt::Dyn{{tag:{tag},payload:map.as_raw() as u64}})"
            )
        };
        writeln!(out,"unsafe fn f{callee}_json{code}(telemetry_site:usize,s:usize,naming:usize,omitted:bool)->R<rt::Dyn>{{unsafe{{rt::telemetry::observed_captured(&O{callee},telemetry_site,||{{let mut c=rt::telemetry::capture_arguments(&[rt::Dyn{{tag:3,payload:s as u64}},rt::Dyn{{tag:if omitted{{u64::MAX}}else{{CAPTURE_NAMING_TAG}},payload:naming as u64}}]);c.types=rt::telemetry::capture_type_args(&[(\"T\",&{ty})]);c}},|r|match r{{Ok(v)=>rt::telemetry::capture_value(*v),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::check_stack().map_err(|e|e.at(&S{callee},0))?;rt::telemetry::observed_captured(&O{alias},JSON_ALIAS_SITE,||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;({decode}).map_err(|mut e|{{rt::telemetry::evidence::raised(&mut e,&S{callee},0,None);e}})}})}}}}").unwrap();
    }
    Ok(())
}

fn decode_value(
    plan: &Plan<'_>,
    ty: &RuntimeTy,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<String> {
    let repr = plan.repr(ty)?;
    Ok(match ty {
        RuntimeTy::Int => "rt::maps::json::integer(cx,reader,path)".into(),
        RuntimeTy::Bool => "rt::maps::json::boolean(cx,reader,path)".into(),
        RuntimeTy::Float => "rt::maps::json::float(cx,reader,path)".into(),
        RuntimeTy::String => "rt::maps::json::string(cx,reader,path)".into(),
        RuntimeTy::Null => "rt::maps::json::null(cx,reader,path)".into(),
        RuntimeTy::Map { key, value } if matches!(key.as_ref(), RuntimeTy::String) => {
            let desc = lists[&repr];
            let nested = decode_value(plan, value, descriptors, lists)?;
            let slot = maps::slot_type(repr, descriptors, lists)?;
            format!(
                "rt::maps::json::decode_at(cx,{desc},reader,path,|cx,reader,path|{{{nested}}}).map(|map|<{slot}>::from_raw(map.as_raw()))"
            )
        }
        RuntimeTy::Union(members)
            if members.len() == 2 && members.iter().any(|m| matches!(m, RuntimeTy::Null)) =>
        {
            let member = members
                .iter()
                .find(|m| !matches!(m, RuntimeTy::Null))
                .unwrap();
            let nested = decode_value(plan, member, descriptors, lists)?;
            let member_repr = plan.repr(member)?;
            if repr == Repr::Tagged {
                let tagged =
                    capture_codegen::capture_dyn(member_repr, "value", descriptors, lists)?;
                format!(
                    "if reader.peek()==Some(b'n'){{rt::maps::json::null(cx,reader,path).map(|_|rt::Dyn{{tag:2,payload:0}})}}else{{({nested}).map(|value|{tagged})}}"
                )
            } else {
                let slot = maps::slot_type(repr, descriptors, lists)?;
                format!(
                    "if reader.peek()==Some(b'n'){{rt::maps::json::null(cx,reader,path).map(|_|<{slot}>::from_raw(0))}}else{{{nested}}}"
                )
            }
        }
        _ => return Err(no("JSON map value specialization")),
    })
}

/// The linked declaration supplies the error descriptor and canonical captures;
/// maps do not introduce a second runtime exception layout.
pub(super) fn serialization_error_class(plan: &Plan<'_>) -> Gen<usize> {
    plan.program
        .packages
        .iter()
        .filter(|p| p.name == "baml")
        .flat_map(|p| &p.classes)
        .find_map(|(path, id)| (path.to_string() == "json.SerializationError").then_some(id.raw()))
        .ok_or_else(|| no("JSON serialization error class"))
}

pub(super) fn schema(
    plan: &Plan<'_>,
    repr: Repr,
    value: &str,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<String> {
    let (key, element) = plan.map_parts(repr)?;
    let key_type = if let Repr::Map(id) = repr {
        plan.map_types.borrow()[id].key_type.clone()
    } else {
        capture_codegen::capture_repr(plan, key)?
    };
    let key_slot = maps::slot_type(key, descriptors, lists)?;
    if !matches!(key, Repr::Str | Repr::Tagged) {
        return Ok("rt::json_lists::Ty::InvalidMap".into());
    }
    let key_value = match key {
        Repr::Str => "key",
        Repr::Tagged => "rt::Dyn{tag:3,payload:key.as_raw() as u64}",
        _ => return Err(no("JSON non-string map key type")),
    };
    let desc = lists[&repr];
    let slot = maps::slot_type(element, descriptors, lists)?;
    let raw = match element {
        Repr::Int => "value.payload as i64".into(),
        Repr::Float => "f64::from_bits(value.payload)".into(),
        Repr::Bool => "value.payload!=0".into(),
        Repr::Enum(_) | Repr::Naming => "value.payload as usize".into(),
        Repr::Null => "rt::maps::Null".into(),
        Repr::Tagged | Repr::Dyn(_) => "value".into(),
        _ if element.pointer() => format!("<{slot}>::from_raw(value.payload as usize)"),
        _ => return Err(no("JSON map slot")),
    };
    let map = format!("rt::maps::Map::<{key_slot},{slot}>");
    let insert = if key == Repr::Tagged {
        let panic_desc = descriptors[&maps::panic_class(plan)?];
        format!(
            "let key={key_value};let hash=rt::maps::keys::hash(cx,key,CAPTURE_SHAPES,|cx,message|rt::maps::keys::panic(cx,{panic_desc},message))?;{map}::from_raw(p).set_with(cx,key,{raw},hash,|cx,a,b|rt::maps::keys::equals(cx,a,b,CAPTURE_SHAPES)).map(|_|())"
        )
    } else {
        format!("{map}::from_raw(p).set(cx,{key_value},{raw}).map(|_|())")
    };
    Ok(format!(
        "rt::json_lists::Ty::Map{{key:&{key_type},value:&{value},adapter:&rt::maps::json::Adapter{{tag:{},new:|cx|unsafe{{{map}::new(cx,{desc}).map(|m|m.as_raw())}},insert:|cx,p,key,value|unsafe{{{insert}}}}}}}",
        desc + 3
    ))
}
fn recursive_value(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Literal(..)
        | RuntimeTy::Class(..)
        | RuntimeTy::List(_)
        | RuntimeTy::Enum(_)
        | RuntimeTy::EnumVariant(..) => true,
        RuntimeTy::Map { value, .. } => recursive_value(value),
        RuntimeTy::Union(m) => {
            m.iter().any(recursive_value)
                || m.len() != 2
                || !m.iter().any(|m| matches!(m, RuntimeTy::Null))
        }
        _ => false,
    }
}

pub(super) fn enum_schema(
    plan: &Plan<'_>,
    repr: Repr,
    variant: Option<&baml_base::Name>,
    aliased: bool,
    descriptors: &BTreeMap<usize, usize>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<String> {
    let id = enum_id(plan, repr)?;
    let Object::Enum(definition) = &plan.program.objects[bex_vm_types::ObjectIndex::from_raw(id)]
    else {
        unreachable!()
    };
    for package in &plan.program.packages {
        for rule in package.impl_rules.values().flatten() {
            if rule.methods.contains_key("from_json")
                && matches!(&rule.for_ty_pattern,bex_vm_types::TyTemplate::Enum(head) if head.tag()==definition.type_tag)
            {
                return Err(no("JSON enum with custom interface implementations"));
            }
        }
    }
    let variant = variant
        .map(|name| {
            definition
                .variants
                .iter()
                .position(|v| v.name == name.as_str())
                .ok_or_else(|| no("JSON enum variant"))
        })
        .transpose()?;
    let variants = definition
        .variants
        .iter()
        .map(|v| {
            format!(
                "rt::json_lists::EnumVariant{{name:{:?},alias:{:?},skip:{}}}",
                v.name,
                v.alias.as_ref().unwrap_or(&v.name),
                v.skip
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let tag = layout::enum_tag(plan, repr, descriptors, lists);
    Ok(format!(
        "rt::json_lists::Ty::Enumeration(&rt::maps::json::enums::Schema{{tag:{tag},name:{:?},variants:&[{variants}],aliased:{aliased},variant:{variant:?}}})",
        definition.path.to_string()
    ))
}

pub(super) fn enum_id(plan: &Plan<'_>, repr: Repr) -> Gen<usize> {
    match repr {
        Repr::Enum(id) => Ok(id),
        Repr::Naming => plan
            .program
            .packages
            .iter()
            .find(|p| p.name == "baml")
            .and_then(|p| {
                p.enums
                    .iter()
                    .find_map(|(path, id)| (path.to_string() == "json.Naming").then_some(id.raw()))
            })
            .ok_or_else(|| no("JSON naming enum")),
        _ => Err(no("JSON enum representation")),
    }
}
