//! Calls through a function value.
//!
//! A function with parameters that have defaults can stand where a type
//! with fewer of them is expected, so a call through a value does not know
//! how many parameters its callee has, or in what order. It knows its own
//! layout, the slots it was checked against, and the callee's layout is in
//! the callee: a number in the header of every callable, and so in the tag
//! of every function value, which the call already has in a register. The
//! call compares the two numbers. Where they are the same, its arguments
//! are the callee's parameters and it enters the callee's code.
//!
//! Where they differ the call goes through a remap thunk, a function made
//! here for that pair of layouts: it takes the call's arguments, lays them
//! out as the callee's parameters, each named slot the call does not have
//! passed as left to its default, and calls the same callable. The
//! program is compiled whole, so the layouts a call can meet are known:
//! those of the function values the program makes that the call's layout
//! can be laid over. The function value itself is never replaced.
//!
//! A call that dispatches an interface method checks nothing. It pushes
//! the layout of the interface's method, and the row of each type holds
//! code entered with exactly that: the type's implementation where it
//! declares no more, and otherwise a thunk made here that calls the
//! implementation with each parameter the interface does not have left to
//! its default.

use std::collections::HashMap;

use baml_type::CallLayout;
use cranelift_codegen::{
    Context,
    ir::{self, InstBuilder, MemFlags, condcodes::IntCC},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use super::{
    Backend, Declared, Runtime, TRAP_LAYOUT, WORD, closure_access, imm, internal, offset, unwind,
};
use crate::{
    CompileError, layout,
    program::{Entry, FnKey},
};

/// What a call through a function value knows of itself.
pub(super) struct ValueSite {
    /// The number of the layout its arguments are in.
    layout: usize,
    /// Each other layout a callee of the call can have, by its number,
    /// with the thunk that enters such a callee from this call.
    remaps: Vec<(usize, ir::FuncRef)>,
    /// What says which two layouts met, where a callee has one the call
    /// cannot enter: only code compiled with checks says, and such code
    /// also tells what it enters the number of its layout.
    report: Option<ir::FuncRef>,
}

/// The thunks made so far.
#[derive(Default)]
pub(super) struct Remaps {
    /// The number of the layout of each function value the program makes:
    /// of every function compiled to be entered through one.
    pub(super) of_values: Vec<usize>,
    /// Each thunk, by the numbers of its callee's layout and of its
    /// caller's, in the order they were first needed.
    thunks: Vec<((usize, usize), FuncId)>,
    /// Each thunk a row holds in place of an implementation, by the
    /// implementation and the number of the layout the row is entered
    /// with.
    of_methods: Vec<((FuncId, usize), FuncId)>,
    /// Whether the thunks are compiled: none can be asked for after.
    defined: bool,
}

impl Backend<'_> {
    /// A call through a function value in `function`, checked against
    /// `layout`.
    pub(super) fn value_site(
        &mut self,
        layout: &baml_type::CallLayout,
        function: &mut ir::Function,
    ) -> Result<ValueSite, CompileError> {
        let report = if self.checks {
            let report = self.runtime(Runtime::LayoutMismatch)?;
            Some(self.function_ref(report, function))
        } else {
            None
        };
        let site = self.layout_id(layout)?;
        let mut remaps = Vec::new();
        for callee in self.remaps.of_values.clone() {
            if callee != site && layout.map_to(&self.layouts[callee - 1]).is_ok() {
                let thunk = self.remap_thunk(callee, site)?;
                remaps.push((callee, self.function_ref(thunk, function)));
            }
        }
        Ok(ValueSite {
            layout: site,
            remaps,
            report,
        })
    }

    /// The thunk a call in the layout numbered `site` enters a callable of
    /// the layout numbered `callee` through: entered as a function value
    /// of the call's layout is, with the callable as its closure.
    fn remap_thunk(&mut self, callee: usize, site: usize) -> Result<FuncId, CompileError> {
        if let Some((_, thunk)) = self
            .remaps
            .thunks
            .iter()
            .find(|(pair, _)| *pair == (callee, site))
        {
            return Ok(*thunk);
        }
        self.thunks_open()?;
        let signature =
            Self::clif_signature(self.layouts[site - 1].len(), Entry::Value, 0, self.checks);
        let thunk = self
            .module
            .declare_function(
                &format!("baml_remap_{callee}_from_{site}"),
                Linkage::Local,
                &signature,
            )
            .map_err(internal)?;
        self.remaps.thunks.push(((callee, site), thunk));
        Ok(thunk)
    }

    fn thunks_open(&self) -> Result<(), CompileError> {
        if self.remaps.defined {
            return Err(internal(
                "a thunk is first needed after the thunks are compiled",
            ));
        }
        Ok(())
    }

    /// What the row of a type holds for an interface method whose calls
    /// push the layout `site`, the receiver's slot first, where the type's
    /// implementation is `implementation`, whose own layout is `declared`:
    /// the implementation, or a thunk entered with the call's layout.
    pub(super) fn row_code(
        &mut self,
        implementation: FuncId,
        declared: &CallLayout,
        site: &CallLayout,
    ) -> Result<FuncId, CompileError> {
        if declared == site {
            return Ok(implementation);
        }
        let key = (implementation, self.layout_id(site)?);
        if let Some((_, thunk)) = self
            .remaps
            .of_methods
            .iter()
            .find(|(known, _)| *known == key)
        {
            return Ok(*thunk);
        }
        self.thunks_open()?;
        let signature = Self::clif_signature(site.len(), Entry::Direct, 0, false);
        let thunk = self
            .module
            .declare_function(
                &format!("baml_method_{}_from_{}", implementation.as_u32(), key.1),
                Linkage::Local,
                &signature,
            )
            .map_err(internal)?;
        self.remaps.of_methods.push((key, thunk));
        Ok(thunk)
    }
}

/// Compile every thunk declared. Called once no call is left to compile
/// and every row knows what it holds.
///
/// A thunk is an ordinary function: it has a frame record, and calls and
/// returns, so the collector and the unwinder walk past it as past any
/// frame. Nothing of its own is live across its call: what it was passed
/// is the callee's from there on. It has no name in a trace.
pub(super) fn define_thunks<'db>(
    backend: &mut Backend<'db>,
    declared: &HashMap<FnKey<'db>, Declared<'db>>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    backend.remaps.defined = true;
    let laid_over = |site: &CallLayout, callee: &CallLayout| {
        site.map_to(callee)
            .map_err(|_| internal("a thunk is for a layout its caller's can be laid over"))
    };
    for ((callee_id, site), thunk) in backend.remaps.thunks.clone() {
        let (callee, site) = (&backend.layouts[callee_id - 1], &backend.layouts[site - 1]);
        let slots = laid_over(site, callee)?;
        let listed = format!("remap {} from {}", described(callee), described(site));
        context.func.signature =
            Backend::clif_signature(site.len(), Entry::Value, 0, backend.checks);
        let callee = Callee::Closure(backend.checks.then_some(callee_id));
        thunk_body(&slots, &callee, &mut context.func, builder_context);
        let named = super::Named::Listed(&listed);
        super::define(backend, context, thunk, &named, clif, disassembly)?;
    }
    for ((implementation, site), thunk) in backend.remaps.of_methods.clone() {
        let callee = declared
            .values()
            .find(|function| function.id == implementation)
            .ok_or_else(|| internal("an implementation is part of the program"))?;
        let site = &backend.layouts[site - 1];
        let slots = laid_over(site, &callee.layout)?;
        let listed = format!(
            "method {} from {}",
            described(&callee.layout),
            described(site)
        );
        context.func.signature = Backend::clif_signature(site.len(), Entry::Direct, 0, false);
        let implementation = backend.function_ref(implementation, &mut context.func);
        thunk_body(
            &slots,
            &Callee::Implementation(implementation),
            &mut context.func,
            builder_context,
        );
        let named = super::Named::Listed(&listed);
        super::define(backend, context, thunk, &named, clif, disassembly)?;
    }
    Ok(())
}

/// A layout as a listing shows it: `(_, _, y?)`.
fn described(layout: &CallLayout) -> String {
    let slots: Vec<String> = layout
        .0
        .iter()
        .map(|slot| {
            slot.as_ref()
                .map_or("_".to_owned(), |name| format!("{name}?"))
        })
        .collect();
    format!("({})", slots.join(", "))
}

/// What a thunk calls.
enum Callee {
    /// The callable it was passed as its closure, which it passes on as
    /// the callee's. In code compiled with checks it was passed the number
    /// of its caller's layout after that, and passes this one in its
    /// place: the number of the layout it lays the arguments out for.
    Closure(Option<usize>),
    /// A function it names: the implementation of an interface method.
    Implementation(ir::FuncRef),
}

/// The body of a thunk. `slots` says, for each parameter of `callee`,
/// which of the arguments the thunk was passed fills it, if any does.
fn thunk_body(
    slots: &[Option<usize>],
    callee: &Callee,
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) {
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let passed = builder.block_params(block).to_vec();
    // What comes before the values, here and in the call.
    let (mut arguments, passed) = match callee {
        Callee::Implementation(_) => (Vec::new(), &passed[..]),
        Callee::Closure(None) => (vec![passed[0]], &passed[1..]),
        Callee::Closure(Some(layout)) => {
            let layout = builder.ins().iconst(WORD, imm(*layout));
            (vec![passed[0], layout], &passed[2..])
        }
    };
    for slot in slots {
        match slot {
            Some(at) => arguments.extend([passed[2 * at], passed[2 * at + 1]]),
            None => {
                let omitted = builder.ins().iconst(WORD, imm(layout::TAG_OMITTED));
                let nothing = builder.ins().iconst(WORD, 0);
                arguments.extend([omitted, nothing]);
            }
        }
    }
    let call = match callee {
        Callee::Implementation(implementation) => builder.ins().call(*implementation, &arguments),
        Callee::Closure(said) => {
            let code = builder.ins().load(
                WORD,
                closure_access(),
                arguments[0],
                offset(layout::CLOSURE_CODE),
            );
            let signature = Backend::clif_signature(slots.len(), Entry::Value, 0, said.is_some());
            let signature = builder.import_signature(signature);
            builder.ins().call_indirect(signature, code, &arguments)
        }
    };
    let returned = builder.inst_results(call).to_vec();
    builder.ins().return_(&returned);
    builder.seal_all_blocks();
    builder.finalize();
}

/// The number of the call layout in `tag`, the tag of a function value or
/// the header of a callable: the bits above the type id, of which a tag
/// has no other and a header none while compiled code runs.
pub(super) fn layout_of(builder: &mut FunctionBuilder<'_>, tag: ir::Value) -> ir::Value {
    builder
        .ins()
        .ushr_imm(tag, i64::from(layout::HEADER_PAYLOAD_SHIFT))
}

/// Call the callable at `closure`, whose call layout is numbered `callee`,
/// with `words`: the tag and the payload of each argument of the call
/// `site`, in the call's own layout. Returns the tag and the payload of
/// the result. `handlers` are the handlers the call is under, if any, and
/// `access` how the closure's code address is read.
///
/// The callee is entered where its layout is the call's. A callee of
/// another layout the call can be laid over is entered through the thunk
/// for the two, found by comparing the callee's number with each such
/// layout's. Any other is not one this call can enter: the program stops.
pub(super) fn call_value(
    builder: &mut FunctionBuilder<'_>,
    site: &ValueSite,
    (callee, closure): (ir::Value, ir::Value),
    words: &[ir::Value],
    access: MemFlags,
    handlers: Option<&[unwind::Edge]>,
) -> (ir::Value, ir::Value) {
    // Code compiled with checks tells what it enters its layout.
    let says = site.report.is_some();
    let mut arguments = vec![closure];
    if says {
        arguments.push(builder.ins().iconst(WORD, imm(site.layout)));
    }
    arguments.extend(words);
    let fits = builder
        .ins()
        .icmp_imm(IntCC::Equal, callee, imm(site.layout));
    let enter = builder.create_block();
    let other = builder.create_block();
    let done = builder.create_block();
    builder.append_block_param(done, WORD);
    builder.append_block_param(done, WORD);
    builder.set_cold_block(other);
    builder.ins().brif(fits, enter, &[], other, &[]);

    builder.switch_to_block(enter);
    let code = builder
        .ins()
        .load(WORD, access, closure, offset(layout::CLOSURE_CODE));
    let signature = Backend::clif_signature(words.len() / 2, Entry::Value, 0, says);
    let signature = builder.import_signature(signature);
    let (tag, payload) = unwind::call(
        builder,
        &unwind::Called::Indirect(signature, code),
        &arguments,
        handlers,
    );
    builder.ins().jump(done, &[tag.into(), payload.into()]);

    builder.switch_to_block(other);
    for (layout, thunk) in &site.remaps {
        let through = builder.create_block();
        let next = builder.create_block();
        let is = builder.ins().icmp_imm(IntCC::Equal, callee, imm(*layout));
        builder.ins().brif(is, through, &[], next, &[]);
        builder.switch_to_block(through);
        let (tag, payload) = unwind::call(
            builder,
            &unwind::Called::Direct(*thunk),
            &arguments,
            handlers,
        );
        builder.ins().jump(done, &[tag.into(), payload.into()]);
        builder.switch_to_block(next);
    }
    if let Some(report) = site.report {
        let site = builder.ins().iconst(WORD, imm(site.layout));
        builder.ins().call(report, &[callee, site]);
    }
    builder.ins().trap(TRAP_LAYOUT);

    builder.switch_to_block(done);
    let returned = builder.block_params(done);
    (returned[0], returned[1])
}
