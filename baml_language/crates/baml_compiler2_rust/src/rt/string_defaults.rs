//! Preserve the omitted argument until regex.new's ordinary BAML prologue.
//! Its high-level input capture must distinguish omission from explicit false.
use super::*;
pub(super) fn parameter(plan: &Plan<'_>, object: usize, index: usize) -> Option<Repr> {
    (index == 2
        && plan.index.display(object) == "baml.regex.new"
        && plan.index.info(object).param_has_default.get(1) == Some(&true))
    .then_some(Repr::Tagged)
}
pub(super) struct Feature;
impl registry::Feature for Feature {
    fn value(
        &self,
        e: &Emitter<'_, '_>,
        value: &Rvalue<'_>,
        _expected: Repr,
    ) -> Option<Gen<(String, Repr)>> {
        if e.plan.index.display(e.object) != "baml.regex.new" {
            return None;
        }
        let Rvalue::BinaryOp {
            op: op @ (BinOp::Eq | BinOp::Ne),
            left,
            right,
        } = value
        else {
            return None;
        };
        let other = if matches!(right, Operand::Constant(Constant::OmittedArg)) {
            left
        } else if matches!(left, Operand::Constant(Constant::OmittedArg)) {
            right
        } else {
            return None;
        };
        Some((|| {
            let value = e.coerce(e.operand(other)?, Repr::Tagged)?;
            Ok((
                format!(
                    "({value}).tag {} u64::MAX",
                    if *op == BinOp::Eq { "==" } else { "!=" }
                ),
                Repr::Bool,
            ))
        })())
    }
}
