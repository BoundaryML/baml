//! Generates inline calls, suspension requests, and interface dispatch operands.

use std::fmt::Write as _;

use baml_compiler2_mir::{BlockId, Constant, Operand, TyTemplateInterface, function_display_name};
use bex_vm_types::{
    FunctionKind,
    layout::{RawKind, SlotKind},
    types::Function,
};

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    admit::Rejection,
    analysis::{EQUALS_EQUALS, Repr, Signature, builtin_is_callable, call_shape, operand_ty},
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
        let shape = call_shape(self.body, args, ntypeargs, &self.analysis.static_types);
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let types = self.type_operands("call", type_args)?;
        if let (FunctionKind::Bytecode, Some(symbol)) = (info.kind, self.symbols.get(&object)) {
            return self.compiled_call(block, object, symbol, &name, info, layout, args, &types);
        }
        let call = Builtin {
            name: &name,
            info,
            shape,
            types: &types,
        };
        if let Some(inline) = self.inline_builtin(block, &call, layout, args)? {
            return Ok(inline);
        }
        let values = self.laid_out_args(block, &name, info, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => Err(Rejection::Invalid(format!(
                "call to uncompiled `{name}` emitted directly"
            ))),
            FunctionKind::NativeUnresolved | FunctionKind::Native(..) => {
                self.builtin_call(block, &name, info, shape, &types, &args)
            }
            FunctionKind::SysOp(_) => Err(Rejection::Invalid(format!(
                "sys op `{name}` emitted inline"
            ))),
        }
    }

    /// A builtin call compiled code makes without the builtin's glue: `x ==
    /// null` as a bit test, a scalar body called directly, or a push onto a
    /// list of known element type. `args` are the value arguments.
    fn inline_builtin(
        &mut self,
        block: BlockId,
        call: &Builtin<'_>,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Option<Expr>> {
        let ntypes = call.types.len();
        if let Some(test) = self.null_test(block, call.info, ntypes, args)? {
            return Ok(Some(test));
        }
        if let Some(native) = self.scalar_native_call(block, call.info, layout, args, ntypes)? {
            return Ok(Some(native));
        }
        self.known_push(block, call, layout, args)
    }

    /// `xs.push(x)` on a list local whose element type is fully known: the
    /// shared list's push in place (`BexVm::push_known`), else the builtin's
    /// own path, which boxes the value there only.
    fn known_push(
        &mut self,
        block: BlockId,
        call: &Builtin<'_>,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Option<Expr>> {
        let named = layout.is_some_and(|layout| layout.0.iter().any(Option::is_some));
        let [list, value] = args else {
            return Ok(None);
        };
        if named || call.info.native_key.as_deref() != Some(ARRAY_PUSH) {
            return Ok(None);
        }
        let Some((descriptor, kind)) = self.known_list(list) else {
            return Ok(None);
        };
        let list = self.value_operand(block, list)?;
        let value = self.operand(block, value)?;
        let slot_value = match (kind, value.repr) {
            (SlotKind::Raw(RawKind::Float), Repr::Float) => {
                format!(
                    "bex_vm_types::object_format::SlotValue::Float({})",
                    value.code
                )
            }
            (SlotKind::Raw(RawKind::Float), _) => return Ok(None),
            _ => format!(
                "bex_vm_types::object_format::SlotValue::Value({})",
                coerce(&value.code, value.repr, Repr::Value)
            ),
        };
        let boxed = coerce(&value.code, value.repr, Repr::Value);
        let fallback = self.builtin_call(
            block,
            call.name,
            call.info,
            call.shape,
            call.types,
            &format!("&[{list}, b]"),
        )?;
        let fallback = coerce(&fallback.code, fallback.repr, Repr::Int);
        Ok(Some(Expr::new(
            format!(
                "(match vm.push_known({list}, {descriptor}, {slot_value}) {{ Some(n) => n, None => {{ let b = {boxed}; {fallback} }} }})"
            ),
            Repr::Int,
        )))
    }

    /// A builtin whose native body takes and returns only scalars, called
    /// directly with unboxed arguments (`baml_builtins2_codegen::
    /// scalar_native_entry`); `None` for any other builtin or a call that
    /// passes type arguments or names its arguments.
    fn scalar_native_call(
        &mut self,
        block: BlockId,
        info: &Function,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
        ntypes: usize,
    ) -> Gen<Option<Expr>> {
        let native = info
            .native_key
            .as_deref()
            .and_then(baml_builtins2_codegen::scalar_native_entry);
        let named = layout.is_some_and(|layout| layout.0.iter().any(Option::is_some));
        let Some(native) = native.filter(|n| !named && ntypes == 0 && n.params.len() == args.len())
        else {
            return Ok(None);
        };
        let repr = |ty: &str| match ty {
            "i64" => Repr::Int,
            "f64" => Repr::Float,
            _ => Repr::Bool,
        };
        let mut values = Vec::with_capacity(args.len());
        for (arg, ty) in args.iter().zip(&native.params) {
            let expr = self.operand(block, arg)?;
            values.push(coerce(&expr.code, expr.repr, repr(ty)));
        }
        Ok(Some(Expr::new(
            format!("{}({})", native.call, values.join(", ")),
            repr(&native.ret),
        )))
    }

    /// A call to a builtin: its generated compiled entry when it has an
    /// adapter, else its interpreter glue (the thread-local twin where the
    /// receiver is proved thread-local).
    fn builtin_call(
        &mut self,
        block: BlockId,
        name: &str,
        info: &bex_vm_types::types::Function,
        shape: crate::analysis::CallShape,
        types: &[String],
        args: &str,
    ) -> Gen<Expr> {
        let Some(key) = info.native_key.as_deref() else {
            return unsupported(format!("builtin `{name}` has no native key"));
        };
        if !builtin_is_callable(key, shape) {
            return Err(Rejection::Invalid(format!(
                "yielding builtin `{key}` emitted inline"
            )));
        }
        let call = |ordinal: usize| {
            if types.is_empty() {
                format!("vm.call_builtin(builtin_{ordinal}()?, {args})")
            } else {
                format!(
                    "vm.call_builtin_typed(builtin_{ordinal}()?, &[{}], {args})",
                    types.join(", ")
                )
            }
        };
        // A thread-local receiver takes the builtin's lock-free twin.
        let needs = if matches!(info.kind, FunctionKind::Native(_, Some(_))) {
            self.analysis.escape.twin_needs(block)
        } else {
            baml_compiler2_mir::escape::NEVER
        };
        let locked = (needs != 0).then(|| call(self.builtin_ordinal(key)));
        let local = (needs != baml_compiler2_mir::escape::NEVER)
            .then(|| call(self.builtin_ordinal(&format!("{key}$local"))));
        let call = self.by_locality(
            needs,
            local.as_deref().unwrap_or_default(),
            locked.as_deref().unwrap_or_default(),
        );
        // An adapted builtin runs its one body over the compiled
        // object model; the interpreter call is the fallback when
        // the body defers.
        let call = match baml_builtins2_codegen::compiled_adapter_entry(key) {
            Some(entry) => format!(
                "vm.call_adapter(bex_vm::adapters::generated::{entry}, {args}, |vm| {call})"
            ),
            None => call,
        };
        self.fallible_expr(block, &call, Repr::Value)
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
        let slots = self.laid_out_operands(block, name, info, layout, args)?;
        self.entry_call(block, object, symbol, slots, types)
    }

    /// A call to the compiled body of `object` with arguments in its slot
    /// order (`None` for a slot left to its default): a plain call, a call
    /// with a frame record, or through the boxed entry, as the VM allows.
    pub(super) fn entry_call(
        &mut self,
        block: BlockId,
        object: usize,
        symbol: &str,
        slots: Vec<Option<Expr>>,
        types: &[String],
    ) -> Gen<Expr> {
        let site = CallSite {
            object,
            symbol: symbol.to_owned(),
            callee: format!("vm.record_object(fr, ObjectIndex::from_raw({object}))"),
            pc: self.site_pc(block),
            type_args: self.bind_type_args(types),
            generic: !types.is_empty(),
        };
        let typed = self.signatures.get(&object).filter(|signature| {
            !self.framed.contains(&object)
                && signature.params.len() == slots.len()
                && slots
                    .iter()
                    .zip(&signature.params)
                    .all(|(slot, repr)| slot.is_some() || *repr == Repr::Value)
        });
        match typed {
            Some(signature) => self.typed_entry_call(block, &site, &slots, signature),
            None => self.boxed_entry_call(block, &site, &slots),
        }
    }

    /// A generic callee's frame starts with the call's type arguments (a
    /// plain function carries none of its own): a binding of them, or `&[]`.
    fn bind_type_args(&mut self, types: &[String]) -> String {
        if types.is_empty() {
            return "&[]".to_string();
        }
        let name = format!("ty{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: [RealizedTy; {}] = [{}];",
            types.len(),
            types.join(", ")
        ));
        format!("&{name}")
    }

    /// Through the boxed entry, with a frame record when the callee has no
    /// VM-frame state.
    fn boxed_entry_call(
        &mut self,
        block: BlockId,
        site: &CallSite,
        slots: &[Option<Expr>],
    ) -> Gen<Expr> {
        let CallSite {
            object,
            symbol,
            callee,
            pc,
            type_args,
            generic,
        } = site;
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
            format!("vm.call_compiled(fr, {callee}, {symbol}, {type_args}, {values}, {pc})");
        let call = if self.framed.contains(object) {
            framed
        } else if !generic {
            format!(
                "(if vm.direct_calls() {{ vm.call_record(fr, {callee}, {pc}, {observed}, |vm, fr| {symbol}(vm, fr, {values})) }} else {{ {framed} }})"
            )
        } else {
            format!(
                "(if vm.direct_calls_typed({type_args}) {{ vm.call_record_typed(fr, {callee}, {type_args}, {pc}, {observed}, |vm, fr| {symbol}(vm, fr, {values})) }} else {{ {framed} }})"
            )
        };
        let code = self.call_code(block, &call)?;
        Ok(Expr::new(code, Repr::Value))
    }

    /// Through the callee's typed direct entry: plain when nothing observes
    /// calls and the callee keeps no record state, else with a frame record,
    /// else through the boxed entry.
    fn typed_entry_call(
        &mut self,
        block: BlockId,
        site: &CallSite,
        slots: &[Option<Expr>],
        signature: &Signature,
    ) -> Gen<Expr> {
        // Arguments are bound once, as the callee holds them.
        let mut args = Vec::with_capacity(slots.len());
        for (slot, repr) in slots.iter().zip(&signature.params) {
            let code = match slot {
                Some(expr) => coerce(&expr.code, expr.repr, *repr),
                None => "Value::OMITTED_ARG".to_string(),
            };
            let temp = format!("a{}", self.temps);
            self.temps += 1;
            self.pending
                .push(format!("let {temp}: {} = {code};", repr.rust_type()));
            args.push((temp, *repr));
        }
        let mask = self.callee_mask(block);
        let passed = args.iter().fold(String::new(), |mut out, (arg, _)| {
            let _ = write!(out, ", {arg}");
            out
        });
        let direct = format!("{}_d(vm, fr{passed}, {mask})", site.symbol);
        let call = if site.generic {
            format!(
                "(if {} {{ {} }} else {})",
                site.allowed(),
                site.recorded(&direct, &args),
                site.boxed(&args, signature.ret)
            )
        } else {
            // The record and boxed calls are the callee's cold entry.
            let plain = self.plain_call(site, &direct);
            let flag = super::plain_const(&site.symbol);
            let CallSite { symbol, pc, .. } = site;
            format!(
                "(if {flag} && vm.plain_calls() {{ {plain} }} else {{ {symbol}_r(vm, fr, {pc}{passed}, {mask}) }})"
            )
        };
        let code = self.call_code(block, &call)?;
        Ok(Expr::new(code, signature.ret))
    }

    /// A plain body runs without a record when nothing observes calls; one
    /// in a recursive cycle checks the stack first, as a record call does.
    fn plain_call(&self, site: &CallSite, direct: &str) -> String {
        let plain = format!(
            "match {direct} {{ Ok(v) => Ok(v), Err(e) => Err(vm.plain_unwound(fr, {}, e)) }}",
            site.callee
        );
        if self.recursive_bodies.contains(&site.object) {
            format!("match vm.stack_room() {{ Ok(()) => {plain}, Err(e) => Err(e) }}")
        } else {
            plain
        }
    }

    /// Route a compiled call's error. A callee that suspends parks this
    /// body's live values on the fiber stack, which the collector scans.
    fn call_code(&mut self, block: BlockId, call: &str) -> Gen<String> {
        self.fallible(block, call)
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

/// The native key of `Array.push`, which compiled code makes in place on a
/// list of known element type.
const ARRAY_PUSH: &str = "baml.Array.push";

/// A call of a builtin, as its glue is called.
struct Builtin<'a> {
    name: &'a str,
    info: &'a Function,
    shape: crate::analysis::CallShape,
    types: &'a [String],
}

/// What every form of a direct call to one compiled callee names.
struct CallSite {
    object: usize,
    symbol: String,
    /// The callee's function object, as generated code loads it.
    callee: String,
    pc: String,
    type_args: String,
    generic: bool,
}

impl CallSite {
    /// Whether the VM allows a call with a frame record.
    fn allowed(&self) -> String {
        if self.generic {
            format!("vm.direct_calls_typed({})", self.type_args)
        } else {
            "vm.direct_calls()".to_string()
        }
    }

    /// The call of `direct` with a frame record; telemetry boxes the
    /// arguments only for an invocation it records.
    fn recorded(&self, direct: &str, args: &[(String, Repr)]) -> String {
        let observed: Vec<String> = args
            .iter()
            .map(|(arg, repr)| coerce(arg, *repr, Repr::Value))
            .collect();
        let observed = format!(
            "|vm: &mut BexVm| -> [Value; {}] {{ [{}] }}",
            observed.len(),
            observed.join(", ")
        );
        let Self {
            callee,
            pc,
            type_args,
            ..
        } = self;
        if self.generic {
            format!(
                "vm.call_record_typed(fr, {callee}, {type_args}, {pc}, {observed}, |vm, fr| {direct})"
            )
        } else {
            format!("vm.call_record(fr, {callee}, {pc}, {observed}, |vm, fr| {direct})")
        }
    }

    /// The call through the boxed entry, with a VM frame, its result
    /// unboxed to the typed entry's return representation.
    fn boxed(&self, args: &[(String, Repr)], ret: Repr) -> String {
        let mut boxed = String::from("{ ");
        let mut values = Vec::with_capacity(args.len());
        for (j, (arg, repr)) in args.iter().enumerate() {
            let _ = write!(boxed, "let v{j} = {}; ", coerce(arg, *repr, Repr::Value));
            values.push(format!("v{j}"));
        }
        let unbox = match ret {
            Repr::Int => "bex_vm::compiled_rt::expect_int(v)",
            Repr::Bool => "bex_vm::compiled_rt::expect_bool(v)",
            Repr::Float => "bex_vm::compiled_rt::expect_f64(v)",
            Repr::Value => "Ok(v)",
        };
        let Self {
            callee,
            symbol,
            pc,
            type_args,
            ..
        } = self;
        let _ = write!(
            boxed,
            "match vm.call_compiled(fr, {callee}, {symbol}, {type_args}, &[{}], {pc}) {{ Ok(v) => {unbox}, Err(e) => Err(e) }} }}",
            values.join(", ")
        );
        boxed
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
