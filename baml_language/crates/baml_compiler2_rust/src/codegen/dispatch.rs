//! Interface calls dispatched by the receiver's class in compiled code.
//!
//! Replaces, for a call site whose every implementation is a compiled method
//! of a known class ([`crate::analysis::dispatch_targets`]), the resolution
//! through `vm.virtual_target` and the VM's call path (`vm.call_inline` or a
//! suspension): the site compares the receiver's class word with each
//! implementor's and calls its body directly, plain where it can. A receiver
//! of any other class (one made at run time) takes the VM's path.

use std::fmt::Write as _;

use baml_compiler2_mir::{BlockId, Terminator};

use super::{Expr, Gen, Generator};
use crate::{analysis::Repr, render::coerce};

impl<'db> Generator<'_, 'db> {
    /// Emit `block`'s virtual call as a dispatch on the receiver's class, when
    /// every target is compiled; `false` leaves it to the VM's path.
    pub(super) fn devirtualized(&mut self, block: BlockId) -> Gen<bool> {
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        let Some(targets) = crate::analysis::dispatch_targets(self.index, terminator) else {
            return Ok(false);
        };
        let Some(compiled) = self.compiled_targets(&targets) else {
            // The site counted as not suspending; its body no longer is plain.
            self.suspends_late = true;
            return Ok(false);
        };
        let Terminator::VirtualCall { args, .. } = terminator else {
            unreachable!("dispatch targets are a virtual call's");
        };
        let (destination, _) = self.continuation(block)?;
        let args = args
            .iter()
            .map(|arg| self.operand(block, arg))
            .collect::<Gen<Vec<Expr>>>()?;
        let receiver = self.value_arg(&args[0]);
        self.flush_bindings();
        let repr = match &destination {
            baml_compiler2_mir::Place::Local(local) => self.repr(*local),
            _ => Repr::Value,
        };
        let mut code = format!("let w = bex_vm::compiled_rt::instance_class_word({receiver}); ");
        for (class, object, symbol) in compiled {
            let slots = args.iter().cloned().map(Some).collect();
            let call = self.entry_call(block, object, &symbol, slots, &[])?;
            let bindings = std::mem::take(&mut self.pending).concat();
            let word = self.class_word(class);
            let value = coerce(&call.code, call.repr, repr);
            let _ = write!(code, "if w == {word} {{ {bindings} {value} }} else ");
        }
        let fallback = self.checked_dispatch(block, terminator, &destination, repr)?;
        let _ = write!(code, "{{ {} }}", coerce(&fallback, Repr::Value, repr));
        let result = format!("r{}", self.temps);
        self.temps += 1;
        self.emit_statement(&format!(
            "let {result}: {} = {{ {code} }};",
            repr.rust_type()
        ));
        let store = self.store(block, &destination, &Expr::atom(result, repr))?;
        self.emit_statement(&store);
        Ok(true)
    }

    /// [`Self::vm_dispatch`], whose target is not one of the compiled ones:
    /// its result enters, checked when `destination` trusts its class.
    fn checked_dispatch(
        &mut self,
        block: BlockId,
        terminator: &Terminator<'db>,
        destination: &baml_compiler2_mir::Place,
        repr: Repr,
    ) -> Gen<String> {
        let fallback = self.vm_dispatch(block, terminator)?;
        let class = match destination {
            baml_compiler2_mir::Place::Local(local) if repr == Repr::Value => {
                self.local_class(*local)
            }
            _ => None,
        };
        match class {
            Some(class) => self.entering(block, &fallback, class),
            None => Ok(fallback),
        }
    }

    /// The targets with their bodies' symbols, if every one is compiled.
    fn compiled_targets(&self, targets: &[(usize, usize)]) -> Option<Vec<(usize, usize, String)>> {
        targets
            .iter()
            .map(|&(class, object)| Some((class, object, self.symbols.get(&object)?.clone())))
            .collect()
    }

    /// The VM's dispatch for `terminator` as one block expression of the
    /// call's `Value`: the statements its request emits stay inside it.
    fn vm_dispatch(&mut self, block: BlockId, terminator: &Terminator<'db>) -> Gen<String> {
        let outer = std::mem::take(&mut self.out);
        let request = self.request(block, terminator);
        let value = request.and_then(|request| self.fallible(block, &request));
        let bindings = std::mem::take(&mut self.pending).concat();
        let statements = std::mem::replace(&mut self.out, outer);
        Ok(format!("{{ {statements}{bindings} {} }}", value?))
    }
}
