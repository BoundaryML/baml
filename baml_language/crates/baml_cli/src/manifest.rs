//! Typed deserialization of the project manifest, `baml.toml`.
//!
//! Historically the manifest was poked at as a raw `toml::Table` in two
//! scattered places — `project_load.rs` for `[package]` and
//! `run_command.rs` for `[scripts]`. This module replaces that with a
//! single set of serde structs so every consumer parses the same way.
//!
//! Design notes:
//! - **Warn on unknown fields, don't deny.** serde has no built-in "warn",
//!   so each table that should surface typos carries a `#[serde(flatten)]`
//!   catch-all map; [`unknown_field_warnings`] turns those into advisories.
//!   `deny_unknown_fields` would hard-error (breaking forward-compat
//!   manifests) and the default would silently swallow typos — the
//!   catch-all is the middle ground. (`flatten` and `deny_unknown_fields`
//!   are mutually exclusive anyway.)
//! - **`toml::Spanned` for diagnostics.** Generator field values are wrapped
//!   in `Spanned<String>` so codegen validation (`generate.rs`) can point a
//!   diagnostic at the exact byte range of an offending value, matching the
//!   per-item span fidelity the old HIR source map gave us.

use indexmap::IndexMap;
use serde::Deserialize;
use toml::Spanned;

/// The whole `baml.toml`.
#[derive(Debug, Deserialize)]
pub(crate) struct BamlToml {
    /// `[package]`. Optional at the type level so the lenient
    /// introspection / `[scripts]` paths can parse a manifest that hasn't
    /// declared a package; the strict loader still requires it (see
    /// [`package_name`]).
    pub package: Option<Package>,

    /// `[scripts]` — cargo-style aliases for `baml run`.
    #[serde(default)]
    pub scripts: IndexMap<String, Script>,

    /// `[generator.<name>]` subtables. The table key is the generator name.
    #[serde(default)]
    pub generator: IndexMap<String, Spanned<GeneratorManifest>>,

    /// `[test]` — saved `baml test` invocations.
    #[serde(default)]
    pub test: TestManifest,

    /// Stray top-level keys. Captured (not denied) so typos surface as
    /// warnings and forward-compatible manifests still load.
    #[serde(flatten)]
    pub unknown: IndexMap<String, toml::Value>,
}

/// Test profiles deliberately store argv rather than duplicating the test
/// command's option schema. They are parsed by clap at invocation time.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct TestManifest {
    /// Profile used by bare `baml test`. No profile is applied when absent.
    pub default: Option<String>,

    /// `[test.profiles.<name>]` tables.
    #[serde(default)]
    pub profiles: IndexMap<String, TestProfileManifest>,

    #[serde(flatten)]
    pub unknown: IndexMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TestProfileManifest {
    /// Argument vector passed through the ordinary `baml test` parser. This is
    /// intentionally an array, never a shell command string.
    #[serde(default)]
    pub args: Vec<String>,

    #[serde(flatten)]
    pub unknown: IndexMap<String, toml::Value>,
}

/// Top-level keys that are valid in `baml.toml` but are *not* consumed by
/// this internal binary, so they legitimately land in [`BamlToml::unknown`].
/// They must not be flagged as typos. Currently just `toolchain` (typically a
/// `[toolchain]` table, e.g. `channel = "nightly"`), which the `baml` wrapper
/// reads to pick a toolchain version *before* exec'ing this binary — by the
/// time we parse the manifest the choice is already made, so there is nothing
/// here to act on, only a key to not warn about. Matching is by key name, so
/// both the `[toolchain]` table and a bare `toolchain = "…"` are covered.
const KNOWN_UNHANDLED_TOP_LEVEL_KEYS: &[&str] = &["toolchain"];

#[derive(Debug, Deserialize)]
pub(crate) struct Package {
    /// `[package].name`. Optional here so a missing name produces our own
    /// guidance-rich error rather than a bare serde "missing field".
    pub name: Option<String>,

    #[serde(flatten)]
    pub unknown: IndexMap<String, toml::Value>,
}

/// A `[scripts]` entry. Mirrors Cargo's `[alias]`: a bare string is
/// whitespace-tokenized, an array is taken verbatim (so values can contain
/// spaces).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub(crate) enum Script {
    /// `dev = "-f main"` — split on whitespace.
    Line(String),
    /// `dev = ["-f", "main"]` — each element is one argument.
    Tokens(Vec<String>),
}

impl Script {
    /// Expand into `baml run` argument tokens.
    pub fn tokens(&self) -> Vec<String> {
        match self {
            Script::Line(s) => s.split_whitespace().map(str::to_string).collect(),
            Script::Tokens(t) => t.clone(),
        }
    }
}

/// `[generator.<name>]` — code-generation configuration. Values are kept as
/// raw `Spanned<String>` here; the string→enum validation (and the spans for
/// any diagnostics) is performed by `generate.rs`, so non-codegen tooling
/// never needs to know codegen rules.
#[derive(Debug, Deserialize)]
pub(crate) struct GeneratorManifest {
    /// e.g. `"python/pydantic"`, `"typescript/node"`, `"go"`. Required for codegen;
    /// `Option` so a missing value yields a precise diagnostic rather than
    /// aborting the whole parse.
    pub output_type: Option<Spanned<String>>,

    /// e.g. `"preserve-case"`, `"language"`. Required for codegen.
    pub naming_convention: Option<Spanned<String>>,

    /// Output directory, resolved relative to the project. Defaults to
    /// `".."` when omitted.
    #[serde(default)]
    pub output_dir: Option<String>,

    /// Import path of the generated SDK root. Required only by Go because
    /// generated subpackages must import one another by module path.
    pub sdk_import_path: Option<Spanned<String>>,

    /// Maximum non-null union arity represented as a closed generated Go
    /// union. Larger unions use `any`. Go-only; defaults to 3.
    pub max_typed_union_arity: Option<Spanned<i64>>,

    #[serde(flatten)]
    pub unknown: IndexMap<String, toml::Value>,
}

/// Parse `baml.toml` text into the typed manifest.
pub(crate) fn parse(content: &str) -> Result<BamlToml, toml::de::Error> {
    toml::from_str(content)
}

/// Resolve and validate `[package].name`, reproducing the Cargo-style rule
/// that a manifest, once written, must name its package. Returns the name so
/// `baml pack` can reuse it for artifact naming.
pub(crate) fn package_name(
    manifest: &BamlToml,
    toml_path: &std::path::Path,
) -> anyhow::Result<String> {
    let package = manifest.package.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "{}: missing `[package]` table.\n\
             Add:\n\n    [package]\n    name = \"<your-project-name>\"\n",
            toml_path.display()
        )
    })?;
    let name = package.name.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "{}: `[package]` is missing `name = \"<your-project-name>\"`.",
            toml_path.display()
        )
    })?;
    if name.trim().is_empty() {
        anyhow::bail!("{}: `[package].name` cannot be empty.", toml_path.display());
    }
    Ok(name.clone())
}

/// Human-readable warnings for keys we didn't recognize, so a typo
/// (`[scriptz]`, `nmae = ...`, `outpt_type = ...`) surfaces instead of being
/// silently ignored. Each warning is non-fatal — forward-compatible
/// manifests must still load.
pub(crate) fn unknown_field_warnings(manifest: &BamlToml) -> Vec<String> {
    let mut warnings = Vec::new();
    for key in manifest.unknown.keys() {
        if KNOWN_UNHANDLED_TOP_LEVEL_KEYS.contains(&key.as_str()) {
            continue;
        }
        warnings.push(format!(
            "ignoring unrecognized top-level key `{key}` in baml.toml"
        ));
    }
    if let Some(pkg) = &manifest.package {
        for key in pkg.unknown.keys() {
            warnings.push(format!("ignoring unrecognized key `{key}` in [package]"));
        }
    }
    for (name, generator) in &manifest.generator {
        for key in generator.get_ref().unknown.keys() {
            warnings.push(format!(
                "ignoring unrecognized key `{key}` in [generator.{name}]"
            ));
        }
    }
    for key in manifest.test.unknown.keys() {
        warnings.push(format!("ignoring unrecognized key `{key}` in [test]"));
    }
    for (name, profile) in &manifest.test.profiles {
        for key in profile.unknown.keys() {
            warnings.push(format!(
                "ignoring unrecognized key `{key}` in [test.profiles.{name}]"
            ));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_and_scripts() {
        let m = parse("[package]\nname = \"app\"\n[scripts]\ndev = \"-f main\"\n").unwrap();
        assert_eq!(m.package.unwrap().name.as_deref(), Some("app"));
        assert_eq!(m.scripts["dev"].tokens(), vec!["-f", "main"]);
    }

    #[test]
    fn script_array_form_preserves_spaces() {
        let m = parse("[scripts]\ng = [\"--name\", \"Ada Lovelace\"]\n").unwrap();
        assert_eq!(m.scripts["g"].tokens(), vec!["--name", "Ada Lovelace"]);
    }

    #[test]
    fn parses_test_profiles_as_argv_arrays() {
        let m = parse(
            "[test]\ndefault = \"regular\"\n\
             [test.profiles.regular]\nargs = [\"-x\", \"*::integration::*\"]\n",
        )
        .unwrap();
        assert_eq!(m.test.default.as_deref(), Some("regular"));
        assert_eq!(
            m.test.profiles["regular"].args,
            vec!["-x", "*::integration::*"]
        );
    }

    #[test]
    fn rejects_profile_args_as_a_shell_string() {
        assert!(parse("[test.profiles.regular]\nargs = \"-x '*::integration::*'\"\n").is_err());
    }

    #[test]
    fn parsing_succeeds_with_unexpected_key() {
        // An unrecognized key must not fail deserialization: forward-compatible
        // manifests still load. The key is captured for a warning (see
        // `unknown_field_warnings`), never denied.
        let result = parse("[package]\nname = \"app\"\nfuture_feature = true\n");
        assert!(
            result.is_ok(),
            "manifest with an unexpected key should parse, got: {result:?}"
        );
    }

    #[test]
    fn captures_unknown_top_level_and_package_keys() {
        let m = parse("[package]\nname = \"a\"\nnmae = \"typo\"\n[wat]\nx = 1\n").unwrap();
        let warns = unknown_field_warnings(&m);
        assert!(warns.iter().any(|w| w.contains("nmae")), "got: {warns:?}");
        assert!(warns.iter().any(|w| w.contains("wat")), "got: {warns:?}");
    }

    #[test]
    fn toolchain_key_is_known_and_not_warned() {
        // `[toolchain]` is consumed by the `baml` wrapper, not this binary, so
        // it lands in `unknown` — but it's a legitimate key, not a typo, and
        // must not produce a warning. This mirrors a real manifest: a
        // `[toolchain]` table sitting alongside `[package]`. A genuine
        // top-level typo (`nmae`, before any table header) still warns.
        let m = parse(
            "nmae = \"typo\"\n\n[package]\nname = \"a\"\n\n[toolchain]\nchannel = \"nightly\"\n",
        )
        .unwrap();
        let warns = unknown_field_warnings(&m);
        assert!(
            !warns.iter().any(|w| w.contains("toolchain")),
            "toolchain must not warn, got: {warns:?}"
        );
        assert!(warns.iter().any(|w| w.contains("nmae")), "got: {warns:?}");
    }

    #[test]
    fn parses_generator_section_with_spans() {
        let m = parse(
            "[package]\nname = \"a\"\n\
             [generator.lang_python]\n\
             output_type = \"python/pydantic\"\n\
             naming_convention = \"preserve-case\"\n\
             output_dir = \"../python\"\n\
             sdk_import_path = \"example.com/project/baml_sdk\"\n",
        )
        .unwrap();
        let g = &m.generator["lang_python"];
        assert_eq!(
            g.get_ref().output_type.as_ref().unwrap().get_ref(),
            "python/pydantic"
        );
        assert_eq!(g.get_ref().output_dir.as_deref(), Some("../python"));
        assert_eq!(
            g.get_ref().sdk_import_path.as_ref().unwrap().get_ref(),
            "example.com/project/baml_sdk"
        );
    }

    #[test]
    fn package_name_requires_table_and_name() {
        let path = std::path::Path::new("baml.toml");
        assert!(
            package_name(&parse("[scripts]\n").unwrap(), path)
                .unwrap_err()
                .to_string()
                .contains("[package]")
        );
        assert!(
            package_name(&parse("[package]\n").unwrap(), path)
                .unwrap_err()
                .to_string()
                .contains("name")
        );
        assert!(
            package_name(&parse("[package]\nname = \"\"\n").unwrap(), path)
                .unwrap_err()
                .to_string()
                .contains("cannot be empty")
        );
    }

    #[test]
    fn parses_zero_go_union_threshold_with_a_value_span() {
        let manifest = parse(
            "[generator.go]\noutput_type = \"go\"\nnaming_convention = \"language\"\nmax_typed_union_arity = 0\n",
        )
        .unwrap();
        let threshold = manifest.generator["go"]
            .get_ref()
            .max_typed_union_arity
            .as_ref()
            .unwrap();
        assert_eq!(*threshold.get_ref(), 0);
        assert!(threshold.span().end > threshold.span().start);
    }
}
