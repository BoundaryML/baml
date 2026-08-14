//! Builtin `.baml` stub files for the compiler2 pipeline.
//!
//! All sources live under `baml_std/` and are embedded at compile time via
//! `include_str!` — no filesystem reads at runtime, works on both native and WASM.
//!
//! # Layout: folder tree = package + ns_* subfolders = namespace
//!
//! Everything lives under `baml_std/baml/` → package **baml**.
//! Sub-namespaces (env, llm, http, etc.) are expressed via `ns_*` subdirectories
//! on disk, and namespace is derived from path segments at runtime.
//!
//! # Virtual path
//!
//! Builtin virtual path is `<builtin>/<package>/<relative_path>`. The HIR derives
//! package and namespace from path segments (see `baml_compiler2_hir::file_package`).

/// A builtin `.baml` file: package, path within package, and embedded contents.
pub struct BuiltinFile {
    /// Package name (e.g. `"baml"`).
    pub package: &'static str,
    /// Relative path within the package directory (e.g. `"containers.baml"`,
    /// `"ns_env/env.baml"`). Namespace is derived from `ns_*` segments.
    pub relative_path: &'static str,
    /// File contents embedded at compile time via `include_str!`.
    pub contents: &'static str,
}

impl BuiltinFile {
    /// Build the virtual path for this builtin file.
    /// e.g. `"<builtin>/baml/ns_env/env.baml"`.
    pub fn virtual_path(&self) -> String {
        format!("<builtin>/{}/{}", self.package, self.relative_path)
    }

    /// Derive the namespace path from `ns_*` segments in `relative_path`.
    /// e.g. `"ns_env/env.baml"` → `["env"]`, `"containers.baml"` → `[]`.
    pub fn namespace_path(&self) -> Vec<&str> {
        let segments: Vec<&str> = self.relative_path.split('/').collect();
        if segments.len() <= 1 {
            return vec![];
        }
        // Exclude the filename (last segment), filter ns_* from intermediates
        segments[..segments.len() - 1]
            .iter()
            .filter_map(|s| s.strip_prefix("ns_"))
            .collect()
    }
}

/// Package name for the main std package (baml types and namespaces).
pub const PACKAGE_BAML: &str = "baml";
/// Package name for boundary identity and capture helpers.
pub const PACKAGE_BOUNDARY: &str = "boundary";

/// Absolute path to the `baml_std/` source tree, captured at compile time via
/// `CARGO_MANIFEST_DIR`. Used by `baml_builtins2_codegen` to produce clickable
/// file paths in build-script diagnostic messages (stderr only, never in
/// generated code or committed artifacts).
pub const BAML_STD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/baml_std");

/// YAML documentation for BAML language-reference topics, embedded at compile time.
pub const BAML_KEYWORDS_YAML: &str = include_str!("../keyword_docs/baml_keywords.yaml");

/// YAML crosswalk documentation for TypeScript/JS keywords, embedded at compile time.
pub const TS_KEYWORDS_YAML: &str = include_str!("../keyword_docs/ts_keywords.yaml");

mod language_docs;
pub use language_docs::{
    LanguageTopic, TypescriptCrosswalkTopic, has_describe_topic, language_topic, language_topics,
    typescript_crosswalk_topic, typescript_crosswalk_topics,
};

/// Builtin registration macro: package, relative virtual path, filesystem include path.
macro_rules! builtin {
    ($pkg:literal, $fs_path:literal) => {
        BuiltinFile {
            package: $pkg,
            relative_path: $fs_path,
            contents: include_str!(concat!("../baml_std/", $pkg, "/", $fs_path)),
        }
    };
}

/// All builtin `.baml` files, in registration order. Namespaces derived from
/// `ns_*` folder segments in `relative_path`.
pub const ALL: &[BuiltinFile] = &[
    // --- Root namespace (no ns_* prefix) ---
    builtin!("baml", "containers.baml"),
    builtin!("baml", "comparable.baml"),
    builtin!("baml", "conversions.baml"),
    builtin!("baml", "core.baml"),
    builtin!("baml", "int.baml"),
    builtin!("baml", "bigint.baml"),
    builtin!("baml", "float.baml"),
    builtin!("baml", "bool.baml"),
    builtin!("baml", "null.baml"),
    builtin!("baml", "string.baml"),
    builtin!("baml", "uint8array.baml"),
    builtin!("baml", "type_class.baml"),
    // --- Namespaced (ns_* folders) ---
    builtin!("baml", "ns_errors/errors.baml"),
    builtin!("baml", "ns_errors/unknown_error.baml"),
    builtin!("baml", "ns_errors/stack_trace.baml"),
    builtin!("baml", "ns_errors/error_context.baml"),
    builtin!("baml", "ns_panics/panics.baml"),
    builtin!("baml", "ns_env/env.baml"),
    builtin!("baml", "ns_io/io.baml"),
    builtin!("baml", "ns_http/http.baml"),
    builtin!("baml", "ns_http/server.baml"),
    builtin!("baml", "ns_events/events.baml"),
    builtin!("baml", "ns_id/id.baml"),
    builtin!("baml", "ns_sys/sys.baml"),
    builtin!("baml", "ns_fs/fs.baml"),
    builtin!("baml", "ns_glob/glob.baml"),
    builtin!("baml", "ns_net/net.baml"),
    builtin!("baml", "ns_media/media.baml"),
    builtin!("baml", "ns_json/json.baml"),
    builtin!("baml", "ns_yaml/yaml.baml"),
    builtin!("baml", "ns_toml/toml.baml"),
    builtin!("baml", "ns_csv/csv.baml"),
    builtin!("baml", "ns_prompt/prompt.baml"),
    builtin!("baml", "ns_prompt/sys_llm_types.baml"),
    builtin!("baml", "ns_sap/sap.baml"),
    builtin!("baml", "ns_ws/ws.baml"),
    builtin!("baml", "ns_iter/iter.baml"),
    builtin!("baml", "ns_future/future.baml"),
    builtin!("baml", "ns_spawn/spawn.baml"),
    builtin!("baml", "ns_host/host.baml"),
    builtin!("baml", "ns_time/duration.baml"),
    builtin!("baml", "ns_time/instant.baml"),
    builtin!("baml", "ns_time/timezone.baml"),
    builtin!("baml", "ns_time/plaintime.baml"),
    builtin!("baml", "ns_time/plaindate.baml"),
    builtin!("baml", "ns_time/plaindatetime.baml"),
    builtin!("baml", "ns_time/zoneddatetime.baml"),
    builtin!("baml", "ns_ops/bitwise.baml"),
    builtin!("baml", "ns_ops/comparison.baml"),
    builtin!("baml", "ns_ops/index.baml"),
    builtin!("baml", "ns_ops/math.baml"),
    builtin!("baml", "ns_random/random.baml"),
    // `baml.reflect` (BEP-066 I-9: `reflect` is a keyword shorthand for it).
    builtin!("baml", "ns_reflect/reflect.baml"),
    builtin!("baml", "ns_reflect/ns_class/class.baml"),
    builtin!("baml", "ns_reflect/ns_enum/enum.baml"),
    builtin!("baml", "ns_reflect/ns_union/union.baml"),
    builtin!("baml", "ns_reflect/ns_literal/literal.baml"),
    builtin!("baml", "ns_reflect/ns_array/array.baml"),
    builtin!("baml", "ns_reflect/ns_map/map.baml"),
    builtin!("baml", "ns_reflect/ns_interface/interface.baml"),
    builtin!("baml", "ns_reflect/ns_primitive/primitive.baml"),
    builtin!("baml", "ns_reflect/ns_function/function.baml"),
    builtin!("baml", "ns_reflect/ns_errors/errors.baml"),
    // `baml.type` (BEP-066 K-13: `type.of` / `type.of_value` resolve here).
    builtin!("baml", "ns_type/type.baml"),
    builtin!("baml", "ns_crypto/errors.baml"),
    builtin!("baml", "ns_crypto/interfaces.baml"),
    builtin!("baml", "ns_crypto/aes_gcm_siv.baml"),
    builtin!("baml", "ns_crypto/chacha20poly1305.baml"),
    builtin!("baml", "ns_crypto/sha2.baml"),
    // --- boundary package ---
    builtin!("boundary", "core.baml"),
    builtin!("boundary", "ns_id/id.baml"),
    // --- testing package ---
    builtin!("testing", "types.baml"),
    builtin!("testing", "registry.baml"),
    builtin!("testing", "runners.baml"),
    // --- assert package ---
    builtin!("assert", "assert.baml"),
    // --- log package ---
    builtin!("log", "log.baml"),
    // --- ai package (specs, journal, runner, client interface) ---
    builtin!("ai", "ns_content/content.baml"),
    builtin!("ai", "ns_events/events.baml"),
    builtin!("ai", "journal.baml"),
    builtin!("ai", "spec.baml"),
    builtin!("ai", "ns_tools/tools.baml"),
    builtin!("ai", "turn.baml"),
    builtin!("ai", "ns_wire/wire.baml"),
    builtin!("ai", "ns_clients/clients.baml"),
    builtin!("ai", "runner.baml"),
    builtin!("ai", "ns_stream/stream.baml"),
    builtin!("ai", "ns_errors/errors.baml"),
    builtin!("ai", "ns_internal/helpers.baml"),
    // --- provider client packages ---
    builtin!("openai", "responses.baml"),
    builtin!("openai", "ns_internal/responses.baml"),
    builtin!("anthropic", "messages.baml"),
    builtin!("anthropic", "ns_internal/messages.baml"),
    builtin!("google", "gemini.baml"),
    builtin!("google", "ns_internal/gemini.baml"),
    builtin!("claude_code", "cli.baml"),
    builtin!("claude_code", "ns_internal/cli.baml"),
    // ai.mcp: MCP servers as ordinary ai tools (part of the ai package).
    builtin!("ai", "ns_mcp/mcp.baml"),
];

/// The distinct standard-library / builtin package names, derived from the
/// embedded manifest [`ALL`] in first-appearance order.
///
/// This is the single authoritative answer to "which packages ship as
/// builtins": a package is a stdlib package iff it contributes at least one
/// file to `ALL` (i.e. it has a `<builtin>/<package>/…` source). There is no
/// hand-maintained parallel list to keep in sync — adding a package to `ALL`
/// automatically enrolls it here.
///
/// Every such package is a compiler-build constant (no user file can contribute
/// to it), so each one's typed `PackageInterface` is a pure function of stdlib
/// source + compiler code — the soundness foundation for caching it under the
/// compiler fingerprint and seeding it back (B-694). Callers that serialize
/// per-package data key it in a sorted map, so the first-appearance iteration
/// order here never leaks into stored bytes.
pub fn stdlib_package_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = Vec::new();
        for file in ALL {
            if !names.contains(&file.package) {
                names.push(file.package);
            }
        }
        names
    })
}

/// Every package name that user-provided mounts may not claim, in stable
/// first-appearance order: all builtin packages, followed by the implicit user
/// package and the two compiler-reserved package names.
///
/// This is the single source of truth shared by mount filtering and runtime
/// reflection, so both paths reject exactly the same aliases.
pub fn reserved_package_names() -> &'static [&'static str] {
    static NAMES: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        let mut names = stdlib_package_names().to_vec();
        names.extend([baml_type::RESERVED_USER_PACKAGE, "root", "env"]);
        names
    })
}

mod adt;
mod media;
pub use adt::*;
pub use media::{MediaContent, MediaValue};
