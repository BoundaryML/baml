//! Rust host-name projection.
//!
//! BAML names are wire identities: the generated codec, the call FQNs and
//! the serde renames keep them verbatim. This module allocates the Rust
//! spelling of every generated binding in one table, keyed by declaration
//! identity, so definitions and references cannot drift. Under
//! `naming_convention = "language"` modules, functions, methods, fields and
//! parameters are `snake_case` and types and enum variants are `UpperCamelCase`;
//! a spelling taken twice in one Rust namespace is de-collided with trailing
//! underscores, the authored spelling keeping its name. At the crate root,
//! BAML names also yield to the items the SDK declares there itself.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use baml_sdkgen_types::{
    Candidate, Case, Function, Name, NamingConvention, Symbol, SymbolPool, allocate_distinct,
    allocate_one, dependency_root_segments, namespace_segments,
};

use crate::idents;

/// A BAML callable the SDK binds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableId {
    Function(Name),
    Method {
        class: Name,
        method: baml_base::Name,
    },
}

/// Names in Rust's type namespace (types, modules, crates) that the
/// generated crate root declares itself: its runtime modules, the
/// `baml_bridge` re-export, `BamlOptions`, and the modules of the
/// invocation API. The last four are declared only when the program has the
/// stdlib's invocation types, as every program outside unit tests does, and
/// are reserved either way. Root BAML types and namespaces are named around
/// these.
pub(crate) const ROOT_TYPE_NAMES: &[&str] = &[
    "_inlinedbaml",
    "_runtime",
    "baml_bridge",
    "BamlOptions",
    "_invocation",
    "trace",
    "invocation",
    "experimental",
];

/// Names in Rust's value namespace (functions) that the generated crate root
/// declares itself. Root BAML functions are named around them.
pub(crate) const ROOT_VALUE_NAMES: &[&str] = &["init"];

/// Root module names rustc treats specially: `mod lib;` finds both
/// `src/lib.rs`, the crate root itself, and `src/lib/mod.rs` (E0761), and
/// `mod main;` trips the `special_module_name` lint. Root BAML namespaces
/// are named around them.
pub(crate) const ROOT_SPECIAL_MODULES: &[&str] = &["lib", "main"];

/// The spellings no root BAML type may take: the crate root's own
/// [`ROOT_TYPE_NAMES`], and the root modules dependencies live in (`baml`,
/// `ai`, `reflect`, `vendor`). The SDK's paths into those modules are
/// fixed, so a root type spelled like one is renamed instead. They are
/// reserved whether or not the program uses the dependency, so a root
/// type's name never depends on which packages the program uses.
fn root_reserved_types(convention: NamingConvention) -> Vec<String> {
    ROOT_TYPE_NAMES
        .iter()
        .map(|name| (*name).to_string())
        .chain(dependency_root_segments().map(|segment| spell(segment, Case::Snake, convention)))
        .collect()
}

/// One of the Rust functions generated for a BAML callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Binding {
    Sync,
    Async,
    /// `@stream` companions only: the sync call taking `CallOptions`.
    StreamWith,
    /// `@stream` companions only: the async call taking `CallOptions`.
    StreamWithAsync,
    SyncWithOptions,
    AsyncWithOptions,
}

impl Binding {
    /// The bindings of a callable, the direct (async) one first.
    fn of(raw: &str) -> &'static [Self] {
        if raw.ends_with("@stream") {
            &[
                Self::Async,
                Self::Sync,
                Self::StreamWithAsync,
                Self::StreamWith,
                Self::AsyncWithOptions,
                Self::SyncWithOptions,
            ]
        } else {
            &[
                Self::Async,
                Self::Sync,
                Self::AsyncWithOptions,
                Self::SyncWithOptions,
            ]
        }
    }

    /// The plain name is the async binding; each blocking twin carries
    /// `_sync`.
    const fn suffix(self) -> &'static str {
        match self {
            Self::Async => "",
            Self::Sync => "_sync",
            Self::StreamWithAsync => "_with",
            Self::StreamWith => "_with_sync",
            Self::AsyncWithOptions => "_with_options",
            Self::SyncWithOptions => "_sync_with_options",
        }
    }
}

/// The Rust spelling of every binding the SDK generates, as on-disk module
/// segments and identifier text (keywords are made raw by
/// [`idents::ident`] at emission).
#[derive(Debug)]
pub(crate) struct RustNames {
    /// Package and namespace path ([`namespace_segments`]) → module path.
    modules: HashMap<Vec<String>, Vec<String>>,
    types: HashMap<Name, String>,
    callables: HashMap<(CallableId, Binding), String>,
    fields: HashMap<(Name, baml_base::Name), String>,
    variants: HashMap<(Name, baml_base::Name), String>,
    params: HashMap<(CallableId, baml_base::Name), String>,
    /// Each class's generated field constructor, named after its methods.
    constructors: HashMap<Name, String>,
}

/// The spelling a class's generated field constructor asks for.
const CONSTRUCTOR: &str = "new";

impl RustNames {
    /// Allocate every name in `pool`. `emitted` are the types the SDK
    /// declares: only they claim a spelling a module must avoid, and they
    /// claim theirs before the types that are skipped.
    pub(crate) fn build(
        pool: &SymbolPool,
        emitted: &HashSet<Name>,
        convention: NamingConvention,
    ) -> Self {
        let mut names = Self {
            modules: HashMap::new(),
            types: HashMap::new(),
            callables: HashMap::new(),
            fields: HashMap::new(),
            variants: HashMap::new(),
            params: HashMap::new(),
            constructors: HashMap::new(),
        };
        let mut symbols: Vec<(&Name, &Symbol)> = pool.iter().collect();
        symbols.sort_by_key(|(name, _)| *name);

        let mut by_path: BTreeMap<Vec<String>, Vec<(&Name, &Symbol)>> = BTreeMap::new();
        for &(name, symbol) in &symbols {
            by_path
                .entry(namespace_segments(name))
                .or_default()
                .push((name, symbol));
        }
        // Rust puts types and modules in one namespace: the types a module
        // declares are reserved against its child modules.
        let mut emitted_types: HashMap<Vec<String>, HashSet<String>> = HashMap::new();
        let root_types = root_reserved_types(convention);
        for (path, members) in &by_path {
            let (declared, skipped): (Vec<_>, Vec<_>) = members
                .iter()
                .filter(|(name, symbol)| {
                    !matches!(symbol, Symbol::Function(_)) && !name.name().as_str().contains('$')
                })
                .partition(|(name, _)| emitted.contains(*name));
            let reserved_types: &[String] = if path.is_empty() { &root_types } else { &[] };
            let types = allocate(
                declared
                    .iter()
                    .chain(&skipped)
                    .map(|(name, _)| ((*name).clone(), name.name().as_str(), Case::UpperCamel)),
                convention,
                reserved_types.iter().map(String::as_str),
            );
            emitted_types.insert(
                path.clone(),
                declared
                    .iter()
                    .map(|(name, _)| types[*name].clone())
                    .collect(),
            );
            names.types.extend(types);

            let functions = members.iter().filter_map(|(name, symbol)| match symbol {
                Symbol::Function(function) => Some((
                    CallableId::Function((*name).clone()),
                    name.name().as_str(),
                    function,
                )),
                Symbol::Class(_) | Symbol::Enum(_) | Symbol::TypeAlias(_) => None,
            });
            let reserved: &[&str] = if path.is_empty() {
                ROOT_VALUE_NAMES
            } else {
                &[]
            };
            names.allocate_callables(functions, convention, reserved);
        }
        names.allocate_modules(by_path.keys(), &emitted_types, convention);

        for (name, symbol) in symbols {
            match symbol {
                Symbol::Class(class) => {
                    names.fields.extend(allocate(
                        class.properties.iter().map(|property| {
                            (
                                (name.clone(), property.name.clone()),
                                property.name.as_str(),
                                Case::Snake,
                            )
                        }),
                        convention,
                        [],
                    ));
                    let methods = class
                        .static_methods
                        .iter()
                        .chain(&class.instance_methods)
                        .map(|method| {
                            (
                                CallableId::Method {
                                    class: name.clone(),
                                    method: method.name.clone(),
                                },
                                method.name.as_str(),
                                method,
                            )
                        });
                    names.allocate_callables(methods, convention, &[]);
                    names.allocate_constructor(name, class);
                }
                Symbol::Enum(enum_) => names.variants.extend(allocate(
                    enum_.variants.iter().map(|variant| {
                        (
                            (name.clone(), variant.name.clone()),
                            variant.name.as_str(),
                            Case::UpperCamel,
                        )
                    }),
                    convention,
                    [],
                )),
                Symbol::Function(_) | Symbol::TypeAlias(_) => {}
            }
        }
        names
    }

    /// The module the items of `name` are emitted in, as on-disk segments.
    pub(crate) fn module_path(&self, name: &Name) -> &[String] {
        let path = namespace_segments(name);
        self.modules
            .get(&path)
            .unwrap_or_else(|| unreachable!("every pool namespace has a module: `{name}`"))
    }

    /// The Rust name of the class, enum or type alias `name`.
    pub(crate) fn type_name(&self, name: &Name) -> &str {
        self.types
            .get(name)
            .unwrap_or_else(|| unreachable!("every pool type has a name: `{name}`"))
    }

    pub(crate) fn binding(&self, callable: &CallableId, binding: Binding) -> &str {
        self.callables
            .get(&(callable.clone(), binding))
            .unwrap_or_else(|| unreachable!("every bound callable has names: {callable:?}"))
    }

    pub(crate) fn field(&self, class: &Name, field: &baml_base::Name) -> &str {
        self.fields
            .get(&(class.clone(), field.clone()))
            .unwrap_or_else(|| unreachable!("every class field has a name: `{class}.{field}`"))
    }

    pub(crate) fn variant(&self, enum_: &Name, variant: &baml_base::Name) -> &str {
        self.variants
            .get(&(enum_.clone(), variant.clone()))
            .unwrap_or_else(|| unreachable!("every enum variant has a name: `{enum_}.{variant}`"))
    }

    pub(crate) fn param(&self, callable: &CallableId, param: &baml_base::Name) -> &str {
        self.params
            .get(&(callable.clone(), param.clone()))
            .unwrap_or_else(|| unreachable!("every parameter has a name: {callable:?}.{param}"))
    }

    /// The Rust name of the field constructor generated for class `name`.
    pub(crate) fn constructor(&self, class: &Name) -> &str {
        self.constructors
            .get(class)
            .unwrap_or_else(|| unreachable!("every class has a constructor name: `{class}`"))
    }

    /// Name the field constructor of `class` after its methods are named.
    /// It shares the class's `impl` namespace with every method binding,
    /// and a BAML method keeps the name it projects to: the constructor is
    /// `new`, or `new_` (and so on) when a binding already spells `new`.
    fn allocate_constructor(&mut self, name: &Name, class: &baml_sdkgen_types::Class) {
        let mut used: HashSet<String> = class
            .static_methods
            .iter()
            .chain(&class.instance_methods)
            .filter(|method| !method.name.as_str().contains('$'))
            .flat_map(|method| {
                let id = CallableId::Method {
                    class: name.clone(),
                    method: method.name.clone(),
                };
                Binding::of(method.name.as_str())
                    .iter()
                    .map(move |&binding| (id.clone(), binding))
            })
            .map(|key| self.callables[&key].clone())
            .collect();
        self.constructors
            .insert(name.clone(), allocate_one(CONSTRUCTOR, &mut used));
    }

    /// Name the callables of one Rust namespace (a module's free functions,
    /// or a class's `impl` block) and their parameters. Every direct binding
    /// is placed before any derived one (`_sync`, `_with_options`, ...), so
    /// a BAML function always keeps the name it projects to.
    fn allocate_callables<'a>(
        &mut self,
        callables: impl Iterator<Item = (CallableId, &'a str, &'a Function)>,
        convention: NamingConvention,
        reserved: &[&str],
    ) {
        // `$` companions are never emitted.
        let callables: Vec<_> = callables.filter(|(_, raw, _)| !raw.contains('$')).collect();
        let direct = allocate(
            callables
                .iter()
                .map(|(id, raw, _)| (id.clone(), *raw, Case::Snake)),
            convention,
            reserved.iter().copied(),
        );
        let mut used: HashSet<String> = direct
            .values()
            .cloned()
            .chain(reserved.iter().map(|name| (*name).to_string()))
            .collect();
        for (id, raw, function) in &callables {
            let base = &direct[id];
            for &binding in Binding::of(raw) {
                let spelling = match binding {
                    Binding::Async => base.clone(),
                    _ => allocate_one(&format!("{base}{}", binding.suffix()), &mut used),
                };
                self.callables.insert((id.clone(), binding), spelling);
            }
            self.params.extend(allocate(
                function.arguments.iter().map(|argument| {
                    (
                        (id.clone(), argument.name.clone()),
                        argument.name.as_str(),
                        Case::Snake,
                    )
                }),
                convention,
                [],
            ));
        }
    }

    /// Name every module, parents before children. A module avoids its
    /// siblings and the types its parent declares, and a root module also
    /// the crate root's own items and the names rustc treats specially.
    // BUG: spellings are compared case-sensitively, but on a case-insensitive
    // file system (macOS, Windows) a preserve-case root namespace `_Runtime`
    // or `Lib` also finds `src/_runtime.rs` or `src/lib.rs` (E0761), and
    // sibling namespaces `Baml` and `baml` share one directory.
    fn allocate_modules<'a>(
        &mut self,
        leaves: impl Iterator<Item = &'a Vec<String>>,
        emitted_types: &HashMap<Vec<String>, HashSet<String>>,
        convention: NamingConvention,
    ) {
        let mut children: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
        for leaf in leaves {
            for depth in 0..leaf.len() {
                children
                    .entry(leaf[..depth].to_vec())
                    .or_default()
                    .insert(leaf[depth].clone());
            }
            children.entry(leaf.clone()).or_default();
        }
        self.modules.insert(Vec::new(), Vec::new());
        // Sorted, so every parent is named before its children.
        for (parent, child_names) in children {
            let projected_parent = self.modules[&parent].clone();
            let root_items = ROOT_TYPE_NAMES
                .iter()
                .chain(ROOT_SPECIAL_MODULES)
                .copied()
                .filter(|_| parent.is_empty());
            let reserved = emitted_types
                .get(&parent)
                .into_iter()
                .flatten()
                .map(String::as_str)
                .chain(root_items);
            let allocated = allocate(
                child_names.iter().map(|child| {
                    let mut path = parent.clone();
                    path.push(child.clone());
                    (path, child.as_str(), Case::Snake)
                }),
                convention,
                reserved,
            );
            for (path, segment) in allocated {
                let mut projected = projected_parent.clone();
                projected.push(segment);
                self.modules.insert(path, projected);
            }
        }
    }
}

/// Spell each name in one Rust namespace ([`spell`]), de-collided from the
/// others and from `reserved`.
fn allocate<'a, 'r, K: Eq + std::hash::Hash>(
    names: impl Iterator<Item = (K, &'a str, Case)>,
    convention: NamingConvention,
    reserved: impl IntoIterator<Item = &'r str>,
) -> HashMap<K, String> {
    let candidates = names
        .map(|(key, raw, case)| {
            let spelling = spell(raw, case, convention);
            Candidate {
                key,
                authored: spelling == raw,
                spelling,
            }
        })
        .collect();
    allocate_distinct(candidates, reserved)
}

/// The spelling `raw` asks for as a `case` binding: cased by `convention`,
/// with the keywords no raw identifier can spell (`self`, `crate`, ...)
/// renamed.
fn spell(raw: &str, case: Case, convention: NamingConvention) -> String {
    // A companion's `@` is no identifier character.
    idents::dir_segment(&convention.spell(raw, case).replace('@', "_"))
}

#[cfg(test)]
mod tests {
    use baml_sdkgen_types::{Class, ClassProperty, FunctionArgument, Origin, Ty};

    use super::*;

    fn name(pkg: &str, ns: &[&str], leaf: &str) -> Name {
        Name::new(
            baml_base::Name::new(pkg),
            ns.iter().map(|s| baml_base::Name::new(*s)).collect(),
            baml_base::Name::new(leaf),
        )
    }

    fn function(leaf: &str, params: &[&str]) -> Function {
        Function {
            name: baml_base::Name::new(leaf),
            generic_params: Vec::new(),
            docstring: None,
            arguments: params
                .iter()
                .map(|param| FunctionArgument {
                    injected: false,
                    name: baml_base::Name::new(*param),
                    docstring: None,
                    ty: Ty::String,
                    default: None,
                })
                .collect(),
            return_type: Ty::String,
            throws: None,
            watchers: Vec::new(),
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        }
    }

    fn class(n: &Name, fields: &[&str]) -> Symbol {
        Symbol::Class(Class {
            crossing: baml_sdkgen_types::ClassCrossing::Fields,
            name: n.clone(),
            generic_params: Vec::new(),
            docstring: None,
            properties: fields
                .iter()
                .map(|field| ClassProperty {
                    name: baml_base::Name::new(*field),
                    docstring: None,
                    ty: Ty::String,
                    json_fallback: false,
                    json_flatten: false,
                    json_rename: None,
                })
                .collect(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
            json_omit_nulls: false,
        })
    }

    fn pool() -> (SymbolPool, HashSet<Name>) {
        let mut pool = SymbolPool::new();
        pool.insert(
            name("user", &["deploymentGroups"], "createToken"),
            Symbol::Function(function("createToken", &["groupId", "group_id"])),
        );
        let create_token_sync = name("user", &["deploymentGroups"], "createTokenSync");
        pool.insert(
            create_token_sync,
            Symbol::Function(function("createTokenSync", &[])),
        );
        let page = name("user", &["deploymentGroups"], "page_result");
        pool.insert(page.clone(), class(&page, &["nextCursor", "type", "self"]));
        let emitted = HashSet::from([page]);
        (pool, emitted)
    }

    /// Allocate `symbols`, every class among them emitted.
    fn build(
        symbols: impl IntoIterator<Item = (Name, Symbol)>,
        convention: NamingConvention,
    ) -> RustNames {
        let pool: SymbolPool = symbols.into_iter().collect();
        let emitted = pool
            .iter()
            .filter(|(_, symbol)| matches!(symbol, Symbol::Class(_)))
            .map(|(name, _)| name.clone())
            .collect();
        RustNames::build(&pool, &emitted, convention)
    }

    #[test]
    fn sync_twins_never_displace_authored_methods() {
        // Methods `bump` and `bump_sync` on one class.
        let counter = name("user", &[], "Counter");
        let Symbol::Class(mut class) = class(&counter, &[]) else {
            unreachable!("`class` builds a class");
        };
        class.instance_methods = vec![function("bump", &[]), function("bump_sync", &[])];
        let names = build(
            [(counter.clone(), Symbol::Class(class))],
            NamingConvention::PreserveCase,
        );

        let method = |raw: &str| CallableId::Method {
            class: counter.clone(),
            method: baml_base::Name::new(raw),
        };
        assert_eq!(names.binding(&method("bump"), Binding::Async), "bump");
        assert_eq!(names.binding(&method("bump"), Binding::Sync), "bump_sync_");
        assert_eq!(
            names.binding(&method("bump_sync"), Binding::Async),
            "bump_sync"
        );
        assert_eq!(
            names.binding(&method("bump_sync"), Binding::Sync),
            "bump_sync_sync"
        );
    }

    /// What the generated crate root binds itself, written out here rather
    /// than read from the tables under test: type-namespace names, the
    /// dependency modules, and the module names rustc treats specially.
    const ROOT_TYPES: [&str; 8] = [
        "_inlinedbaml",
        "_runtime",
        "baml_bridge",
        "BamlOptions",
        "_invocation",
        "trace",
        "invocation",
        "experimental",
    ];
    const DEPENDENCY_ROOTS: [&str; 4] = ["baml", "ai", "reflect", "vendor"];
    const SPECIAL_MODULES: [&str; 2] = ["lib", "main"];

    #[test]
    fn root_symbols_yield_to_the_crate_roots_own_names() {
        let convention = NamingConvention::PreserveCase;
        let class_named = |ns: &[&str], leaf: &str| {
            let class_name = name("user", ns, leaf);
            let symbol = class(&class_name, &[]);
            (class_name, symbol)
        };
        let function_named = |ns: &[&str], leaf: &str| {
            let function_name = name("user", ns, leaf);
            (function_name, Symbol::Function(function(leaf, &[])))
        };
        let binding = |names: &RustNames, ns: &[&str], leaf: &str| {
            let id = CallableId::Function(name("user", ns, leaf));
            names.binding(&id, Binding::Async).to_string()
        };

        // The dependency modules are reserved even when the program has none
        // of their packages, as here.
        for &leaf in ROOT_TYPES.iter().chain(&DEPENDENCY_ROOTS) {
            let (class, symbol) = class_named(&[], leaf);
            let names = build([(class.clone(), symbol)], convention);
            assert_eq!(names.type_name(&class), format!("{leaf}_"));
        }
        for &ns in ROOT_TYPES.iter().chain(&SPECIAL_MODULES) {
            let (thing, symbol) = class_named(&[ns], "Thing");
            let names = build([(thing.clone(), symbol)], convention);
            assert_eq!(names.module_path(&thing), [format!("{ns}_")]);
        }
        let names = build([function_named(&[], "init")], convention);
        assert_eq!(binding(&names, &[], "init"), "init_");

        // Functions live apart from types and modules, in Rust's value
        // namespace: each keeps a type-namespace name, and a class keeps
        // `init` and the special module names. The dynamic `invoke` lives
        // in `experimental`, leaving the root's `invoke` free.
        for &leaf in ROOT_TYPES
            .iter()
            .chain(&DEPENDENCY_ROOTS)
            .chain(&SPECIAL_MODULES)
            .chain(&["invoke", "invoke_sync"])
        {
            let names = build([function_named(&[], leaf)], convention);
            assert_eq!(binding(&names, &[], leaf), leaf);
        }
        for &leaf in SPECIAL_MODULES.iter().chain(&["init"]) {
            let (class, symbol) = class_named(&[], leaf);
            let names = build([(class.clone(), symbol)], convention);
            assert_eq!(names.type_name(&class), leaf);
        }

        // Only the root is reserved: one namespace down, every name is free.
        for &leaf in ROOT_TYPES.iter().chain(&DEPENDENCY_ROOTS) {
            let (class, symbol) = class_named(&["ns"], leaf);
            let names = build([(class.clone(), symbol)], convention);
            assert_eq!(names.type_name(&class), leaf);
        }
        for &ns in ROOT_TYPES.iter().chain(&SPECIAL_MODULES) {
            let (thing, symbol) = class_named(&["ns", ns], "Thing");
            let names = build([(thing.clone(), symbol)], convention);
            assert_eq!(names.module_path(&thing), ["ns", ns]);
        }
        let names = build([function_named(&["ns"], "init")], convention);
        assert_eq!(binding(&names, &["ns"], "init"), "init");

        // The stdlib's module keeps its path beside a root class named like it.
        let (baml, baml_class) = class_named(&[], "baml");
        let cancel = name("baml", &["spawn"], "CancelToken");
        let cancel_class = class(&cancel, &[]);
        let names = build(
            [(baml.clone(), baml_class), (cancel.clone(), cancel_class)],
            convention,
        );
        assert_eq!(names.type_name(&baml), "baml_");
        assert_eq!(names.module_path(&cancel), ["baml", "spawn"]);
        assert_eq!(names.type_name(&cancel), "CancelToken");
    }

    #[test]
    fn root_projections_yield_to_the_crate_roots_own_names() {
        let options = name("user", &[], "baml_options");
        let experimental = name("user", &["Experimental"], "Thing");
        let lib = name("user", &["Lib"], "Thing");
        let init = name("user", &[], "Init");
        let names = build(
            [
                (options.clone(), class(&options, &[])),
                (experimental.clone(), class(&experimental, &[])),
                (lib.clone(), class(&lib, &[])),
                (init.clone(), Symbol::Function(function("Init", &[]))),
            ],
            NamingConvention::Language,
        );

        assert_eq!(names.type_name(&options), "BamlOptions_");
        assert_eq!(names.module_path(&experimental), ["experimental_"]);
        assert_eq!(names.module_path(&lib), ["lib_"]);
        assert_eq!(
            names.binding(&CallableId::Function(init), Binding::Async),
            "init_"
        );
    }

    #[test]
    fn language_convention_respells_and_decollides() {
        let (pool, emitted) = pool();
        let names = RustNames::build(&pool, &emitted, NamingConvention::Language);

        let create_token = name("user", &["deploymentGroups"], "createToken");
        let callable = CallableId::Function(create_token.clone());
        assert_eq!(names.module_path(&create_token), ["deployment_groups"]);
        assert_eq!(names.binding(&callable, Binding::Async), "create_token");
        // `createTokenSync` is a direct binding, so it keeps the name
        // `createToken`'s blocking binding would have derived.
        let sibling = CallableId::Function(name("user", &["deploymentGroups"], "createTokenSync"));
        assert_eq!(names.binding(&sibling, Binding::Async), "create_token_sync");
        assert_eq!(
            names.binding(&callable, Binding::Sync),
            "create_token_sync_"
        );
        assert_eq!(
            names.binding(&callable, Binding::SyncWithOptions),
            "create_token_sync_with_options"
        );
        // The authored `group_id` keeps its spelling.
        assert_eq!(
            names.param(&callable, &baml_base::Name::new("group_id")),
            "group_id"
        );
        assert_eq!(
            names.param(&callable, &baml_base::Name::new("groupId")),
            "group_id_"
        );

        let page = name("user", &["deploymentGroups"], "page_result");
        assert_eq!(names.type_name(&page), "PageResult");
        assert_eq!(
            names.field(&page, &baml_base::Name::new("nextCursor")),
            "next_cursor"
        );
        assert_eq!(names.field(&page, &baml_base::Name::new("type")), "type");
        assert_eq!(names.field(&page, &baml_base::Name::new("self")), "self_");
    }

    #[test]
    fn preserve_case_keeps_baml_spellings() {
        let (pool, emitted) = pool();
        let names = RustNames::build(&pool, &emitted, NamingConvention::PreserveCase);

        let create_token = name("user", &["deploymentGroups"], "createToken");
        let callable = CallableId::Function(create_token.clone());
        assert_eq!(names.module_path(&create_token), ["deploymentGroups"]);
        assert_eq!(names.binding(&callable, Binding::Async), "createToken");
        assert_eq!(names.binding(&callable, Binding::Sync), "createToken_sync");
        assert_eq!(
            names.param(&callable, &baml_base::Name::new("groupId")),
            "groupId"
        );
        let page = name("user", &["deploymentGroups"], "page_result");
        assert_eq!(names.type_name(&page), "page_result");
        assert_eq!(
            names.field(&page, &baml_base::Name::new("nextCursor")),
            "nextCursor"
        );
    }

    #[test]
    fn constructor_takes_the_first_spelling_no_method_binding_has() {
        let mut pool = SymbolPool::new();
        let plain = name("user", &[], "Plain");
        // A field named `new` lives in the field namespace: no clash.
        pool.insert(plain.clone(), class(&plain, &["new"]));
        let account = name("user", &[], "Account");
        let Symbol::Class(mut decl) = class(&account, &[]) else {
            unreachable!("`class` builds a class");
        };
        decl.static_methods = vec![function("new", &[])];
        decl.instance_methods = vec![function("new_", &[])];
        pool.insert(account.clone(), Symbol::Class(decl));
        let emitted = HashSet::from([plain.clone(), account.clone()]);
        let names = RustNames::build(&pool, &emitted, NamingConvention::Language);

        assert_eq!(names.constructor(&plain), "new");
        let method = |raw: &str| CallableId::Method {
            class: account.clone(),
            method: baml_base::Name::new(raw),
        };
        // The BAML methods keep their names; the constructor yields.
        assert_eq!(names.binding(&method("new"), Binding::Async), "new");
        assert_eq!(names.binding(&method("new"), Binding::Sync), "new_sync");
        assert_eq!(names.binding(&method("new_"), Binding::Async), "new_");
        assert_eq!(names.constructor(&account), "new__");
    }

    #[test]
    fn module_avoids_a_type_its_parent_declares() {
        let mut pool = SymbolPool::new();
        let shadow = name("user", &[], "shapes");
        pool.insert(shadow.clone(), class(&shadow, &[]));
        let inner = name("user", &["shapes"], "Circle");
        pool.insert(inner.clone(), class(&inner, &[]));
        let emitted = HashSet::from([shadow, inner.clone()]);
        for convention in [NamingConvention::PreserveCase, NamingConvention::Language] {
            let names = RustNames::build(&pool, &emitted, convention);
            let expected: &[&str] = match convention {
                NamingConvention::PreserveCase => &["shapes_"],
                NamingConvention::Language => &["shapes"],
            };
            assert_eq!(names.module_path(&inner), expected, "{convention}");
        }
    }

    #[test]
    fn module_segments_escape_non_raw_keywords() {
        let mut pool = SymbolPool::new();
        for thing in [
            name("ai", &["crate"], "Thing"),
            name("reflect", &["crate"], "Thing"),
            name("aws", &["s3"], "Bucket"),
        ] {
            pool.insert(thing.clone(), class(&thing, &[]));
        }
        let names = RustNames::build(&pool, &HashSet::new(), NamingConvention::Language);
        for package in ["ai", "reflect"] {
            assert_eq!(
                names.module_path(&name(package, &["crate"], "Thing")),
                [package, "crate_"]
            );
        }
        assert_eq!(
            names.module_path(&name("aws", &["s3"], "Bucket")),
            ["vendor", "aws", "s3"],
            "vendor paths route under `vendor`"
        );
    }
}
