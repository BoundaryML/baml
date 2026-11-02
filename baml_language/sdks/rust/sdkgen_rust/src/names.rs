//! Rust host-name projection.
//!
//! BAML names are wire identities: the generated codec, the call FQNs and
//! the serde renames keep them verbatim. This module allocates the Rust
//! spelling of every generated binding in one table, keyed by declaration
//! identity, so definitions and references cannot drift. Under
//! `naming_convention = "language"` modules, functions, methods, fields and
//! parameters are `snake_case` and types and enum variants are `UpperCamelCase`;
//! a spelling taken twice in one Rust namespace is de-collided with trailing
//! underscores, the authored spelling keeping its name.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use baml_sdkgen_types::{
    Candidate, Case, Function, Name, NamingConvention, Symbol, SymbolPool, allocate_distinct,
    allocate_one, namespace_segments,
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
    /// The bindings of a callable, the direct one first.
    fn of(raw: &str) -> &'static [Self] {
        if raw.ends_with("@stream") {
            &[
                Self::Sync,
                Self::Async,
                Self::StreamWith,
                Self::StreamWithAsync,
                Self::SyncWithOptions,
                Self::AsyncWithOptions,
            ]
        } else {
            &[
                Self::Sync,
                Self::Async,
                Self::SyncWithOptions,
                Self::AsyncWithOptions,
            ]
        }
    }

    const fn suffix(self) -> &'static str {
        match self {
            Self::Sync => "",
            Self::Async => "_async",
            Self::StreamWith => "_with",
            Self::StreamWithAsync => "_with_async",
            Self::SyncWithOptions => "_with_options",
            Self::AsyncWithOptions => "_async_with_options",
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
}

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
        for (path, members) in &by_path {
            let (declared, skipped): (Vec<_>, Vec<_>) = members
                .iter()
                .filter(|(name, symbol)| {
                    !matches!(symbol, Symbol::Function(_)) && !name.name().as_str().contains('$')
                })
                .partition(|(name, _)| emitted.contains(*name));
            let types = allocate(
                declared
                    .iter()
                    .chain(&skipped)
                    .map(|(name, _)| ((*name).clone(), name.name().as_str(), Case::UpperCamel)),
                convention,
                [],
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
            names.allocate_callables(functions, convention);
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
                    names.allocate_callables(methods, convention);
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

    /// Name the callables of one Rust namespace (a module's free functions,
    /// or a class's `impl` block) and their parameters. Every direct binding
    /// is placed before any derived one (`_async`, `_with_options`, ...), so
    /// a BAML function always keeps the name it projects to.
    fn allocate_callables<'a>(
        &mut self,
        callables: impl Iterator<Item = (CallableId, &'a str, &'a Function)>,
        convention: NamingConvention,
    ) {
        // `$` companions are never emitted.
        let callables: Vec<_> = callables.filter(|(_, raw, _)| !raw.contains('$')).collect();
        let direct = allocate(
            callables
                .iter()
                .map(|(id, raw, _)| (id.clone(), *raw, Case::Snake)),
            convention,
            [],
        );
        let mut used: HashSet<String> = direct.values().cloned().collect();
        for (id, raw, function) in &callables {
            let base = &direct[id];
            for &binding in Binding::of(raw) {
                let spelling = match binding {
                    Binding::Sync => base.clone(),
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
    /// siblings and the types its parent declares.
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
            let reserved = emitted_types.get(&parent).into_iter().flatten();
            let allocated = allocate(
                child_names.iter().map(|child| {
                    let mut path = parent.clone();
                    path.push(child.clone());
                    (path, child.as_str(), Case::Snake)
                }),
                convention,
                reserved.map(String::as_str),
            );
            for (path, segment) in allocated {
                let mut projected = projected_parent.clone();
                projected.push(segment);
                self.modules.insert(path, projected);
            }
        }
    }
}

/// Spell each name in one Rust namespace: cased by `convention`, made a
/// legal identifier, and de-collided from the others and from `reserved`.
fn allocate<'a, 'r, K: Eq + std::hash::Hash>(
    names: impl Iterator<Item = (K, &'a str, Case)>,
    convention: NamingConvention,
    reserved: impl IntoIterator<Item = &'r str>,
) -> HashMap<K, String> {
    let candidates = names
        .map(|(key, raw, case)| {
            // A companion's `@` is no identifier character.
            let spelling = idents::dir_segment(&convention.spell(raw, case).replace('@', "_"));
            Candidate {
                key,
                authored: spelling == raw,
                spelling,
            }
        })
        .collect();
    allocate_distinct(candidates, reserved)
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
                })
                .collect(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            origin: Origin {
                source_file_path: "main.baml".to_string(),
                span_start: 0,
            },
        })
    }

    fn pool() -> (SymbolPool, HashSet<Name>) {
        let mut pool = SymbolPool::new();
        pool.insert(
            name("user", &["deploymentGroups"], "createToken"),
            Symbol::Function(function("createToken", &["groupId", "group_id"])),
        );
        let create_token_async = name("user", &["deploymentGroups"], "createTokenAsync");
        pool.insert(
            create_token_async,
            Symbol::Function(function("createTokenAsync", &[])),
        );
        let page = name("user", &["deploymentGroups"], "page_result");
        pool.insert(page.clone(), class(&page, &["nextCursor", "type", "self"]));
        let emitted = HashSet::from([page]);
        (pool, emitted)
    }

    #[test]
    fn language_convention_respells_and_decollides() {
        let (pool, emitted) = pool();
        let names = RustNames::build(&pool, &emitted, NamingConvention::Language);

        let create_token = name("user", &["deploymentGroups"], "createToken");
        let callable = CallableId::Function(create_token.clone());
        assert_eq!(names.module_path(&create_token), ["deployment_groups"]);
        assert_eq!(names.binding(&callable, Binding::Sync), "create_token");
        // `createTokenAsync` is a direct binding, so it keeps the name
        // `createToken`'s async binding would have derived.
        let sibling = CallableId::Function(name("user", &["deploymentGroups"], "createTokenAsync"));
        assert_eq!(names.binding(&sibling, Binding::Sync), "create_token_async");
        assert_eq!(
            names.binding(&callable, Binding::Async),
            "create_token_async_"
        );
        assert_eq!(
            names.binding(&callable, Binding::AsyncWithOptions),
            "create_token_async_with_options"
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
        assert_eq!(names.binding(&callable, Binding::Sync), "createToken");
        assert_eq!(
            names.binding(&callable, Binding::Async),
            "createToken_async"
        );
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
