use baml_compiler2_emit::OptLevel;

pub(crate) const OPT_LEVEL: OptLevel = OptLevel::One;

pub(crate) fn artifact_key() -> String {
    let opt_level = match OPT_LEVEL {
        OptLevel::Zero => "zero",
        OptLevel::One => "one",
        OptLevel::Two => "two",
    };
    format!(
        "bex-project-stdlib-prefix-v2:version={}:channel={}:opt={opt_level}",
        baml_version::CANONICAL_VERSION,
        baml_version::CHANNEL,
    )
}
