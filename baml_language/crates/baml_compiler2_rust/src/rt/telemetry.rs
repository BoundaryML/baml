use super::*;

impl Emitter<'_, '_> {
    pub(super) fn telemetry_site(&self, callee: usize) -> usize {
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
    pub(super) fn capture_value(&self, value: String, repr: Repr) -> Gen<String> {
        match repr {
            Repr::Dyn(_) | Repr::Tagged => Ok(value),
            Repr::Naming => Ok(format!(
                "rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:({value}) as u64}}"
            )),
            _ => self.coerce((value, repr), Repr::Tagged),
        }
    }
}

pub(super) fn telemetry_metadata_source(m: &btel_types::FunctionMetadata) -> String {
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
    let kind = &m.kind;
    format!(
        "FunctionMetadata{{function_id,fqn:{},display_name:{},source_file:{},source_span:{span},kind:RuntimeFunctionKind::{kind:?},origin:RuntimeFunctionOrigin::{:?},owner_type:{},parent_function:{},lambda_path:{},definition_key:{},package_name:{},namespace:vec![{}],argument_layout:{args},source_map:{map},model_invoke:{}}}",
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

pub(super) fn emit(
    mut out: &mut String,
    cx: &ProgramEmission<'_, '_>,
    reached: &BTreeSet<usize>,
    json_decode: Option<usize>,
    host: (usize, usize),
    telemetry_sites: &std::cell::RefCell<Vec<(Option<usize>, usize, usize)>>,
) -> Gen<usize> {
    let ProgramEmission { db, plan, all, .. } = *cx;
    let program = plan.program;
    let (stringify, aliased) = host;
    // The pack host formats an integer through stringify and _aliased(Declared).
    // Preserve these logical calls in the typed host specialization too.
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
    if plan.list_stringify {
        let native=host_body.blocks.iter().find(|b|matches!(&b.terminator,Some(Terminator::Call{callee:Operand::Constant(Constant::Function(f)),..}) if plan.index.function(db,*f).is_some_and(|(id,_)|plan.index.display(id)=="baml.json._stringify"))).ok_or_else(||no("stringify native boundary"))?;
        let instruction = bytecode.block_pcs[native.id.0].terminator.unwrap();
        let pc = bytecode
            .lower_to_compact_with_offsets()
            .offset_of(instruction)
            .unwrap();
        let line = bytecode.source_line_for_pc(instruction);
        writeln!(out,"static S_LIST_STRINGIFY:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}],pcs:&[{pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};",plan.index.info(stringify).source_file,plan.index.display(stringify)).unwrap();
        let alias_site = format_site + 1;
        writeln!(out,"unsafe fn f{stringify}_list_json(site:usize,value:rt::Dyn,naming:usize,indent:rt::Dyn,omit:bool,on:bool,oi:bool,oo:bool)->R<usize>{{unsafe{{rt::telemetry::observed_captured(&O{stringify},site,||rt::telemetry::capture_arguments(&[value,rt::Dyn{{tag:if on{{u64::MAX}}else{{CAPTURE_NAMING_TAG}},payload:naming as u64}},if oi{{rt::Dyn{{tag:u64::MAX,payload:0}}}}else{{indent}},rt::Dyn{{tag:if oo{{u64::MAX}}else{{1}},payload:omit as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:3,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::check_stack().map_err(|e|e.at(&S_LIST_STRINGIFY,0))?;let aliased=rt::telemetry::observed_captured(&O{aliased},{alias_site},||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;rt::json_lists::stringify(&mut rt::strings::Cx::current(),value,JSON_LIST_SHAPES,aliased,indent,omit).map(|s|s.as_raw()).map_err(|e|e.at(&S_LIST_STRINGIFY,0))}})}}}}").unwrap();
    }
    let mut observed = reached.clone();
    if plan.json_public_to {
        let to = plan
            .program
            .objects
            .iter()
            .enumerate()
            .find_map(|(i, o)| {
                (matches!(o, Object::Function(_)) && plan.index.display(i) == "baml.json.to")
                    .then_some(i)
            })
            .unwrap();
        observed.insert(to);
        let MirFunctionKind::Bytecode(body) = &all[&to].kind else {
            unreachable!()
        };
        let block=body.blocks.iter().find(|b|matches!(&b.terminator,Some(Terminator::Call{callee:Operand::Constant(Constant::Function(f)),..})if plan.index.function(db,*f).is_some_and(|(id,_)|id==aliased))).unwrap();
        let positions = plan.index.positions(to);
        let instruction = positions.block_pcs[block.id.0].terminator.unwrap();
        let pc = positions
            .lower_to_compact_with_offsets()
            .offset_of(instruction)
            .unwrap();
        let site = telemetry_sites.borrow().len();
        writeln!(out, "const JSON_TO_ALIAS_SITE:usize={site};").unwrap();
        telemetry_sites.borrow_mut().push((Some(to), pc, aliased));
    }
    for (chain, _) in &plan.init_chains {
        observed.insert(*chain);
        writeln!(
            out,
            "static O{chain}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);"
        )
        .unwrap();
    }
    if let Some(decode) = json_decode {
        observed.insert(decode);
        let MirFunctionKind::Bytecode(body) = &all[&decode].kind else {
            unreachable!()
        };
        let block = body
            .blocks
            .iter()
            .find(|b| {
                matches!(&b.terminator,
            Some(Terminator::Call { callee: Operand::Constant(Constant::Function(f)), .. })
            if plan.index.function(db,*f).is_some_and(|(id,_)|id==aliased))
            })
            .ok_or_else(|| no("decode naming call"))?;
        let bytecode = plan.index.positions(decode);
        let pc = bytecode
            .lower_to_compact_with_offsets()
            .offset_of(bytecode.block_pcs[block.id.0].terminator.unwrap())
            .unwrap();
        writeln!(
            &mut out,
            "const JSON_ALIAS_SITE:usize={};",
            telemetry_sites.borrow().len()
        )
        .unwrap();
        telemetry_sites
            .borrow_mut()
            .push((Some(decode), pc, aliased));
    }
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
                    .contains(&u32::try_from(plan.index.rt_origin(id)).expect("function index"))
            })
            .map(|p| baml_base::Name::new(&p.name));
        let metadata = info.telemetry_metadata(allocator.allocate().unwrap(), label.as_ref());
        let metadata = telemetry_metadata_source(&metadata);
        let hidden = matches!(
            all.get(&id).map(|m| &m.kind),
            Some(MirFunctionKind::Builtin(_))
        ) || info.trace_passthrough
            || matches!(all.get(&id).map(|m|&m.kind),
            Some(MirFunctionKind::Bytecode(body)) if body.blocks.iter().flat_map(|b|&b.statements).any(|s|matches!(&s.kind,
                StatementKind::Intrinsic {op:baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(baml_compiler2_mir::BuiltinTraceHook::Hidden),..})));

        writeln!(&mut out,"rt::telemetry::Function{{captures:Some(&CAPTURE_METADATA),metadata:|function_id|{{use rt::telemetry::types::*;{metadata}}},word:&O{id},policy:rt::telemetry::Policy::NONE,hidden:{hidden}}},").unwrap();
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
    Ok(format_site)
}

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        if let registry::Admission::Statement(statement) = event {
            let unsupported_hook = match statement {
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
        }
        Ok(())
    }
}
