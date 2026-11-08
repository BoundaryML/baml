use super::*;

impl Plan<'_> {
    pub(super) fn type_code(&self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<usize> {
        use baml_compiler2_mir::TyTemplate as T;
        match ty {
            T::Int => Ok(0),
            T::Float | T::Bool | T::Null | T::Map { .. } | T::Literal(..) => Ok(usize::MAX),
            T::String => Ok(1),
            T::List(_) => self
                .json_list_types
                .iter()
                .position(|t| t == ty)
                .map(|i| i + json_lists::TYPE_BASE)
                .map_or(Ok(usize::MAX), Ok),
            T::Class(_, _) | T::Union(_) => {
                let Ok(classes) = self.json_class_types(ty) else {
                    return self
                        .json_list_types
                        .iter()
                        .position(|t| t == ty)
                        .map(|i| i + json_lists::TYPE_BASE)
                        .map_or(Ok(usize::MAX), Ok);
                };
                self.json_types
                    .iter()
                    .position(|c| *c == classes)
                    .map(|i| i + 3)
                    .ok_or_else(|| no("unregistered JSON specialization"))
            }
            _ => Err(no(format!("type argument {ty:?}"))),
        }
    }
    pub(super) fn json_class_types(&self, ty: &baml_compiler2_mir::TyTemplate) -> Gen<Vec<usize>> {
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
}

pub(super) fn emit(
    mut out: &mut String,
    cx: &ProgramEmission<'_, '_>,
    json_decode: Option<usize>,
    alias_function: usize,
    lists: &BTreeMap<Repr, usize>,
) -> Gen<()> {
    let ProgramEmission {
        db,
        plan,
        all,
        descriptors,
    } = *cx;
    let program = plan.program;
    if let Some(callee) = json_decode {
        if plan.json_types.is_empty() && plan.json_list_types.is_empty() {
            return Err(no("missing JSON specialization"));
        }
        let specs = plan
            .json_types
            .iter()
            .enumerate()
            .map(|(i, c)| (i + 3, c.as_slice(), None))
            .chain(
                plan.json_list_types
                    .iter()
                    .enumerate()
                    .map(|(i, t)| (i + json_lists::TYPE_BASE, &[][..], Some(t))),
            );
        for (specialization, (code, json_classes, list_type)) in specs
            .filter(|(code, _, _)| plan.json_decode_codes.contains(code))
            .enumerate()
        {
            for aliased in [false, true] {
                let mode = usize::from(aliased);
                let schema_name = format!("JSON_SCHEMA_{code}_{mode}");
                let compound = list_type.is_some()
                    || json_classes.iter().any(|c| {
                        plan.layouts[c].fields.iter().any(|r| {
                            r.growing() || matches!(r, Repr::Float | Repr::Bool | Repr::Ptr(_))
                        })
                    });
                if compound {
                    json_lists::emit(
                        out,
                        plan,
                        descriptors,
                        lists,
                        list_type,
                        json_classes,
                        code,
                        aliased,
                    )?;
                } else {
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
                }
                let mir = all[&callee];
                let MirFunctionKind::Bytecode(body) = &mir.kind else {
                    return Err(no("JSON wrapper body"));
                };
                let mut line = None;
                let mut decode_pc = None;
                for block in &body.blocks {
                    if let Some(Terminator::Call {
                        callee: Operand::Constant(Constant::Function(f)),
                        ..
                    }) = &block.terminator
                        && let Some((target, _)) = plan.index.function(db, *f)
                        && plan.index.display(target) == "baml.json._decode"
                    {
                        let bytecode = plan.index.positions(callee);
                        decode_pc = Some(
                            bytecode
                                .lower_to_compact_with_offsets()
                                .offset_of(bytecode.block_pcs[block.id.0].terminator.unwrap())
                                .unwrap(),
                        );
                        line = plan.index.positions(callee).block_pcs[block.id.0]
                            .terminator
                            .map(|pc| plan.index.positions(callee).source_line_for_pc(pc));
                    }
                }
                let line = line.ok_or_else(|| no("JSON wrapper source line"))?;
                let decode_pc = decode_pc.unwrap();
                if specialization == 0 && !aliased {
                    writeln!(&mut out,"static O{callee}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);static S{callee}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{line}],pcs:&[{decode_pc}],kinds:&[rt::telemetry::RaiseKind::NativeBoundary]}};",plan.index.info(callee).source_file,plan.index.display(callee)).unwrap();
                }
                if !aliased {
                    let ty = if let Some(list_type) = list_type {
                        json_lists::capture_template(plan, list_type)?
                    } else {
                        let members = json_classes
                            .iter()
                            .map(|class| {
                                format!("rt::telemetry::capture::Type::Class(CAPTURE_D{class})")
                            })
                            .collect::<Vec<_>>();
                        if members.len() == 1 {
                            members[0].clone()
                        } else {
                            format!(
                                "rt::telemetry::capture::Type::Union(&[{}])",
                                members.join(",")
                            )
                        }
                    };
                    writeln!(&mut out,"unsafe fn f{callee}_json{code}(telemetry_site:usize,s:usize,naming:usize,omitted:bool)->R<rt::Dyn>{{unsafe{{rt::telemetry::observed_captured(&O{callee},telemetry_site,||{{let mut c=rt::telemetry::capture_arguments(&[rt::Dyn{{tag:3,payload:s as u64}},rt::Dyn{{tag:if omitted{{u64::MAX}}else{{CAPTURE_NAMING_TAG}},payload:naming as u64}}]);c.types=rt::telemetry::capture_type_args(&[(\"T\",&{ty})]);c}},|r|match r{{Ok(v)=>rt::telemetry::capture_value(*v),Err(e)=>rt::telemetry::capture_error(e)}},||{{rt::check_stack().map_err(|e|e.at(&S{callee},0))?;let aliased=rt::telemetry::observed_captured(&O{alias_function},JSON_ALIAS_SITE,||rt::telemetry::capture_arguments(&[rt::Dyn{{tag:CAPTURE_NAMING_TAG,payload:naming as u64}}]),|r|match r{{Ok(v)=>rt::telemetry::capture_value(rt::Dyn{{tag:1,payload:*v as u64}}),Err(e)=>rt::telemetry::capture_error(e)}},||Ok(naming!=0))?;(if !aliased{{json_specialized_{code}_0(&mut rt::strings::Cx::current(),s)}}else{{json_specialized_{code}_1(&mut rt::strings::Cx::current(),s)}}).map_err(|e|e.at(&S{callee},0))}})}}}}").unwrap();
                }
            }
        }
    }

    Ok(())
}
