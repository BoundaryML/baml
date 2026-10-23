//! Emit structured control flow, block statements, and error handling.

use baml_compiler2_mir::{
    BlockId, IntrinsicOp, Local, Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, class_display_name,
};

use super::{Expr, Gen, Generator, YIELD_EVERY, pc_arg, unsupported};
use crate::{
    admit::Rejection,
    analysis::{Repr, array_iter_walk, array_next_call},
    render::coerce,
    structure::Stmt,
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn stmts(&mut self, stmts: &[Stmt]) -> Gen<()> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Gen<()> {
        match stmt {
            Stmt::Leaf(block) => self.leaf(BlockId(*block)),
            Stmt::Block { label, body } => {
                self.emit_line(&format!("'b{label}: {{"));
                self.stmts(body)?;
                self.emit_line("}");
                Ok(())
            }
            Stmt::Loop { label, body } => {
                self.emit_line(&format!("'l{label}: loop {{"));
                self.stmts(body)?;
                self.emit_line("}");
                Ok(())
            }
            Stmt::If {
                block,
                then_branch,
                else_branch,
            } => {
                self.emit_line(&format!("if c{block} {{"));
                self.stmts(then_branch)?;
                self.emit_line("} else {");
                self.stmts(else_branch)?;
                self.emit_line("}");
                Ok(())
            }
            Stmt::Switch {
                block,
                arms,
                otherwise,
            } => {
                let Some(Terminator::Switch { arms: keys, .. }) =
                    &self.body.blocks[*block].terminator
                else {
                    return Err(Rejection::Invalid(
                        "switch stmt on a non-switch block".into(),
                    ));
                };
                self.emit_line(&format!("match d{block} {{"));
                for (indices, body) in arms {
                    let mut pattern = Vec::with_capacity(indices.len());
                    for &i in indices {
                        pattern.push(self.switch_pattern(&keys[i].0)?);
                    }
                    self.emit_line(&format!("{} => {{", pattern.join(" | ")));
                    self.stmts(body)?;
                    self.emit_line("}");
                }
                self.emit_line("_ => {");
                self.stmts(otherwise)?;
                self.emit_line("}");
                self.emit_line("}");
                Ok(())
            }
            Stmt::Break(label) => {
                self.emit_line(&format!("break 'b{label};"));
                Ok(())
            }
            Stmt::BreakLoop(label) => {
                self.emit_line(&format!("break 'l{label};"));
                Ok(())
            }
            Stmt::Continue(label) => {
                self.emit_line(&format!("continue 'l{label};"));
                Ok(())
            }
            Stmt::Exit(block) => self.exit(BlockId(*block)),
        }
    }

    /// Class keys use dispatch tags; enum keys use variant indices.
    pub(super) fn switch_pattern(&self, key: &SwitchKey<'_>) -> Gen<String> {
        Ok(match key {
            SwitchKey::Int(key) => format!("{key}i64"),
            SwitchKey::Class(class) => {
                let Some((object, _)) = self.index.class(self.db, *class) else {
                    let name = class_display_name(self.db, *class);
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                format!("{}i64", self.index.class_tag(object))
            }
        })
    }

    pub(super) fn leaf(&mut self, id: BlockId) -> Gen<()> {
        // A leaf can have several predecessors, so its entry PC is unknown.
        self.current_mark = None;
        let pcs = self.positions.block(id.0).cloned().unwrap_or_default();
        self.loop_check(id, pcs.start)?;
        self.update_cause(id);
        for (index, statement) in self.body.blocks[id.0].statements.iter().enumerate() {
            let pc = pcs.statements.get(index).copied().flatten();
            self.set_pc(pc);
            self.block_statement(id, &statement.kind, pc)?;
        }
        self.set_pc(pcs.terminator);
        self.leaf_terminator(id)
    }

    /// Poll at a loop back edge so the engine can collect, park or cancel.
    fn loop_check(&mut self, id: BlockId, pc: Option<usize>) -> Gen<()> {
        if !self.pauses || !self.analysis.loop_headers.contains(&id.0) {
            return Ok(());
        }
        let live = self.header_locals(id.0);
        let at = pc_arg(pc);
        let request = format!("{{ fr.note_pc({at}); vm.suspend(Step::Yield) }}");
        let call = self.rooted(&live, &request);
        let code = self.fallible(id, &call)?;
        self.emit_line("iterations = iterations.wrapping_add(1);");
        self.emit_line(&format!(
            "if iterations % {YIELD_EVERY} == 0 && vm.should_yield() {{ {code}; }}"
        ));
        Ok(())
    }

    // Errors raised in this block inherit the active handler context.
    fn update_cause(&mut self, id: BlockId) {
        let block = &self.body.blocks[id.0];
        if let Some(slot) = self.cause_slot
            && self.cause_changes_at(id.0)
        {
            let cause = match block.handling.and_then(|h| self.body.blocks[h.0].landing) {
                Some(landing) => {
                    let context = Expr::atom(
                        format!("l{}", landing.context_local.0),
                        self.repr(landing.context_local),
                    );
                    self.value_arg(&context)
                }
                None => "Value::NULL".to_string(),
            };
            self.emit_statement(&format!("vm.set_slot(fr, {slot}, {cause});"));
        }
    }

    fn block_statement(
        &mut self,
        id: BlockId,
        statement: &StatementKind<'db>,
        pc: Option<usize>,
    ) -> Gen<()> {
        match statement {
            StatementKind::Assign { destination, value } => {
                // Type locals have no runtime binding; reads construct their values.
                if let (Place::Local(local), Rvalue::LoadType(_)) = (destination, value)
                    && self.analysis.is_type_local(local.0)
                {
                    return Ok(());
                }
                let value = self.rvalue(id, value)?;
                let code = self.store(id, destination, &value)?;
                self.emit_statement(&code);
            }
            StatementKind::Drop(_) | StatementKind::Nop => {}
            StatementKind::FreshCell { local, carry_value } => {
                let init = if *carry_value {
                    self.hoist(Expr::new(
                        format!("vm.cell_load(l{})?", local.0),
                        Repr::Value,
                    ))
                    .code
                } else {
                    "Value::NULL".to_string()
                };
                let cell = Expr::new(format!("vm.new_cell({init})"), Repr::Value);
                let code = self.store(id, &Place::Local(*local), &cell)?;
                self.emit_statement(&code);
            }
            StatementKind::Intrinsic {
                op: IntrinsicOp::BindType(slot),
                args,
            } => {
                let [value] = args.as_slice() else {
                    return Err(Rejection::Invalid("`BindType` takes one operand".into()));
                };
                let value = self.value_operand(id, value)?;
                let code = self.fallible(id, &format!("vm.bind_type(fr, {slot}, {value})"))?;
                self.emit_statement(&format!("{code};"));
            }
            StatementKind::Intrinsic {
                op: IntrinsicOp::Log(level),
                args,
            } => {
                let level = match level {
                    baml_compiler2_mir::LogLevel::Info => "Info",
                    baml_compiler2_mir::LogLevel::Debug => "Debug",
                    baml_compiler2_mir::LogLevel::Warn => "Warn",
                    baml_compiler2_mir::LogLevel::Error => "Error",
                };
                let Some(data) = args.first() else {
                    return Err(Rejection::Invalid("`log` without its data".into()));
                };
                let data = self.value_operand(id, data)?;
                let name = match args.get(1) {
                    Some(name) => {
                        let name = self.operand(id, name)?;
                        self.value_arg(&name)
                    }
                    None => "Value::NULL".to_string(),
                };
                let at = pc_arg(pc);
                let code = self.fallible(
                    id,
                    &format!(
                        "vm.log_event(fr, bex_vm::compiled_rt::LogLevel::{level}, {data}, {name}, {at})"
                    ),
                )?;
                self.emit_statement(&format!("{code};"));
            }
            StatementKind::Intrinsic { op, args } => match op {
                // The declaration's hook prologue resolves the hook the VM's
                // call path left pending for this frame.
                IntrinsicOp::BuiltinTraceHook(mode) => {
                    let mode = match mode {
                        baml_compiler2_mir::BuiltinTraceHook::Hidden => "Hidden",
                        baml_compiler2_mir::BuiltinTraceHook::Timing => "Timing",
                        baml_compiler2_mir::BuiltinTraceHook::Span => "Span",
                        baml_compiler2_mir::BuiltinTraceHook::Rich => "Rich",
                        baml_compiler2_mir::BuiltinTraceHook::EmptySpan => "EmptySpan",
                    };
                    let code = self.fallible(
                        id,
                        &format!(
                            "vm.builtin_trace_hook(fr, bex_vm::compiled_rt::TraceHookMode::{mode})"
                        ),
                    )?;
                    self.emit_statement(&format!("{code};"));
                }
                IntrinsicOp::ApplyTraceHook => {
                    let [result] = args.as_slice() else {
                        return Err(Rejection::Invalid(
                            "`ApplyTraceHook` takes one operand".into(),
                        ));
                    };
                    let result = self.value_operand(id, result)?;
                    let code =
                        self.fallible(id, &format!("vm.compiled_finish_trace_hook(fr, {result})"))?;
                    self.emit_statement(&format!("{code};"));
                }
                IntrinsicOp::BindType(_) | IntrinsicOp::Log(_) => {
                    return unsupported("compiler intrinsic");
                }
            },
            StatementKind::VirtualFieldStore {
                iface,
                receiver,
                field_index,
                value,
                ..
            } => {
                let (iface, iface_args) = self.interface_operands(iface)?;
                let receiver = self.value_operand(id, receiver)?;
                let value = self.value_operand(id, value)?;
                let code = self.fallible(
                    id,
                    &format!(
                        "vm.virtual_store_field({receiver}, {iface}, {iface_args}, {field_index}, {value})"
                    ),
                )?;
                self.emit_statement(&format!("{code};"));
            }
        }
        Ok(())
    }

    fn leaf_terminator(&mut self, id: BlockId) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Branch { condition, .. } => {
                let condition = self.operand(id, condition)?;
                let code = coerce(&condition.code, condition.repr, Repr::Bool);
                self.emit_statement(&format!("c{} = {code};", id.0));
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => {
                let source = self.operand(id, source)?;
                let source_value = self.value_arg(&source);
                self.emit_statement(&format!("let narrowed = {source_value};"));
                let test = self.type_test("narrowed", test)?;
                self.emit_statement(&format!("c{} = {test};", id.0));
                let store = self.store(
                    id,
                    &Place::Local(*destination),
                    &Expr::atom("narrowed", Repr::Value),
                )?;
                self.emit_statement(&format!("if c{} {{ {store} }}", id.0));
            }
            Terminator::Switch { discriminant, .. } => {
                let discriminant = self.operand(id, discriminant)?;
                let code = coerce(&discriminant.code, discriminant.repr, Repr::Int);
                self.emit_statement(&format!("d{} = {code};", id.0));
            }
            terminator @ Terminator::Call { .. } if self.call_suspends(terminator) => {
                self.suspension(id)?;
            }
            terminator @ Terminator::VirtualCall { .. } if !self.array_iteration(terminator) => {
                self.suspension(id)?;
            }
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                destination,
                ..
            } => {
                let call = self.call(id, callee, args, *ntypeargs, argument_layout.as_ref())?;
                let code = self.store(id, destination, &call)?;
                self.emit_statement(&code);
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let operand = self.operand(id, operand)?;
                let value = self.value_arg(&operand);
                self.emit_statement(&format!("let lhs = {value};"));
                let short = match kind {
                    ShortCircuitKind::And => "!vm.is_truthy(lhs)",
                    ShortCircuitKind::Or => "vm.is_truthy(lhs)",
                    ShortCircuitKind::Coalesce => "!lhs.is_null()",
                };
                self.emit_statement(&format!("c{} = {short};", id.0));
                let store = self.store(id, destination, &Expr::atom("lhs", Repr::Value))?;
                self.emit_statement(&format!("if c{} {{ {store} }}", id.0));
            }
            terminator @ Terminator::VirtualCall { destination, .. } => {
                self.array_iteration_call(id, terminator, destination)?;
            }
            Terminator::Goto { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => {}
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => self.suspension(id)?,
        }
        Ok(())
    }

    fn array_iteration_call(
        &mut self,
        id: BlockId,
        terminator: &Terminator<'db>,
        destination: &Place,
    ) -> Gen<()> {
        if let Some((iter, array)) = array_iter_walk(
            self.body,
            self.analysis.arity,
            terminator,
            &self.analysis.array_iters,
        ) {
            let array = coerce(&format!("l{array}"), self.repr(Local(array)), Repr::Value);
            self.emit_statement(&format!("l{iter} = {array}; i{iter} = 0;"));
        } else if let Some(iter) = array_next_call(terminator, &self.analysis.array_iters) {
            // Re-read the length each step because the array can change during iteration.
            let done = baml_compiler2_hir_ty::layout::lang_class(
                self.db,
                baml_base::LangPackage::Baml,
                &["iter"],
                "Done",
            )
            .and_then(|done| self.index.class(self.db, done));
            let Some((done, _)) = done else {
                return unsupported("`baml.iter.Done` not found in the program");
            };
            let step = format!(
                "if i{iter} < bex_vm::compiled_rt::expect_int(vm.container_len(l{iter})?)? {{ \
                 let v = vm.load_array_element(l{iter}, Value::int(i{iter}))?; i{iter} += 1; v }} \
                 else {{ vm.new_instance(vm.record_object(fr, ObjectIndex::from_raw({done})), Vec::new()) }}"
            );
            let code = self.store(id, destination, &Expr::new(step, Repr::Value))?;
            self.emit_statement(&code);
        } else {
            return Err(Rejection::Invalid(
                "virtual call admitted without a lowering".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn exit(&mut self, id: BlockId) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Return => {
                // Type locals have no runtime binding.
                // The result is held as the body's `Signature` returns it.
                let result = if self.analysis.is_type_local(0) {
                    self.load(id, &Place::Local(Local(0)))?.code
                } else {
                    "l0".to_string()
                };
                self.emit_statement(&format!("return Ok({result});"));
            }
            Terminator::Throw { value } => {
                let value = self.value_operand(id, value)?;
                let error = format!("bex_vm::compiled_rt::throw({value})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error)?;
                    self.flush_bindings();
                    self.out.push_str(&jump);
                } else {
                    self.emit_statement(&format!("return Err({error});"));
                }
            }
            Terminator::Rethrow { value, context } => {
                let value = self.value_operand(id, value)?;
                let context = self.value_operand(id, context)?;
                let error = format!("bex_vm::compiled_rt::rethrow({value}, {context})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error)?;
                    self.flush_bindings();
                    self.out.push_str(&jump);
                } else {
                    self.emit_statement(&format!("return Err({error});"));
                }
            }
            Terminator::Unreachable => {
                self.emit_line("let unreachable = vm.unreachable_panic();");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, "unreachable")?;
                    self.flush_bindings();
                    self.out.push_str(&jump);
                } else {
                    self.emit_line("return Err(unreachable);");
                }
            }
            other => {
                return Err(Rejection::Invalid(format!(
                    "exit through a non-exiting terminator {other:?}"
                )));
            }
        }
        Ok(())
    }

    /// Bind the error and context in the handler, then jump to it.
    /// Internal errors bypass handlers.
    pub(super) fn unwind_jump(&mut self, block: BlockId, error: &str) -> Gen<String> {
        let Some(handler) = self.body.blocks[block.0].unwind else {
            return Err(Rejection::Invalid(
                "unwind jump from a block without a handler".into(),
            ));
        };
        let Some(landing) = self.body.blocks[handler.0].landing else {
            return Err(Rejection::Invalid(format!(
                "{handler} is an unwind target but has no landing"
            )));
        };
        let mut out = String::new();
        let value = self.store(
            block,
            &Place::Local(landing.error_local),
            &Expr::atom("thrown", Repr::Value),
        )?;
        let context = self.store(
            block,
            &Place::Local(landing.context_local),
            &Expr::atom("context", Repr::Value),
        )?;
        let saved = std::mem::take(&mut self.out);
        self.emit_line("fr.note_pc(pc);");
        let slot = self.slot_base()
            + usize::from(self.cause_slot.is_some())
            + self
                .body
                .blocks
                .iter()
                .filter(|block| block.landing.is_some())
                .position(|block| block.id == handler)
                .expect("handler has a landing");
        let handler_pc = self
            .positions
            .block(handler.0)
            .and_then(|pcs| pcs.start)
            .ok_or_else(|| Rejection::Unsupported("catch handler without a bytecode PC".into()))?;
        self.emit_line(&format!(
            "let (thrown, context) = vm.catch_at({error}, fr, {slot}, {handler_pc})?;"
        ));
        self.emit_line(&value);
        self.emit_line(&context);
        let structured = self
            .structured
            .as_ref()
            .expect("a body is structured before its leaves are emitted");
        let jumps = structured.unwind_jumps[block.0].clone();
        self.stmts(&jumps)?;
        out.push_str(&self.out);
        self.out = saved;
        Ok(out)
    }

    /// Route a thrown value to the block's handler, or propagate it with `?`.
    pub(super) fn fallible(&mut self, block: BlockId, code: &str) -> Gen<String> {
        if self.body.blocks[block.0].unwind.is_none() {
            return Ok(format!("{code}?"));
        }
        let jump = self.unwind_jump(block, "e")?;
        Ok(format!(
            "(match {code} {{ Ok(v) => v, Err(e) => {{ {jump} }} }})"
        ))
    }

    pub(super) fn fallible_expr(&mut self, block: BlockId, code: &str, repr: Repr) -> Gen<Expr> {
        let code = self.fallible(block, code)?;
        Ok(Expr::new(code, repr))
    }
}
