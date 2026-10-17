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
    analysis::{Repr, builtin_is_callable, static_realized},
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
            return unsupported("call through a function value");
        };
        let name = function_display_name(self.db, *func);
        let Some((object, info)) = self.index.function(self.db, *func)? else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        // Type arguments must be known statically: a non-generic caller has
        // no frame type arguments to substitute.
        let mut types = Vec::with_capacity(type_args.len());
        let mut primitive = true;
        for arg in type_args {
            let ty = match arg {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                    self.analysis.static_types.get(&local.0).map(|ty| {
                        primitive &= ty.primitive;
                        ty.code.clone()
                    })
                }
                Operand::Constant(_) | Operand::Copy(_) | Operand::Move(_) => None,
            };
            let Some(ty) = ty else {
                return unsupported(format!(
                    "call to `{name}` with a type argument known only at runtime"
                ));
            };
            types.push(ty);
        }
        let values = self.laid_out_args(block, &name, info, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => {
                if !types.is_empty() {
                    return unsupported(format!("call to generic function `{name}`"));
                }
                let Some(symbol) = self
                    .symbols
                    .get(&object)
                    .filter(|_| !self.resumable.contains(&object))
                else {
                    return Err(Rejection::Invalid(format!(
                        "suspending call to `{name}` emitted inline"
                    )));
                };
                self.fallible(
                    block,
                    &format!(
                        "vm.call_compiled(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {symbol}, {args}, line)"
                    ),
                )
            }
            FunctionKind::NativeUnresolved | FunctionKind::Native(_) => {
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported(format!("builtin `{name}` has no native key"));
                };
                if !builtin_is_callable(key, !types.is_empty() && primitive) {
                    return unsupported(format!("builtin `{key}` may yield"));
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
            FunctionKind::SysOp(_) => unsupported(format!("`{name}` is a sys op")),
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
        self.statement(0, &format!("let callee = {code};"));
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

    /// The `RealizedTy` expressions of a call's type-argument operands, each
    /// a local holding a statically known type; `what` names the construct
    /// in the rejection otherwise. A non-generic caller has no frame type
    /// arguments to substitute, so a type known only at run time keeps the
    /// function bytecode.
    pub(super) fn static_type_operands(
        &self,
        what: &str,
        operands: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        let mut types = Vec::with_capacity(operands.len());
        for operand in operands {
            let ty = match operand {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => self
                    .analysis
                    .static_types
                    .get(&local.0)
                    .map(|ty| ty.code.clone()),
                _ => None,
            };
            let Some(ty) = ty else {
                return unsupported(format!("{what} with a type argument known only at runtime"));
            };
            types.push(ty);
        }
        Ok(types)
    }

    /// The operands of a dispatch through `iface`: the interface object's
    /// pointer and the `&[RealizedTy]` of its instantiation. Both are known
    /// at compile time when the interface is in the program and every
    /// argument is a static type; associated types are outputs of the
    /// implementation, not part of its selection, so they are not spelled.
    pub(super) fn static_interface(&self, iface: &TyTemplateInterface) -> Gen<(String, String)> {
        let name = decl_display_name(self.db, &iface.name);
        let Some(object) = self.index.interface_at(&iface.name)? else {
            return unsupported(format!("interface `{name}` not found in the program"));
        };
        let mut args = Vec::with_capacity(iface.generics.len());
        for template in &iface.generics {
            let Some(ty) = static_realized(self.index, template) else {
                return unsupported(format!(
                    "interface `{name}` instantiated with a type known only at runtime"
                ));
            };
            args.push(ty);
        }
        Ok((
            format!("vm.idx_to_ptr(ObjectIndex::from_raw({object}))"),
            format!("&[{}]", args.join(", ")),
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
        depth: usize,
        iface: &TyTemplateInterface,
        method: &str,
        args: &[Operand<'db>],
        ntypeargs: usize,
        self_arg: usize,
        layout: Option<&baml_type::CallLayout>,
        destination: &Place,
        target: BlockId,
    ) -> Gen<String> {
        let (iface, iface_args) = self.static_interface(iface)?;
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        let types = self.static_type_operands("interface method call", type_args)?;
        let values = self.arg_values(block, args)?;
        let Some(receiver) = values.get(self_arg) else {
            return Err(Rejection::Invalid(format!(
                "virtual call dispatches on argument {self_arg} of {}",
                values.len()
            )));
        };
        // The slots this site pushes, for the VM to lay over the resolved
        // callee's; a site that pushes the method's own slots needs none,
        // and only such a site calls an inline callee in place.
        let layout = match layout {
            Some(layout) if layout.0.iter().any(Option::is_some) => {
                let slots: Vec<String> = layout
                    .0
                    .iter()
                    .map(|slot| match slot {
                        Some(name) => format!("Some({:?})", name.as_str()),
                        None => "None".to_string(),
                    })
                    .collect();
                Some(format!("Some(&[{}])", slots.join(", ")))
            }
            _ => None,
        };
        let site = format!("S{}", block.0);
        self.statement(
            depth,
            &format!(
                "static {site}: bex_vm::compiled_rt::VirtualSite = bex_vm::compiled_rt::VirtualSite::new();"
            ),
        );
        self.statement(
            depth,
            &format!(
                "let callee = vm.virtual_target(&{site}, {receiver}, {iface}, {iface_args}, {method:?}, &[{}])?;",
                types.join(", ")
            ),
        );
        let layout = match layout {
            Some(layout) => layout,
            None => {
                let call = self.fallible(
                    block,
                    &format!("vm.call_inline(callee, &[{}], line)", values.join(", ")),
                )?;
                let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
                let jump = self.jump_to(target.0);
                self.line(depth, &format!("if let Some(v) = {call} {{"));
                self.line(depth + 1, &store);
                self.line(depth + 1, &jump);
                self.line(depth, "}");
                "None".to_string()
            }
        };
        self.statement(
            depth,
            &format!(
                "let argc = vm.push_virtual_args(callee, {layout}, &[{}])?;",
                values.join(", ")
            ),
        );
        Ok("Step::Call { callee, argc }".to_string())
    }
}
