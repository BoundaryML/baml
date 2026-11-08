use super::Emitter;

impl Emitter<'_, '_> {
    pub(super) fn error_at(&self, error: &str) -> String {
        if self.plan.error_contexts {
            format!("{error}.at_cx(&S{},{},cx)", self.object, self.site)
        } else {
            format!("{error}.at(&S{},{})", self.object, self.site)
        }
    }
    pub(super) fn mark(&mut self, pc: Option<usize>) {
        use bex_vm_types::bytecode::OpCode;
        if let Some(pc) = pc {
            let bytecode = self.plan.index.positions(self.object);
            let compact = bytecode.lower_to_compact_with_offsets();
            let offset = compact.offset_of(pc).expect("source instruction");
            self.compact_sites.push(u32::try_from(offset).unwrap());
            let kind = match OpCode::try_from(bytecode.lower_to_compact().code[offset]).unwrap() {
                OpCode::Throw => "Throw",
                OpCode::Rethrow => "Rethrow",
                OpCode::Call
                | OpCode::CallHooked
                | OpCode::CallLocal
                | OpCode::CallExactArgs
                | OpCode::CallIndirect
                | OpCode::VirtualCall
                | OpCode::SysOp => "NativeBoundary",
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
            let context = if self.plan.error_contexts {
                let c = super::strings::builtin_class(self.plan, "baml.errors.Context")
                    .expect("planned Context");
                let value = self
                    .coerce(
                        ("context".into(), super::Repr::Ptr(c)),
                        self.local(landing.context_local.0).expect("context local"),
                    )
                    .expect("context storage");
                format!(
                    "let context=rooted.materialize({})?;l{}={value};",
                    super::exceptions::schema(self).expect("context schema"),
                    landing.context_local.0
                )
            } else {
                String::new()
            };
            format!(
                "{{let mut error={error};rt::telemetry::evidence::caught(&mut error,{pc}u32);if error.value==0{{return Err(error);}}let mut rooted=rt::CaughtError::new(error);{context}l{}=rooted.dynamic_value();caught{}=Some(rooted);{}}}",
                landing.error_local.0, handler.0, self.unwind_jumps[self.block]
            )
        } else {
            format!("return Err({error});")
        }
    }
    pub(super) fn fallible(&self, code: &str) -> String {
        let error = self.error_at("e");
        format!(
            "match ({code}){{Ok(v)=>v,Err(e)=>{{{}}}}}",
            self.raise(&error)
        )
    }
}
