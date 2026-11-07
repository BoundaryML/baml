pub(super) fn no(what: impl std::fmt::Display) -> String {
    format!("unsupported in runtime v2: {what}")
}
// Explicit diagnostic builds alone emit work-boundary markers. Ordinary packs
// have identical source and runtime behavior when this switch is absent.
pub(super) fn diagnostic_barrier() -> &'static str {
    if baml_env::os_var("BAML_V2_DIAGNOSTIC_CODEGEN").is_some() {
        "diag_boundary_work();\n"
    } else {
        ""
    }
}
