//! Statements: structured control flow, a block's statements and
//! terminator, and how an error leaves a block.

use baml_compiler2_mir::{
    BlockId, IntrinsicOp, Local, Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, class_display_name,
};

use super::{Expr, Gen, Generator, YIELD_EVERY, pc_arg, unsupported};
use crate::{
    admit::Rejection,
    analysis::{Repr, array_iter_call, array_next_call},
    render::coerce,
    structure::Stmt,
};

impl Generator<'_, '_> {
    pub(super) fn stmts(&mut self, stmts: &[Stmt], depth: usize) -> Gen<()> {
        for stmt in stmts {
            self.stmt(stmt, depth)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt, depth: usize) -> Gen<()> {
        match stmt {
            Stmt::Leaf(block) => self.leaf(BlockId(*block), depth),
            Stmt::Block { label, body } => {
                self.line(depth, &format!("'b{label}: {{"));
                self.stmts(body, depth + 1)?;
                self.line(depth, "}");
                Ok(())
            }
            Stmt::Loop { label, body } => {
                self.line(depth, &format!("'l{label}: loop {{"));
                self.stmts(body, depth + 1)?;
                self.line(depth, "}");
                Ok(())
            }
            Stmt::If {
                block,
                then_branch,
                else_branch,
            } => {
                self.line(depth, &format!("if c{block} {{"));
                self.stmts(then_branch, depth + 1)?;
                self.line(depth, "} else {");
                self.stmts(else_branch, depth + 1)?;
                self.line(depth, "}");
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
                self.line(depth, &format!("match d{block} {{"));
                for (indices, body) in arms {
                    let mut pattern = Vec::with_capacity(indices.len());
                    for &i in indices {
                        pattern.push(self.switch_pattern(&keys[i].0)?);
                    }
                    self.line(depth + 1, &format!("{} => {{", pattern.join(" | ")));
                    self.stmts(body, depth + 2)?;
                    self.line(depth + 1, "}");
                }
                self.line(depth + 1, "_ => {");
                self.stmts(otherwise, depth + 2)?;
                self.line(depth + 1, "}");
                self.line(depth, "}");
                Ok(())
            }
            Stmt::Break(label) => {
                self.line(depth, &format!("break 'b{label};"));
                Ok(())
            }
            Stmt::BreakLoop(label) => {
                self.line(depth, &format!("break 'l{label};"));
                Ok(())
            }
            Stmt::Continue(label) => {
                self.line(depth, &format!("continue 'l{label};"));
                Ok(())
            }
            Stmt::Exit(block) => self.exit(BlockId(*block), depth),
        }
    }

    /// The pattern a switch arm's key matches against the discriminant: an
    /// enum's variant index, or a class's dispatch tag (what `Rvalue::TypeTag`
    /// reads off an instance, which the linker fixed for every class of the
    /// program). Two keys with one tag match in order, as bytecode's solved
    /// table does: the later arm is unreachable.
    pub(super) fn switch_pattern(&self, key: &SwitchKey<'_>) -> Gen<String> {
        Ok(match key {
            SwitchKey::Int(key) => format!("{key}i64"),
            SwitchKey::Class(class) => {
                let Some((object, _)) = self.index.class(self.db, *class)? else {
                    let name = class_display_name(self.db, *class);
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                format!("{}i64", self.index.class_tag(object))
            }
        })
    }

    /// The statements of a block and whatever its terminator computes before
    /// control transfers.
    pub(super) fn leaf(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        // A leaf can be entered from several places; nothing about `pc` is
        // known on entry. Every PC of the leaf lies in its block's bytecode,
        // so what it records says whether the block is a shielded `defer`
        // body, as the engine reads it (`vm_thread_is_shielded`).
        self.current_mark = None;
        let pcs = self.positions.block(id.0).cloned().unwrap_or_default();
        // A resumable body lets the scheduler in from its loops. It counts
        // iterations in a register and takes the VM's control-flow check
        // every `YIELD_EVERY`th: a compiled iteration is a fraction of an
        // interpreted one, so this still polls about as often in time.
        if self.suspends && self.loop_headers.contains(&id.0) {
            let live = self.live_in[id.0].clone();
            let point = self.yield_point(id.0);
            let point_slot = self.point_slot;
            self.line(depth, "iterations = iterations.wrapping_add(1);");
            self.line(
                depth,
                &format!("if iterations % {YIELD_EVERY} == 0 && vm.should_yield() {{"),
            );
            self.save(self.saved(live), depth + 1);
            self.line(
                depth + 1,
                &format!("vm.set_slot(depth, {point_slot}, Value::int({point}));"),
            );
            // It yields standing at the loop header.
            let at = pc_arg(pcs.start);
            self.line(depth + 1, &format!("vm.note_pc(depth, {at});"));
            self.line(depth + 1, "return Ok(Step::Yield);");
            self.line(depth, "}");
        } else if self.gc_checks.loops && self.loop_headers.contains(&id.0) {
            // An inline body collects in place at an allocating loop's
            // header, as often as a resumable one would yield.
            let live = self.live_in[id.0].clone();
            self.line(depth, "iterations = iterations.wrapping_add(1);");
            self.gc_check(
                &live,
                &format!("iterations % {YIELD_EVERY} == 0 && vm.gc_due()"),
                depth,
            );
        }
        // Entering or leaving a handler's body: record the context of the
        // error it handles, which a raise here chains onto as its cause.
        if let Some(slot) = self.cause_slot
            && self.cause_changes_at(id.0)
        {
            let cause = match block.handling.and_then(|h| self.body.blocks[h.0].landing) {
                Some(landing) => {
                    let context = format!("l{}", landing.context_local.0);
                    self.value_arg(&context, self.repr(landing.context_local))
                }
                None => "Value::NULL".to_string(),
            };
            self.statement(depth, &format!("vm.set_slot(depth, {slot}, {cause});"));
        }
        for (index, statement) in block.statements.iter().enumerate() {
            let pc = pcs.statements.get(index).copied().flatten();
            self.set_pc(pc, depth);
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    // A type local is built where it is consumed (a call's
                    // type argument, a `type` value); it has no runtime
                    // value of its own.
                    if let (Place::Local(local), Rvalue::LoadType(_)) = (destination, value)
                        && self.analysis.is_type_local(local.0)
                    {
                        continue;
                    }
                    let value = self.rvalue(id, value)?;
                    let code = self.store(id, destination, &value)?;
                    self.statement(depth, &code);
                }
                StatementKind::Drop(_) | StatementKind::Nop => {}
                // The captured local's binding is created here: a new cell,
                // holding null or the value of the cell it replaces (a loop
                // header's binding, carried into the next iteration).
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
                    self.statement(depth, &code);
                }
                // `type T = value`: bind the value into the frame's slot, as
                // `BindType` does. Every type the body derives after this
                // reads the frame again (`Generator::realized`).
                StatementKind::Intrinsic {
                    op: IntrinsicOp::BindType(slot),
                    args,
                } => {
                    let [value] = args.as_slice() else {
                        return Err(Rejection::Invalid("`BindType` takes one operand".into()));
                    };
                    let value = self.operand(id, value)?;
                    let value = self.value_arg(&value.code, value.repr);
                    let code =
                        self.fallible(id, &format!("vm.bind_type(depth, {slot}, {value})"))?;
                    self.statement(depth, &format!("{code};"));
                }
                // `log.*`: the `$baml_log` record bytecode's `SendEvent`
                // makes, queued for the engine (`vm.log_event`).
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
                    let data = self.operand(id, data)?;
                    let data = self.value_arg(&data.code, data.repr);
                    let name = match args.get(1) {
                        Some(name) => {
                            let name = self.operand(id, name)?;
                            self.value_arg(&name.code, name.repr)
                        }
                        None => "Value::NULL".to_string(),
                    };
                    // The statement's PC is its `SendEvent`'s, whose line entry
                    // locates the record.
                    let at = pc_arg(pc);
                    let code =
                        self.fallible(id, &format!("vm.log_event(bex_vm::compiled_rt::LogLevel::{level}, {data}, {name}, {at})"))?;
                    self.statement(depth, &format!("{code};"));
                }
                StatementKind::Intrinsic { .. } => return unsupported("compiler intrinsic"),
                StatementKind::VirtualFieldStore {
                    iface,
                    receiver,
                    field_index,
                    value,
                    ..
                } => {
                    let (iface, iface_args, _) = self.interface_operands(iface)?;
                    let receiver = self.operand(id, receiver)?;
                    let receiver = self.value_arg(&receiver.code, receiver.repr);
                    let value = self.operand(id, value)?;
                    let value = self.value_arg(&value.code, value.repr);
                    let code = self.fallible(
                        id,
                        &format!(
                            "vm.virtual_store_field({receiver}, {iface}, {iface_args}, {field_index}, {value})"
                        ),
                    )?;
                    self.statement(depth, &format!("{code};"));
                }
            }
        }
        self.set_pc(pcs.terminator, depth);
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Branch { condition, .. } => {
                let condition = self.operand(id, condition)?;
                let code = coerce(&condition.code, condition.repr, Repr::Bool);
                self.statement(depth, &format!("c{} = {code};", id.0));
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => {
                let source = self.operand(id, source)?;
                let source_value = self.value_arg(&source.code, source.repr);
                self.statement(depth, &format!("let narrowed = {source_value};"));
                let test = self.type_test("narrowed", test)?;
                self.statement(depth, &format!("c{} = {test};", id.0));
                let store = self.store(
                    id,
                    &Place::Local(*destination),
                    &Expr::new("narrowed", Repr::Value),
                )?;
                self.statement(depth, &format!("if c{} {{ {store} }}", id.0));
            }
            Terminator::Switch { discriminant, .. } => {
                let discriminant = self.operand(id, discriminant)?;
                let code = coerce(&discriminant.code, discriminant.repr, Repr::Int);
                self.statement(depth, &format!("d{} = {code};", id.0));
            }
            terminator @ Terminator::Call { .. } if self.call_suspends(terminator) => {}
            // Emitted by `pause`.
            terminator @ Terminator::VirtualCall { .. } if !self.array_iteration(terminator) => {}
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                destination,
                ..
            } => {
                let call = self.call(id, callee, args, *ntypeargs, argument_layout.as_ref())?;
                let code = self.store(id, destination, &Expr::new(call, Repr::Value))?;
                self.statement(depth, &code);
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let operand = self.operand(id, operand)?;
                let value = self.value_arg(&operand.code, operand.repr);
                self.statement(depth, &format!("let lhs = {value};"));
                let short = match kind {
                    ShortCircuitKind::And => "!vm.is_truthy(lhs)",
                    ShortCircuitKind::Or => "vm.is_truthy(lhs)",
                    ShortCircuitKind::Coalesce => "!lhs.is_null()",
                };
                self.statement(depth, &format!("c{} = {short};", id.0));
                let store = self.store(id, destination, &Expr::new("lhs", Repr::Value))?;
                self.statement(depth, &format!("if c{} {{ {store} }}", id.0));
            }
            Terminator::VirtualCall { destination, .. } => {
                if let Some((iter, array)) = array_iter_call(
                    self.body,
                    self.analysis.arity,
                    block.terminator.as_ref().expect("checked"),
                ) {
                    let array = coerce(&format!("l{array}"), self.repr(Local(array)), Repr::Value);
                    self.statement(depth, &format!("l{iter} = {array}; i{iter} = 0;"));
                } else if let Some(iter) = array_next_call(
                    block.terminator.as_ref().expect("checked"),
                    &self.analysis.array_iters,
                ) {
                    // `baml.iter.ArrayIterator.next`: the length is re-read on
                    // every step, so a loop that grows or shrinks the array
                    // sees it, and the end is a `Done` instance.
                    let done = baml_compiler2_hir_ty::layout::lang_class(
                        self.db,
                        baml_base::LangPackage::Baml,
                        &["iter"],
                        "Done",
                    )
                    .map(|done| self.index.class(self.db, done))
                    .transpose()?
                    .flatten();
                    let Some((done, _)) = done else {
                        return unsupported("`baml.iter.Done` not found in the program");
                    };
                    let step = format!(
                        "if i{iter} < bex_vm::compiled_rt::expect_int(vm.container_len(l{iter})?)? {{ \
                         let v = vm.load_array_element(l{iter}, Value::int(i{iter}))?; i{iter} += 1; v }} \
                         else {{ vm.new_instance(vm.idx_to_ptr(ObjectIndex::from_raw({done})), Vec::new()) }}"
                    );
                    let code = self.store(id, destination, &Expr::new(step, Repr::Value))?;
                    self.statement(depth, &code);
                } else {
                    return Err(Rejection::Invalid(
                        "virtual call admitted without a lowering".into(),
                    ));
                }
            }
            Terminator::Goto { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => {}
            // Emitted by `pause`, in a resumable function.
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => {}
            // Before a wildcard catch arm: a panic is raised again, with its
            // context, rather than swallowed.
            Terminator::ThrowIfPanic { value, context, .. } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let context = self.operand(id, context)?;
                let context = self.value_arg(&context.code, context.repr);
                let error = format!("bex_vm::compiled_rt::rethrow({value}, {context})");
                let raise = if block.unwind.is_some() {
                    self.unwind_jump(id, &error, depth + 1)?
                } else {
                    format!("return Err({error});")
                };
                self.statement(depth, &format!("if vm.is_panic({value}) {{"));
                self.out.push_str(&raise);
                self.out.push('\n');
                self.line(depth, "}");
            }
        }
        Ok(())
    }

    /// How a block leaves the function.
    pub(super) fn exit(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Return => {
                if self.analysis.array_iters.contains_key(&0) {
                    return unsupported("array iterator used as a value");
                }
                // A returned type local (`reflect.Type.of<T>()`) has no
                // binding; its value is built here, as at any other read.
                let result = if self.analysis.is_type_local(0) {
                    self.load(id, &Place::Local(Local(0)))?.code
                } else {
                    coerce("l0", self.repr(Local(0)), Repr::Value)
                };
                if self.suspends {
                    self.statement(depth, &format!("return Ok(Step::Return({result}));"));
                } else {
                    self.statement(depth, &format!("return Ok({result});"));
                }
            }
            Terminator::Throw { value } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let error = format!("bex_vm::compiled_rt::throw({value})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error, depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.statement(depth, &format!("return Err({error});"));
                }
            }
            Terminator::Rethrow { value, context } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let context = self.operand(id, context)?;
                let context = self.value_arg(&context.code, context.repr);
                let error = format!("bex_vm::compiled_rt::rethrow({value}, {context})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error, depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.statement(depth, &format!("return Err({error});"));
                }
            }
            // Bytecode's `Unreachable`: a catchable `baml.panics.Unreachable`.
            Terminator::Unreachable => {
                self.line(depth, "let unreachable = vm.unreachable_panic();");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, "unreachable", depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.line(depth, "return Err(unreachable);");
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

    /// The statements that land `error` (an expression of type
    /// `Box<VmError>`) in `block`'s handler: build its context and pop the
    /// frames of compiled callees above this one (`BexVm::catch`), bind value
    /// and context into the handler's landing locals, and jump there. An
    /// internal error is not catchable and propagates.
    pub(super) fn unwind_jump(&mut self, block: BlockId, error: &str, depth: usize) -> Gen<String> {
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
            &Expr::new("thrown", Repr::Value),
        )?;
        let context = self.store(
            block,
            &Place::Local(landing.context_local),
            &Expr::new("context", Repr::Value),
        )?;
        let saved = std::mem::take(&mut self.out);
        self.line(depth, "vm.note_pc(depth, pc);");
        self.line(
            depth,
            &format!("let (thrown, context) = vm.catch({error}, depth)?;"),
        );
        self.line(depth, &value);
        self.line(depth, &context);
        if self.suspends {
            let jump = self.jump_to(handler.0);
            self.line(depth, &jump);
        } else {
            let structured = self
                .structured
                .as_ref()
                .expect("an inline body is structured before its leaves are emitted");
            let jumps = structured.unwind_jumps[block.0].clone();
            self.stmts(&jumps, depth)?;
        }
        out.push_str(&self.out);
        self.out = saved;
        Ok(out)
    }

    /// Wrap a fallible expression (`Result<_, Box<VmError>>`) for `block`:
    /// route a thrown value to its handler, otherwise propagate with `?`.
    pub(super) fn fallible(&mut self, block: BlockId, code: &str) -> Gen<String> {
        if self.body.blocks[block.0].unwind.is_none() {
            return Ok(format!("{code}?"));
        }
        let jump = self.unwind_jump(block, "e", 0)?;
        Ok(format!(
            "(match {code} {{ Ok(v) => v, Err(e) => {{ {jump} }} }})"
        ))
    }

    pub(super) fn fallible_expr(&mut self, block: BlockId, code: &str, repr: Repr) -> Gen<Expr> {
        let code = self.fallible(block, code)?;
        Ok(Expr::new(code, repr))
    }
}
