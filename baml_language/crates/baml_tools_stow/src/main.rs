//! cargo-stow: Validates and fixes Cargo.toml files in the workspace.
//!
//! Rules:
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
//! Usage:
//!     cargo stow --check
//!     cargo stow --fix

// CLI tool - print statements and exit are expected
#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use clap::Parser;
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
}

/// Stow configuration - can be loaded from stow.toml or [workspace.metadata.stow]
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Crate namespaces (e.g., baml, bex)
    pub namespaces: Vec<Namespace>,
    /// Global dependency rules that apply to all namespaces
    #[serde(default)]
    pub dependency_rules: Vec<DependencyRule>,

    // Legacy fields for backward compatibility (converted to single namespace)
    #[serde(default)]
    approved_prefixes: Vec<String>,
    #[serde(default)]
    test_crate_exceptions: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            namespaces: vec![
                Namespace {
                    name: "baml".into(),
                    // Note: lsp, vm, ide removed - auto-allowed via _types/_tests suffix
                    approved_prefixes: vec![
                        "tools".into(),
                        "compiler".into(),
                        "builtins".into(),
                        "playground".into(),
                    ],
                    test_crate_exceptions: vec!["baml_tests".into()],
                    dependency_rules: vec![
                        DependencyRule {
                            pattern: Pattern::Simple("baml_compiler*".into()),
                            allowed_prefixes: vec!["compiler".into(), "lsp".into()],
                            allowed_crates: vec!["baml_db".into(), "baml_project".into()],
                            regular_deps_only: true,
                            reason: "Use baml_db or baml_project to access compiler interfaces."
                                .into(),
                        },
                        DependencyRule {
                            pattern: Pattern::WithExclusions {
                                select: "bex_*".into(),
                                exclude: vec!["bex_vm_types".into()],
                            },
                            allowed_prefixes: vec![],
                            allowed_crates: vec![],
                            regular_deps_only: true,
                            reason: "baml_* crates should not depend on bex_* crates.".into(),
                        },
                    ],
                },
                Namespace {
                    name: "bex".into(),
                    approved_prefixes: vec![], // bex_vm_types auto-allowed via _types suffix
                    test_crate_exceptions: vec![],
                    dependency_rules: vec![],
                },
            ],
            dependency_rules: vec![
                // Global rule: anyhow is only for CLI/LSP crates
                DependencyRule {
                    pattern: Pattern::Simple("anyhow".into()),
                    allowed_prefixes: vec!["lsp".into()],
                    allowed_crates: vec!["*_cli".into()],
                    regular_deps_only: true,
                    reason: "Use thiserror for proper error types in library crates.".into(),
                },
            ],
            // Legacy fields (empty when using namespaces)
            approved_prefixes: vec![],
            test_crate_exceptions: vec![],
        }
    }
}

impl Config {
    /// Load config from stow.toml or [workspace.metadata.stow] in Cargo.toml
    /// Priority: stow.toml > Cargo.toml metadata > defaults
    pub fn load(workspace_root: &Path) -> Self {
        // Try stow.toml first
        let stow_toml = workspace_root.join("stow.toml");
        if stow_toml.exists() {
            if let Ok(content) = std::fs::read_to_string(&stow_toml) {
                match toml::from_str::<Config>(&content) {
                    Ok(config) => {
                        eprintln!("Loaded config from {}", stow_toml.display());
                        return config.normalize();
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse {}: {}", stow_toml.display(), e);
                    }
                }
            }
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
                                match stow.clone().try_into::<Config>() {
                                    Ok(config) => {
                                        eprintln!(
                                            "Loaded config from [workspace.metadata.stow] in {}",
                                            cargo_toml.display()
                                        );
                                        return config.normalize();
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Warning: Failed to parse [workspace.metadata.stow]: {e}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fall back to defaults
        eprintln!("Using default config (no stow.toml or [workspace.metadata.stow] found)");
        Config::default()
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
            }];
        }
        self
    }

    /// Get the namespace for a given crate name
    pub fn get_namespace(&self, crate_name: &str) -> Option<&Namespace> {
        self.namespaces
            .iter()
            .find(|ns| crate_name.starts_with(&format!("{}_", ns.name)))
    }

    /// Check if a crate name belongs to any known namespace
    pub fn is_internal_crate(&self, crate_name: &str) -> bool {
        self.get_namespace(crate_name).is_some()
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

        // tools crates (in any namespace) are allowed to depend on anything
        for ns in &config.namespaces {
            if crate_name.starts_with(&format!("{}_tools_", ns.name)) {
                return true;
            }
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

        // If no allowed patterns specified, allow by default
        self.allowed_prefixes.is_empty() && self.allowed_crates.is_empty()
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
#[command(author, version, about = "Validate and fix Cargo.toml files")]
struct Args {
    /// Check for validation errors without fixing
    #[arg(long, conflicts_with = "fix")]
    check: bool,

    /// Automatically fix issues
    #[arg(long, conflicts_with = "check")]
    fix: bool,

    /// Show verbose output
    #[arg(short, long)]
    verbose: bool,
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

fn find_workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml).ok()?;
            if content.contains("[workspace]") {
                return Some(dir);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
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

fn get_workspace_dependencies(workspace_cargo: &Path) -> HashSet<String> {
    let content = std::fs::read_to_string(workspace_cargo).unwrap_or_default();
    let doc: DocumentMut = content.parse().unwrap_or_default();

    let mut deps = HashSet::new();
    if let Some(workspace) = doc.get("workspace").and_then(|w| w.as_table()) {
        if let Some(ws_deps) = workspace.get("dependencies").and_then(|d| d.as_table()) {
            for key in ws_deps.iter().map(|(k, _)| k) {
                deps.insert(key.to_string());
            }
        }
    }
    deps
}

// =============================================================================
// VALIDATION RULES
// =============================================================================

fn check_no_nested_crates(crate_dir: &Path, crates_dir: &Path) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Ok(relative) = crate_dir.strip_prefix(crates_dir) {
        if relative.components().count() > 1 {
            errors.push(ValidationError {
                crate_name: crate_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                file_path: crate_dir.join("Cargo.toml"),
                message: format!(
                    "Nested crate detected at '{}'. All crates must be directly under crates/",
                    relative.display()
                ),
            });
        }
    }
    errors
}

fn check_crate_name_matches_folder(
    folder_name: &str,
    doc: &DocumentMut,
    cargo_path: &Path,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if let Some(package) = doc.get("package").and_then(|p| p.as_table()) {
        if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
            if name != folder_name {
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
    workspace_deps: &HashSet<String>,
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
                    if !is_workspace_dependency(item) && workspace_deps.contains(&key) {
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

fn format_all_cargo_tomls(crates_dir: &Path, workspace_cargo: &Path) -> std::io::Result<usize> {
    let mut count = 0;

    // Format workspace Cargo.toml
    if format_toml_file(workspace_cargo)? {
        count += 1;
    }

    // Format all crate Cargo.tomls
    if let Ok(entries) = std::fs::read_dir(crates_dir) {
        for entry in entries.flatten() {
            let cargo_toml = entry.path().join("Cargo.toml");
            if cargo_toml.exists() {
                if format_toml_file(&cargo_toml)? {
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

// =============================================================================
// MAIN
// =============================================================================

/// Load all crate info from the crates directory.
fn load_crates(crate_dirs: &[PathBuf]) -> Vec<CrateInfo> {
    crate_dirs
        .iter()
        .filter_map(|dir| {
            CrateInfo::load(dir).or_else(|| {
                eprintln!("Warning: Failed to load crate at {}", dir.display());
                None
            })
        })
        .collect()
}

fn main() {
    let Cargo::Stow(args) = Cargo::parse();

    let workspace_root = find_workspace_root().expect("Could not find workspace root");
    let crates_dir = workspace_root.join("crates");
    let workspace_cargo = workspace_root.join("Cargo.toml");

    // Load configuration
    let config = Config::load(&workspace_root);

    println!("Validating crates in: {}", crates_dir.display());
    println!("Workspace Cargo.toml: {}", workspace_cargo.display());
    println!();

    let workspace_deps = get_workspace_dependencies(&workspace_cargo);
    let crate_dirs = find_crate_dirs(&crates_dir);

    // Load all crate data
    let mut crates = load_crates(&crate_dirs);

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
            match format_all_cargo_tomls(&crates_dir, &workspace_cargo) {
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
        all_errors.extend(check_no_nested_crates(crate_dir, &crates_dir));
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
