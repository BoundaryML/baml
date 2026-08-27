//! Argument slots: `f(<here>)`.
//!
//! An optional parameter is named-only (the ruling: a defaulted parameter is
//! a kwarg, never positional), so the labels offered here are exactly the
//! callee's optional parameters that the call has not already spelled. The
//! parameter list comes from the callee's INFERRED type, not from a
//! declaration looked up by name — a callee that is a local holding a
//! function value works the same way.

use baml_base::SourceFile;
use baml_type::interned::InferTy;

use super::completions::Completions;
use crate::resolve::CallPosition;

pub(crate) fn complete(
    db: &dyn baml_compiler2_ppir::Db,
    file: SourceFile,
    call: &CallPosition,
    out: &mut Completions,
) {
    let InferTy::Function { params, .. } = call.callee.kind() else {
        return;
    };
    for param in params {
        let Some(name) = &param.name else {
            continue;
        };
        if param.mode != baml_type::FunctionParamMode::Optional {
            continue;
        }
        if call.written.iter().any(|written| written == name) {
            continue;
        }
        out.add_argument_label(db, file, name, &param.ty);
    }
}
