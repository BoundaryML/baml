use super::*;

impl Emitter<'_, '_> {
    pub(super) fn function(
        &mut self,
        arity: usize,
        inline: bool,
        stack_leaf: bool,
        recursive: bool,
    ) -> Gen<String> {
        let cfg = Cfg {
            entry: self.body.entry.0,
            unwinds: self
                .body
                .blocks
                .iter()
                .map(|b| b.unwind.map(|h| h.0))
                .collect(),
            flows: self
                .body
                .blocks
                .iter()
                .map(|b| match b.terminator.as_ref().unwrap() {
                    Terminator::Goto { target }
                    | Terminator::Call { target, .. }
                    | Terminator::VirtualCall { target, .. }
                    | Terminator::Await { target, .. }
                    | Terminator::AwaitAny { target, .. }
                    | Terminator::SysOp { target, .. } => Ok(Flow::Goto(target.0)),
                    Terminator::Switch {
                        arms, otherwise, ..
                    } => Ok(Flow::Switch {
                        arms: arms.iter().map(|(_, b)| b.0).collect(),
                        otherwise: otherwise.0,
                    }),
                    Terminator::NarrowBind {
                        then_block,
                        else_block,
                        ..
                    } => Ok(Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
                    }),
                    Terminator::Unreachable => Ok(Flow::Exit),
                    Terminator::Spawn { resume, .. } => Ok(Flow::Goto(resume.0)),
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => Ok(Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
                    }),
                    Terminator::ShortCircuit { eval_rhs, join, .. } => Ok(Flow::Branch {
                        then_block: join.0,
                        else_block: eval_rhs.0,
                    }),
                    Terminator::Return | Terminator::Throw { .. } | Terminator::Rethrow { .. } => {
                        Ok(Flow::Exit)
                    }
                    _ => Err(no("control flow")),
                })
                .collect::<Gen<_>>()?,
        };
        let structured = structurize(&cfg).map_err(no)?;
        self.proven_arithmetic = loop_ranges::analyze(self, &structured.stmts);
        self.unwind_jumps = structured
            .unwind_jumps
            .iter()
            .map(|jumps| {
                jumps
                    .iter()
                    .map(|jump| match jump {
                        Stmt::Break(l) => Ok(format!("break 'b{l};")),
                        Stmt::BreakLoop(l) => Ok(format!("break 'l{l};")),
                        Stmt::Continue(l) => Ok(format!("continue 'l{l};")),
                        _ => Err(no("unwind jump shape")),
                    })
                    .collect::<Gen<Vec<_>>>()
                    .map(|s| s.join(""))
            })
            .collect::<Gen<_>>()?;

        let positions = self.plan.index.positions(self.object);
        if positions.block_pcs.len() != self.body.blocks.len() {
            return Err(no("missing/mismatched MIR source positions"));
        }
        let environment = self.plan.captures.contains_key(&self.object);
        let params = (1..=arity)
            .map(|i| Ok(format!("mut l{i}:{}", self.argument_repr(i)?.rust())))
            .collect::<Gen<Vec<_>>>()?
            .join(",");
        let params = if environment {
            format!("env:usize,{params}")
        } else {
            params
        };
        let attribute = if inline { "#[inline(always)]\n" } else { "" };
        let object = self.object;
        // These bounded leaf bodies are forced inline at every call site, so
        // they add no frame. Their caller retains the ordinary stack guard.
        let guard = if stack_leaf {
            assert!(inline);
            String::new()
        } else {
            format!("cx.check_stack().map_err(|e|e.at(&S{object},0))?;")
        };
        let returned = self.local(0)?.rust();
        let arguments = (1..=arity)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let arguments = if environment {
            format!("env,{arguments}")
        } else {
            arguments
        };
        let captured_args = (1..=arity)
            .map(|i| self.capture_value(format!("l{i}"), self.argument_repr(i)?))
            .collect::<Gen<Vec<_>>>()?
            .join(",");
        let captured_result = self.capture_value("*value".to_owned(), self.local(0)?)?;
        let capture_arguments = if let Some(frame) = self.plan.function_frames.get(&self.object) {
            let types = frame
                .iter()
                .map(|(n, t)| {
                    Ok(format!(
                        "({n:?},&{})",
                        capture_codegen::capture_type(self.plan, t)?
                    ))
                })
                .collect::<Gen<Vec<_>>>()?
                .join(",");
            format!(
                "{{let mut c=rt::telemetry::capture_arguments(&[{captured_args}]);c.types=rt::telemetry::capture_type_args(&[{types}]);c}}"
            )
        } else {
            format!("rt::telemetry::capture_arguments(&[{captured_args}])")
        };
        let captures = format!(
            "||{capture_arguments},|result:&R<{returned}>|match result{{Ok(value)=>rt::telemetry::capture_value({captured_result}),Err(error)=>rt::telemetry::capture_error(error)}}"
        );
        let mut code = call_cost::prologue(
            object, &params, &arguments, returned, &captures, attribute, &guard,
        );
        if recursive {
            code.push_str(&format!("{};\n", self.fallible("cx.poll()")));
        }
        for i in 1..=arity {
            if self.body.locals[i].is_captured {
                let value = self.coerce((format!("l{i}"), self.argument_repr(i)?), Repr::Tagged)?;
                writeln!(
                    &mut code,
                    "let mut l{i}=rt::closures::cell({},{value}).map_err(|e|e.at(&S{object},0))?;",
                    self.plan.closure_descriptors().unwrap().0
                )
                .unwrap();
            }
        }
        for i in 0..self.body.locals.len() {
            if i > 0 && i <= arity {
                continue;
            }
            let t = self.local(i)?;
            writeln!(&mut code, "let mut l{i}:{}={};", t.rust(), t.zero()).unwrap();
        }
        for block in &self.body.blocks {
            if let Some(term) = &block.terminator
                && let Some(local) = super::tasks::ephemeral_plan(self, term)
            {
                writeln!(
                    &mut code,
                    "let mut fast_plan_body{local}=0usize;let mut fast_plan_name{local}=0usize;"
                )
                .unwrap();
            }
        }
        for iter in &self.iters {
            writeln!(&mut code, "let mut iter_index{iter}=0i64;").unwrap();
        }
        // Private task environments stay addressable until spawn copies their
        // metadata and capture cells into the managed future's trailing slots.
        for (block, b) in self.body.blocks.iter().enumerate() {
            for (statement, s) in b.statements.iter().enumerate() {
                if let StatementKind::Assign {
                    value: Rvalue::MakeClosure { captures, .. },
                    ..
                } = &s.kind
                    && captures.len() <= 6
                    && super::tasks::private_environment_at(self, block, statement)
                {
                    writeln!(&mut code, "let mut private_env_{block}_{statement}=rt::closures::PrivateEnvironment::<{}>::empty();", captures.len()).unwrap();
                }
            }
        }
        code.push_str(&self.stack.declarations(self));
        for b in &self.body.blocks {
            if b.landing.is_some() {
                writeln!(
                    &mut code,
                    "let mut caught{}:Option<rt::CaughtError>=None;",
                    b.id.0
                )
                .unwrap();
            }

            if matches!(
                b.terminator,
                Some(
                    Terminator::Branch { .. }
                        | Terminator::ShortCircuit { .. }
                        | Terminator::NarrowBind { .. }
                )
            ) {
                writeln!(&mut code, "let mut c{}=false;", b.id.0).unwrap();
            }
            if matches!(b.terminator, Some(Terminator::Switch { .. })) {
                writeln!(&mut code, "let mut switch{}=0i64;", b.id.0).unwrap();
            }
        }
        for h in cfg.loop_headers() {
            writeln!(&mut code, "let mut poll{h}=0u32;").unwrap();
        }
        let body = self.stmts(&structured.stmts, &cfg.loop_headers())?;
        code.push_str(&self.string_literal_locals());
        code.push_str(&body);
        code.push_str("}}\n");
        let lines = self
            .sites
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let pcs = format!("{:?}", self.compact_sites);
        let kinds = self
            .raise_kinds
            .iter()
            .map(|k| format!("rt::telemetry::RaiseKind::{k}"))
            .collect::<Vec<_>>()
            .join(",");
        code.push_str(&self.closure_adapter()?);
        Ok(format!(
            "static O{}:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(0);\nstatic S{}:rt::Source=rt::Source{{file:{:?},function:{:?},lines:&[{lines}],pcs:&{pcs},kinds:&[{kinds}]}};\n{code}",
            self.object,
            self.object,
            self.plan.index.info(self.object).source_file,
            if matches!(
                self.plan.index.info(self.object).identity,
                bex_vm_types::types::FunctionIdentity::Synthetic { .. }
            ) {
                self.plan
                    .program
                    .function_name(bex_vm_types::ObjectIndex::from_raw(
                        self.plan.index.rt_origin(self.object),
                    ))
                    .unwrap()
            } else {
                self.plan.index.display(self.object).to_owned()
            }
        ))
    }
    pub(super) fn stmts(&mut self, stmts: &[Stmt], headers: &BTreeSet<usize>) -> Gen<String> {
        let mut out = String::new();
        for s in stmts {
            match s {
                Stmt::Leaf(b) => out.push_str(&self.leaf(*b, headers)?),
                Stmt::Block { label, body } => {
                    writeln!(&mut out, "'b{label}:{{ {} }}", self.stmts(body, headers)?).unwrap();
                }
                Stmt::Loop { label, body } => writeln!(
                    &mut out,
                    "'l{label}:loop{{ {} }}",
                    self.stmts(body, headers)?
                )
                .unwrap(),
                Stmt::If {
                    block,
                    then_branch,
                    else_branch,
                } => writeln!(
                    &mut out,
                    "if c{block}{{ {} }}else{{ {} }}",
                    self.stmts(then_branch, headers)?,
                    self.stmts(else_branch, headers)?
                )
                .unwrap(),
                Stmt::Break(l) => writeln!(&mut out, "break 'b{l};").unwrap(),
                Stmt::BreakLoop(l) => writeln!(&mut out, "break 'l{l};").unwrap(),
                Stmt::Continue(l) => writeln!(&mut out, "continue 'l{l};").unwrap(),
                Stmt::Exit(_) => out.push_str("return Ok(l0);\n"),
                Stmt::Switch {
                    block,
                    arms,
                    otherwise,
                } => {
                    let Terminator::Switch { arms: keys, .. } =
                        self.body.blocks[*block].terminator.as_ref().unwrap()
                    else {
                        unreachable!()
                    };
                    writeln!(&mut out, "match switch{block}{{").unwrap();
                    for (indices, body) in arms {
                        let keys = indices
                            .iter()
                            .map(|i| self.switch_key(keys[*i].0))
                            .collect::<Gen<Vec<_>>>()?
                            .join("|");
                        writeln!(&mut out, "{keys}=>{{{}}},", self.stmts(body, headers)?).unwrap();
                    }
                    writeln!(&mut out, "_=>{{{}}}}}", self.stmts(otherwise, headers)?).unwrap();
                }
            }
        }
        Ok(out)
    }
}

pub(super) fn recursive(calls: &BTreeMap<usize, BTreeSet<usize>>, function: usize) -> bool {
    let mut pending: Vec<_> = calls
        .get(&function)
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(callee) = pending.pop() {
        if callee == function {
            return true;
        }
        if seen.insert(callee) {
            pending.extend(calls.get(&callee).into_iter().flatten().copied());
        }
    }
    false
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    #[test]
    fn recursion_including_indirect_cycles_keeps_real_frames() {
        let calls = BTreeMap::from([
            (0, BTreeSet::from([1])),
            (1, BTreeSet::from([2])),
            (2, BTreeSet::from([1])),
            (3, BTreeSet::from([3])),
        ]);
        assert!(!recursive(&calls, 0));
        assert!(recursive(&calls, 1));
        assert!(recursive(&calls, 2));
        assert!(recursive(&calls, 3));
        assert!(!recursive(&calls, 4));
    }
}
