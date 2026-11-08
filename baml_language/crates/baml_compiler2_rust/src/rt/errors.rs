use super::Emitter;

impl Emitter<'_, '_> {
    pub(super) fn mark(&mut self, pc: Option<usize>) {
        use bex_vm_types::bytecode::OpCode;
        if let Some(pc) = pc {
            let bytecode = self.plan.index.positions(self.object);
            let compact = bytecode.lower_to_compact_with_offsets();
            let offset = compact.offset_of(pc).expect("source instruction");
            self.compact_sites.push(u32::try_from(offset).unwrap());
            let kind = match OpCode::try_from(bytecode.lower_to_compact().code[offset]).unwrap() {
                OpCode::Throw => "Throw",
                OpCode::Await | OpCode::AwaitAny => "Await",
                OpCode::SysOp => "HostBoundary",
                OpCode::Rethrow => "Rethrow",
                OpCode::Call
                | OpCode::CallHooked
                | OpCode::CallLocal
                | OpCode::CallExactArgs
                | OpCode::CallIndirect
                | OpCode::VirtualCall => "NativeBoundary",
                _ => "Runtime",
            };
            self.raise_kinds.push(kind);
            self.site = self.sites.len();
            self.sites.push(
                self.plan
                    .index
                    .positions(self.object)
                    .source_line_for_pc(pc),
            );
        }
    }
    pub(super) fn raise(&self, error: &str) -> String {
        if let Some(handler) = self.body.blocks[self.block].unwind {
            let landing = self.body.blocks[handler.0]
                .landing
                .expect("verified handler landing");
            let bytecode = self.plan.index.positions(self.object);
            let pc = bytecode
                .lower_to_compact_with_offsets()
                .offset_of(bytecode.block_pcs[handler.0].start.expect("handler pc"))
                .expect("compact handler");
            format!(
                "{{let mut error={error};rt::telemetry::evidence::caught(&mut error,{pc}u32);if error.value==0{{return Err(error);}}let rooted=rt::CaughtError::new(error);let p=rooted.value();l{}=rt::Dyn{{tag:(*(p as *const baml_rt_gc::Header)).desc as u64+3,payload:p as u64}};caught{}=Some(rooted);{}}}",
                landing.error_local.0, handler.0, self.unwind_jumps[self.block]
            )
        } else {
            format!("return Err({error});")
        }
    }
    pub(super) fn fallible(&self, code: &str) -> String {
        if matches!(
            self.all[&self.object].kind,
            super::MirFunctionKind::Builtin(_)
        ) {
            return format!("({code})?");
        }
        let error = format!("e.at(&S{},{})", self.object, self.site);
        format!(
            "match ({code}){{Ok(v)=>v,Err(e)=>{{{}}}}}",
            self.raise(&error)
        )
    }
}

/// Debug display for thrown task error values uses the same concrete layouts
/// as their traced/captured payload. No user formatting callback is invoked.
pub(super) fn thrown_display(
    e: &super::Emitter<'_, '_>,
    repr: super::Repr,
    value: &str,
    seen: &mut super::BTreeSet<usize>,
) -> super::Gen<String> {
    use super::Repr;
    Ok(match repr {
        Repr::Int | Repr::Bool => format!("format!(\"{{}}\",{value})"),
        Repr::Str => format!(
            "format!(\"{{:?}}\",rt::strings::Str::from_raw({value}).as_str(&rt::strings::Cx::current()))"
        ),
        Repr::Bigint => format!("format!(\"{{}}n\",rt::tasks::bigint_value({value}))"),
        Repr::Null => "String::from(\"null\")".into(),
        Repr::Ptr(class) => {
            if !seen.insert(class) {
                return Err(super::no("thrown recursive class display"));
            }
            let name = thrown_class(e, class)?;
            let info = e.plan.class(class);
            // Retain the compact formatter for the original scalar error ABI.
            if e.plan.layouts[&class]
                .fields
                .iter()
                .all(|r| *r == Repr::Int)
            {
                let fields = info
                    .fields
                    .iter()
                    .map(|f| format!("{}: {{}}", f.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                let display = format!("{name} {{{{{fields}}}}}");
                let values = (0..info.fields.len())
                    .map(|i| format!("(*({value} as *const C{class})).f{i}"))
                    .collect::<Vec<_>>()
                    .join(",");
                seen.remove(&class);
                return Ok(format!("format!({display:?},{values})"));
            }
            let mut fields = Vec::new();
            let mut values = Vec::new();
            for (i, field) in info.fields.iter().enumerate() {
                fields.push(format!("{}: {{}}", field.name));
                let repr = e.plan.layouts[&class].fields[i];
                let raw = format!("(*(({value}) as *const C{class})).f{i}");
                let raw = if repr == Repr::Bool {
                    format!("({raw}!=0)")
                } else {
                    raw
                };
                values.push(thrown_display(e, repr, &raw, seen)?);
            }
            seen.remove(&class);
            let display = format!("{name} {{{{{}}}}}", fields.join(", "));
            if values.is_empty() {
                format!("String::from({:?})", format!("{name} {{}}"))
            } else {
                format!(
                    "{{let object={value};if object==0{{String::from(\"null\")}}else{{format!({display:?},{})}}}}",
                    values.join(",")
                )
            }
        }
        _ => return Err(super::no("thrown value display")),
    })
}
pub(super) fn thrown_class(e: &super::Emitter<'_, '_>, class: usize) -> super::Gen<String> {
    let origin = e.plan.class_origin(class);
    let package = e
        .plan
        .program
        .packages
        .iter()
        .find(|p| p.objects.contains(&(origin as u32)))
        .ok_or_else(|| super::no("throw class package"))?;
    Ok(format!("{}.{}", package.name, e.plan.class(class).path))
}
