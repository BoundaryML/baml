//! Generates inline calls, suspension requests, and interface dispatch operands.

use std::fmt::Write as _;

use baml_compiler2_mir::{BlockId, Constant, Operand, TyTemplateInterface, function_display_name};
use bex_vm_types::{FunctionKind, types::Function};

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    admit::Rejection,
    analysis::{EQUALS_EQUALS, Repr, builtin_is_callable, call_shape, operand_ty},
    render::coerce,
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn call(
        &mut self,
        block: BlockId,
        callee: &Operand<'db>,
        args: &[Operand<'db>],
        ntypeargs: usize,
        layout: Option<&baml_type::CallLayout>,
    ) -> Gen<Expr> {
        let Operand::Constant(Constant::Function(func)) = callee else {
            return Err(Rejection::Invalid(
                "call through a function value emitted inline".into(),
            ));
        };
        let name = function_display_name(self.db, *func);
        let Some((object, info)) = self.index.function(self.db, *func) else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        if let Some(test) = self.null_test(block, info, ntypeargs, args)? {
            return Ok(test);
        }
        let shape = call_shape(self.body, args, ntypeargs, &self.analysis.static_types);
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let types = self.type_operands("call", type_args)?;
        if let (FunctionKind::Bytecode, Some(symbol)) = (info.kind, self.symbols.get(&object)) {
            return self.compiled_call(block, object, symbol, &name, info, layout, args, &types);
        }
        let values = self.laid_out_args(block, &name, info, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => Err(Rejection::Invalid(format!(
                "call to uncompiled `{name}` emitted directly"
            ))),
            FunctionKind::NativeUnresolved | FunctionKind::Native(..) => {
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported(format!("builtin `{name}` has no native key"));
                };
                if !builtin_is_callable(key, shape) {
                    return Err(Rejection::Invalid(format!(
                        "yielding builtin `{key}` emitted inline"
                    )));
                }
                // A thread-local receiver takes the builtin's lock-free twin.
                let ordinal = if self.analysis.escape.calls_thread_local_twin(block) {
                    self.builtin_ordinal(&format!("{key}$local"))
                } else {
                    self.builtin_ordinal(key)
                };
                let call = if types.is_empty() {
                    format!("vm.call_builtin(builtin_{ordinal}()?, {args})")
                } else {
                    format!(
                        "vm.call_builtin_typed(builtin_{ordinal}()?, &[{}], {args})",
                        types.join(", ")
                    )
                };
                self.fallible_expr(block, &call, Repr::Value)
            }
            FunctionKind::SysOp(_) => Err(Rejection::Invalid(format!(
                "sys op `{name}` emitted inline"
            ))),
        }
    }

    /// Use a typed direct entry when the callee has no VM-frame state and the VM allows it.
    #[allow(clippy::too_many_arguments)]
    fn compiled_call(
        &mut self,
        block: BlockId,
        object: usize,
        symbol: &str,
        name: &str,
        info: &Function,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
        types: &[String],
    ) -> Gen<Expr> {
        let site = self.site_pc(block);
        let callee = format!("vm.record_object(fr, ObjectIndex::from_raw({object}))");
        // A generic callee's frame starts with the call's type arguments (a
        // plain function carries none of its own).
        let type_args = if types.is_empty() {
            "&[]".to_string()
        } else {
            let name = format!("ty{}", self.temps);
            self.temps += 1;
            self.pending.push(format!(
                "let {name}: [RealizedTy; {}] = [{}];",
                types.len(),
                types.join(", ")
            ));
            format!("&{name}")
        };
        let slots = self.laid_out_operands(block, name, info, layout, args)?;
        let typed = self.signatures.get(&object).filter(|signature| {
            !self.framed.contains(&object)
                && signature.params.len() == slots.len()
                && slots
                    .iter()
                    .zip(&signature.params)
                    .all(|(slot, repr)| slot.is_some() || *repr == Repr::Value)
        });
        let Some(signature) = typed else {
            let values: Vec<String> = slots
                .iter()
                .map(|slot| match slot {
                    Some(expr) => self.value_arg(expr),
                    None => "Value::OMITTED_ARG".to_string(),
                })
                .collect();
            let observed = format!(
                "|_: &mut BexVm| -> [Value; {}] {{ [{}] }}",
                values.len(),
                values.join(", ")
            );
            let values = format!("&[{}]", values.join(", "));
            let framed =
                format!("vm.call_compiled(fr, {callee}, {symbol}, {type_args}, {values}, {site})");
            // Through the boxed entry, with a frame record when the callee
            // has no VM-frame state.
            let call = if self.framed.contains(&object) {
                framed
            } else if types.is_empty() {
                format!(
                    "(if vm.direct_calls() {{ vm.call_record(fr, {callee}, {site}, {observed}, |vm, fr| {symbol}(vm, fr, {values})) }} else {{ {framed} }})"
                )
            } else {
                format!(
                    "(if vm.direct_calls_typed({type_args}) {{ vm.call_record_typed(fr, {callee}, {type_args}, {site}, {observed}, |vm, fr| {symbol}(vm, fr, {values})) }} else {{ {framed} }})"
                )
            };
            let code = self.call_code(block, object, &call)?;
            return Ok(Expr::new(code, Repr::Value));
        };
        // Arguments are bound once, as the callee holds them.
        let mut typed_args = Vec::with_capacity(slots.len());
        for (slot, repr) in slots.iter().zip(&signature.params) {
            let code = match slot {
                Some(expr) => coerce(&expr.code, expr.repr, *repr),
                None => "Value::OMITTED_ARG".to_string(),
            };
            let temp = format!("a{}", self.temps);
            self.temps += 1;
            self.pending
                .push(format!("let {temp}: {} = {code};", repr.rust_type()));
            typed_args.push(temp);
        }
        let direct_args = typed_args.iter().fold(String::new(), |mut out, arg| {
            let _ = write!(out, ", {arg}");
            out
        });
        // Telemetry boxes the arguments only for an invocation it records.
        let observed: Vec<String> = typed_args
            .iter()
            .zip(&signature.params)
            .map(|(arg, repr)| coerce(arg, *repr, Repr::Value))
            .collect();
        let observed = format!(
            "|vm: &mut BexVm| -> [Value; {}] {{ [{}] }}",
            observed.len(),
            observed.join(", ")
        );
        let direct = if types.is_empty() {
            format!(
                "vm.call_record(fr, {callee}, {site}, {observed}, |vm, fr| {symbol}_d(vm, fr{direct_args}))"
            )
        } else {
            format!(
                "vm.call_record_typed(fr, {callee}, {type_args}, {site}, {observed}, |vm, fr| {symbol}_d(vm, fr{direct_args}))"
            )
        };
        let mut boxed = String::from("{ ");
        let mut values = Vec::with_capacity(typed_args.len());
        for (j, (arg, repr)) in typed_args.iter().zip(&signature.params).enumerate() {
            let _ = write!(boxed, "let v{j} = {}; ", coerce(arg, *repr, Repr::Value));
            values.push(format!("v{j}"));
        }
        let unbox = match signature.ret {
            Repr::Int => "bex_vm::compiled_rt::expect_int(v)",
            Repr::Bool => "bex_vm::compiled_rt::expect_bool(v)",
            Repr::Float => "bex_vm::compiled_rt::expect_f64(v)",
            Repr::Value => "Ok(v)",
        };
        let _ = write!(
            boxed,
            "match vm.call_compiled(fr, {callee}, {symbol}, {type_args}, &[{}], {site}) {{ Ok(v) => {unbox}, Err(e) => Err(e) }} }}",
            values.join(", ")
        );
        let allowed = if types.is_empty() {
            "vm.direct_calls()".to_string()
        } else {
            format!("vm.direct_calls_typed({type_args})")
        };
        let call = format!("(if {allowed} {{ {direct} }} else {boxed})");
        let code = self.call_code(block, object, &call)?;
        Ok(Expr::new(code, signature.ret))
    }

    /// Route a compiled call's error, rooting the caller's live values when
    /// the callee may suspend.
    fn call_code(&mut self, block: BlockId, object: usize, call: &str) -> Gen<String> {
        if self.pausing.contains(&object) {
            self.rooted_call(block, call)
        } else {
            self.fallible(block, call)
        }
    }

    /// The operands of a call in callee slot order; `None` for a slot left to
    /// its default.
    fn laid_out_operands(
        &mut self,
        block: BlockId,
        name: &str,
        info: &Function,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Vec<Option<Expr>>> {
        let exprs = args
            .iter()
            .map(|arg| self.operand(block, arg))
            .collect::<Gen<Vec<_>>>()?;
        layout_args(name, info, layout, exprs)
    }

    /// `x == null` as a bit test. The `==` driver answers a pair with a
    /// `null` side by kind alone: `null` equals only `null`, and no user
    /// `Equals.eq` can run.
    fn null_test(
        &mut self,
        block: BlockId,
        info: &Function,
        ntypeargs: usize,
        args: &[Operand<'db>],
    ) -> Gen<Option<Expr>> {
        if info.native_key.as_deref() != Some(EQUALS_EQUALS) || ntypeargs != 0 {
            return Ok(None);
        }
        let [lhs, rhs] = args else {
            return Ok(None);
        };
        let is_null =
            |arg: &Operand<'db>| operand_ty(self.body, arg).is_some_and(|ty| ty.is_null());
        let other = if is_null(rhs) {
            lhs
        } else if is_null(lhs) {
            rhs
        } else {
            return Ok(None);
        };
        let other = self.operand(block, other)?;
        let value = self.value_arg(&other);
        Ok(Some(Expr::new(format!("{value}.is_null()"), Repr::Bool)))
    }

    pub(super) fn callee_ptr(
        &mut self,
        block: BlockId,
        callee: &Operand<'db>,
        trace: Option<&Operand<'db>>,
    ) -> Gen<()> {
        let code = match callee {
            Operand::Constant(Constant::Function(func)) => {
                let Some((object, _)) = self.index.function(self.db, *func) else {
                    let name = function_display_name(self.db, *func);
                    return unsupported(format!("callee `{name}` not found in the program"));
                };
                format!("vm.record_object(fr, ObjectIndex::from_raw({object}))")
            }
            Operand::Constant(Constant::GlobalItem(_)) | Operand::Copy(_) | Operand::Move(_) => {
                let value = self.value_operand(block, callee)?;
                if let Some(trace) = trace {
                    // SetCallTrace precedes CallIndirect's callable check.
                    let trace = self.value_operand(block, trace)?;
                    let set_trace = self.fallible(block, &format!("vm.set_call_trace({trace})"))?;
                    self.emit_statement(&format!("{set_trace};"));
                }
                self.fallible(block, &format!("vm.indirect_callee({value})"))?
            }
            Operand::Constant(_) => {
                return unsupported("call of a constant that is not a function");
            }
        };
        self.emit_statement(&format!("let callee = {code};"));
        Ok(())
    }

    /// Lay out arguments in callee slot order, filling defaults with `OMITTED_ARG`.
    pub(super) fn laid_out_args(
        &mut self,
        block: BlockId,
        name: &str,
        info: &Function,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        let values = self.arg_values(block, args)?;
        Ok(layout_args(name, info, layout, values)?
            .into_iter()
            .map(|slot| slot.unwrap_or_else(|| "Value::OMITTED_ARG".to_string()))
            .collect())
    }

    pub(super) fn arg_values(&mut self, block: BlockId, args: &[Operand<'db>]) -> Gen<Vec<String>> {
        args.iter()
            .map(|arg| self.value_operand(block, arg))
            .collect()
    }

    pub(super) fn type_operands(
        &mut self,
        what: &str,
        operands: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        operands
            .iter()
            .map(|operand| self.type_operand(what, operand))
            .collect()
    }

    /// Associated types are implementation outputs and do not select the implementation.
    pub(super) fn interface_operands(
        &mut self,
        iface: &TyTemplateInterface,
    ) -> Gen<(String, String)> {
        let name = decl_display_name(self.db, &iface.name);
        let Some(object) = self.index.interface_at(&iface.name) else {
            return unsupported(format!("interface `{name}` not found in the program"));
        };
        let args = self.realized_all(&iface.generics)?;
        Ok((
            format!("vm.record_object(fr, ObjectIndex::from_raw({object}))"),
            format!("&[{}]", args.join(", ")),
        ))
    }

    /// Reuse the interpreter's resolver/cache; only matching arities run inline.
    /// `args` starts with type arguments; `self_arg` indexes the value arguments.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn virtual_request(
        &mut self,
        block: BlockId,
        iface: &TyTemplateInterface,
        method: &str,
        args: &[Operand<'db>],
        traced: bool,
        ntypeargs: usize,
        self_arg: usize,
        layout: Option<&baml_type::CallLayout>,
    ) -> Gen<String> {
        let (iface, iface_args) = self.interface_operands(iface)?;
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let types = self.type_operands("interface method call", type_args)?;
        let values = self.arg_values(block, args)?;
        let Some(receiver) = values.get(self_arg) else {
            return Err(Rejection::Invalid(format!(
                "virtual call dispatches on argument {self_arg} of {}",
                values.len()
            )));
        };
        // Implementations may add optional parameters, so only matching arities run inline.
        let named = layout.is_some_and(|layout| layout.0.iter().any(Option::is_some));
        let layout = layout.map_or_else(
            || "None".to_string(),
            |layout| format!("Some(&[{}])", named_arg_slots(layout)),
        );
        // A known site shares the interpreter's cache for its `VirtualCall`.
        let cache_site = self
            .positions
            .block_pcs
            .get(block.0)
            .and_then(|pcs| pcs.terminator)
            .map_or_else(|| "None".to_string(), |pc| format!("Some({pc})"));
        self.emit_statement(&format!(
            "let callee = vm.virtual_target({receiver}, {iface}, {iface_args}, {method:?}, &[{}], {cache_site})?;",
            types.join(", ")
        ));
        let site = self.site_pc(block);
        let values = values.join(", ");
        let suspend = if traced {
            let call = self.attach_trace("vm.suspend(Step::Call { callee, argc })");
            format!(
                "match vm.push_virtual_args(callee, {layout}, &[{values}]) {{ \
                 Ok(argc) => {{ fr.note_pc({site}); {call} }}, Err(e) => Err(e) }}"
            )
        } else {
            format!(
                "{{ let argc = vm.push_virtual_args(callee, {layout}, &[{values}])?; \
                 fr.note_pc({site}); vm.suspend(Step::Call {{ callee, argc }}) }}"
            )
        };
        // The VM must apply trace attachments, so traced calls cannot run inline.
        if named || traced {
            return Ok(suspend);
        }
        Ok(format!(
            "match vm.call_inline(fr, callee, &[{values}], {site}) {{ \
             Ok(Some(v)) => Ok(v), Ok(None) => {suspend}, Err(e) => Err(e) }}"
        ))
    }
}

/// Named arguments in callee slot order; positional calls keep their original order.
fn layout_args<T>(
    name: &str,
    info: &Function,
    layout: Option<&baml_type::CallLayout>,
    args: Vec<T>,
) -> Gen<Vec<Option<T>>> {
    let slots = layout
        .filter(|layout| layout.0.iter().any(Option::is_some))
        .map(|layout| layout.map_to(&info.argument_layout()))
        .transpose()
        .map_err(|mismatch| {
            Rejection::Invalid(format!("call to `{name}` does not fit it: {mismatch:?}"))
        })?;
    let arity = slots.as_ref().map_or(args.len(), Vec::len);
    if arity != info.arity {
        return Err(Rejection::Invalid(format!(
            "`{name}` called with {arity} of {} arguments",
            info.arity
        )));
    }
    let mut args: Vec<_> = args.into_iter().map(Some).collect();
    Ok(match slots {
        Some(slots) => slots
            .into_iter()
            .map(|slot| slot.and_then(|i| args[i].take()))
            .collect(),
        None => args,
    })
}

pub(super) fn named_arg_slots(layout: &baml_type::CallLayout) -> String {
    layout
        .0
        .iter()
        .map(|slot| match slot {
            Some(name) => format!("Some({:?})", name.as_str()),
            None => "None".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn decl_display_name(db: &dyn baml_compiler2_mir::Db, name: &baml_type::DeclName) -> String {
    let mut parts = vec![
        baml_compiler2_hir_ty::render::Viewpoint::canonical(db)
            .display_label(name.root())
            .to_string(),
    ];
    parts.extend(name.namespace().iter().map(ToString::to_string));
    parts.push(name.name().to_string());
    parts.join(".")
}
