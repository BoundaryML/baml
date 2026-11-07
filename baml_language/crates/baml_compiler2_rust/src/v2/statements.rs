use super::*;

impl Emitter<'_, '_> {
    pub(super) fn leaf(&mut self, b: usize, headers: &BTreeSet<usize>) -> Gen<String> {
        self.block = b;
        let block = &self.body.blocks[b];
        let pcs = &self.plan.index.positions(self.object).block_pcs[b];
        self.mark(pcs.start);
        let mut out = String::new();
        if headers.contains(&b) {
            writeln!(
                &mut out,
                "poll{b}=poll{b}.wrapping_add(1);if poll{b}&15==0{{{};}}",
                self.fallible("rt::poll()")
            )
            .unwrap();
        }
        for (i, s) in block.statements.iter().enumerate() {
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
        self.mark(pcs.terminator);
        let term = block.terminator.as_ref().unwrap();
        if let Some(result) = registry::terminator(self, term) {
            out.push_str(&result?);
            return Ok(out);
        }
        match term {
            Terminator::Throw { value } => {
                let (value, Repr::Ptr(c)) = self.operand(value)? else {
                    return Err(no("non-class throw"));
                };
                let Object::Class(class) =
                    &self.plan.program.objects[bex_vm_types::ObjectIndex::from_raw(c)]
                else {
                    unreachable!()
                };
                if self.plan.layouts[&c].fields.iter().any(|r| *r != Repr::Int) {
                    return Err(no("throw class with non-int fields"));
                }
                let package = self
                    .plan
                    .program
                    .packages
                    .iter()
                    .find(|p| p.objects.contains(&u32::try_from(c).expect("object index")))
                    .ok_or_else(|| no("throw class package"))?;
                let name = format!("{}.{}", package.name, class.path);
                let fields = class
                    .fields
                    .iter()
                    .map(|f| format!("{}: {{}}", f.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let display = format!("{name} {{{{{fields}}}}}");
                let values = (0..class.fields.len())
                    .map(|i| format!("(*(p as *const C{c})).f{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let error = format!(
                    "Box::new(rt::Error{{telemetry_at:None,telemetry_raise:None,value:p,class:{name:?},message:String::new(),frames:Vec::new(),display_value:Some(format!({display:?},{values}))}}).at(&S{},{})",
                    self.object, self.site
                );
                writeln!(&mut out, "{{let p={value};{}}}", self.raise(&error)).unwrap();
            }
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
                    "caught{handler}.take().expect(\"missing caught error\").rethrow()"
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
        Ok(out)
    }
}
