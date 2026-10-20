//! Generates inline calls, suspension requests, and interface dispatch operands.

use baml_compiler2_mir::{
    BlockId, Constant, Operand, TyTemplateInterface, decl_display_name, function_display_name,
};
use bex_vm_types::FunctionKind;

use super::{Gen, Generator, unsupported};
use crate::{
    admit::{FunctionInfo, Rejection},
    analysis::{builtin_is_callable, call_shape, static_realized},
};

impl<'db> Generator<'_, 'db> {
    pub(super) fn call(
        &mut self,
        block: BlockId,
        callee: &Operand<'db>,
        args: &[Operand<'db>],
        ntypeargs: usize,
        layout: Option<&baml_type::CallLayout>,
    ) -> Gen<String> {
        let Operand::Constant(Constant::Function(func)) = callee else {
            return Err(Rejection::Invalid(
                "call through a function value emitted inline".into(),
            ));
        };
        let name = function_display_name(self.db, *func);
        let Some((object, info)) = self.index.function(self.db, *func)? else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        let shape = call_shape(self.body, args, ntypeargs, &self.analysis.static_types);
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let (types, _) = self.call_type_args(type_args)?;
        let values = self.laid_out_args(block, &name, info, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => {
                let Some(symbol) = self.symbols.get(&object) else {
                    return Err(Rejection::Invalid(format!(
                        "call to uncompiled `{name}` emitted directly"
                    )));
                };
                // A generic callee's frame starts with the call's type
                // arguments (a plain function carries none of its own).
                let site = self.site_pc(block);
                let call = if types.is_empty() {
                    format!(
                        "vm.call_compiled(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {symbol}, {args}, {site})"
                    )
                } else {
                    format!(
                        "vm.call_compiled_with_types(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {symbol}, &[{}], {args}, {site})",
                        types.join(", ")
                    )
                };
                if self.pausing.contains(&object) {
                    self.rooted_call(block, &call)
                } else {
                    self.fallible(block, &call)
                }
            }
            FunctionKind::NativeUnresolved | FunctionKind::Native(_) => {
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported(format!("builtin `{name}` has no native key"));
                };
                if !builtin_is_callable(key, shape) {
                    return Err(Rejection::Invalid(format!(
                        "yielding builtin `{key}` emitted inline"
                    )));
                }
                let next = self.builtins.len();
                let ordinal = *self.builtins.entry(key.to_owned()).or_insert(next);
                if types.is_empty() {
                    self.fallible(
                        block,
                        &format!("vm.call_builtin(builtin_{ordinal}(), {args})"),
                    )
                } else {
                    self.fallible(
                        block,
                        &format!(
                            "vm.call_builtin_typed(builtin_{ordinal}(), &[{}], {args})",
                            types.join(", ")
                        ),
                    )
                }
            }
            FunctionKind::SysOp(_) => Err(Rejection::Invalid(format!(
                "sys op `{name}` emitted inline"
            ))),
        }
    }

    pub(super) fn callee_ptr(&mut self, block: BlockId, callee: &Operand<'db>) -> Gen<()> {
        let code = match callee {
            Operand::Constant(Constant::Function(func)) => {
                let Some((object, _)) = self.index.function(self.db, *func)? else {
                    let name = function_display_name(self.db, *func);
                    return unsupported(format!("callee `{name}` not found in the program"));
                };
                format!("vm.idx_to_ptr(ObjectIndex::from_raw({object}))")
            }
            Operand::Constant(_) => {
                return unsupported("call of a constant that is not a function");
            }
            Operand::Copy(_) | Operand::Move(_) => {
                let value = self.value_operand(block, callee)?;
                format!("bex_vm::compiled_rt::callee_ptr({value})?")
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
        info: &FunctionInfo,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        let values = match layout {
            Some(layout) if layout.0.iter().any(Option::is_some) => layout
                .map_to(&info.layout)
                .map_err(|mismatch| {
                    Rejection::Invalid(format!("call to `{name}` does not fit it: {mismatch:?}"))
                })?
                .into_iter()
                .map(|slot| {
                    slot.map_or_else(|| "Value::OMITTED_ARG".to_string(), |i| values[i].clone())
                })
                .collect(),
            _ => values,
        };
        if values.len() != info.arity {
            return Err(Rejection::Invalid(format!(
                "`{name}` called with {} of {} arguments",
                values.len(),
                info.arity
            )));
        }
        Ok(values)
    }

    pub(super) fn arg_values(&mut self, block: BlockId, args: &[Operand<'db>]) -> Gen<Vec<String>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        Ok(values)
    }

    /// Realize type operands and report whether all are static.
    pub(super) fn type_operands(
        &mut self,
        what: &str,
        operands: &[Operand<'db>],
    ) -> Gen<(Vec<String>, bool)> {
        let mut types = Vec::with_capacity(operands.len());
        let mut fixed = true;
        for operand in operands {
            let (code, primitive) = self.type_operand(what, operand)?;
            fixed &= primitive.is_some();
            types.push(code);
        }
        Ok((types, fixed))
    }

    /// Associated types are implementation outputs and do not select the implementation.
    pub(super) fn interface_operands(
        &mut self,
        iface: &TyTemplateInterface,
    ) -> Gen<(String, String, bool)> {
        let name = decl_display_name(self.db, &iface.name);
        let Some(object) = self.index.interface_at(&iface.name)? else {
            return unsupported(format!("interface `{name}` not found in the program"));
        };
        let mut args = Vec::with_capacity(iface.generics.len());
        let mut fixed = true;
        for template in &iface.generics {
            match static_realized(self.index, template) {
                Some(ty) => args.push(ty),
                None => {
                    fixed = false;
                    args.push(self.realized(template)?);
                }
            }
        }
        Ok((
            format!("vm.idx_to_ptr(ObjectIndex::from_raw({object}))"),
            format!("&[{}]", args.join(", ")),
            fixed,
        ))
    }

    /// An `R<Value>` expression for an interface method call: bind `callee` to
    /// the method's implementation for the receiver through the site's inline
    /// cache (a `static` of this body, which also pushes the call's type
    /// arguments); a compiled implementation with a matching arity runs in
    /// place, anything else is requested from the VM with the value arguments
    /// laid over its slots. `args` are the method's type arguments then its
    /// value arguments in declared order; `self_arg` indexes the receiver
    /// among the value arguments.
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
        let (iface, iface_args, iface_fixed) = self.interface_operands(iface)?;
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let (types, types_fixed) = self.type_operands("interface method call", type_args)?;
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
            |layout| {
                let slots: Vec<String> = layout
                    .0
                    .iter()
                    .map(|slot| match slot {
                        Some(name) => format!("Some({:?})", name.as_str()),
                        None => "None".to_string(),
                    })
                    .collect();
                format!("Some(&[{}])", slots.join(", "))
            },
        );
        // The receiver-only cache is valid only for a static instantiation.
        if iface_fixed && types_fixed {
            let site = format!("S{}", block.0);
            self.emit_statement(&format!(
                    "static {site}: bex_vm::compiled_rt::VirtualSite = bex_vm::compiled_rt::VirtualSite::new();"
                ),
            );
            self.emit_statement(&format!(
                    "let callee = vm.virtual_target(&{site}, {receiver}, {iface}, {iface_args}, {method:?}, &[{}])?;",
                    types.join(", ")
                ),
            );
        } else {
            self.emit_statement(&format!(
                    "let callee = vm.virtual_target_uncached({receiver}, {iface}, {iface_args}, {method:?}, &[{}])?;",
                    types.join(", ")
                ),
            );
        }
        let site = self.site_pc(block);
        let values = values.join(", ");
        let suspend = format!(
            "{{ let argc = vm.push_virtual_args(callee, {layout}, &[{values}])?; \
             vm.note_pc(depth, {site}); vm.suspend(Step::Call {{ callee, argc }}) }}"
        );
        // The VM must apply trace attachments, so traced calls cannot run inline.
        if named || traced {
            return Ok(suspend);
        }
        Ok(format!(
            "match vm.call_inline(callee, &[{values}], {site}) {{ \
             Ok(Some(v)) => Ok(v), Ok(None) => {suspend}, Err(e) => Err(e) }}"
        ))
    }
}
