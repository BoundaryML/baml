use std::collections::{BTreeMap, BTreeSet};

use baml_sdkgen_types::{Name, NamingConvention, Symbol, SymbolPool, Ty, namespace_segments};

/// Claims include every callable sibling, even when its type is not mapped yet.
#[derive(Default)]
pub(crate) struct NameTable {
    used: BTreeSet<(String, String)>,
    allocated: BTreeMap<(String, String), String>,
    modules: BTreeMap<Vec<String>, String>,
    input_aliases: BTreeMap<Name, String>,
    alias_components: BTreeMap<Name, usize>,
}

impl NameTable {
    pub(crate) fn build(pool: &SymbolPool, naming: NamingConvention) -> Self {
        let mut table = Self {
            alias_components: crate::alias_components::components(pool),
            ..Self::default()
        };
        for reserved in ["BYTECODE", "METADATA"] {
            table.used.insert(("BamlSdk".into(), reserved.into()));
        }
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
                            &callable_identifier(
                                callable_parts(method.name.as_str()).0,
                                naming,
                                !instance,
                            ),
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
                            &variant_constant(variant.name.as_str()),
                            &format!("{name}.{}", variant.name),
                        );
                    }
                }
                Symbol::Function(function) => {
                    table.callable(
                        &format!("{}.self", table.module_name(name)),
                        &callable_identifier(
                            callable_parts(function.name.as_str()).0,
                            naming,
                            true,
                        ),
                        &name.to_string(),
                    );
                }
                Symbol::TypeAlias(_) => {}
            }
        }
        // Propagate widening through named edges without expanding alias bodies.
        let mut widened = BTreeSet::new();
        loop {
            let additions: Vec<_> = pool
                .iter()
                .filter_map(|(name, symbol)| match symbol {
                    Symbol::TypeAlias(alias)
                        if !widened.contains(name)
                            && input_differs(&alias.resolves_to, &widened) =>
                    {
                        Some(name.clone())
                    }
                    _ => None,
                })
                .collect();
            if additions.is_empty() {
                break;
            }
            widened.extend(additions);
        }
        // Claim companions after authored constants so user names keep priority.
        for name in widened {
            let module = table.module_name(&name);
            let output = table.qualified(&name);
            let base = output.rsplit("::").next().unwrap();
            let ruby = table.claim(
                &module,
                &format!("{base}Input"),
                &format!("input alias {name}"),
            );
            table
                .input_aliases
                .insert(name, format!("{module}::{ruby}"));
        }
        table
    }

    pub(crate) fn same_alias_component(&self, owner: &Name, target: &Name) -> bool {
        self.alias_components
            .get(owner)
            .is_some_and(|component| self.alias_components.get(target) == Some(component))
    }

    pub(crate) fn input_qualified(&self, name: &Name) -> String {
        self.input_aliases
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.qualified(name))
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

fn input_differs(ty: &Ty, widened: &BTreeSet<Name>) -> bool {
    match ty {
        Ty::Float => true,
        Ty::TypeAlias(name) => widened.contains(name),
        Ty::List(item) => input_differs(item, widened),
        Ty::Map { key, value } => input_differs(key, widened) || input_differs(value, widened),
        Ty::Union(items) | Ty::Class(_, items) => items.iter().any(|ty| input_differs(ty, widened)),
        Ty::Function { params, ret, .. } => {
            params.iter().any(|param| input_differs(&param.ty, widened))
                || input_differs(ret, widened)
        }
        _ => false,
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

pub(crate) fn variant_constant(name: &str) -> String {
    if name.starts_with('_') {
        format!("Baml{}", variant_constant(name.trim_start_matches('_')))
    } else {
        let mut chars = name.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
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

// Generated once with Ruby 3.2.2, `require "bundler/setup"` and
// `require "sorbet-runtime"` (Bundler makes Kernel#gem public):
// Object.instance_methods.map(&:to_s).sort
// (Module.instance_methods | Class.instance_methods).map(&:to_s).sort
// T::Struct.singleton_methods.map(&:to_s).sort
// Class adds singleton callables such as allocate and superclass.
const OBJECT_METHODS: &[&str] = &[
    "!",
    "!=",
    "!~",
    "<=>",
    "==",
    "===",
    "__id__",
    "__send__",
    "class",
    "clone",
    "define_singleton_method",
    "display",
    "dup",
    "enum_for",
    "eql?",
    "equal?",
    "extend",
    "freeze",
    "frozen?",
    "gem",
    "hash",
    "inspect",
    "instance_eval",
    "instance_exec",
    "instance_of?",
    "instance_variable_defined?",
    "instance_variable_get",
    "instance_variable_set",
    "instance_variables",
    "is_a?",
    "itself",
    "kind_of?",
    "method",
    "methods",
    "nil?",
    "object_id",
    "pretty_inspect",
    "pretty_print",
    "pretty_print_cycle",
    "pretty_print_inspect",
    "pretty_print_instance_variables",
    "private_methods",
    "protected_methods",
    "public_method",
    "public_methods",
    "public_send",
    "remove_instance_variable",
    "respond_to?",
    "send",
    "singleton_class",
    "singleton_method",
    "singleton_methods",
    "tap",
    "then",
    "to_enum",
    "to_s",
    "yield_self",
];
const MODULE_METHODS: &[&str] = &[
    "!",
    "!=",
    "!~",
    "<",
    "<=",
    "<=>",
    "==",
    "===",
    ">",
    ">=",
    "__id__",
    "__send__",
    "alias_method",
    "allocate",
    "ancestors",
    "attached_object",
    "attr",
    "attr_accessor",
    "attr_reader",
    "attr_writer",
    "autoload",
    "autoload?",
    "class",
    "class_eval",
    "class_exec",
    "class_variable_defined?",
    "class_variable_get",
    "class_variable_set",
    "class_variables",
    "clone",
    "const_defined?",
    "const_get",
    "const_missing",
    "const_set",
    "const_source_location",
    "constants",
    "define_method",
    "define_singleton_method",
    "deprecate_constant",
    "display",
    "dup",
    "enum_for",
    "eql?",
    "equal?",
    "extend",
    "freeze",
    "frozen?",
    "gem",
    "hash",
    "include",
    "include?",
    "included_modules",
    "inspect",
    "instance_eval",
    "instance_exec",
    "instance_method",
    "instance_methods",
    "instance_of?",
    "instance_variable_defined?",
    "instance_variable_get",
    "instance_variable_set",
    "instance_variables",
    "is_a?",
    "itself",
    "kind_of?",
    "method",
    "method_defined?",
    "methods",
    "module_eval",
    "module_exec",
    "name",
    "new",
    "nil?",
    "object_id",
    "prepend",
    "pretty_inspect",
    "pretty_print",
    "pretty_print_cycle",
    "pretty_print_inspect",
    "pretty_print_instance_variables",
    "private_class_method",
    "private_constant",
    "private_instance_methods",
    "private_method_defined?",
    "private_methods",
    "protected_instance_methods",
    "protected_method_defined?",
    "protected_methods",
    "public_class_method",
    "public_constant",
    "public_instance_method",
    "public_instance_methods",
    "public_method",
    "public_method_defined?",
    "public_methods",
    "public_send",
    "refinements",
    "remove_class_variable",
    "remove_instance_variable",
    "remove_method",
    "respond_to?",
    "send",
    "singleton_class",
    "singleton_class?",
    "singleton_method",
    "singleton_methods",
    "subclasses",
    "superclass",
    "tap",
    "then",
    "to_enum",
    "to_s",
    "undef_method",
    "undefined_instance_methods",
    "yield_self",
];

const STRUCT_METHODS: &[&str] = &[
    "const",
    "decorator",
    "decorator_class",
    "extended",
    "from_hash",
    "from_hash!",
    "included",
    "inherited",
    "plugin",
    "plugins",
    "prepended",
    "prop",
    "prop_by_serialized_forms",
    "props",
    "reload_decorator!",
    "validate_prop_value",
];

pub(crate) fn identifier(name: &str, naming: NamingConvention) -> String {
    callable_identifier(name, naming, false)
}

fn callable_identifier(name: &str, naming: NamingConvention, singleton: bool) -> String {
    let mut result = project(name, naming);
    // Keywords retain their suffix (notably then_), even if Object defines them.
    if KEYWORDS.contains(&result.as_str())
        || ["_baml", "_types", "__baml_type_args"].contains(&result.as_str())
    {
        result.push('_');
        return result;
    }
    // Keep lifecycle hooks and the original T::Struct/runtime protections.
    if OBJECT_METHODS.contains(&result.as_str())
        || (singleton
            && (MODULE_METHODS.contains(&result.as_str())
                || STRUCT_METHODS.contains(&result.as_str())))
        || [
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
            "method_added",
            "method_removed",
            "method_undefined",
            "singleton_method_added",
            "singleton_method_removed",
            "singleton_method_undefined",
            "inherited",
            "included",
            "extended",
            "prepended",
            "const_added",
            "const_missing",
            "method_missing",
            "respond_to_missing?",
            "initialize_copy",
            "initialize_clone",
            "initialize_dup",
        ]
        .contains(&result.as_str())
    {
        return format!("baml_{result}");
    }
    result
}

pub(crate) fn parameter(
    name: &str,
    naming: NamingConvention,
    used: &mut BTreeSet<String>,
) -> String {
    let mut result = project(name, naming);
    if !result.starts_with(|c: char| c.is_lowercase() || c == '_') {
        result = project(name, NamingConvention::Language);
    }
    while KEYWORDS.contains(&result.as_str()) || used.contains(&result) {
        result.push('_');
    }
    used.insert(result.clone());
    result
}

pub(crate) fn quote(value: &str) -> String {
    let mut quoted = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\0' => quoted.push_str("\\u0000"),
            '#' => quoted.push_str("\\#"),
            '\'' => quoted.push(ch),
            _ => quoted.extend(ch.escape_debug()),
        }
    }
    quoted.push('"');
    quoted
}
