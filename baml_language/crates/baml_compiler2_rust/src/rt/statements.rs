use super::*;

impl Emitter<'_, '_> {
    pub(super) fn leaf(&mut self, b: usize, headers: &BTreeSet<usize>) -> Gen<String> {
        self.block = b;
        let block = &self.body.blocks[b];
        let pcs = &self.plan.index.positions(self.object).block_pcs[b];
        self.mark(pcs.start);
        let mut out = if block.shielded {
            "{let _shield=rt::tasks::shield();".to_owned()
        } else {
            String::new()
        };
        if self.plan.error_contexts {
            let cause = block.handling.map_or_else(
                || "base_cx.handling()".into(),
                |h| {
                    format!(
                        "caught{}.as_ref().map_or(base_cx.handling(),|e|e.context())",
                        h.0
                    )
                },
            );
            writeln!(&mut out, "let cx=base_cx.with_handling({cause});").unwrap();
        }
        if headers.contains(&b) {
            writeln!(
                &mut out,
                "poll{b}=poll{b}.wrapping_add(1);if poll{b}&15==0{{{};}}",
                self.fallible("cx.poll()")
            )
            .unwrap();
        }
        for (i, s) in block.statements.iter().enumerate() {
            self.statement = i;
            self.mark(pcs.statements.get(i).copied().flatten());
            if let Some(result) = registry::statement(self, &s.kind) {
                out.push_str(&result?);
                continue;
            }
            match &s.kind {
                StatementKind::Assign { destination, value } => {
                    out.push_str(
                        &self
                            .assign(destination, self.value(value, self.place(destination)?.1)?)?,
                    );
                }
                StatementKind::Intrinsic {
                    op:
                        baml_compiler2_mir::IntrinsicOp::BuiltinTraceHook(
                            baml_compiler2_mir::BuiltinTraceHook::Hidden,
                        ),
                    args,
                } if args.is_empty() => {} // Hidden observation branch is in the prologue.
                StatementKind::Nop => {}
                _ => return Err(no(format!("statement {:?}", s.kind))),
            }
        }
        self.statement = block.statements.len();
        self.mark(pcs.terminator);
        let term = block.terminator.as_ref().unwrap();
        if let Some(result) = registry::terminator(self, term) {
            out.push_str(&result?);
            if block.shielded {
                out.push('}');
            }
            return Ok(out);
        }
        match term {
            Terminator::Rethrow {
                context: Operand::Copy(Place::Local(context)) | Operand::Move(Place::Local(context)),
                ..
            } => {
                let handler = self
                    .body
                    .blocks
                    .iter()
                    .find(|b| b.landing.is_some_and(|l| l.context_local == *context))
                    .ok_or_else(|| no("rethrow context"))?
                    .id
                    .0;
                out.push_str(&self.raise(&format!(
                    "caught{handler}.take().expect(\"missing caught error\").rethrow().rethrow_at(&S{},{})",self.object,self.site
                )));
            }

            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let (v, t) = self.operand(operand)?;
                if matches!(kind, baml_compiler2_mir::ShortCircuitKind::Coalesce) {
                    let short = format!("!({})", Self::null_test(&(v.clone(), t)));
                    let store = self.assign(destination, (v, t))?;
                    writeln!(&mut out, "c{b}={short};if c{b}{{{store}}}").unwrap();
                    if block.shielded {
                        out.push('}');
                    }
                    return Ok(out);
                }
                if t != Repr::Bool {
                    return Err(no("non-boolean short circuit"));
                }
                let short = match kind {
                    baml_compiler2_mir::ShortCircuitKind::And => format!("!({v})"),
                    baml_compiler2_mir::ShortCircuitKind::Or => v.clone(),
                    baml_compiler2_mir::ShortCircuitKind::Coalesce => return Err(no("coalesce")),
                };
                let store = self.assign(destination, (v, t))?;
                writeln!(&mut out, "c{b}={short};if c{b}{{{store}}}").unwrap();
            }
            Terminator::Branch { condition, .. } => {
                let (v, t) = self.operand(condition)?;
                if t != Repr::Bool {
                    return Err(no("branch condition"));
                }
                writeln!(&mut out, "c{b}={v};").unwrap();
            }
            Terminator::Return | Terminator::Goto { .. } => {}
            _ => return Err(no("terminator")),
        }
        if block.shielded {
            out.push('}');
        }
        Ok(out)
    }
}
