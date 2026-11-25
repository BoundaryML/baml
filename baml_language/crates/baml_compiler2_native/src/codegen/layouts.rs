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

use cranelift_codegen::ir::{self, InstBuilder, MemFlags, condcodes::IntCC};
use cranelift_frontend::FunctionBuilder;

use super::{Backend, Runtime, TRAP_LAYOUT, WORD, imm, offset, unwind};
use crate::{CompileError, layout, program::Entry};

/// What a call through a function value knows of itself.
pub(super) struct ValueSite {
    /// The number of the layout its arguments are in.
    layout: usize,
    /// What says which two layouts met, where a callee has one the call
    /// cannot enter: only a build with checks says.
    report: Option<ir::FuncRef>,
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
        Ok(ValueSite {
            layout: self.layout_id(layout)?,
            report,
        })
    }
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
/// The callee is entered only where its layout is the call's. Any other
/// is not one this call can enter: the program stops.
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
    if let Some(report) = site.report {
        let site = builder.ins().iconst(WORD, imm(site.layout));
        builder.ins().call(report, &[callee, site]);
    }
    builder.ins().trap(TRAP_LAYOUT);

    builder.switch_to_block(done);
    let returned = builder.block_params(done);
    (returned[0], returned[1])
}
