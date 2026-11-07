pub(super) fn no(what: impl std::fmt::Display) -> String {
    format!("unsupported in runtime v2: {what}")
}
