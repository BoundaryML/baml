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

use baml_type::CallLayout;
use cranelift_codegen::{
    Context,
    ir::{self, InstBuilder, MemFlags, condcodes::IntCC},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Linkage, Module};

use super::{Backend, Runtime, TRAP_LAYOUT, WORD, closure_access, imm, internal, offset, unwind};
use crate::{CompileError, layout, program::Entry};

/// What a call through a function value knows of itself.
pub(super) struct ValueSite {
    /// The number of the layout its arguments are in.
    layout: usize,
    /// Each other layout a callee of the call can have, by its number,
    /// with the thunk that enters such a callee from this call.
    remaps: Vec<(usize, ir::FuncRef)>,
    /// What says which two layouts met, where a callee has one the call
    /// cannot enter: only a build with checks says.
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
        let signature = Self::clif_signature(self.layouts[site - 1].len(), Entry::Value, 0);
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
}

/// Compile every thunk declared. Called once no call is left to compile.
///
/// A thunk is an ordinary function: it has a frame record, and calls and
/// returns, so the collector and the unwinder walk past it as past any
/// frame. Nothing of its own is live across its call: what it was passed
/// is the callee's from there on. It has no name in a trace.
pub(super) fn define_remaps(
    backend: &mut Backend<'_>,
    context: &mut Context,
    builder_context: &mut FunctionBuilderContext,
    clif: &mut Option<String>,
    disassembly: &mut Option<String>,
) -> Result<(), CompileError> {
    for ((callee, site), thunk) in backend.remaps.thunks.clone() {
        let (callee_layout, site_layout) =
            (&backend.layouts[callee - 1], &backend.layouts[site - 1]);
        let slots = site_layout
            .map_to(callee_layout)
            .map_err(|_| internal("a thunk is for a layout its caller's can be laid over"))?;
        let listed = format!(
            "remap {} from {}",
            described(callee_layout),
            described(site_layout)
        );
        context.func.signature = Backend::clif_signature(site_layout.len(), Entry::Value, 0);
        remap_body(&slots, &mut context.func, builder_context);
        super::define(
            backend,
            context,
            thunk,
            &super::Named::Listed(&listed),
            clif,
            disassembly,
        )?;
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

/// The body of a thunk: the closure it was passed is the callable, and
/// `slots` says, for each parameter of the callable, which of the
/// arguments the thunk was passed fills it, if any does.
fn remap_body(
    slots: &[Option<usize>],
    function: &mut ir::Function,
    builder_context: &mut FunctionBuilderContext,
) {
    let mut builder = FunctionBuilder::new(function, builder_context);
    let block = builder.create_block();
    builder.append_block_params_for_function_params(block);
    builder.switch_to_block(block);
    let passed = builder.block_params(block).to_vec();
    let closure = passed[0];
    let mut arguments = vec![closure];
    for slot in slots {
        match slot {
            Some(at) => arguments.extend([passed[1 + 2 * at], passed[2 + 2 * at]]),
            None => {
                let omitted = builder.ins().iconst(WORD, imm(layout::TAG_OMITTED));
                let nothing = builder.ins().iconst(WORD, 0);
                arguments.extend([omitted, nothing]);
            }
        }
    }
    let code = builder.ins().load(
        WORD,
        closure_access(),
        closure,
        offset(layout::CLOSURE_CODE),
    );
    let signature = Backend::clif_signature(slots.len(), Entry::Value, 0);
    let signature = builder.import_signature(signature);
    let call = builder.ins().call_indirect(signature, code, &arguments);
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
    let signature = Backend::clif_signature(words.len() / 2, Entry::Value, 0);
    let signature = builder.import_signature(signature);
    let mut arguments = vec![closure];
    arguments.extend(words);
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
