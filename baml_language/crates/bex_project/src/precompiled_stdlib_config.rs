pub(crate) fn artifact_key() -> String {
    format!(
        "bex-project-stdlib-interfaces-v3:version={}:channel={}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}
