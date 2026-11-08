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
    emit_encoders(out, cx, lists)?;
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
    if plan.json_types.is_empty() && plan.json_list_types.is_empty() {
        writeln!(out,"static O{callee}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);static S{callee}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}],pcs:&[{pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};",plan.index.info(callee).source_file,plan.index.display(callee)).unwrap();
    }
    for (&repr, ty) in &plan.json_maps {
        let RuntimeTy::Map { key, value } = ty else {
            unreachable!()
        };
        if !matches!(key.as_ref(), RuntimeTy::String) {
            return Err(no("JSON non-string map key type"));
        }
        let (_, value_repr) = plan.map_parts(repr)?;
        let decoder = format!(
            "|cx,reader,path|{{{}}}",
            decode_value(plan, value, cx.descriptors, lists)?
        );
        let code = code(repr)?;
        let tag = lists[&repr] + 3;
        let desc = lists[&repr];
        let slot = maps::slot_type(value_repr, cx.descriptors, lists)?;
        let ty = capture_codegen::capture_repr(plan, repr)?;
        writeln!(out,"unsafe fn f{callee}_json{code}(telemetry_site:usize,s:usize,naming:usize,omitted:bool)->R<rt::Dyn>{{unsafe{{rt::telemetry::observed_captured(&O{callee},telemetry_site,||{{let mut c=rt::telemetry::capture_arguments(&[rt::Dyn{{tag:3,payload:s as u64}},rt::Dyn{{tag:if omitted{{u64::MAX}}else{{CAPTURE_NAMING_TAG}},payload:naming as u64}}]);c.types=rt::telemetry::capture_type_args(&[(\"T\",&{ty})]);c}},|r|match r{{Ok(v)=>rt::telemetry::capture_value(*v),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::check_stack().map_err(|e|e.at(&S{callee},0))?;rt::telemetry::observed_captured(&O{alias},JSON_ALIAS_SITE,||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;rt::maps::json::decode::<{slot}>(&mut rt::strings::Cx::current(),{desc},rt::strings::Str::from_raw(s),{decoder}).map(|map|rt::Dyn{{tag:{tag},payload:map.as_raw() as u64}}).map_err(|mut e|{{rt::telemetry::evidence::raised(&mut e,&S{callee},0,None);e}})}})}}}}").unwrap();
    }
    Ok(())
}

fn encodable(plan: &Plan<'_>, repr: Repr) -> bool {
    match repr {
        Repr::Int | Repr::Float | Repr::Bool | Repr::Str | Repr::Null => true,
        Repr::ListInt | Repr::ListStr | Repr::ListListStr => true,
        Repr::MapStrInt | Repr::Map(_) => {
            let Ok((key, value)) = plan.map_parts(repr) else {
                return false;
            };
            key != Repr::Str
                || encodable(plan, value)
                || matches!(repr,Repr::Map(id) if value==Repr::Tagged && plan.map_types.borrow()[id].scalar_json_value)
        }
        _ => false,
    }
}
fn emit_encoders(
    out: &mut String,
    cx: &ProgramEmission<'_, '_>,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<()> {
    if !cx.plan.map_stringify {
        return Ok(());
    }
    let error_desc = cx.descriptors[&serialization_error_class(cx.plan)?];
    for (&repr, &desc) in lists {
        if !matches!(repr, Repr::MapStrInt | Repr::Map(_)) || !encodable(cx.plan, repr) {
            continue;
        }
        let (key, value) = cx.plan.map_parts(repr)?;
        if key != Repr::Str {
            let code = code(repr)?;
            writeln!(out,"unsafe fn rt_map_json_encode_{code}(cx:&mut rt::strings::Cx,p:usize,path:&str)->R<rt::strings::Str>{{Err(unsafe{{rt::maps::json::serialization_error(cx,{error_desc},\"JSON requires maps with string keys\".into(),path,\"map_key\")}})}}").unwrap();
            continue;
        }
        let slot = maps::slot_type(value, cx.descriptors, lists)?;
        let code = code(repr)?;
        let mut write = match value {
            Repr::Int | Repr::Bool => "out.push_str(&value.to_string());".to_owned(),
            Repr::Float => format!("rt::maps::json::write_float(cx,value,path,{error_desc},out)?;"),
            Repr::Tagged => format!("rt::maps::json::write_scalar(cx,value,path,{error_desc},out)?;"),
            Repr::ListInt => "rt::maps::json::write_int_list(value.as_raw(),out)?;".into(),
            Repr::ListStr => "rt::maps::json::write_string_list(cx,rt::lists::List::from_raw(value.as_raw()),out)?;".into(),
            Repr::ListListStr => "rt::maps::json::write_nested_string_list(cx,rt::lists::List::from_raw(value.as_raw()),out)?;".into(),
            Repr::Str => "rt::maps::json::write_string(out,value.try_as_str(cx)?);".to_owned(),
            Repr::Null => "out.push_str(\"null\");".to_owned(),
            Repr::MapStrInt | Repr::Map(_) => format!(
                "let text=rt_map_json_encode_{}(cx,value.as_raw(),path)?;out.push_str(text.try_as_str(cx)?);",
                self::code(value)?
            ),
            _ => unreachable!(),
        };
        if value.pointer() {
            write = format!("if value.as_raw()==0{{out.push_str(\"null\");}}else{{{write}}}");
        }
        writeln!(out,"unsafe fn rt_map_json_encode_{code}(cx:&mut rt::strings::Cx,p:usize,path:&str)->R<rt::strings::Str>{{unsafe{{rt::maps::json::stringify_at(cx,rt::maps::Map::<rt::strings::Str,{slot}>::from_raw(p),path,|cx,value,path,out|{{{write}Ok(())}})}}}}").unwrap();
        let _ = desc;
    }
    Ok(())
}
fn map_argument(emitter: &Emitter<'_, '_>, operand: &Operand<'_>, depth: usize) -> Gen<Repr> {
    let (_, repr) = emitter.operand(operand)?;
    if matches!(repr, Repr::MapStrInt | Repr::Map(_)) {
        return Ok(repr);
    }
    if depth >= emitter.body.locals.len() {
        return Err(no("JSON stringify dynamic map argument"));
    }
    let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = operand else {
        return Err(no("JSON stringify non-map argument"));
    };
    let mut inferred = None;
    for statement in emitter.body.blocks.iter().flat_map(|b| &b.statements) {
        if let StatementKind::Assign {
            destination: Place::Local(dst),
            value,
        } = &statement.kind
            && dst == local
        {
            let Rvalue::Use(value) = value else {
                return Err(no("JSON stringify dynamic map argument"));
            };
            let next = map_argument(emitter, value, depth + 1)?;
            if inferred.is_some_and(|old| old != next) {
                return Err(no("JSON stringify mixed map argument"));
            }
            inferred = Some(next);
        }
    }
    inferred.ok_or_else(|| no("JSON stringify dynamic map argument"))
}
pub(super) fn terminator(emitter: &Emitter<'_, '_>, term: &Terminator<'_>) -> Option<Gen<String>> {
    let Terminator::Call {
        callee: Operand::Constant(Constant::Function(f)),
        args,
        ntypeargs: 0,
        has_trace: false,
        destination,
        ..
    } = term
    else {
        return None;
    };
    let (callee, _) = emitter.plan.index.function(emitter.db, *f)?;
    if emitter.plan.index.display(callee) != "baml.json.stringify" {
        return None;
    }
    Some((|| {
        if args.len() != 4 {
            return Err(no("JSON stringify arguments"));
        }
        let omitted = |i| matches!(args[i], Operand::Constant(Constant::OmittedArg));
        let naming = if omitted(1) {
            "0usize".into()
        } else {
            emitter.coerce(emitter.operand(&args[1])?, Repr::Naming)?
        };
        let naming_capture: String = if omitted(1) {
            "rt::Dyn{tag:u64::MAX,payload:0}".into()
        } else {
            "rt::Dyn{tag:CAPTURE_NAMING_TAG,payload:naming as u64}".into()
        };
        let (indent_binding, indent_capture) = if omitted(2) {
            (String::new(), "rt::Dyn{tag:u64::MAX,payload:0}".into())
        } else {
            let (value, repr) = emitter.operand(&args[2])?;
            (
                format!("let indent_value={value};"),
                emitter.capture_value("indent_value".into(), repr)?,
            )
        };
        let (omit_binding, omit_capture): (String, String) = if omitted(3) {
            (String::new(), "rt::Dyn{tag:u64::MAX,payload:0}".into())
        } else {
            let value = emitter.coerce(emitter.operand(&args[3])?, Repr::Bool)?;
            (
                format!("let omit_value={value};"),
                "rt::Dyn{tag:1,payload:omit_value as u64}".into(),
            )
        };
        let repr = map_argument(emitter, &args[0], 0)?;
        if !encodable(emitter.plan, repr) {
            return Err(no("JSON map value encoding"));
        }
        let pointer = emitter.coerce(emitter.operand(&args[0])?, repr)?;
        let value = emitter.coerce(("p".into(), repr), Repr::Tagged)?;
        let code = code(repr)?;
        let site = emitter.telemetry_site(callee);
        // Keep stringify's logical naming helper, at its own source location.
        let MirFunctionKind::Bytecode(body) = &emitter.all[&callee].kind else {
            return Err(no("stringify body"));
        };
        let (block, alias) = body
            .blocks
            .iter()
            .find_map(|b| match &b.terminator {
                Some(Terminator::Call {
                    callee: Operand::Constant(Constant::Function(f)),
                    ..
                }) => emitter
                    .plan
                    .index
                    .function(emitter.db, *f)
                    .filter(|(id, _)| emitter.plan.index.display(*id) == "baml.json._aliased")
                    .map(|(id, _)| (b, id)),
                _ => None,
            })
            .ok_or_else(|| no("JSON stringify naming helper"))?;
        let bytecode = emitter.plan.index.positions(callee);
        let pc = bytecode
            .lower_to_compact_with_offsets()
            .offset_of(bytecode.block_pcs[block.id.0].terminator.unwrap())
            .unwrap();
        let alias_site = emitter.telemetry_sites.borrow().len();
        emitter
            .telemetry_sites
            .borrow_mut()
            .push((Some(callee), pc, alias));
        let error_desc = emitter.descriptors[&serialization_error_class(emitter.plan)?];
        let indent_value = if omitted(2) {
            "rt::Dyn{tag:0,payload:0}".to_owned()
        } else {
            indent_capture.clone()
        };
        let native=body.blocks.iter().find(|b| matches!(&b.terminator,Some(Terminator::Call{callee:Operand::Constant(Constant::Function(f)),..}) if emitter.plan.index.function(emitter.db,*f).is_some_and(|(id,_)|emitter.plan.index.display(id)=="baml.json._stringify"))).ok_or_else(||no("JSON stringify source location"))?;
        let native_pc = bytecode.block_pcs[native.id.0].terminator.unwrap();
        let line = bytecode.source_line_for_pc(native_pc);
        let native_pc = bytecode
            .lower_to_compact_with_offsets()
            .offset_of(native_pc)
            .unwrap();
        let file = &emitter.plan.index.info(callee).source_file;
        let function = emitter.plan.index.display(callee);
        let source = format!(
            "static SOURCE:rt::Source=rt::Source{{file:{file:?},function:{function:?},lines:&[{line}],pcs:&[{native_pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};"
        );
        let encoded = format!(
            "(||->R<usize>{{let indent=rt::maps::json::indent(&mut rt::strings::Cx::current(),{indent_value},{error_desc})?;let text=rt_map_json_encode_{code}(&mut rt::strings::Cx::current(),p,\"\")?;rt::maps::json::pretty(&mut rt::strings::Cx::current(),text,&indent).map(|s|s.as_raw())}})().map_err(|mut e|{{rt::telemetry::evidence::raised(&mut e,&SOURCE,0,None);e}})"
        );
        let call = format!(
            "{{{source}let p={pointer};let naming={naming};{indent_binding}{omit_binding}rt::telemetry::observed_captured(&O{callee},{site},||rt::telemetry::capture_arguments(&[{value},{naming_capture},{indent_capture},{omit_capture}]),|r:&R<usize>|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:3,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::telemetry::observed_captured(&O{alias},{alias_site},||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r:&R<bool>|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;{encoded}}})}}"
        );
        emitter.assign(destination, (emitter.fallible(&call), Repr::Str))
    })())
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
