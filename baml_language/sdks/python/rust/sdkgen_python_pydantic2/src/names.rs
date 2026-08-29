//! Python host-name projection.
//!
//! BAML identifiers are wire identities.  This module allocates a separate,
//! valid Python spelling for every public binding and keeps the mapping in one
//! table so definitions, references, stubs, and runtime dispatch cannot drift.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::routing::{LeafPath, raw_route_segments, sanitize_python_module_segment};

/// One sync or async Python binding for a concrete BAML callable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum BindingRole {
    DirectSync,
    DirectAsync,
}

impl BindingRole {
    pub(crate) const fn is_async(self) -> bool {
        matches!(self, Self::DirectAsync)
    }

    pub(crate) const fn registry_key(self) -> &'static str {
        match self {
            Self::DirectSync => "direct",
            Self::DirectAsync => "direct_async",
        }
    }

    fn candidate(self, direct_name: &str) -> String {
        match self {
            Self::DirectSync => direct_name.to_string(),
            Self::DirectAsync => format!("{direct_name}_async"),
        }
    }

    fn report_label(self) -> &'static str {
        match self {
            Self::DirectSync => "direct binding",
            Self::DirectAsync => "async binding",
        }
    }
}

/// Why a public BAML identifier needed a different Python spelling.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdentifierRenameReason {
    PythonKeyword,
    InvalidIdentifier,
    HostControl,
    FrameworkProtected,
    Collision,
}

impl std::fmt::Display for IdentifierRenameReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::PythonKeyword => "Python keyword",
            Self::InvalidIdentifier => "invalid Python identifier",
            Self::HostControl => "generated call-control parameter",
            Self::FrameworkProtected => "framework-protected spelling",
            Self::Collision => "collision after Python projection",
        })
    }
}

/// One logical, user-visible host rename.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IdentifierRename {
    pub kind: String,
    pub fqn: String,
    pub original: String,
    pub generated: String,
    pub reason: IdentifierRenameReason,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PythonNames {
    module_segments: HashMap<Vec<String>, String>,
    symbol_names: HashMap<Name, String>,
    callable_names: HashMap<(String, BindingRole), String>,
    field_names: HashMap<(Name, String), String>,
    enum_variant_names: HashMap<(Name, String), String>,
    param_names: HashMap<(String, String), String>,
    generic_names: HashMap<(String, String), String>,
    renames: Vec<IdentifierRename>,
}

#[derive(Clone)]
struct Entry {
    id: String,
    raw: String,
    kind: String,
    fqn: String,
    protected: Option<IdentifierRenameReason>,
    report: bool,
}

impl PythonNames {
    pub(crate) fn build(pool: &SymbolPool) -> Self {
        let mut names = Self::default();
        names.allocate_modules(pool);
        names.allocate_leaf_bindings(pool);
        names.allocate_member_bindings(pool);
        names.renames.sort();
        names.renames.dedup();
        names
    }

    pub(crate) fn renames(&self) -> &[IdentifierRename] {
        &self.renames
    }

    pub(crate) fn route(&self, name: &Name, symbol: &Symbol) -> LeafPath {
        self.route_inner(name, !matches!(symbol, Symbol::Function(_)))
    }

    pub(crate) fn route_class_ref(&self, name: &Name) -> LeafPath {
        self.route_inner(name, true)
    }

    fn route_inner(&self, name: &Name, honor_stream_suffix: bool) -> LeafPath {
        let raw = raw_route_segments(name, honor_stream_suffix);
        let mut prefix = Vec::new();
        let mut projected = Vec::with_capacity(raw.len());
        for segment in raw {
            prefix.push(segment.clone());
            projected.push(
                self.module_segments
                    .get(&prefix)
                    .cloned()
                    .unwrap_or_else(|| sanitize_python_module_segment(&segment)),
            );
        }
        LeafPath {
            segments: projected,
        }
    }

    pub(crate) fn symbol<'a>(&'a self, name: &Name) -> std::borrow::Cow<'a, str> {
        self.symbol_names
            .get(name)
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| std::borrow::Cow::Owned(project_identifier(name.bare_name()).0))
    }

    pub(crate) fn callable<'a>(
        &'a self,
        fqn: &str,
        role: BindingRole,
    ) -> std::borrow::Cow<'a, str> {
        self.callable_names
            .get(&(fqn.to_string(), role))
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| {
                let raw = fqn.rsplit('.').next().unwrap_or(fqn);
                let root = project_identifier(raw).0;
                std::borrow::Cow::Owned(role.candidate(&root))
            })
    }

    pub(crate) fn field<'a>(&'a self, owner: &Name, raw: &str) -> std::borrow::Cow<'a, str> {
        self.field_names
            .get(&(owner.clone(), raw.to_string()))
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| std::borrow::Cow::Owned(project_field_identifier(raw).0))
    }

    pub(crate) fn enum_variant<'a>(&'a self, owner: &Name, raw: &str) -> std::borrow::Cow<'a, str> {
        self.enum_variant_names
            .get(&(owner.clone(), raw.to_string()))
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| std::borrow::Cow::Owned(project_identifier(raw).0))
    }

    pub(crate) fn param<'a>(&'a self, fqn: &str, raw: &str) -> std::borrow::Cow<'a, str> {
        self.param_names
            .get(&(fqn.to_string(), raw.to_string()))
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| std::borrow::Cow::Owned(project_identifier(raw).0))
    }

    pub(crate) fn generic<'a>(&'a self, owner: &str, raw: &str) -> std::borrow::Cow<'a, str> {
        self.generic_names
            .get(&(owner.to_string(), raw.to_string()))
            .map(|value| std::borrow::Cow::Borrowed(value.as_str()))
            .unwrap_or_else(|| std::borrow::Cow::Owned(project_identifier(raw).0))
    }

    fn allocate_modules(&mut self, pool: &SymbolPool) {
        let mut paths = BTreeSet::new();
        let mut reportable_paths = BTreeSet::new();
        for (name, symbol) in pool {
            let symbol_path = raw_route_segments(name, !matches!(symbol, Symbol::Function(_)));
            let type_path = raw_route_segments(name, true);
            if is_reportable_user_name(name) {
                for path in [&symbol_path, &type_path] {
                    for len in 1..=path.len() {
                        reportable_paths.insert(path[..len].to_vec());
                    }
                }
            }
            paths.insert(symbol_path);
            paths.insert(type_path);
        }

        let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
        for path in &paths {
            for index in 0..path.len() {
                children
                    .entry(path[..index].to_vec())
                    .or_default()
                    .insert(path[index].clone());
            }
        }

        for (parent, child_names) in children {
            let entries = child_names
                .into_iter()
                .map(|raw| {
                    let path = parent
                        .iter()
                        .chain(std::iter::once(&raw))
                        .cloned()
                        .collect::<Vec<_>>();
                    Entry {
                        id: raw.clone(),
                        kind: "module segment".to_string(),
                        fqn: path.join("."),
                        protected: (raw == "type")
                            .then_some(IdentifierRenameReason::FrameworkProtected),
                        raw,
                        report: reportable_paths.contains(&path),
                    }
                })
                .collect();
            for (entry, generated, reason) in allocate(entries, &[]) {
                let mut key = parent.clone();
                key.push(entry.raw.clone());
                self.module_segments.insert(key, generated.clone());
                self.record(entry, generated, reason);
            }
        }
    }

    fn allocate_leaf_bindings(&mut self, pool: &SymbolPool) {
        let mut by_leaf: BTreeMap<LeafPath, Vec<(&Name, &Symbol)>> = BTreeMap::new();
        for (name, symbol) in pool {
            by_leaf
                .entry(self.route(name, symbol))
                .or_default()
                .push((name, symbol));
        }

        for (_leaf, mut symbols) in by_leaf {
            symbols.sort_by_key(|(name, _)| *name);
            let primaries = symbols
                .iter()
                .map(|(name, symbol)| {
                    let (raw, kind, fqn) = match symbol {
                        Symbol::Class(_) => (name.bare_name(), "class", name.to_string()),
                        Symbol::Enum(_) => (name.bare_name(), "enum", name.to_string()),
                        Symbol::TypeAlias(_) => (name.bare_name(), "type alias", name.to_string()),
                        Symbol::Function(_) => (name.bare_name(), "function", name.to_string()),
                    };
                    Entry {
                        id: fqn.clone(),
                        raw: raw.to_string(),
                        kind: kind.to_string(),
                        fqn,
                        protected: None,
                        report: is_reportable_user_name(name),
                    }
                })
                .collect();

            let mut used = HashSet::new();
            for (entry, generated, reason) in allocate(primaries, &[]) {
                used.insert(generated.clone());
                let name = symbols
                    .iter()
                    .find(|(name, _)| name.to_string() == entry.id)
                    .map(|(name, _)| (*name).clone())
                    .expect("leaf binding must refer to a pool symbol");
                match pool.get(&name) {
                    Some(Symbol::Function(_)) => {
                        self.callable_names.insert(
                            (name.to_string(), BindingRole::DirectSync),
                            generated.clone(),
                        );
                    }
                    Some(_) => {
                        self.symbol_names.insert(name, generated.clone());
                    }
                    None => unreachable!("name came from the pool"),
                }
                self.record(entry, generated, reason);
            }

            // All derived host roles are allocated after authored declarations,
            // so a real BAML spelling always wins a projected-name collision.
            for (name, symbol) in &symbols {
                let Symbol::Function(function) = symbol else {
                    continue;
                };
                let fqn = name.to_string();
                let direct = self.callable(&fqn, BindingRole::DirectSync).into_owned();
                for role in secondary_roles(function) {
                    let candidate = role.candidate(&direct);
                    let generated = allocate_one(&candidate, &mut used);
                    self.callable_names
                        .insert((fqn.clone(), role), generated.clone());
                    if generated != candidate && is_reportable_user_name(name) {
                        self.renames.push(IdentifierRename {
                            kind: format!("function {}", role.report_label()),
                            fqn: fqn.clone(),
                            original: candidate,
                            generated,
                            reason: IdentifierRenameReason::Collision,
                        });
                    }
                }
            }
        }
    }

    fn allocate_member_bindings(&mut self, pool: &SymbolPool) {
        let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
        entries.sort_by_key(|(name, _)| *name);
        for (owner, symbol) in entries {
            match symbol {
                Symbol::Enum(value) => {
                    let variants = value
                        .variants
                        .iter()
                        .map(|variant| {
                            let raw = variant.name.as_str().to_string();
                            Entry {
                                id: raw.clone(),
                                kind: "enum variant".to_string(),
                                fqn: format!("{owner}.{raw}"),
                                raw,
                                protected: None,
                                report: is_reportable_user_name(owner),
                            }
                        })
                        .collect();
                    for (entry, generated, reason) in allocate(variants, &[]) {
                        self.enum_variant_names
                            .insert((owner.clone(), entry.raw.clone()), generated.clone());
                        self.record(entry, generated, reason);
                    }
                }
                Symbol::Class(value) => {
                    self.allocate_class_members(owner, value);
                    self.allocate_generics(
                        owner.to_string().as_str(),
                        "class type parameter",
                        value.generic_params.iter().map(baml_base::Name::as_str),
                    );
                    for method in value
                        .static_methods
                        .iter()
                        .chain(value.instance_methods.iter())
                    {
                        let fqn = format!("{owner}.{}", method.name);
                        self.allocate_callable_signature(
                            &fqn,
                            &method.arguments,
                            value
                                .instance_methods
                                .iter()
                                .any(|candidate| std::ptr::eq(candidate, method)),
                        );
                        self.allocate_generics(
                            &fqn,
                            "method type parameter",
                            method.generic_params.iter().map(baml_base::Name::as_str),
                        );
                    }
                }
                Symbol::Function(value) => {
                    let fqn = owner.to_string();
                    self.allocate_callable_signature(&fqn, &value.arguments, false);
                    self.allocate_generics(
                        &fqn,
                        "function type parameter",
                        value.generic_params.iter().map(baml_base::Name::as_str),
                    );
                }
                Symbol::TypeAlias(_) => {}
            }
        }
    }

    fn allocate_class_members(&mut self, owner: &Name, class: &baml_codegen_types::Class) {
        let mut primaries = Vec::new();
        for field in &class.properties {
            let raw = field.name.as_str().to_string();
            primaries.push(Entry {
                id: format!("0:{raw}"),
                kind: "class field".to_string(),
                fqn: format!("{owner}.{raw}"),
                protected: is_pydantic_protected(&raw)
                    .then_some(IdentifierRenameReason::FrameworkProtected),
                raw,
                report: is_reportable_user_name(owner),
            });
        }
        for method in class
            .static_methods
            .iter()
            .chain(class.instance_methods.iter())
        {
            let raw = method.name.as_str().to_string();
            primaries.push(Entry {
                id: format!("1:{raw}"),
                kind: "method".to_string(),
                fqn: format!("{owner}.{raw}"),
                protected: is_pydantic_protected(&raw)
                    .then_some(IdentifierRenameReason::FrameworkProtected),
                raw,
                report: is_reportable_user_name(owner),
            });
        }

        let mut used = HashSet::new();
        for (entry, generated, reason) in allocate(primaries, &["model_config"]) {
            used.insert(generated.clone());
            let raw = entry.raw.clone();
            if entry.kind == "class field" {
                self.field_names
                    .insert((owner.clone(), raw), generated.clone());
            } else {
                self.callable_names.insert(
                    (format!("{owner}.{raw}"), BindingRole::DirectSync),
                    generated.clone(),
                );
            }
            self.record(entry, generated, reason);
        }

        for method in class
            .static_methods
            .iter()
            .chain(class.instance_methods.iter())
        {
            let fqn = format!("{owner}.{}", method.name);
            let direct = self.callable(&fqn, BindingRole::DirectSync).into_owned();
            for role in secondary_roles(method) {
                let candidate = role.candidate(&direct);
                let generated = allocate_one(&candidate, &mut used);
                self.callable_names
                    .insert((fqn.clone(), role), generated.clone());
                if generated != candidate && is_reportable_user_name(owner) {
                    self.renames.push(IdentifierRename {
                        kind: format!("method {}", role.report_label()),
                        fqn: fqn.clone(),
                        original: candidate,
                        generated,
                        reason: IdentifierRenameReason::Collision,
                    });
                }
            }
        }
    }

    fn allocate_callable_signature(
        &mut self,
        fqn: &str,
        arguments: &[baml_codegen_types::FunctionArgument],
        instance: bool,
    ) {
        let entries = arguments
            .iter()
            .map(|argument| {
                let raw = argument.name.as_str().to_string();
                Entry {
                    id: raw.clone(),
                    kind: "parameter".to_string(),
                    fqn: format!("{fqn}.{raw}"),
                    protected: matches!(raw.as_str(), "_ctx" | "_types")
                        .then_some(IdentifierRenameReason::HostControl),
                    raw,
                    report: is_reportable_user_fqn(fqn),
                }
            })
            .collect();
        let reserved = if instance {
            vec!["self", "_ctx", "_types"]
        } else {
            vec!["_ctx", "_types"]
        };
        for (entry, generated, reason) in allocate(entries, &reserved) {
            self.param_names
                .insert((fqn.to_string(), entry.raw.clone()), generated.clone());
            self.record(entry, generated, reason);
        }
    }

    fn allocate_generics<'a>(
        &mut self,
        owner: &str,
        kind: &str,
        params: impl Iterator<Item = &'a str>,
    ) {
        let entries = params
            .map(|raw| Entry {
                id: raw.to_string(),
                raw: raw.to_string(),
                kind: kind.to_string(),
                fqn: format!("{owner}.<{raw}>"),
                protected: None,
                report: is_reportable_user_fqn(owner),
            })
            .collect();
        for (entry, generated, reason) in allocate(entries, &[]) {
            self.generic_names
                .insert((owner.to_string(), entry.raw.clone()), generated.clone());
            self.record(entry, generated, reason);
        }
    }

    fn record(&mut self, entry: Entry, generated: String, reason: Option<IdentifierRenameReason>) {
        if entry.report && generated != entry.raw {
            self.renames.push(IdentifierRename {
                kind: entry.kind,
                fqn: canonical_report_fqn(&entry.fqn),
                original: entry.raw,
                generated,
                reason: reason.unwrap_or(IdentifierRenameReason::Collision),
            });
        }
    }
}

fn secondary_roles(function: &baml_codegen_types::Function) -> Vec<BindingRole> {
    let _ = function;
    vec![BindingRole::DirectAsync]
}

fn allocate(
    mut entries: Vec<Entry>,
    reserved: &[&str],
) -> Vec<(Entry, String, Option<IdentifierRenameReason>)> {
    // Legal authored spellings win over names that only project onto them.
    entries.sort_by_key(|entry| {
        let legal = entry.protected.is_none() && is_python_identifier(&entry.raw);
        (!legal, entry.id.clone())
    });
    let mut used: HashSet<String> = reserved.iter().map(|name| (*name).to_string()).collect();
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let (mut candidate, mut reason) = if entry.kind == "class field" {
            project_field_identifier(&entry.raw)
        } else {
            project_identifier(&entry.raw)
        };
        if let Some(protected) = &entry.protected {
            reason = Some(protected.clone());
            if candidate == entry.raw {
                candidate.push('_');
            }
        }
        let before_collision = candidate.clone();
        let generated = allocate_one(&candidate, &mut used);
        if generated != before_collision {
            reason = Some(IdentifierRenameReason::Collision);
        }
        out.push((entry, generated, reason));
    }
    out
}

fn allocate_one(candidate: &str, used: &mut HashSet<String>) -> String {
    let mut generated = candidate.to_string();
    while used.contains(&generated) {
        generated.push('_');
    }
    used.insert(generated.clone());
    generated
}

pub(crate) const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

pub(crate) fn is_python_keyword(value: &str) -> bool {
    PYTHON_KEYWORDS.contains(&value)
}

pub(crate) fn is_python_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && chars.all(|value| value == '_' || value.is_alphanumeric())
        && !is_python_keyword(value)
}

fn project_identifier(value: &str) -> (String, Option<IdentifierRenameReason>) {
    let mut generated = String::with_capacity(value.len().max(1));
    for (index, ch) in value.chars().enumerate() {
        if ch == '_' || ch.is_alphanumeric() && (index > 0 || ch.is_alphabetic()) {
            generated.push(ch);
        } else {
            generated.push('_');
        }
    }
    if generated.is_empty() {
        generated.push('_');
    }
    let mut reason = (generated != value).then_some(IdentifierRenameReason::InvalidIdentifier);
    if is_python_keyword(&generated) {
        generated.push('_');
        reason = Some(IdentifierRenameReason::PythonKeyword);
    }
    (generated, reason)
}

fn project_field_identifier(value: &str) -> (String, Option<IdentifierRenameReason>) {
    if value.starts_with('_') {
        let suffix = value.trim_start_matches('_');
        let (suffix, _) = project_identifier(if suffix.is_empty() { "field" } else { suffix });
        return (
            format!("field_{suffix}"),
            Some(IdentifierRenameReason::FrameworkProtected),
        );
    }
    project_identifier(value)
}

fn is_pydantic_protected(value: &str) -> bool {
    value.starts_with('_')
        || matches!(
            value,
            "model_config"
                | "model_fields"
                | "model_computed_fields"
                | "model_extra"
                | "model_fields_set"
                | "model_construct"
                | "model_copy"
                | "model_dump"
                | "model_dump_json"
                | "model_json_schema"
                | "model_parametrized_name"
                | "model_post_init"
                | "model_rebuild"
                | "model_validate"
                | "model_validate_json"
                | "model_validate_strings"
        )
}

fn is_reportable_user_name(name: &Name) -> bool {
    name.package().as_str() == "user" && !name.bare_name().contains('$')
}

fn is_reportable_user_fqn(fqn: &str) -> bool {
    fqn.starts_with("user.") && !fqn.contains('$')
}

fn canonical_report_fqn(fqn: &str) -> String {
    fqn.strip_prefix("stream_types.")
        .unwrap_or(fqn)
        .replace("$stream", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hard_keywords_project_to_identifiers() {
        for keyword in [
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
            "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
            "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield",
        ] {
            let (projected, reason) = project_identifier(keyword);
            assert_eq!(projected, format!("{keyword}_"));
            assert_eq!(reason, Some(IdentifierRenameReason::PythonKeyword));
            assert!(is_python_identifier(&projected));
        }
    }

    #[test]
    fn legal_spelling_wins_normalization_collision() {
        let entries = vec![
            Entry {
                id: "a".into(),
                raw: "foo-bar".into(),
                kind: "test".into(),
                fqn: "foo-bar".into(),
                protected: None,
                report: true,
            },
            Entry {
                id: "z".into(),
                raw: "foo_bar".into(),
                kind: "test".into(),
                fqn: "foo_bar".into(),
                protected: None,
                report: true,
            },
        ];
        let result = allocate(entries, &[]);
        let by_raw: BTreeMap<_, _> = result
            .into_iter()
            .map(|(entry, generated, _)| (entry.raw, generated))
            .collect();
        assert_eq!(by_raw["foo_bar"], "foo_bar");
        assert_eq!(by_raw["foo-bar"], "foo_bar_");
    }

    #[test]
    fn leading_underscore_fields_get_a_non_private_candidate() {
        let (projected, reason) = project_field_identifier("_secret");
        assert_eq!(projected, "field_secret");
        assert_eq!(reason, Some(IdentifierRenameReason::FrameworkProtected));
    }
}
