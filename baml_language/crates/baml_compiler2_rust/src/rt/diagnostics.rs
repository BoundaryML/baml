pub(super) fn no(what: impl std::fmt::Display) -> String {
    format!("unsupported in baml_rt: {what}")
}
