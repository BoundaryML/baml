//! cargo-stow: Workspace linting and structure validation for Rust monorepos.
//!
//! A cargo subcommand that validates workspace structure, enforces naming conventions,
//! and keeps dependencies organized.
//!
//! ## Features
//! - **Dependency sorting**: Keep deps organized (internal first, then external)
//! - **Structure validation**: Enforce flat crate layout, naming conventions
//! - **Dependency rules**: Control who can depend on what
//! - **Dependency graph**: Visualize workspace structure as SVG
//! - **Auto-fix**: Automatically fix sortable issues
//!
//! ## Rules
//! 1. No nested crates (flat structure only)
//! 2. Crate names must match folder names
//! 3. Crate names must be `<namespace>_<word>` or `<namespace>_<approved_prefix>_<word>`
//!    - Auto-allowed suffixes: `_types`, `_test`, `_tests` (no prefix needed)
//! 4. Crates ending in `_test` or `_tests` must have a corresponding prefix crate
//! 5. All dependencies must use `{ workspace = true }` format
//! 6. Dependency restrictions (configurable whitelist/blacklist per dependency)
//!    - Global rules apply to all namespaces
//!    - Per-namespace rules apply only to crates in that namespace
//!    - `_types` dependencies are auto-allowed (can be depended on by any crate)
//! 7. Dependencies must be sorted: internal deps first (sorted), then external deps (sorted)
//! 8. Internal deps must be grouped together (not interleaved with external deps)
//!
//! ## Usage
//! ```bash
//! cargo stow              # Validate (default)
//! cargo stow --fix        # Auto-fix sortable issues
//! cargo stow --graph out.svg  # Generate dependency graph
//! cargo stow init         # Generate config file
//! ```

// CLI tool - print statements and exit are expected
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fmt::Write,
    path::{Path, PathBuf},
};

use cargo_metadata::{MetadataCommand, PackageId};
use clap::Parser;
use graphviz_rust::{
    cmd::{CommandArg, Format},
    exec_dot,
};
use serde::Deserialize;
use toml_edit::{DocumentMut, Formatted, InlineTable, Item, Table, Value};

// =============================================================================
// CONFIGURATION
// =============================================================================

/// A namespace defines a crate prefix (e.g., "baml" or "bex") and its rules
#[derive(Debug, Clone, Deserialize)]
pub struct Namespace {
    /// The namespace name (e.g., "baml" for baml_* crates)
    pub name: String,
    /// Approved multi-word prefixes (e.g., "compiler" for `baml_compiler`_*)
    #[serde(default)]
    pub approved_prefixes: Vec<String>,
    /// Test crates exempt from "must have prefix crate" rule
    #[serde(default)]
    pub test_crate_exceptions: Vec<String>,
    /// Dependency rules that only apply to crates in this namespace
    #[serde(default)]
    pub dependency_rules: Vec<DependencyRule>,
    /// Name exceptions: allows a crate folder to have a different package name
    /// Key is folder name, value is allowed package name
    /// Example: { "`tools_stow`" = "cargo-stow" }
    #[serde(default)]
    pub name_exceptions: HashMap<String, String>,
}

/// Stow configuration - can be loaded from stow.toml or [workspace.metadata.stow]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    /// Crate namespaces (e.g., baml, bex)
    pub namespaces: Vec<Namespace>,
    /// Global dependency rules that apply to all namespaces
    #[serde(default)]
    pub dependency_rules: Vec<DependencyRule>,
    /// Crate name patterns to ignore entirely (e.g., "rig_*")
    #[serde(default)]
    pub ignore_crates: Vec<String>,
    /// External dependencies to include in the dependency graph visualization
    #[serde(default)]
    pub graph_external_deps: Vec<String>,

    // Legacy fields for backward compatibility (converted to single namespace)
    #[serde(default)]
    approved_prefixes: Vec<String>,
    #[serde(default)]
    test_crate_exceptions: Vec<String>,
}

/// Error type for configuration loading
#[derive(Debug)]
pub enum ConfigError {
    /// No configuration file found
    NotFound,
    /// Configuration file exists but failed to parse
    ParseError(String),
}

impl Config {
    /// Load config from stow.toml or [workspace.metadata.stow] in Cargo.toml
    /// Priority: stow.toml > Cargo.toml metadata
    /// Returns an error if no configuration is found.
    pub fn load(workspace_root: &Path) -> Result<Self, ConfigError> {
        // Try stow.toml first
        let stow_toml = workspace_root.join("stow.toml");
        if stow_toml.exists() {
            let content = std::fs::read_to_string(&stow_toml)
                .map_err(|e| ConfigError::ParseError(format!("Failed to read stow.toml: {e}")))?;
            let config = toml::from_str::<Config>(&content).map_err(|e| {
                ConfigError::ParseError(format!("Failed to parse {}: {e}", stow_toml.display()))
            })?;
            eprintln!("Loaded config from {}", stow_toml.display());
            return Ok(config.normalize());
        }

        // Try [workspace.metadata.stow] in Cargo.toml
        let cargo_toml = workspace_root.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if let Ok(doc) = content.parse::<toml::Table>() {
                    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
                        if let Some(metadata) = workspace.get("metadata").and_then(|m| m.as_table())
                        {
                            if let Some(stow) = metadata.get("stow") {
                                let config = stow.clone().try_into::<Config>().map_err(|e| {
                                    ConfigError::ParseError(format!(
                                        "Failed to parse [workspace.metadata.stow]: {e}"
                                    ))
                                })?;
                                eprintln!(
                                    "Loaded config from [workspace.metadata.stow] in {}",
                                    cargo_toml.display()
                                );
                                return Ok(config.normalize());
                            }
                        }
                    }
                }
            }
        }

        // No configuration found
        Err(ConfigError::NotFound)
    }

    /// Check if a stow.toml file already exists
    pub fn config_exists(workspace_root: &Path) -> bool {
        workspace_root.join("stow.toml").exists()
    }

    /// Normalize config by converting legacy flat format to namespace format
    fn normalize(mut self) -> Self {
        // If legacy fields are set and namespaces is empty, convert to namespace format
        if self.namespaces.is_empty() && !self.approved_prefixes.is_empty() {
            self.namespaces = vec![Namespace {
                name: "baml".into(),
                approved_prefixes: std::mem::take(&mut self.approved_prefixes),
                test_crate_exceptions: std::mem::take(&mut self.test_crate_exceptions),
                dependency_rules: vec![],
                name_exceptions: HashMap::new(),
            }];
        }
        self
    }

    /// Get the namespace for a given crate name (by prefix match)
    pub fn get_namespace(&self, crate_name: &str) -> Option<&Namespace> {
        self.namespaces
            .iter()
            .find(|ns| crate_name.starts_with(&format!("{}_", ns.name)))
    }

    /// Get the namespace for a crate, considering `name_exceptions`
    /// This checks both the standard prefix match AND if the crate name
    /// is listed as an exception in any namespace
    pub fn get_namespace_for_crate(&self, crate_name: &str) -> Option<&Namespace> {
        // First try standard prefix match
        if let Some(ns) = self.get_namespace(crate_name) {
            return Some(ns);
        }
        // Then check if it's a name exception in any namespace
        self.namespaces
            .iter()
            .find(|ns| ns.name_exceptions.values().any(|v| v == crate_name))
    }

    /// Check if a crate name belongs to any known namespace
    pub fn is_internal_crate(&self, crate_name: &str) -> bool {
        self.get_namespace_for_crate(crate_name).is_some()
    }

    /// Check if a crate should be ignored based on config patterns
    pub fn is_ignored_crate(&self, crate_name: &str) -> bool {
        self.ignore_crates
            .iter()
            .any(|pattern| glob_match(pattern, crate_name))
    }
}

/// Pattern for matching dependencies - can be a simple string or select/exclude
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Pattern {
    /// Simple glob pattern (e.g., "bex_*")
    Simple(String),
    /// Pattern with exclusions
    WithExclusions {
        select: String,
        #[serde(default)]
        exclude: Vec<String>,
    },
}

impl Pattern {
    fn matches(&self, dep_name: &str) -> bool {
        match self {
            Pattern::Simple(pattern) => glob_match(pattern, dep_name),
            Pattern::WithExclusions { select, exclude } => {
                // First check if it matches the select pattern
                if !glob_match(select, dep_name) {
                    return false;
                }
                // Then check if it's excluded
                for excl in exclude {
                    if glob_match(excl, dep_name) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

/// Dependency rules configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DependencyRule {
    /// Dependency name pattern (supports glob-style wildcards)
    pub pattern: Pattern,
    /// Allowed crate prefixes (e.g., "compiler" allows `baml_compiler`_*)
    #[serde(default)]
    pub allowed_prefixes: Vec<String>,
    /// Allowed crate names or patterns
    #[serde(default)]
    pub allowed_crates: Vec<String>,
    /// If true, rule only applies to regular dependencies (not dev/build)
    #[serde(default)]
    pub regular_deps_only: bool,
    /// Reason for the restriction
    #[serde(default)]
    pub reason: String,
}

impl DependencyRule {
    fn matches_dependency(&self, dep_name: &str) -> bool {
        self.pattern.matches(dep_name)
    }

    fn is_allowed(&self, crate_name: &str, config: &Config) -> bool {
        // _test and _tests crates are allowed to depend on anything
        if crate_name.ends_with("_test") || crate_name.ends_with("_tests") {
            return true;
        }

        // Check allowed prefixes (relative to any namespace)
        for prefix in &self.allowed_prefixes {
            for ns in &config.namespaces {
                let prefix_pattern = format!("{}_{prefix}_", ns.name);
                let exact_match = format!("{}_{prefix}", ns.name);
                if crate_name.starts_with(&prefix_pattern) || crate_name == exact_match {
                    return true;
                }
            }
        }

        // Check allowed crates patterns
        for pattern in &self.allowed_crates {
            if glob_match(pattern, crate_name) {
                return true;
            }
        }

        // If no allowed patterns matched, deny by default
        false
    }
}

// =============================================================================
// CLI
// =============================================================================

#[derive(Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
enum Cargo {
    Stow(Args),
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Workspace linting and structure validation for Rust monorepos",
    long_about = "cargo-stow validates workspace structure, enforces naming conventions, and keeps dependencies organized.\n\n\
    Run `cargo stow init` to generate a configuration file, then `cargo stow` to validate."
)]
#[allow(clippy::struct_excessive_bools)]
struct Args {
    /// Subcommand to run
    #[command(subcommand)]
    command: Option<Command>,

    /// Check for validation errors (default if no subcommand)
    #[arg(long, conflicts_with = "fix")]
    check: bool,

    /// Automatically fix issues
    #[arg(long, conflicts_with = "check")]
    fix: bool,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Generate dependency graph SVG at the specified path
    #[arg(long, value_name = "FILE")]
    graph: Option<PathBuf>,

    /// Include test crates (*_test, *_tests) in the dependency graph
    #[arg(long)]
    include_tests: bool,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Initialize a new stow.toml configuration file
    Init {
        /// Generate a minimal configuration instead of a fully commented template
        #[arg(long)]
        minimal: bool,
    },
}

// =============================================================================
// VALIDATION ERROR
// =============================================================================

#[derive(Debug)]
struct ValidationError {
    crate_name: String,
    #[allow(dead_code)]
    file_path: PathBuf,
    message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.crate_name, self.message)
    }
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Glob matching with support for wildcards at any position.
/// Supports:
/// - `*` matches everything
/// - `prefix*` matches strings starting with prefix
/// - `*suffix` matches strings ending with suffix
/// - `*middle*` matches strings containing middle
/// - `pre*suf` matches strings starting with pre and ending with suf
fn glob_match(pattern: &str, text: &str) -> bool {
    // Fast path: exact match or match-all
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == text;
    }

    // Split on wildcards and match segments
    let parts: Vec<&str> = pattern.split('*').collect();

    match parts.as_slice() {
        // Single wildcard cases (most common)
        [prefix, ""] => text.starts_with(prefix),
        ["", suffix] => text.ends_with(suffix),
        ["", middle, ""] => text.contains(middle),
        [prefix, suffix] => {
            text.starts_with(prefix)
                && text.ends_with(suffix)
                && text.len() >= prefix.len() + suffix.len()
        }
        // Multiple wildcards - use recursive matching
        _ => glob_match_recursive(text, &parts, 0),
    }
}

/// Recursive glob matching for complex patterns with multiple wildcards.
fn glob_match_recursive(text: &str, parts: &[&str], part_idx: usize) -> bool {
    if part_idx >= parts.len() {
        return text.is_empty();
    }

    let part = parts[part_idx];
    let is_last = part_idx == parts.len() - 1;
    let is_first = part_idx == 0;

    if part.is_empty() {
        // Empty part means wildcard - skip to next part
        if is_last {
            return true; // Trailing wildcard matches everything
        }
        // Find next part anywhere in remaining text
        let next_part = parts[part_idx + 1];
        if next_part.is_empty() {
            return glob_match_recursive(text, parts, part_idx + 1);
        }
        // Try matching next_part at every position
        for i in 0..=text.len().saturating_sub(next_part.len()) {
            if text[i..].starts_with(next_part)
                && glob_match_recursive(&text[i + next_part.len()..], parts, part_idx + 2)
            {
                return true;
            }
        }
        false
    } else if is_first {
        // First part must match at start
        text.starts_with(part) && glob_match_recursive(&text[part.len()..], parts, part_idx + 1)
    } else if is_last {
        // Last part must match at end
        text.ends_with(part)
    } else {
        // Middle part - find it and continue
        if let Some(pos) = text.find(part) {
            glob_match_recursive(&text[pos + part.len()..], parts, part_idx + 1)
        } else {
            false
        }
    }
}

fn is_internal_dep(name: &str, config: &Config) -> bool {
    config.is_internal_crate(name)
}

/// Extract crate name from a parsed Cargo.toml document.
/// Falls back to `folder_name` if `[package.name]` is not found.
#[inline]
fn extract_crate_name<'a>(doc: &'a DocumentMut, folder_name: &'a str) -> &'a str {
    doc.get("package")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(folder_name)
}

/// Parsed crate data for validation.
struct CrateInfo {
    cargo_path: PathBuf,
    folder_name: String,
    crate_name: String,
    doc: DocumentMut,
}

impl CrateInfo {
    /// Load crate info from a directory containing Cargo.toml.
    fn load(crate_dir: &Path) -> Option<Self> {
        let cargo_path = crate_dir.join("Cargo.toml");
        let folder_name = crate_dir.file_name()?.to_string_lossy().into_owned();

        let content = std::fs::read_to_string(&cargo_path).ok()?;
        let doc: DocumentMut = content.parse().ok()?;
        let crate_name = extract_crate_name(&doc, &folder_name).to_owned();

        Some(CrateInfo {
            cargo_path,
            folder_name,
            crate_name,
            doc,
        })
    }

    /// Reload the document from disk (after fixes).
    fn reload(&mut self) -> std::io::Result<()> {
        let content = std::fs::read_to_string(&self.cargo_path)?;
        self.doc = content.parse().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Parse error: {e}"))
        })?;
        self.crate_name = extract_crate_name(&self.doc, &self.folder_name).to_owned();
        Ok(())
    }
}

struct ManifestSelection {
    root_dir: PathBuf,
    manifest_path: PathBuf,
    is_workspace: bool,
}

fn find_best_manifest() -> Option<ManifestSelection> {
    let mut dir = std::env::current_dir().ok()?;
    let mut fallback_manifest: Option<PathBuf> = None;

    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
                if let Ok(doc) = content.parse::<toml::Table>() {
                    if doc.get("workspace").is_some() {
                        return Some(ManifestSelection {
                            root_dir: dir,
                            manifest_path: cargo_toml,
                            is_workspace: true,
                        });
                    }
                }
            }
            if fallback_manifest.is_none() {
                fallback_manifest = Some(cargo_toml);
            }
        }
        if !dir.pop() {
            break;
        }
    }

    fallback_manifest.and_then(|manifest_path| {
        let root_dir = manifest_path.parent()?.to_path_buf();
        Some(ManifestSelection {
            root_dir,
            manifest_path,
            is_workspace: false,
        })
    })
}

fn find_crate_dirs(crates_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("Cargo.toml").exists() {
                dirs.push(path);
            }
        }
    }
    dirs.sort();
    dirs
}

fn load_metadata(manifest_path: &Path) -> cargo_metadata::Metadata {
    MetadataCommand::new()
        .manifest_path(manifest_path)
        .exec()
        .unwrap_or_else(|e| panic!("Failed to load cargo metadata: {e}"))
}

fn toml_array_strings(value: Option<&toml::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

fn derive_member_roots(workspace_root: &Path, members: &[String]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();

    for member in members {
        let normalized = member.replace('\\', "/");
        let wildcard_pos = normalized.find(['*', '?', '[']);
        let base = match wildcard_pos {
            None => normalized.as_str(),
            Some(pos) => {
                let prefix = &normalized[..pos];
                match prefix.rfind('/') {
                    Some(idx) => &prefix[..idx],
                    None => "",
                }
            }
        };
        let base = base.trim_end_matches('/');
        let root = if base.is_empty() {
            workspace_root.to_path_buf()
        } else {
            workspace_root.join(base)
        };
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    }

    roots.sort();
    roots
}

fn workspace_member_roots(workspace_root: &Path, workspace_cargo: &Path) -> Vec<PathBuf> {
    let content = std::fs::read_to_string(workspace_cargo).unwrap_or_default();
    let doc: toml::Table = content.parse().unwrap_or_default();

    let workspace = doc.get("workspace").and_then(|w| w.as_table());
    let members = workspace
        .map(|w| toml_array_strings(w.get("members")))
        .unwrap_or_default();

    if members.is_empty() {
        return vec![workspace_root.join("crates")];
    }

    derive_member_roots(workspace_root, &members)
}

fn workspace_crate_dirs(metadata: &cargo_metadata::Metadata, fallback_root: &Path) -> Vec<PathBuf> {
    let member_ids: HashSet<_> = metadata.workspace_members.iter().collect();
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();

    for package in &metadata.packages {
        if !member_ids.contains(&package.id) {
            continue;
        }
        let manifest_path = PathBuf::from(package.manifest_path.as_str());
        if let Some(dir) = manifest_path.parent() {
            let dir = dir.to_path_buf();
            if seen.insert(dir.clone()) {
                dirs.push(dir);
            }
        }
    }

    if dirs.is_empty() {
        let crates_dir = fallback_root.join("crates");
        return find_crate_dirs(&crates_dir);
    }

    dirs.sort();
    dirs
}

/// Workspace dependency info including features
#[derive(Debug, Clone, Default)]
struct WorkspaceDep {
    features: HashSet<String>,
}

/// Extract features from a dependency value (inline table or table)
fn extract_features_from_dep(item: &Item) -> HashSet<String> {
    let mut features = HashSet::new();

    // Try inline table first: { workspace = true, features = ["a", "b"] }
    if let Some(table) = item.as_inline_table() {
        if let Some(feat_val) = table.get("features") {
            if let Some(arr) = feat_val.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        features.insert(s.to_string());
                    }
                }
            }
        }
    }
    // Try regular table: [dependencies.foo]\nfeatures = ["a", "b"]
    else if let Some(table) = item.as_table_like() {
        if let Some(feat_item) = table.get("features") {
            if let Some(arr) = feat_item.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        features.insert(s.to_string());
                    }
                }
            }
        }
    }

    features
}

fn get_workspace_dependencies(workspace_cargo: &Path) -> HashMap<String, WorkspaceDep> {
    let content = std::fs::read_to_string(workspace_cargo).unwrap_or_default();
    let doc: DocumentMut = content.parse().unwrap_or_default();

    let mut deps = HashMap::new();
    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
        if let Some(ws_deps) = workspace.get("dependencies").and_then(|d| d.as_table()) {
            for (key, value) in ws_deps {
                let features = extract_features_from_dep(value);
                deps.insert(key.to_string(), WorkspaceDep { features });
            }
        }
    }
    deps
}

/// Get all dependencies from a crate with their features
fn get_crate_dependencies(doc: &DocumentMut) -> HashMap<String, HashSet<String>> {
    let mut deps = HashMap::new();
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in dep_sections {
        if let Some(table) = doc.get(section).and_then(|d| d.as_table_like()) {
            for (dep_name, dep_value) in table.iter() {
                let features = extract_features_from_dep(dep_value);
                // Merge features if already present (from another section)
                deps.entry(dep_name.to_string())
                    .or_insert_with(HashSet::new)
                    .extend(features);
            }
        }
    }

    // Also check target-specific dependencies
    if let Some(target) = doc.get("target").and_then(|t| t.as_table_like()) {
        for (_, target_data) in target.iter() {
            if let Some(target_table) = target_data.as_table_like() {
                for section in dep_sections {
                    if let Some(table) = target_table.get(section).and_then(|d| d.as_table_like()) {
                        for (dep_name, dep_value) in table.iter() {
                            let features = extract_features_from_dep(dep_value);
                            deps.entry(dep_name.to_string())
                                .or_insert_with(HashSet::new)
                                .extend(features);
                        }
                    }
                }
            }
        }
    }

    deps
}

fn collect_dep_names(doc: &DocumentMut) -> HashSet<String> {
    let mut deps = HashSet::new();
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in dep_sections {
        if let Some(table) = doc.get(section).and_then(|d| d.as_table_like()) {
            for (dep_name, _) in table.iter() {
                deps.insert(dep_name.to_string());
            }
        }
    }

    if let Some(target) = doc.get("target").and_then(|t| t.as_table_like()) {
        for (_, target_data) in target.iter() {
            if let Some(target_table) = target_data.as_table_like() {
                for section in dep_sections {
                    if let Some(table) = target_table.get(section).and_then(|d| d.as_table_like()) {
                        for (dep_name, _) in table.iter() {
                            deps.insert(dep_name.to_string());
                        }
                    }
                }
            }
        }
    }

    deps
}

// =============================================================================
// VALIDATION RULES
// =============================================================================

fn check_no_nested_crates(
    crate_dir: &Path,
    workspace_root: &Path,
    member_roots: &[PathBuf],
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    for root in member_roots {
        if let Ok(relative) = crate_dir.strip_prefix(root) {
            if relative.components().count() > 1 {
                let root_display = match root.strip_prefix(workspace_root) {
                    Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
                    _ => root.display().to_string(),
                };
                errors.push(ValidationError {
                    crate_name: crate_dir
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    file_path: crate_dir.join("Cargo.toml"),
                    message: format!(
                        "Nested crate detected at '{}'. All crates must be directly under {}/",
                        relative.display(),
                        root_display
                    ),
                });
            }
            break;
        }
    }
    errors
}

fn check_crate_name_matches_folder(
    folder_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
    config: &Config,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(package) = doc.get("package").and_then(|p| p.as_table()) {
        if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
            if name != folder_name {
                // Check if there's a name exception for this folder
                let has_exception = config.namespaces.iter().any(|ns| {
                    ns.name_exceptions
                        .get(folder_name)
                        .is_some_and(|allowed| allowed == name)
                });

                if !has_exception {
                    errors.push(ValidationError {
                        crate_name: folder_name.to_string(),
                        file_path: cargo_path.to_path_buf(),
                        message: format!(
                            "Crate name '{name}' does not match folder name '{folder_name}'"
                        ),
                    });
                }
            }
        }
    }
    errors
}

/// Auto-allowed suffixes for crate naming (no prefix needed)
const AUTO_ALLOWED_SUFFIXES: &[&str] = &["types", "test", "tests"];

fn check_crate_naming_convention(
    folder_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
    config: &Config,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let crate_name = doc
        .get("package")
        .and_then(|p| p.as_table())
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(folder_name);

    // Check if this folder has a name exception - if so, skip naming convention check
    // (the folder name determines the namespace, not the crate name)
    let has_name_exception = config.namespaces.iter().any(|ns| {
        ns.name_exceptions
            .get(folder_name)
            .is_some_and(|allowed| allowed == crate_name)
    });
    if has_name_exception {
        return errors;
    }

    // Find which namespace this crate belongs to
    let Some(namespace) = config.get_namespace(crate_name) else {
        let ns_names: Vec<_> = config.namespaces.iter().map(|ns| &ns.name).collect();
        errors.push(ValidationError {
            crate_name: crate_name.to_string(),
            file_path: cargo_path.to_path_buf(),
            message: format!(
                "Crate name must start with a known namespace prefix. Known namespaces: {ns_names:?}, got '{crate_name}'"
            ),
        });
        return errors;
    };

    // Remove the namespace prefix (e.g., "baml_" or "bex_")
    let suffix = &crate_name[namespace.name.len() + 1..];
    let parts: Vec<&str> = suffix.split('_').collect();

    match parts.len() {
        1 => {
            // namespace_word (e.g., baml_cli)
            if !parts[0].chars().all(|c| c.is_ascii_lowercase()) {
                errors.push(ValidationError {
                    crate_name: crate_name.to_string(),
                    file_path: cargo_path.to_path_buf(),
                    message: format!(
                        "Crate suffix '{}' should be a simple lowercase word",
                        parts[0]
                    ),
                });
            }
        }
        2 => {
            // namespace_prefix_word (e.g., baml_compiler_emit)
            // OR namespace_word_suffix (e.g., bex_vm_types - auto-allowed)
            let (first, second) = (parts[0], parts[1]);

            // Check if it's an auto-allowed suffix pattern (e.g., bex_vm_types)
            if AUTO_ALLOWED_SUFFIXES.contains(&second) {
                // Auto-allowed - no prefix validation needed
            } else {
                // Check if first part is an approved prefix
                let prefix_approved = namespace.approved_prefixes.iter().any(|p| p == first);
                if !prefix_approved {
                    errors.push(ValidationError {
                        crate_name: crate_name.to_string(),
                        file_path: cargo_path.to_path_buf(),
                        message: format!(
                            "Crate name '{}' has unapproved prefix '{}'. Approved for {}: {:?}",
                            crate_name, first, namespace.name, namespace.approved_prefixes
                        ),
                    });
                }
            }
        }
        3 => {
            // namespace_prefix_word_suffix (e.g., baml_ide_foo_tests)
            let (prefix, _, last) = (parts[0], parts[1], parts[2]);

            // Must have an approved prefix AND end with auto-allowed suffix
            let prefix_approved = namespace.approved_prefixes.iter().any(|p| p == prefix);
            let suffix_allowed = AUTO_ALLOWED_SUFFIXES.contains(&last);

            if !prefix_approved || !suffix_allowed {
                errors.push(ValidationError {
                    crate_name: crate_name.to_string(),
                    file_path: cargo_path.to_path_buf(),
                    message: format!(
                        "Crate name '{crate_name}' has unapproved structure. Expected {ns}_<word>, {ns}_<approved>_<word>, or ending in _types/_test/_tests",
                        ns = namespace.name
                    ),
                });
            }
        }
        _ => {
            errors.push(ValidationError {
                crate_name: crate_name.to_string(),
                file_path: cargo_path.to_path_buf(),
                message: format!(
                    "Crate name '{crate_name}' has too many underscore-separated parts"
                ),
            });
        }
    }

    errors
}

fn check_test_crate_has_prefix(
    crate_name: &str,
    all_crate_names: &HashSet<String>,
    cargo_path: &Path,
    config: &Config,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Check namespace-specific exceptions first
    if let Some(namespace) = config.get_namespace(crate_name) {
        if namespace
            .test_crate_exceptions
            .iter()
            .any(|e| e == crate_name)
        {
            return errors;
        }
    }

    for suffix in ["_test", "_tests"] {
        if let Some(prefix_crate) = crate_name.strip_suffix(suffix) {
            if !all_crate_names.contains(prefix_crate) {
                errors.push(ValidationError {
                    crate_name: crate_name.to_string(),
                    file_path: cargo_path.to_path_buf(),
                    message: format!(
                        "Test crate '{crate_name}' requires corresponding crate '{prefix_crate}' which was not found"
                    ),
                });
            }
            break;
        }
    }

    errors
}

fn is_workspace_dependency(item: &Item) -> bool {
    if let Some(table) = item.as_inline_table() {
        return table.get("workspace").and_then(toml_edit::Value::as_bool) == Some(true);
    }
    if let Some(table) = item.as_table_like() {
        return table.get("workspace").and_then(toml_edit::Item::as_bool) == Some(true);
    }
    false
}

fn check_workspace_dependencies(
    crate_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in dep_sections {
        if let Some(deps) = doc.get(section).and_then(|d| d.as_table_like()) {
            for (dep_name, dep_value) in deps.iter() {
                if !is_workspace_dependency(dep_value) {
                    errors.push(ValidationError {
                        crate_name: crate_name.to_string(),
                        file_path: cargo_path.to_path_buf(),
                        message: format!(
                            "Dependency '{dep_name}' in [{section}] must use '{{ workspace = true }}' format"
                        ),
                    });
                }
            }
        }
    }

    // Check target-specific dependencies
    if let Some(target) = doc.get("target").and_then(|t| t.as_table_like()) {
        for (target_name, target_data) in target.iter() {
            if let Some(target_table) = target_data.as_table_like() {
                for section in dep_sections {
                    if let Some(deps) = target_table.get(section).and_then(|d| d.as_table_like()) {
                        for (dep_name, dep_value) in deps.iter() {
                            if !is_workspace_dependency(dep_value) {
                                errors.push(ValidationError {
                                    crate_name: crate_name.to_string(),
                                    file_path: cargo_path.to_path_buf(),
                                    message: format!(
                                        "Dependency '{dep_name}' in [target.'{target_name}'.{section}] must use '{{ workspace = true }}' format"
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    errors
}

fn check_dependency_restrictions(
    crate_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
    config: &Config,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    let mut all_deps: Vec<(&str, &str)> = Vec::new();

    for section in dep_sections {
        if let Some(deps) = doc.get(section).and_then(|d| d.as_table_like()) {
            for (dep_name, _) in deps.iter() {
                all_deps.push((dep_name, section));
            }
        }
    }

    // Collect all applicable rules: global rules + namespace-specific rules
    let mut applicable_rules: Vec<&DependencyRule> = config.dependency_rules.iter().collect();

    // Add namespace-specific rules if crate belongs to a namespace
    if let Some(namespace) = config.get_namespace(crate_name) {
        applicable_rules.extend(namespace.dependency_rules.iter());
    }

    for (dep_name, section) in all_deps {
        for rule in &applicable_rules {
            // Skip if rule only applies to regular deps and this is dev/build deps
            if rule.regular_deps_only && section != "dependencies" {
                continue;
            }

            if rule.matches_dependency(dep_name) && !rule.is_allowed(crate_name, config) {
                errors.push(ValidationError {
                    crate_name: crate_name.to_string(),
                    file_path: cargo_path.to_path_buf(),
                    message: format!(
                        "Crate '{}' is not allowed to use '{}' (found in [{}]). {}",
                        crate_name, dep_name, section, rule.reason
                    ),
                });
            }
        }
    }

    errors
}

fn check_dependencies_sorted(
    crate_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
    config: &Config,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in dep_sections {
        if let Some(deps) = doc.get(section).and_then(|d| d.as_table_like()) {
            let dep_names: Vec<&str> = deps.iter().map(|(k, _)| k).collect();
            if dep_names.is_empty() {
                continue;
            }

            // Separate internal and external deps
            let mut internal: Vec<&str> = dep_names
                .iter()
                .copied()
                .filter(|n| is_internal_dep(n, config))
                .collect();
            let mut external: Vec<&str> = dep_names
                .iter()
                .copied()
                .filter(|n| !is_internal_dep(n, config))
                .collect();

            // Get expected order: internal first (sorted), then external (sorted)
            internal.sort_by_key(|s| s.to_lowercase());
            external.sort_by_key(|s| s.to_lowercase());
            let expected: Vec<&str> = internal.into_iter().chain(external).collect();

            if dep_names != expected {
                // Check for grouping issue
                let mut seen_external = false;
                let mut has_grouping_issue = false;
                for name in &dep_names {
                    if is_internal_dep(name, config) {
                        if seen_external {
                            has_grouping_issue = true;
                            break;
                        }
                    } else {
                        seen_external = true;
                    }
                }

                if has_grouping_issue {
                    errors.push(ValidationError {
                        crate_name: crate_name.to_string(),
                        file_path: cargo_path.to_path_buf(),
                        message: format!(
                            "Dependencies in [{section}] have internal deps interleaved with external deps. Internal deps should come first."
                        ),
                    });
                } else {
                    errors.push(ValidationError {
                        crate_name: crate_name.to_string(),
                        file_path: cargo_path.to_path_buf(),
                        message: format!(
                            "Dependencies in [{section}] are not sorted. Expected internal first (sorted), then external (sorted)."
                        ),
                    });
                }
            }
        }
    }

    errors
}

// =============================================================================
// FIX FUNCTIONS
// =============================================================================

fn sort_dependencies_table(table: &mut Table, config: &Config) {
    // Collect all entries with normalized values
    let entries: Vec<(String, Item)> = table
        .iter()
        .map(|(k, v)| {
            let mut item = v.clone();
            // Normalize: ensure inline table format
            if let Some(inline) = item.as_inline_table_mut() {
                inline.decor_mut().clear();
                inline.set_dotted(false);
            }
            (k.to_string(), item)
        })
        .collect();

    // Separate internal and external
    let mut internal: Vec<(String, Item)> = entries
        .iter()
        .filter(|(k, _)| is_internal_dep(k, config))
        .cloned()
        .collect();
    let mut external: Vec<(String, Item)> = entries
        .iter()
        .filter(|(k, _)| !is_internal_dep(k, config))
        .cloned()
        .collect();

    // Sort each group
    internal.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    external.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    let has_internal = !internal.is_empty();
    let has_external = !external.is_empty();

    // Clear table
    table.clear();

    // Re-add in sorted order (let taplo handle comments and formatting)
    for (key, value) in internal {
        table.insert(&key, value);
    }

    for (key, value) in external {
        table.insert(&key, value);
    }

    // Mark the table as sorted (taplo will add proper formatting)
    let _ = (has_internal, has_external); // suppress unused warnings
}

fn convert_to_workspace_dep(item: &Item) -> Item {
    let mut new_table = InlineTable::new();
    new_table.insert("workspace", Value::Boolean(Formatted::new(true)));

    // Preserve features if present
    if let Some(table) = item.as_inline_table() {
        if let Some(features) = table.get("features") {
            new_table.insert("features", features.clone());
        }
    } else if let Some(table) = item.as_table_like() {
        if let Some(features) = table.get("features") {
            if let Some(arr) = features.as_array() {
                new_table.insert("features", Value::Array(arr.clone()));
            }
        }
    }

    Item::Value(Value::InlineTable(new_table))
}

fn fix_cargo_toml(
    cargo_path: &Path,
    workspace_deps: &HashMap<String, WorkspaceDep>,
    config: &Config,
) -> std::io::Result<bool> {
    let content = std::fs::read_to_string(cargo_path)?;
    let mut doc: DocumentMut = content.parse().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Parse error: {e}"))
    })?;

    let mut modified = false;
    let dep_sections = ["dependencies", "dev-dependencies", "build-dependencies"];

    for section in dep_sections {
        if let Some(deps) = doc.get_mut(section).and_then(|d| d.as_table_mut()) {
            if deps.is_empty() {
                continue;
            }

            // Convert non-workspace deps
            let keys: Vec<String> = deps.iter().map(|(k, _)| k.to_string()).collect();
            for key in keys {
                if let Some(item) = deps.get(&key) {
                    if !is_workspace_dependency(item) && workspace_deps.contains_key(&key) {
                        let new_item = convert_to_workspace_dep(item);
                        deps.insert(&key, new_item);
                    }
                }
            }

            // Sort dependencies - always mark as modified since we may reorder
            sort_dependencies_table(deps, config);
            modified = true;
        }
    }

    if modified {
        // Write and sync to ensure data is flushed to disk before re-reading
        use std::io::Write;
        let file = std::fs::File::create(cargo_path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(doc.to_string().as_bytes())?;
        writer.flush()?;
        writer.into_inner()?.sync_all()?;
    }

    Ok(modified)
}

fn format_toml_file(path: &Path) -> std::io::Result<bool> {
    let content = std::fs::read_to_string(path)?;

    let formatted = taplo::formatter::format(
        &content,
        taplo::formatter::Options {
            align_entries: false,
            align_comments: true,
            align_single_comments: true,
            array_trailing_comma: true,
            array_auto_expand: true,
            array_auto_collapse: false,
            compact_arrays: false,
            compact_inline_tables: false,
            inline_table_expand: false,
            compact_entries: false,
            column_width: 100,
            indent_tables: false,
            indent_entries: false,
            indent_string: "  ".to_string(),
            trailing_newline: true,
            reorder_keys: false, // We handle ordering ourselves
            reorder_arrays: false,
            allowed_blank_lines: 1,
            crlf: false,
        },
    );

    if formatted != content {
        // Write and sync to ensure data is flushed to disk
        use std::io::Write;
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(formatted.as_bytes())?;
        writer.flush()?;
        writer.into_inner()?.sync_all()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn format_all_cargo_tomls(
    crate_dirs: &[PathBuf],
    workspace_cargo: &Path,
) -> std::io::Result<usize> {
    let mut count = 0;

    if format_toml_file(workspace_cargo)? {
        count += 1;
    }

    for crate_dir in crate_dirs {
        let cargo_toml = crate_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if format_toml_file(&cargo_toml)? {
                count += 1;
            }
        }
    }

    Ok(count)
}

// =============================================================================
// DEPENDENCY GRAPH GENERATION
// =============================================================================

/// Post-process SVG to add arrow markers along dashed edges (-->--->-->)
/// Adds a small arrow at the midpoint of each line segment
fn post_process_dashed_edges(svg: &str) -> String {
    let mut result = svg.to_string();
    let mut arrows_to_add: Vec<String> = Vec::new();

    // Find dashed paths and extract their coordinates to add arrows
    for (start, _) in svg.match_indices("<path") {
        let Some(path_end_offset) = svg[start..].find("/>") else {
            continue;
        };
        let path_end = start + path_end_offset + 2;
        let path_content = &svg[start..path_end];

        // Check if this is a dashed edge
        if !path_content.contains("stroke-dasharray") {
            continue;
        }

        // Extract the d= attribute
        let Some(d_start) = path_content.find(" d=\"") else {
            continue;
        };
        let d_start = d_start + 4;
        let Some(d_end) = path_content[d_start..].find('"') else {
            continue;
        };
        let path_data = &path_content[d_start..d_start + d_end];

        // Parse path to get line segments and add arrow at midpoint of each
        let points = parse_path_points(path_data);
        for arrow in generate_midpoint_arrows(&points) {
            arrows_to_add.push(arrow);
        }
    }

    // Insert arrows before the closing </g> of the graph
    if !arrows_to_add.is_empty() {
        let arrows_svg = arrows_to_add.join("\n");
        if let Some(pos) = result.rfind("</g>\n</svg>") {
            result.insert_str(pos, &format!("\n{arrows_svg}\n"));
        }
    }

    result
}

/// Parse SVG path data to extract coordinate points (handles M and C commands)
fn parse_path_points(path_data: &str) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    let mut nums: Vec<f64> = Vec::new();
    let mut num_buf = String::new();

    for c in path_data.chars() {
        match c {
            'M' => {
                // Move command - flush and prepare for new point
                flush_num(&mut num_buf, &mut nums);
                if nums.len() >= 2 {
                    points.push((nums[nums.len() - 2], nums[nums.len() - 1]));
                }
                nums.clear();
            }
            'C' => {
                // Cubic curve - flush, take last point, then take endpoints of curves
                flush_num(&mut num_buf, &mut nums);
                // First point from M command
                if nums.len() >= 2 && points.is_empty() {
                    points.push((nums[0], nums[1]));
                }
                nums.clear();
            }
            '0'..='9' | '.' | '-' | 'e' | 'E' => {
                if c == '-'
                    && !num_buf.is_empty()
                    && !num_buf.ends_with('e')
                    && !num_buf.ends_with('E')
                {
                    flush_num(&mut num_buf, &mut nums);
                }
                num_buf.push(c);
            }
            ' ' | ',' => {
                flush_num(&mut num_buf, &mut nums);
                // Every 6 numbers in a C command = one curve segment, take the endpoint
                if nums.len() >= 6 && nums.len() % 6 == 0 {
                    points.push((nums[nums.len() - 2], nums[nums.len() - 1]));
                }
            }
            _ => {}
        }
    }

    flush_num(&mut num_buf, &mut nums);
    // Handle final point
    if nums.len() >= 2 {
        let last = (nums[nums.len() - 2], nums[nums.len() - 1]);
        if points.last() != Some(&last) {
            points.push(last);
        }
    }

    points
}

fn flush_num(buf: &mut String, nums: &mut Vec<f64>) {
    if !buf.is_empty() {
        if let Ok(n) = buf.parse::<f64>() {
            nums.push(n);
        }
        buf.clear();
    }
}

/// Generate arrows along line segments
/// - Short segments (< 80px): no mid arrow
/// - Medium segments: arrow at midpoint
/// - Long segments (> 200px): arrow near source + midpoint
fn generate_midpoint_arrows(points: &[(f64, f64)]) -> Vec<String> {
    let mut arrows = Vec::new();

    for i in 0..points.len().saturating_sub(1) {
        let (x1, y1) = points[i];
        let (x2, y2) = points[i + 1];

        let dx = x2 - x1;
        let dy = y2 - y1;
        let length = (dx * dx + dy * dy).sqrt();

        // Skip short segments
        if length < 80.0 {
            continue;
        }

        // Direction angle
        let angle = dy.atan2(dx);

        // For long segments, add arrow near the source (at 20% along the line)
        if length > 200.0 {
            let t = 0.2;
            let ax = x1 + dx * t;
            let ay = y1 + dy * t;
            arrows.push(make_chevron_arrow(ax, ay, angle));
        }

        // Midpoint arrow
        let mx = (x1 + x2) / 2.0;
        let my = (y1 + y2) / 2.0;
        arrows.push(make_chevron_arrow(mx, my, angle));
    }

    arrows
}

/// Create a small filled triangle arrow at the given position pointing in the given direction
/// Sized to match the graphviz arrowheads (~5px)
fn make_chevron_arrow(x: f64, y: f64, angle: f64) -> String {
    // Match graphviz arrowhead size (about 5px wide, 7px long)
    let length = 5.0;
    let width = 3.5;

    // Triangle points: tip at (x,y), base perpendicular to direction
    let tip_x = x + length * 0.5 * angle.cos();
    let tip_y = y + length * 0.5 * angle.sin();
    let base_x = x - length * 0.5 * angle.cos();
    let base_y = y - length * 0.5 * angle.sin();

    format!(
        r##"<polygon fill="#666666" stroke="#666666" points="{:.2},{:.2} {:.2},{:.2} {:.2},{:.2}"/>"##,
        tip_x,
        tip_y,
        base_x + width * angle.sin(),
        base_y - width * angle.cos(),
        base_x - width * angle.sin(),
        base_y + width * angle.cos(),
    )
}

/// Feature additions for an external dependency
#[derive(Debug, Clone, Default)]
struct ExternalDepFeatures {
    /// Additional features added by crates (`crate_name` -> `added_features`)
    crate_additions: HashMap<String, HashSet<String>>,
}

/// Generate a dependency graph SVG using graphviz-rust with cluster support.
///
/// This function:
/// 1. Identifies starting crates from stow.toml config (namespaces - ignored crates)
/// 2. BFS through dependencies
/// 3. Filters nodes: local crates (in namespace, not ignored) OR in `graph_external_deps`
/// 4. Filters edges: only if both source and target pass the node filter
/// 5. Renders to SVG with clusters for namespaces
/// 6. Shows feature additions for external dependencies
fn generate_dependency_graph_svg(
    metadata: &cargo_metadata::Metadata,
    config: &Config,
    output_path: &Path,
    include_tests: bool,
    workspace_deps: &HashMap<String, WorkspaceDep>,
    crate_features: &HashMap<String, HashMap<String, HashSet<String>>>,
) -> std::io::Result<()> {
    // Build a lookup map from PackageId to Package
    let packages_by_id: HashMap<&PackageId, &cargo_metadata::Package> =
        metadata.packages.iter().map(|p| (&p.id, p)).collect();

    // Build external deps lookup set
    let graph_external_deps: HashSet<&str> = config
        .graph_external_deps
        .iter()
        .map(std::string::String::as_str)
        .collect();

    // Helper to check if a crate should be included in the graph
    let should_include_crate = |name: &str| -> bool {
        if config.get_namespace_for_crate(name).is_some() && !config.is_ignored_crate(name) {
            return true;
        }
        graph_external_deps.contains(name)
    };

    // Find starting crates
    let starting_crates: Vec<&PackageId> = metadata
        .workspace_members
        .iter()
        .filter(|id| {
            if let Some(pkg) = packages_by_id.get(id) {
                config.get_namespace_for_crate(&pkg.name).is_some()
                    && !config.is_ignored_crate(&pkg.name)
            } else {
                false
            }
        })
        .collect();

    // Get the resolve graph
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| std::io::Error::other("No resolve graph found"))?;

    let nodes_by_id: HashMap<&PackageId, &cargo_metadata::Node> =
        resolve.nodes.iter().map(|n| (&n.id, n)).collect();

    // BFS through dependencies
    let mut visited: HashSet<&PackageId> = HashSet::new();
    let mut queue: VecDeque<&PackageId> = VecDeque::new();
    let mut included_crates: HashSet<&str> = HashSet::new();
    let mut edges: Vec<(&str, &str)> = Vec::new();

    for id in &starting_crates {
        queue.push_back(id);
        visited.insert(id);
    }

    while let Some(pkg_id) = queue.pop_front() {
        let Some(pkg) = packages_by_id.get(pkg_id) else {
            continue;
        };
        if !should_include_crate(&pkg.name) {
            continue;
        }

        included_crates.insert(&pkg.name);

        let Some(node) = nodes_by_id.get(pkg_id) else {
            continue;
        };

        for dep in &node.deps {
            let Some(dep_pkg) = packages_by_id.get(&dep.pkg) else {
                continue;
            };

            let is_normal_dep = dep
                .dep_kinds
                .iter()
                .any(|dk| matches!(dk.kind, cargo_metadata::DependencyKind::Normal));
            if !is_normal_dep {
                continue;
            }

            if should_include_crate(&dep_pkg.name) {
                edges.push((&pkg.name, &dep_pkg.name));
                if !visited.contains(&dep.pkg) {
                    visited.insert(&dep.pkg);
                    queue.push_back(&dep.pkg);
                }
            }
        }
    }

    // Transitive reduction
    let edges = {
        let nodes: Vec<&str> = included_crates.iter().copied().collect();
        let node_index: HashMap<&str, usize> =
            nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let n = nodes.len();

        let mut adjacency: Vec<Vec<usize>> = vec![vec![]; n];
        for (from, to) in &edges {
            if let (Some(&src), Some(&dst)) = (node_index.get(from), node_index.get(to)) {
                adjacency[src].push(dst);
            }
        }

        let mut reachable: Vec<Vec<bool>> = vec![vec![false; n]; n];

        #[allow(clippy::items_after_statements)]
        fn dfs(
            source: usize,
            current: usize,
            adjacency: &[Vec<usize>],
            reachable: &mut [Vec<bool>],
        ) {
            if reachable[source][current] {
                return;
            }
            reachable[source][current] = true;
            for &next in &adjacency[current] {
                dfs(source, next, adjacency, reachable);
            }
        }

        for i in 0..n {
            dfs(i, i, &adjacency, &mut reachable);
        }

        let mut reduced_edges: Vec<(&str, &str)> = Vec::new();
        for (from, to) in &edges {
            let Some(&src) = node_index.get(from) else {
                continue;
            };
            let Some(&dst) = node_index.get(to) else {
                continue;
            };

            let is_external_target = graph_external_deps.contains(to);
            let source_namespace = config.get_namespace_for_crate(from);

            // Get features for this crate's dependency on the external target
            let source_features: Option<&HashSet<String>> = if is_external_target {
                crate_features.get(*from).and_then(|deps| deps.get(*to))
            } else {
                None
            };

            // For external targets, only apply transitive reduction if there's a path
            // through an intermediate crate in the SAME namespace as the source,
            // AND the intermediate has the same features for that dependency.
            // This ensures dependencies with different features are always shown.
            let is_redundant = adjacency[src].iter().any(|&intermediate| {
                if intermediate == dst {
                    return false;
                }
                if !reachable[intermediate][dst] {
                    return false;
                }
                // For external targets, only count intermediates in the same namespace
                // AND with the same features
                if is_external_target {
                    let intermediate_name = nodes[intermediate];
                    let intermediate_namespace = config.get_namespace_for_crate(intermediate_name);
                    // Both must be in the same namespace for it to count as redundant
                    let same_namespace = match (source_namespace, intermediate_namespace) {
                        (Some(src_ns), Some(int_ns)) => src_ns.name == int_ns.name,
                        _ => false,
                    };
                    if !same_namespace {
                        return false;
                    }
                    // Also check if features are the same
                    let intermediate_features = crate_features
                        .get(intermediate_name)
                        .and_then(|deps| deps.get(*to));
                    match (source_features, intermediate_features) {
                        (Some(src_feat), Some(int_feat)) => src_feat == int_feat,
                        (None, None) => true,
                        _ => false,
                    }
                } else {
                    true
                }
            });

            if !is_redundant {
                reduced_edges.push((*from, *to));
            }
        }
        reduced_edges
    };

    // Filter test crates if needed
    let is_test_crate = |name: &str| name.ends_with("_test") || name.ends_with("_tests");

    let included_crates: HashSet<&str> = if include_tests {
        included_crates
    } else {
        included_crates
            .into_iter()
            .filter(|name| !is_test_crate(name))
            .collect()
    };

    let edges: Vec<(&str, &str)> = if include_tests {
        edges
    } else {
        edges
            .into_iter()
            .filter(|(from, to)| !is_test_crate(from) && !is_test_crate(to))
            .collect()
    };

    // Sort for deterministic output
    let mut sorted_crates: Vec<&str> = included_crates.iter().copied().collect();
    sorted_crates.sort_unstable();
    let mut sorted_edges = edges.clone();
    sorted_edges.sort_unstable();

    // Compute feature additions for external deps
    // For each external dep, collect features added by each crate beyond workspace defaults
    let mut external_dep_features: HashMap<&str, ExternalDepFeatures> = HashMap::new();

    for ext_dep in &config.graph_external_deps {
        let ws_features = workspace_deps
            .get(ext_dep)
            .map(|d| d.features.clone())
            .unwrap_or_default();

        let mut crate_additions: HashMap<String, HashSet<String>> = HashMap::new();

        // Check each crate that uses this external dep
        for (crate_name, deps) in crate_features {
            if let Some(crate_features_for_dep) = deps.get(ext_dep) {
                // Find features added by this crate (not in workspace)
                let added: HashSet<String> = crate_features_for_dep
                    .difference(&ws_features)
                    .cloned()
                    .collect();
                if !added.is_empty() {
                    crate_additions.insert(crate_name.clone(), added);
                }
            }
        }

        if !crate_additions.is_empty() {
            external_dep_features.insert(ext_dep.as_str(), ExternalDepFeatures { crate_additions });
        }
    }

    // Extract namespace and tag from crate name
    let get_crate_parts = |name: &str| -> (String, Option<String>) {
        for ns in &config.namespaces {
            // Check if this crate is a name exception for this namespace
            if ns.name_exceptions.values().any(|v| v == name) {
                return (ns.name.clone(), None);
            }
            // Standard prefix match
            if let Some(suffix) = name.strip_prefix(&format!("{}_", ns.name)) {
                let parts: Vec<&str> = suffix.split('_').collect();
                if parts.len() >= 2 {
                    if ns.approved_prefixes.iter().any(|p| p == parts[0]) {
                        return (ns.name.clone(), Some(parts[0].to_string()));
                    }
                    let last = *parts.last().unwrap();
                    if ["types", "test", "tests"].contains(&last) {
                        return (ns.name.clone(), Some(parts[0].to_string()));
                    }
                }
                return (ns.name.clone(), None);
            }
        }
        ("external".to_string(), None)
    };

    // Deterministic color generation using HSL
    #[allow(
        clippy::items_after_statements,
        clippy::many_single_char_names,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r, g, b) = match h as u32 {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        format!(
            "#{:02x}{:02x}{:02x}",
            ((r + m) * 255.0) as u8,
            ((g + m) * 255.0) as u8,
            ((b + m) * 255.0) as u8
        )
    }

    #[allow(clippy::items_after_statements, clippy::cast_precision_loss)]
    fn string_to_hue(s: &str) -> f32 {
        let hash: u32 = s.bytes().fold(0u32, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u32::from(b))
        });
        (hash % 360) as f32
    }

    // Collect unique local namespaces (excluding "external") - sorted for consistency
    let mut local_namespaces: Vec<String> = included_crates
        .iter()
        .map(|name| get_crate_parts(name).0)
        .filter(|ns| ns != "external")
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    local_namespaces.sort();

    // Generate deterministic namespace colors (evenly spaced hues)
    #[allow(clippy::cast_precision_loss)]
    let namespace_colors: HashMap<String, (String, String)> = local_namespaces
        .iter()
        .enumerate()
        .map(|(i, ns)| {
            let hue = (i as f32 * 360.0 / local_namespaces.len().max(1) as f32) % 360.0;
            let border = hsl_to_hex(hue, 0.7, 0.35); // Dark, saturated
            let default_fill = hsl_to_hex(hue, 0.3, 0.85); // Light, desaturated
            (ns.clone(), (border, default_fill))
        })
        .collect();

    // External crates get a distinct gray style
    let external_border = "#606060".to_string();
    let external_fill = "#d0d0d0".to_string();

    // Group local crates by namespace (excluding external)
    let mut local_crates_by_namespace: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    let mut external_crates: Vec<&str> = Vec::new();
    for crate_name in &sorted_crates {
        let (namespace, _) = get_crate_parts(crate_name);
        if namespace == "external" {
            external_crates.push(crate_name);
        } else {
            local_crates_by_namespace
                .entry(namespace)
                .or_default()
                .push(crate_name);
        }
    }

    // Build a map of external deps with their features per crate
    // Key: (namespace, external_dep, features_key) -> list of crates using it
    // This allows separate nodes for different feature sets within the same namespace
    #[allow(clippy::items_after_statements)]
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    struct ExternalDepKey {
        namespace: String,
        dep_name: String,
        features: Vec<String>, // sorted for deterministic hashing
    }

    let mut external_dep_nodes: BTreeMap<ExternalDepKey, Vec<String>> = BTreeMap::new();

    for (from, to) in &sorted_edges {
        let (from_ns, _) = get_crate_parts(from);
        let (to_ns, _) = get_crate_parts(to);

        // If a local crate depends on an external crate
        if from_ns != "external" && to_ns == "external" {
            // Get features this crate adds for this dep
            let features: Vec<String> = if let Some(feat_info) = external_dep_features.get(to) {
                feat_info
                    .crate_additions
                    .get(*from)
                    .map(|f| {
                        let mut v: Vec<_> = f.iter().cloned().collect();
                        v.sort();
                        v
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let key = ExternalDepKey {
                namespace: from_ns.clone(),
                dep_name: (*to).to_string(),
                features,
            };

            external_dep_nodes
                .entry(key)
                .or_default()
                .push((*from).to_string());
        }
    }

    // Helper to create a valid graphviz node ID
    let node_id = |name: &str| -> String { name.replace('-', "_") };

    // Helper to create external dep node ID (includes namespace and feature hash for uniqueness)
    let ext_node_id = |key: &ExternalDepKey| -> String {
        let safe_name = key.dep_name.replace('-', "_");
        if key.features.is_empty() {
            format!("{}_{}", key.namespace, safe_name)
        } else {
            // Include a hash of features for uniqueness
            let feat_hash: u32 = key.features.iter().fold(0u32, |acc, f| {
                acc.wrapping_mul(31).wrapping_add(
                    f.bytes()
                        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(u32::from(b))),
                )
            });
            format!("{}_{}_{:x}", key.namespace, safe_name, feat_hash)
        }
    };

    // Build DOT format string directly
    let mut dot = String::new();
    dot.push_str("digraph dependencies {\n");

    // Graph-level attributes
    dot.push_str("    rankdir=TB;\n");
    dot.push_str("    bgcolor=\"#f8f9fa\";\n");
    dot.push_str("    splines=ortho;\n");
    dot.push_str("    nodesep=0.5;\n");
    dot.push_str("    ranksep=0.8;\n");
    dot.push_str("    compound=true;\n");

    // Default node attributes
    dot.push_str("    node [shape=box, style=filled, fontname=\"Helvetica\", fontsize=11];\n");

    // Default edge attributes
    dot.push_str("    edge [arrowsize=0.7, color=\"#444444\"];\n\n");

    // Create subgraphs (clusters) for each local namespace
    for (namespace, crates) in &local_crates_by_namespace {
        let (cluster_border, cluster_fill) = namespace_colors
            .get(namespace)
            .cloned()
            .unwrap_or_else(|| (external_border.clone(), external_fill.clone()));

        // Cluster name must start with "cluster_" for graphviz to recognize it
        let _ = writeln!(dot, "    subgraph cluster_{namespace} {{");
        let _ = writeln!(dot, "        label=\"{namespace}\";");
        dot.push_str("        style=\"filled,rounded\";\n");
        let _ = writeln!(dot, "        fillcolor=\"{cluster_fill}\";");
        let _ = writeln!(dot, "        color=\"{cluster_border}\";");
        dot.push_str("        penwidth=2;\n");
        dot.push_str("        fontname=\"Helvetica Bold\";\n");
        dot.push_str("        fontsize=14;\n\n");

        // Add local crate nodes to cluster
        for crate_name in crates {
            let (_, tag) = get_crate_parts(crate_name);

            let (border_color, fill_color) = if let Some(tag_name) = &tag {
                let border = namespace_colors
                    .get(namespace)
                    .map(|(b, _)| b.clone())
                    .unwrap_or_else(|| external_border.clone());
                let tag_hue = string_to_hue(tag_name);
                let fill = hsl_to_hex(tag_hue, 0.5, 0.75);
                (border, fill)
            } else {
                namespace_colors
                    .get(namespace)
                    .cloned()
                    .unwrap_or_else(|| (external_border.clone(), external_fill.clone()))
            };

            let id = node_id(crate_name);
            let _ = writeln!(
                dot,
                "        {id} [label=\"{crate_name}\", fillcolor=\"{fill_color}\", color=\"{border_color}\", penwidth=2];"
            );
        }

        // Add external deps used by this namespace (one node per unique feature set)
        for key in external_dep_nodes.keys() {
            if key.namespace != *namespace {
                continue;
            }

            let id = ext_node_id(key);
            let label = if key.features.is_empty() {
                key.dep_name.clone()
            } else {
                let features_str = key.features.join(", ");
                format!("{}\\n[{}]", key.dep_name, features_str)
            };

            let _ = writeln!(
                dot,
                "        {id} [label=\"{label}\", fillcolor=\"{external_fill}\", color=\"{external_border}\", penwidth=2];"
            );
        }

        dot.push_str("    }\n\n");
    }

    // Add edges (outside of clusters) - arrows point from dependent to dependency
    for (from, to) in &sorted_edges {
        let (from_ns, _) = get_crate_parts(from);
        let (to_ns, _) = get_crate_parts(to);

        let to_is_external = to_ns == "external";

        let from_id = node_id(from);
        let to_id = if to_is_external {
            // External dep - find the matching node with the right features
            let features: Vec<String> = if let Some(feat_info) = external_dep_features.get(to) {
                feat_info
                    .crate_additions
                    .get(*from)
                    .map(|f| {
                        let mut v: Vec<_> = f.iter().cloned().collect();
                        v.sort();
                        v
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let key = ExternalDepKey {
                namespace: from_ns.clone(),
                dep_name: (*to).to_string(),
                features,
            };
            ext_node_id(&key)
        } else {
            node_id(to)
        };

        // Edges crossing namespace boundaries should be dashed
        let crosses_boundary = from_ns != to_ns && !to_is_external;
        if crosses_boundary {
            let _ = writeln!(
                dot,
                "    {from_id} -> {to_id} [style=dashed, color=\"#666666\", penwidth=1.5];"
            );
        } else {
            let _ = writeln!(dot, "    {from_id} -> {to_id};");
        }
    }

    dot.push_str("}\n");

    // Render to SVG using graphviz
    let svg_content = exec_dot(dot, vec![CommandArg::Format(Format::Svg)])
        .map_err(|e| std::io::Error::other(format!("graphviz error: {e}")))?;

    // Post-process SVG to add arrow markers along dashed edges
    let svg_content = String::from_utf8_lossy(&svg_content);
    let svg_content = post_process_dashed_edges(&svg_content);

    // Write to file
    std::fs::write(output_path, svg_content.as_bytes())?;

    println!(
        "Generated dependency graph with {} nodes and {} edges",
        sorted_crates.len(),
        sorted_edges.len()
    );

    Ok(())
}

// =============================================================================
// MAIN
// =============================================================================

/// Load all crate info from the provided crate directories.
fn load_crates(crate_dirs: &[PathBuf], config: &Config) -> Vec<CrateInfo> {
    crate_dirs
        .iter()
        .filter_map(|dir| {
            CrateInfo::load(dir).or_else(|| {
                eprintln!("Warning: Failed to load crate at {}", dir.display());
                None
            })
        })
        .filter(|info| !config.is_ignored_crate(&info.crate_name))
        .collect()
}

/// Embedded template for stow.toml
const STOW_TEMPLATE: &str = include_str!("stow_template.toml");

/// Embedded minimal template for stow.toml
const STOW_TEMPLATE_MINIMAL: &str = r#"# cargo-stow configuration
# See https://github.com/boundaryml/baml/tree/main/baml_language/crates/tools_stow for documentation

[[namespaces]]
name = "myproject"
approved_prefixes = []
test_crate_exceptions = []
"#;

fn run_init(workspace_root: &Path, minimal: bool) {
    let stow_toml = workspace_root.join("stow.toml");

    if stow_toml.exists() {
        eprintln!("Error: stow.toml already exists at {}", stow_toml.display());
        eprintln!("Remove it first if you want to regenerate.");
        std::process::exit(1);
    }

    let template = if minimal {
        STOW_TEMPLATE_MINIMAL
    } else {
        STOW_TEMPLATE
    };

    if let Err(e) = std::fs::write(&stow_toml, template) {
        eprintln!("Error: Failed to write stow.toml: {e}");
        std::process::exit(1);
    }

    println!("✅ Created {}", stow_toml.display());
    println!();
    println!("Next steps:");
    println!("  1. Edit stow.toml to configure your namespace(s)");
    println!("  2. Run `cargo stow` to validate your workspace");
    println!("  3. Run `cargo stow --fix` to auto-fix sortable issues");
}

fn main() {
    let Cargo::Stow(args) = Cargo::parse();

    let ManifestSelection {
        root_dir: manifest_root,
        manifest_path: workspace_cargo,
        is_workspace,
    } = find_best_manifest().expect("Could not find Cargo.toml");

    let metadata = load_metadata(&workspace_cargo);
    let workspace_root = PathBuf::from(metadata.workspace_root.as_str());

    // Handle init subcommand first (doesn't require config to exist)
    if let Some(Command::Init { minimal }) = args.command {
        run_init(&workspace_root, minimal);
        return;
    }

    // Load configuration (required for all other operations)
    let config = match Config::load(&workspace_root) {
        Ok(config) => config,
        Err(ConfigError::NotFound) => {
            eprintln!("Error: No stow.toml found.");
            eprintln!();
            eprintln!("Run `cargo stow init` to create a configuration file.");
            eprintln!();
            eprintln!("Alternatively, add [workspace.metadata.stow] to your Cargo.toml.");
            std::process::exit(1);
        }
        Err(ConfigError::ParseError(msg)) => {
            eprintln!("Error: {msg}");
            std::process::exit(1);
        }
    };

    // Load workspace dependencies with features (needed for both graph and validation)
    let workspace_deps = get_workspace_dependencies(&workspace_cargo);

    // Handle graph generation mode
    if let Some(graph_path) = &args.graph {
        println!("Generating dependency graph...");

        // Load crate feature info for external deps visualization
        let crate_dirs = workspace_crate_dirs(&metadata, &workspace_root);
        let crates = load_crates(&crate_dirs, &config);
        let crate_features: HashMap<String, HashMap<String, HashSet<String>>> = crates
            .iter()
            .map(|c| (c.crate_name.clone(), get_crate_dependencies(&c.doc)))
            .collect();

        match generate_dependency_graph_svg(
            &metadata,
            &config,
            graph_path,
            args.include_tests,
            &workspace_deps,
            &crate_features,
        ) {
            Ok(()) => {
                println!("Saved to: {}", graph_path.display());
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("Error generating graph: {e}");
                std::process::exit(1);
            }
        }
    }

    println!("Selected Cargo.toml: {}", workspace_cargo.display());
    if is_workspace {
        println!("Workspace root: {}", workspace_root.display());
    }
    println!();

    let mut discovered_crate_dirs = workspace_crate_dirs(&metadata, &workspace_root);
    if discovered_crate_dirs.is_empty() {
        discovered_crate_dirs.push(manifest_root);
    }
    let workspace_member_ids: HashSet<_> = metadata.workspace_members.iter().collect();
    let mut workspace_member_names: HashSet<String> = HashSet::new();
    for package in &metadata.packages {
        if workspace_member_ids.contains(&package.id) {
            workspace_member_names.insert(package.name.to_string());
        }
    }
    let member_roots = if is_workspace {
        workspace_member_roots(&workspace_root, &workspace_cargo)
    } else {
        vec![workspace_root.clone()]
    };

    // Load all crate data
    let mut crates = load_crates(&discovered_crate_dirs, &config);
    let crate_dirs: Vec<PathBuf> = crates
        .iter()
        .filter_map(|info| info.cargo_path.parent().map(PathBuf::from))
        .collect();

    // Collect all crate names for cross-crate validation
    let all_crate_names: HashSet<String> = crates.iter().map(|c| c.crate_name.clone()).collect();

    // Run fix mode first if requested
    if args.fix {
        println!("🔧 Fixing crates...\n");
        let mut total_fixed = 0;

        for crate_info in &crates {
            match fix_cargo_toml(&crate_info.cargo_path, &workspace_deps, &config) {
                Ok(true) => {
                    println!("Fixed {}", crate_info.crate_name);
                    total_fixed += 1;
                }
                Ok(false) => {}
                Err(e) => {
                    eprintln!("Error fixing {}: {e}", crate_info.crate_name);
                }
            }
        }

        if total_fixed > 0 {
            println!("\n✅ Fixed {total_fixed} file(s)");

            // Format all TOML files
            println!("\nFormatting TOML files...");
            match format_all_cargo_tomls(&crate_dirs, &workspace_cargo) {
                Ok(formatted) => {
                    if formatted > 0 {
                        println!("Formatted {formatted} file(s)");
                    }
                }
                Err(e) => {
                    eprintln!("Warning: formatting failed: {e}");
                }
            }

            println!("\nRe-running validation...\n");

            // Reload crate data after fixes
            for crate_info in &mut crates {
                if let Err(e) = crate_info.reload() {
                    eprintln!("Warning: Failed to reload {}: {e}", crate_info.crate_name);
                }
            }
        } else {
            println!("No fixes needed.\n");
        }
    }

    // Run validation
    let mut all_errors: Vec<ValidationError> = Vec::new();

    // Check for nested crates
    for crate_dir in &crate_dirs {
        all_errors.extend(check_no_nested_crates(
            crate_dir,
            &workspace_root,
            &member_roots,
        ));
    }

    // Per-crate validation
    for crate_info in &crates {
        if args.verbose {
            println!("Checking crate: {}", crate_info.crate_name);
        }

        all_errors.extend(check_crate_name_matches_folder(
            &crate_info.folder_name,
            &crate_info.doc,
            &crate_info.cargo_path,
            &config,
        ));
        all_errors.extend(check_crate_naming_convention(
            &crate_info.folder_name,
            &crate_info.doc,
            &crate_info.cargo_path,
            &config,
        ));
        all_errors.extend(check_test_crate_has_prefix(
            &crate_info.crate_name,
            &all_crate_names,
            &crate_info.cargo_path,
            &config,
        ));
        all_errors.extend(check_workspace_dependencies(
            &crate_info.crate_name,
            &crate_info.doc,
            &crate_info.cargo_path,
        ));
        all_errors.extend(check_dependency_restrictions(
            &crate_info.crate_name,
            &crate_info.doc,
            &crate_info.cargo_path,
            &config,
        ));
        all_errors.extend(check_dependencies_sorted(
            &crate_info.crate_name,
            &crate_info.doc,
            &crate_info.cargo_path,
            &config,
        ));
    }

    // Check for unused workspace dependencies (not referenced by any crate)
    let mut used_workspace_deps: HashSet<String> = HashSet::new();
    for crate_info in &crates {
        used_workspace_deps.extend(collect_dep_names(&crate_info.doc));
    }
    for dep_name in workspace_deps.keys() {
        if workspace_member_names.contains(dep_name) {
            continue;
        }
        if !used_workspace_deps.contains(dep_name) {
            all_errors.push(ValidationError {
                crate_name: "workspace".to_string(),
                file_path: workspace_cargo.clone(),
                message: format!("Workspace dependency '{dep_name}' is not used by any crate"),
            });
        }
    }

    if all_errors.is_empty() {
        println!("✅ All crates passed validation!");
        std::process::exit(0);
    }

    println!("❌ Found {} validation error(s):\n", all_errors.len());

    // Group errors by crate
    let mut errors_by_crate: BTreeMap<String, Vec<&ValidationError>> = BTreeMap::new();
    for error in &all_errors {
        errors_by_crate
            .entry(error.crate_name.clone())
            .or_default()
            .push(error);
    }

    for (crate_name, crate_errors) in &errors_by_crate {
        println!("  {crate_name}:");
        for error in crate_errors {
            println!("    • {}", error.message);
        }
        println!();
    }

    std::process::exit(1);
}
