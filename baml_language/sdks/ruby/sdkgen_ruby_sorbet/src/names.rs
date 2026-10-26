use std::collections::{BTreeMap, BTreeSet};

use baml_sdkgen_types::{Name, NamingConvention, Symbol, SymbolPool, namespace_segments};

/// Claims include every callable sibling, even when its type is not mapped yet.
#[derive(Default)]
pub(crate) struct NameTable {
    used: BTreeSet<(String, String)>,
    allocated: BTreeMap<(String, String), String>,
    modules: BTreeMap<Vec<String>, String>,
}

impl NameTable {
    pub(crate) fn build(pool: &SymbolPool, naming: NamingConvention) -> Self {
        let mut table = Self::default();
        let symbols: BTreeMap<_, _> = pool.iter().collect();
        // Namespace modules and type constants share Ruby's constant scope.
        let mut children: BTreeMap<Vec<String>, BTreeSet<(String, String)>> = BTreeMap::new();
        for (name, symbol) in &symbols {
            let path = namespace_segments(name);
            for depth in 0..path.len() {
                children.entry(path[..depth].to_vec()).or_default().insert((
                    path[depth].clone(),
                    format!("namespace {}", path[..=depth].join(".")),
                ));
            }
            if !matches!(symbol, Symbol::Function(_)) {
                children
                    .entry(path)
                    .or_default()
                    .insert((name.name().to_string(), name.to_string()));
            }
        }
        table.modules.insert(vec![], "BamlSdk".into());
        for (parent, children) in children {
            let scope = table.modules[&parent].clone();
            let mut children: Vec<_> = children.into_iter().collect();
            children.sort_by(|(a, a_wire), (b, b_wire)| {
                (allocation_key(a), a_wire).cmp(&(allocation_key(b), b_wire))
            });
            for (raw, wire) in children {
                let ruby = table.claim(&scope, &constant(&raw), &wire);
                if wire.starts_with("namespace ") {
                    let mut path = parent.clone();
                    path.push(raw);
                    table.modules.insert(path, format!("{scope}::{ruby}"));
                }
            }
        }
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        symbols.sort_by_key(|(name, _)| (allocation_key(name.name().as_str()).0, name.to_string()));
        for (name, symbol) in symbols {
            match symbol {
                Symbol::Class(class) => {
                    let scope = table.qualified(name);
                    let mut fields: Vec<_> = class.properties.iter().collect();
                    fields.sort_by_key(|field| allocation_key(field.name.as_str()));
                    for field in fields {
                        table.claim(
                            &scope,
                            &identifier(field.name.as_str(), naming),
                            &format!("field {name}.{}", field.name),
                        );
                    }
                    let mut methods: Vec<_> = class
                        .static_methods
                        .iter()
                        .map(|m| (false, m))
                        .chain(class.instance_methods.iter().map(|m| (true, m)))
                        .collect();
                    methods.sort_by_key(|(_, method)| allocation_key(method.name.as_str()));
                    for (instance, method) in methods {
                        let method_scope = if instance {
                            scope.clone()
                        } else {
                            format!("{scope}.self")
                        };
                        table.callable(
                            &method_scope,
                            &identifier(callable_parts(method.name.as_str()).0, naming),
                            &format!("{name}.{}", method.name),
                        );
                    }
                }
                Symbol::Enum(enum_) => {
                    let scope = table.qualified(name);
                    let mut variants: Vec<_> = enum_.variants.iter().collect();
                    variants.sort_by_key(|variant| allocation_key(variant.name.as_str()));
                    for variant in variants {
                        table.claim(
                            &scope,
                            &constant(variant.name.as_str()),
                            &format!("{name}.{}", variant.name),
                        );
                    }
                }
                Symbol::Function(function) => {
                    table.callable(
                        &format!("{}.self", table.module_name(name)),
                        &identifier(callable_parts(function.name.as_str()).0, naming),
                        &name.to_string(),
                    );
                }
                Symbol::TypeAlias(_) => {}
            }
        }
        table
    }

    pub(crate) fn claim(&mut self, scope: &str, ruby: &str, baml: &str) -> String {
        let key = (scope.to_owned(), baml.to_owned());
        if let Some(allocated) = self.allocated.get(&key) {
            return allocated.clone();
        }
        let mut ruby = ruby.to_owned();
        while self.used.contains(&(scope.to_owned(), ruby.clone())) {
            ruby.push('_');
        }
        self.used.insert((scope.to_owned(), ruby.clone()));
        self.allocated.insert(key, ruby.clone());
        ruby
    }

    pub(crate) fn callable(&mut self, scope: &str, ruby: &str, wire: &str) -> String {
        let (base, _) = callable_parts(wire);
        let key = (scope.to_owned(), base.to_owned());
        if let Some(allocated) = self.allocated.get(&key) {
            return allocated.clone();
        }
        let suffixes = [
            "",
            "_async",
            "_stream",
            "_stream_async",
            "_spec",
            "_spec_async",
        ];
        let mut ruby = ruby.to_owned();
        // Fields are allocated first. Escape the whole family until every reader
        // and callable spelling is free, including repeated underscore collisions.
        while suffixes.iter().any(|suffix| {
            self.used
                .contains(&(scope.to_owned(), format!("{ruby}{suffix}")))
        }) {
            ruby.push('_');
        }
        for suffix in suffixes {
            self.used
                .insert((scope.to_owned(), format!("{ruby}{suffix}")));
        }
        self.allocated.insert(key, ruby.clone());
        ruby
    }

    pub(crate) fn module_name(&self, name: &Name) -> String {
        self.modules
            .get(&namespace_segments(name))
            .cloned()
            .unwrap_or_else(|| module_name(name))
    }

    pub(crate) fn qualified(&self, name: &Name) -> String {
        let module = self.module_name(name);
        let ruby = self
            .allocated
            .get(&(module.clone(), name.to_string()))
            .cloned()
            .unwrap_or_else(|| constant(name.name().as_str()));
        format!("{module}::{ruby}")
    }
}

// Match Python names.rs::allocate: legal authored identifiers first, then BAML
// identity. Ruby projection must not decide which BAML declaration wins.
fn allocation_key(raw: &str) -> (bool, &str) {
    let python_keyword = [
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield",
    ]
    .contains(&raw);
    let mut chars = raw.chars();
    let legal = chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
        && !python_keyword;
    (!legal, raw)
}

pub(crate) fn callable_parts(name: &str) -> (&str, &str) {
    for (wire, ruby) in [("@stream", "_stream"), ("@spec", "_spec")] {
        if let Some(base) = name.strip_suffix(wire) {
            return (base, ruby);
        }
    }
    (name, "")
}

pub(crate) fn module_name(name: &Name) -> String {
    std::iter::once("BamlSdk".to_owned())
        .chain(namespace_segments(name).iter().map(|n| constant(n)))
        .collect::<Vec<_>>()
        .join("::")
}

pub(crate) fn constant(name: &str) -> String {
    let mut capitalize = true;
    let mut result = String::new();
    for c in name.chars() {
        if c == '_' {
            capitalize = true;
        } else if capitalize {
            result.extend(c.to_uppercase());
            capitalize = false;
        } else {
            result.push(c);
        }
    }
    if name.starts_with('_') {
        format!("Baml{result}")
    } else {
        result
    }
}

pub(crate) fn snake_case(name: &str) -> String {
    project(name, NamingConvention::Language)
}

fn project(name: &str, naming: NamingConvention) -> String {
    let mut result = String::new();
    let chars: Vec<_> = name.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if naming == NamingConvention::Language && c.is_uppercase() {
            if i > 0
                && chars[i - 1] != '_'
                && (chars[i - 1].is_lowercase()
                    || chars.get(i + 1).is_some_and(|c| c.is_lowercase()))
            {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(*c);
        }
    }
    result
}

const KEYWORDS: &[&str] = &[
    "BEGIN",
    "END",
    "alias",
    "and",
    "begin",
    "break",
    "case",
    "class",
    "def",
    "defined",
    "do",
    "else",
    "elsif",
    "end",
    "ensure",
    "false",
    "for",
    "if",
    "in",
    "module",
    "next",
    "nil",
    "not",
    "or",
    "redo",
    "rescue",
    "retry",
    "return",
    "self",
    "super",
    "then",
    "true",
    "undef",
    "unless",
    "until",
    "when",
    "while",
    "yield",
    "__FILE__",
    "__LINE__",
    "__ENCODING__",
];

pub(crate) fn identifier(name: &str, naming: NamingConvention) -> String {
    let mut result = project(name, naming);
    // Preserve the existing protection for Object/Module/T::Struct methods.
    if [
        "method",
        "methods",
        "new",
        "send",
        "public_send",
        "object_id",
        "hash",
        "freeze",
        "dup",
        "clone",
        "inspect",
        "to_s",
        "to_h",
        "initialize",
        "instance_variable_get",
        "instance_variable_set",
        "define_method",
        "const_get",
        "extend",
        "include",
        "sig",
        "props",
        "serialize",
        "deserialize",
    ]
    .contains(&result.as_str())
    {
        return format!("baml_{result}");
    }
    if KEYWORDS.contains(&result.as_str()) || ["_baml", "_types"].contains(&result.as_str()) {
        result.push('_');
    }
    result
}

pub(crate) fn parameter(
    name: &str,
    naming: NamingConvention,
    used: &mut BTreeSet<String>,
) -> String {
    let mut result = project(name, naming);
    while KEYWORDS.contains(&result.as_str()) || used.contains(&result) {
        result.push('_');
    }
    used.insert(result.clone());
    result
}

pub(crate) fn quote(value: &str) -> String {
    format!("{value:?}").replace('#', "\\#")
}
