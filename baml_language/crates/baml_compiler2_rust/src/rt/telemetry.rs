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
    let mut observed = reached.clone();
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
        let hidden = info.trace_passthrough
            || matches!(&all[&id].kind,
            MirFunctionKind::Bytecode(body) if body.blocks.iter().flat_map(|b|&b.statements).any(|s|matches!(&s.kind,
                StatementKind::Intrinsic {op:baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(baml_compiler2_mir::BuiltinTraceHook::Hidden),..})));

        let hook = match &all[&id].kind {
            MirFunctionKind::Bytecode(body) => body
                .blocks
                .iter()
                .flat_map(|b| &b.statements)
                .find_map(|s| match s.kind {
                    StatementKind::Intrinsic {
                        op: baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(mode),
                        ..
                    } => Some(mode),
                    _ => None,
                }),
            _ => None,
        };
        use baml_compiler2_mir::BuiltinTraceHook as Hook;
        let hook_mode = match hook {
            Some(Hook::Hidden) => "Some(rt::telemetry::types::InvocationMode::Hidden)",
            Some(Hook::Timing) => "Some(rt::telemetry::types::InvocationMode::Timing)",
            Some(_) => "Some(rt::telemetry::types::InvocationMode::Span)",
            None => "None",
        };
        let policy = match hook {
            Some(Hook::Span) => {
                "rt::telemetry::Policy{span_from_entry:true,capture_output:true,capture_error:true,..rt::telemetry::Policy::NONE}"
            }
            Some(Hook::Rich) => {
                "rt::telemetry::Policy{span_from_entry:true,capture_inputs:true,capture_output:true,capture_error:true,..rt::telemetry::Policy::NONE}"
            }
            Some(Hook::EmptySpan) => {
                "rt::telemetry::Policy{span_from_entry:true,..rt::telemetry::Policy::NONE}"
            }
            _ => "rt::telemetry::Policy::NONE",
        };
        writeln!(&mut out,"rt::telemetry::Function{{captures:Some(&CAPTURE_METADATA),metadata:|function_id|{{use rt::telemetry::types::*;{metadata}}},word:&O{id},policy:{policy},hidden:{hidden},hook_mode:{hook_mode}}},").unwrap();
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
    fn statement(
        &self,
        _e: &mut Emitter<'_, '_>,
        statement: &StatementKind<'_>,
    ) -> Option<Gen<String>> {
        match statement {
            StatementKind::Intrinsic {
                op: baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(_),
                args,
            } => Some(if args.is_empty() {
                Ok(String::new())
            } else {
                Err(no("builtin hook arguments"))
            }),
            _ => None,
        }
    }

    fn admit(&self, event: &registry::Admission<'_, '_>) -> Gen<()> {
        if let registry::Admission::Statement(statement) = event {
            let unsupported_hook = match statement {
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
