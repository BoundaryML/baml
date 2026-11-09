use super::{
    Constant, Db, Emitter, Gen, MirFunctionKind, Operand, Plan, Repr, Terminator, no, registry,
};

pub(super) struct Feature;
impl registry::Feature for Feature {
    fn reach(
        &self,
        db: &dyn Db,
        plan: &mut Plan<'_>,
        term: Option<&Terminator<'_>>,
    ) -> Option<Gen<registry::Reach>> {
        (matches!(
            term,
            Some(Terminator::Call {
                callee: Operand::Constant(Constant::Function(_)),
                has_trace: false,
                ..
            })
        ))
        .then(|| admit_call(db, plan, term))
    }

    fn terminator(
        &self,
        emitter: &mut Emitter<'_, '_>,
        term: &Terminator<'_>,
    ) -> Option<Gen<String>> {
        (matches!(
            term,
            Terminator::Call {
                callee: Operand::Constant(Constant::Function(_)),
                ..
            }
        ))
        .then(|| emitter.calls_terminator(term))
    }
}
impl Emitter<'_, '_> {
    fn calls_terminator(&self, term: &Terminator<'_>) -> Gen<String> {
        let mut out = String::new();
        match term {
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
                if self.plan.index.builtin_path(c) == "baml.json.decode" {
                    let ordered;
                    let args = if let Some(layout) = argument_layout {
                        let mapping = layout.map_to(&info.argument_layout()).map_err(no)?;
                        let mut arguments = args[..*ntypeargs].to_vec();
                        arguments.extend(mapping.into_iter().map(|i| {
                            i.map(|i| args[*ntypeargs + i].clone())
                                .unwrap_or(Operand::Constant(Constant::OmittedArg))
                        }));
                        ordered = arguments;
                        &ordered
                    } else {
                        args
                    };
                    let Repr::Type(code) = self.operand(&args[0])?.1 else {
                        return Err(no("JSON type argument"));
                    };
                    if *ntypeargs != 1 || args.len() != 3 || code == usize::MAX {
                        return Err(no("JSON decode specialization/naming"));
                    }
                    let code = self.plan.decode_code(code)?;
                    let omitted = matches!(args[2], Operand::Constant(Constant::OmittedArg));
                    let naming = if omitted {
                        "0usize".to_owned()
                    } else {
                        self.coerce(self.operand(&args[2])?, Repr::Naming)?
                    };
                    let text = self.coerce(self.operand(&args[1])?, Repr::Str)?;
                    let telemetry_site = self.telemetry_site(c);
                    let (destination, _) = crate::analysis::continuation(term).unwrap();
                    let decoded = self.fallible(&format!(
                        "f{c}_json{code}({telemetry_site},{text},{naming},{omitted})"
                    ));
                    let mut value = (decoded, Repr::Tagged);
                    if let Some(ty) = code
                        .checked_sub(super::json_lists::TYPE_BASE)
                        .and_then(|i| self.plan.json_list_types.get(i))
                    {
                        let repr = self.plan.template_repr(ty)?;
                        // Decode validates the explicit type argument. MIR may
                        // widen literal unions in the destination; first unpack
                        // the actual schema layout, then apply that static coercion.
                        if repr.growing() {
                            value = (self.coerce(value, repr)?, repr);
                        }
                    }
                    out.push_str(&self.assign(destination, value)?);
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
                            Ok(i.map(|i| values[i].clone()).unwrap_or_else(|| {
                                ("rt::Dyn{tag:u64::MAX,payload:0}".into(), Repr::Tagged)
                            }))
                        })
                        .collect::<Gen<_>>()?;
                }
                let name = self.plan.index.builtin_path(c);
                let expression = if let Some(result) = registry::call(self, name, &types, &values) {
                    result?
                } else if name == "baml._to_string_default"
                    && types == [Repr::Type(0)]
                    && values.len() == 1
                    && values[0].1 == Repr::Int
                {
                    (self.fallible(&format!("rt_int_to_string(&mut rt::strings::Cx::current(),{}).map(|s|s.as_raw())",values[0].0)),Repr::Str)
                } else if name == "baml.String.to_code_points"
                    && types.is_empty()
                    && values.len() == 1
                    && values[0].1 == Repr::Str
                {
                    (self.fallible(&format!("rt_code_points(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw({}),{})",values[0].0,self.lists[&Repr::ListInt])),Repr::ListInt)
                } else if !types.is_empty() {
                    return Err(no("generic call"));
                } else if name == "baml.String.char_count" {
                    if values.len() != 1 || values[0].1 != Repr::Str {
                        return Err(no("String.char_count arguments"));
                    }
                    (
                        format!(
                            "rt_string_char_count(&rt::strings::Cx::current(),rt::strings::Str::from_raw({}))",
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
                    (self.fallible(&format!("rt_string_slice(&mut rt::strings::Cx::current(),rt::strings::Str::from_raw({}),{},{}).map(|s|s.as_raw())", values[0].0,values[1].0,values[2].0)),Repr::Str)
                } else if name == "baml.ops.equals_equals" {
                    if values.len() != 2 {
                        return Err(no("equality arity"));
                    }
                    if values.iter().all(|v| v.1 == Repr::Null) {
                        ("true".into(), Repr::Bool)
                    } else if let Some(other) = values.iter().find(|v| v.1 != Repr::Null)
                        && values.iter().any(|v| v.1 == Repr::Null)
                    {
                        (Self::null_test(other), Repr::Bool)
                    } else if values.iter().any(|v| v.1 == Repr::Str) {
                        let a = self.coerce(values[0].clone(), Repr::Tagged)?;
                        let b = self.coerce(values[1].clone(), Repr::Tagged)?;
                        let compare=self.fallible("rt::strings::equals_raw(&rt::strings::Cx::current(),a.payload as usize,b.payload as usize)");
                        (
                            format!(
                                "{{let a={a};let b={b};a.tag==b.tag && (a.tag==2 || (a.tag==3 && {compare}))}}"
                            ),
                            Repr::Bool,
                        )
                    } else {
                        return Err(no("equality other than primitive string or null test"));
                    }
                } else {
                    let mir = &self.all[&c];
                    let MirFunctionKind::Bytecode(body) = &mir.kind else {
                        return Err(no("builtin"));
                    };
                    if values.len() != mir.arity {
                        return Err(no("call arity"));
                    }
                    for (i, value) in values.iter_mut().enumerate() {
                        let expected = if let Some(repr) =
                            super::string_defaults::parameter(self.plan, c, i + 1)
                        {
                            repr
                        } else {
                            self.plan.local_repr(body, i + 1, 0)?
                        };
                        *value = (self.coerce(value.clone(), expected)?, expected);
                    }
                    let arguments = values
                        .iter()
                        .map(|(v, _)| v.as_str())
                        .collect::<Vec<_>>()
                        .join(",");
                    let observed = format!(
                        "f{c}::<OBSERVE>(cx,{}, {arguments})",
                        self.telemetry_site(c)
                    );
                    (
                        self.fallible(&self.stack_leaf_call(c, &arguments, &observed)),
                        self.plan.repr(&body.locals[0].ty)?,
                    )
                };
                let Terminator::Call { destination, .. } = term else {
                    unreachable!()
                };
                out.push_str(&self.assign(destination, expression)?);
            }
            _ => unreachable!("feature terminator guard"),
        }
        Ok(out)
    }
}

fn admit_call(db: &dyn Db, plan: &Plan<'_>, term: Option<&Terminator<'_>>) -> Gen<registry::Reach> {
    let mut reach = registry::Reach::default();
    match term {
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
            let name = plan.index.builtin_path(callee);
            if name == "baml.json.decode" {
                reach.json_decode = Some(callee);
            } else if baml_builtins2_codegen::supports_rt(name) {
                reach.builtin = Some(name.to_owned());
            } else if *ntypeargs != 0 {
                return Err(no("generic user call"));
            } else if name != "baml.ops.equals_equals" {
                reach.targets.push(callee);
            }
        }
        _ => unreachable!("reachability guard"),
    }
    Ok(reach)
}
