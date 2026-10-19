//! Calls: an inline call's expression, the callee and arguments of a
//! request a resumable body hands the VM, and the static operands of an
//! interface dispatch.

use baml_compiler2_mir::{
    BlockId, Constant, Operand, Place, TyTemplateInterface, decl_display_name,
    function_display_name,
};
use bex_vm_types::FunctionKind;

use super::{Expr, Gen, Generator, unsupported};
use crate::{
    admit::{FunctionInfo, Rejection},
    analysis::{Repr, builtin_is_callable, call_shape, static_realized},
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
                let Some(symbol) = self
                    .symbols
                    .get(&object)
                    .filter(|_| !self.resumable.contains(&object))
                else {
                    return Err(Rejection::Invalid(format!(
                        "suspending call to `{name}` emitted inline"
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
                if self.may_collect.contains(&object) {
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
                let next = self.natives.len();
                let ordinal = *self.natives.entry(key.to_owned()).or_insert(next);
                if types.is_empty() {
                    self.fallible(
                        block,
                        &format!("vm.call_builtin(native_{ordinal}(), {args})"),
                    )
                } else {
                    self.fallible(
                        block,
                        &format!(
                            "vm.call_builtin_typed(native_{ordinal}(), &[{}], {args})",
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

    /// Bind `callee` as `let callee: HeapPtr`: a named function's object, or
    /// a function value's.
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
                let value = self.operand(block, callee)?;
                let value = self.value_arg(&value.code, value.repr);
                format!("bex_vm::compiled_rt::callee_ptr({value})?")
            }
        };
        self.statement(&format!("let callee = {code};"));
        Ok(())
    }

    /// The argument `Value`s of a call to `name` (the callee `info`
    /// describes), in the callee's slot order: a call with named or omitted
    /// optionals is laid over the callee's declared slots at compile time, as
    /// the VM's `remap_call_arguments` does at run time, with `OMITTED_ARG`
    /// where the callee's default applies.
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

    /// The comma-separated `Value`s of a request's arguments.
    pub(super) fn arg_values(&mut self, block: BlockId, args: &[Operand<'db>]) -> Gen<Vec<String>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        Ok(values)
    }

    /// Type operands (type locals) as `RealizedTy` expressions, and whether
    /// every one is known statically: a type built from the frame's type
    /// arguments, or read from a pooled constant, is built where it is used.
    pub(super) fn type_operands(
        &mut self,
        what: &str,
        operands: &[Operand<'db>],
    ) -> Gen<(Vec<String>, bool)> {
        let mut types = Vec::with_capacity(operands.len());
        let mut fixed = true;
        for operand in operands {
            let local = match operand {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => local.0,
                _ => {
                    return unsupported(format!(
                        "{what} with a type argument that is not a type local"
                    ));
                }
            };
            if let Some(ty) = self.analysis.static_types.get(&local) {
                types.push(ty.code.clone());
            } else if let Some(template) = self.analysis.frame_types.get(&local).cloned() {
                fixed = false;
                types.push(self.realized(&template)?);
            } else {
                return unsupported(format!(
                    "{what} with a type argument that is not a type local"
                ));
            }
        }
        Ok((types, fixed))
    }

    /// The operands of a dispatch through `iface`: the interface object's
    /// pointer, the `&[RealizedTy]` of its instantiation, and whether that
    /// instantiation is known statically (else it is built from the frame or
    /// a pooled constant where the dispatch runs). Associated types are
    /// outputs of the implementation, not part of its selection, so they are
    /// not spelled.
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

    /// The request of an interface method call in a resumable body: bind
    /// `callee` to the method's implementation for the receiver through the
    /// site's inline cache (a `static` of this body, which also pushes the
    /// call's type arguments); when that callee is a compiled body that runs
    /// to completion, call it in place and continue at `target`; otherwise
    /// push the value arguments laid over the callee's slots and return the
    /// `Step` expression. `args` are the method's type arguments then its
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
        destination: &Place,
        target: BlockId,
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
        // The slots this site pushes, for the VM to lay over the resolved
        // callee's: an implementation may declare optionals the interface
        // method does not, which the VM fills with `OMITTED_ARG` even when
        // the site names none. Only a site naming no optional calls an
        // inline callee in place, which `call_inline` declines unless the
        // callee's arity is the site's.
        let named = layout.is_some_and(|layout| layout.0.iter().any(Option::is_some));
        let layout = layout.map(|layout| {
            let slots: Vec<String> = layout
                .0
                .iter()
                .map(|slot| match slot {
                    Some(name) => format!("Some({:?})", name.as_str()),
                    None => "None".to_string(),
                })
                .collect();
            format!("Some(&[{}])", slots.join(", "))
        });
        // The site's cache keys on the receiver alone, so only a dispatch
        // whose instantiation is the same on every pass uses it.
        if iface_fixed && types_fixed {
            let site = format!("S{}", block.0);
            self.statement(&format!(
                    "static {site}: bex_vm::compiled_rt::VirtualSite = bex_vm::compiled_rt::VirtualSite::new();"
                ),
            );
            self.statement(&format!(
                    "let callee = vm.virtual_target(&{site}, {receiver}, {iface}, {iface_args}, {method:?}, &[{}])?;",
                    types.join(", ")
                ),
            );
        } else {
            self.statement(&format!(
                    "let callee = vm.virtual_target_uncached({receiver}, {iface}, {iface_args}, {method:?}, &[{}])?;",
                    types.join(", ")
                ),
            );
        }
        // A traced call reaches the VM's call path, which applies the
        // attachment, so it is never run in place.
        if !named && !traced {
            let site = self.site_pc(block);
            let call = self.rooted_call(
                block,
                &format!("vm.call_inline(callee, &[{}], {site})", values.join(", ")),
            )?;
            self.flush();
            let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
            let jump = self.jump_to(target.0);
            self.line(&format!("if let Some(v) = {call} {{"));
            self.line(&store);
            self.line(&jump);
            self.line("}");
        }
        let layout = layout.unwrap_or_else(|| "None".to_string());
        self.statement(&format!(
            "let argc = vm.push_virtual_args(callee, {layout}, &[{}])?;",
            values.join(", ")
        ));
        Ok("Step::Call { callee, argc }".to_string())
    }
}
