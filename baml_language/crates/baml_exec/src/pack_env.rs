//! The environment variables a packed program reads, which a publisher may
//! rename in `[pack.env_var_names]`.

use btel_settings::artifact::PACK_ENVVAR_FIELDS;

/// Read by the packed host under these names, which a publisher cannot
/// rename, so no binding may take them.
const FIXED: &[&str] = &[
    "BOUNDARY_API_URL",
    crate::CONTEXT_ENV,
    "BAML_HOME",
    "BAML_COMPILED_BODIES",
];

/// The names a binding in `own` may not take: every other binding's standard
/// name and the name the manifest renames it to, and the fixed variables.
/// Compared ignoring case: a program packed on a case-sensitive host may run
/// on Windows, where differently cased names alias.
pub(crate) fn taken_names<'a>(names: Option<&'a toml::Table>, own: &[&str]) -> Vec<&'a str> {
    let others = PACK_ENVVAR_FIELDS
        .iter()
        .copied()
        .filter(|key| !own.contains(key));
    let renamed = others.clone().filter_map(|key| names?.get(key)?.as_str());
    others.chain(FIXED.iter().copied()).chain(renamed).collect()
}

pub(crate) fn valid_env_var(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_'))
}
