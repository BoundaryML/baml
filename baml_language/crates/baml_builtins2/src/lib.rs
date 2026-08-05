//! Builtin `.baml` stub files for the compiler2 pipeline.
//!
//! All sources live under `baml_std/` and are embedded at compile time via
//! `include_str!` — no filesystem reads at runtime, works on both native and WASM.
//!
//! # Layout: folder tree = package + ns_* subfolders = namespace
//!
//! Each first-level directory under `baml_std/` is a package (`baml`, `ai`,
//! `testing`, `assert`, etc.). Sub-namespaces are expressed via `ns_*`
//! subdirectories on disk, and namespace is derived from path segments at
//! runtime.
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
/// Package name for typed AI tasks, providers, agents, tools, and sessions.
pub const PACKAGE_AI: &str = "ai";

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
    builtin!("baml", "ns_schema/schema.baml"),
    builtin!("baml", "ns_yaml/yaml.baml"),
    builtin!("baml", "ns_toml/toml.baml"),
    builtin!("baml", "ns_csv/csv.baml"),
    builtin!("baml", "ns_llm/llm_types.baml"),
    builtin!("baml", "ns_llm/llm.baml"),
    builtin!("baml", "ns_sap/sap.baml"),
    builtin!("baml", "ns_ws/ws.baml"),
    builtin!("baml", "ns_stream/stream.baml"),
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
    builtin!("baml", "ns_ops/comparison.baml"),
    builtin!("baml", "ns_ops/math.baml"),
    builtin!("baml", "ns_random/random.baml"),
    // --- boundary package ---
    builtin!("boundary", "core.baml"),
    builtin!("boundary", "ns_id/id.baml"),
    // --- reflect package (standalone, accessible as `reflect.type_of(...)`) ---
    builtin!("reflect", "reflect.baml"),
    // --- testing package ---
    builtin!("testing", "types.baml"),
    builtin!("testing", "registry.baml"),
    builtin!("testing", "runners.baml"),
    // --- assert package ---
    builtin!("assert", "assert.baml"),
    // --- log package ---
    builtin!("log", "log.baml"),
    // --- ai package ---
    // Root namespace. Non-`ns_*` directories are organizational only.
    builtin!("ai", "execution/agent_outcome.baml"),
    builtin!("ai", "execution/prompt_recipe.baml"),
    builtin!("ai", "execution/resource.baml"),
    builtin!("ai", "execution/runner.baml"),
    builtin!("ai", "execution/task.baml"),
    builtin!("ai", "failures/defaults.baml"),
    builtin!("ai", "failures/protocol.baml"),
    builtin!("ai", "failures/unsupported.baml"),
    builtin!("ai", "messages/conversation.baml"),
    builtin!("ai", "messages/history.baml"),
    builtin!("ai", "messages/protocol.baml"),
    builtin!("ai", "provider/protocol.baml"),
    builtin!("ai", "provider/response.baml"),
    builtin!("ai", "reliability/fallback.baml"),
    builtin!("ai", "reliability/retry.baml"),
    // Namespaced public and internal modules.
    builtin!("ai", "ns_harness/model_harness.baml"),
    builtin!("ai", "ns_harness/models.baml"),
    builtin!("ai", "ns_internal/agent.baml"),
    builtin!("ai", "ns_internal/bridges.baml"),
    builtin!("ai", "ns_internal/client_provider.baml"),
    builtin!("ai", "ns_internal/conversation_append.baml"),
    builtin!("ai", "ns_internal/http.baml"),
    builtin!("ai", "ns_internal/replay.baml"),
    builtin!("ai", "ns_jobs/background.baml"),
    builtin!("ai", "ns_jobs/batch.baml"),
    builtin!("ai", "ns_messages/parts.baml"),
    builtin!("ai", "ns_messages/prompt_adapter.baml"),
    builtin!("ai", "ns_observe/events.baml"),
    builtin!("ai", "ns_observe/usage.baml"),
    builtin!("ai", "ns_realtime/audio.baml"),
    builtin!("ai", "ns_realtime/automatic_tools.baml"),
    builtin!("ai", "ns_realtime/collect.baml"),
    builtin!("ai", "ns_realtime/protocol.baml"),
    builtin!("ai", "ns_realtime/provider.baml"),
    builtin!("ai", "ns_run/agent.baml"),
    builtin!("ai", "ns_run/agent_session.baml"),
    builtin!("ai", "ns_run/background.baml"),
    builtin!("ai", "ns_run/batch.baml"),
    builtin!("ai", "ns_run/harness.baml"),
    builtin!("ai", "ns_run/streaming.baml"),
    builtin!("ai", "ns_run/transcription.baml"),
    builtin!("ai", "ns_run/voice_agent.baml"),
    builtin!("ai", "ns_testing/background.baml"),
    builtin!("ai", "ns_testing/batch.baml"),
    builtin!("ai", "ns_testing/fake_tools.baml"),
    builtin!("ai", "ns_testing/fakes.baml"),
    builtin!("ai", "ns_testing/realtime.baml"),
    builtin!("ai", "ns_testing/transcription.baml"),
    builtin!("ai", "ns_tools/callbacks.baml"),
    builtin!("ai", "ns_tools/models.baml"),
    builtin!("ai", "ns_transcription/audio_stream.baml"),
    builtin!("ai", "ns_transcription/protocol.baml"),
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

mod adt;
mod media;
pub use adt::*;
pub use media::{MediaContent, MediaValue};

#[cfg(test)]
mod tests {
    use super::{ALL, PACKAGE_AI, stdlib_package_names};

    #[test]
    fn ai_package_is_embedded_with_expected_namespaces() {
        let ai_files = ALL
            .iter()
            .filter(|file| file.package == PACKAGE_AI)
            .collect::<Vec<_>>();

        assert_eq!(ai_files.len(), 54);
        assert!(ai_files.iter().all(|file| !file.contents.is_empty()));
        assert!(ai_files.iter().any(|file| {
            file.relative_path == "execution/task.baml" && file.namespace_path().is_empty()
        }));
        assert!(ai_files.iter().any(|file| {
            file.relative_path == "ns_run/agent.baml" && file.namespace_path() == ["run"]
        }));
        assert!(ai_files.iter().any(|file| {
            file.relative_path == "ns_realtime/protocol.baml"
                && file.namespace_path() == ["realtime"]
        }));
    }

    #[test]
    fn ai_is_a_stdlib_package() {
        let names = stdlib_package_names();
        assert!(names.contains(&PACKAGE_AI));
        assert_eq!(names.iter().filter(|name| **name == PACKAGE_AI).count(), 1);
    }
}
