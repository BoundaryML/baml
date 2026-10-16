//! Calls: an inline call's expression, and the callee and arguments of a
//! request a resumable body hands the VM.

use baml_compiler2_mir::{BlockId, Constant, Operand, Place, function_display_name};
use bex_vm_types::FunctionKind;

use super::{Gen, Generator, unsupported};
use crate::{admit::Rejection, analysis::builtin_is_callable};

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
        let Some(info) = self.index.functions.get(&name) else {
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
        let values = self.laid_out_args(block, &name, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => {
                if !types.is_empty() {
                    return unsupported(format!("call to generic function `{name}`"));
                }
                let Some(symbol) = self
                    .symbols
                    .get(&name)
                    .filter(|_| !self.resumable.contains(&name))
                else {
                    return Err(Rejection::Invalid(format!(
                        "suspending call to `{name}` emitted inline"
                    )));
                };
                let object = self
                    .index
                    .objects
                    .get(&name)
                    .copied()
                    .ok_or_else(|| Rejection::Invalid(format!("`{name}` has no object")))?;
                self.fallible(
                    block,
                    &format!(
                        "vm.call_compiled(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {symbol}, {args})"
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
                let name = function_display_name(self.db, *func);
                let Some(object) = self.index.objects.get(&name) else {
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

    /// The argument `Value`s of a call to `name`, in the callee's slot order:
    /// a call with named or omitted optionals is laid over the callee's
    /// declared slots at compile time, as the VM's `remap_call_arguments` does
    /// at run time, with `OMITTED_ARG` where the callee's default applies.
    pub(super) fn laid_out_args(
        &mut self,
        block: BlockId,
        name: &str,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        let Some(info) = self.index.functions.get(name) else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
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
}
