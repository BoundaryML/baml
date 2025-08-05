use crate::hir::Hir;
use crate::thir::THir;
use internal_baml_diagnostics::Diagnostics;

pub fn typecheck(hir: &Hir, diagnostics: &mut Diagnostics) -> THir {
    let llm_functions = hir.llm_functions.clone();
    let classes = hir.classes.clone();
    let enums = hir.enums.clone();

    THir {
        llm_functions,
        classes,
        enums,
    }
}
