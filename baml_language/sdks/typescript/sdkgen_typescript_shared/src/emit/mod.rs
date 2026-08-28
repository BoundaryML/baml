//! Shared emitter-internal representation of TypeScript-side symbols.
//!
//! These types describe what the emitter will render to TypeScript, as
//! opposed to `baml_codegen_types` which describes BAML-side input
//! symbols. Authored callables fan out through their structured Direct, Spec,
//! and Stream operation metadata; generated sibling names are allocated inside
//! their TypeScript scope.

pub(crate) mod class;
pub(crate) mod enum_;
pub(crate) mod function;
pub(crate) mod method;
pub(crate) mod type_alias;
pub(crate) mod typemap_file;

use std::collections::{BTreeMap, BTreeSet};

use baml_codegen_types::{FunctionArgument, Name, Symbol, SymbolPool, Ty};

use crate::{
    emit::{
        class::{TypeScriptClass, TypeScriptClassProperty},
        enum_::{TypeScriptEnum, TypeScriptEnumVariant},
        function::{BindingRole, SyncAsync, TypeScriptFunction},
        method::{MethodKind, OptionalArg, RequiredArg, TypeScriptMethodBinding},
        type_alias::TypeScriptTypeAlias,
    },
    routing::{LeafPath, route},
};

/// Emitter-internal representation of one rendered TypeScript symbol.
pub(crate) enum EmittedSymbol {
    Class(TypeScriptClass),
    Enum(TypeScriptEnum),
    TypeAlias(TypeScriptTypeAlias),
    Function(TypeScriptFunction),
}

/// Build-time sort key. Tuple of `(source_file_path, span_start)`
/// derived from a `Symbol`'s `Origin`.
pub(crate) type SortKey = (String, u32);

/// Allocate generated callable projections inside one TypeScript sibling
/// scope. Authored names are reserved up front and always keep their exact
/// spelling; a generated collision gains trailing `$` characters until it is
/// unique. `$` is a legal identifier character and keeps the public
/// `<name>$stream` spelling recognizable when that exact name is occupied.
#[derive(Default)]
struct BindingNameAllocator {
    used: BTreeSet<String>,
}

impl BindingNameAllocator {
    fn reserve(&mut self, name: impl Into<String>) {
        self.used.insert(name.into());
    }

    fn allocate_derived(&mut self, mut preferred: String) -> String {
        while !self.used.insert(preferred.clone()) {
            preferred.push('$');
        }
        preferred
    }

    fn binding_name(&mut self, role: BindingRole, authored_name: &str) -> String {
        let preferred = role.binding_name(authored_name);
        if role == BindingRole::DirectSync {
            preferred
        } else {
            self.allocate_derived(preferred)
        }
    }
}

/// Walk every `(Name, Symbol)` in the pool and build the
/// `(LeafPath, EmittedSymbol, SortKey)` triples that drive emission.
/// Function symbols fan out into sync + async bindings; all other
/// variants are 1:1.
pub(crate) fn build_emitted(pool: &SymbolPool) -> Vec<(LeafPath, EmittedSymbol, SortKey)> {
    // Determinism: SymbolPool is a HashMap, so iteration order is
    // nondeterministic. Sort pool entries by Name before the walk.
    let mut entries: Vec<(&Name, &Symbol)> = pool.iter().collect();
    entries.sort_by_key(|e| e.0);

    // A derived function binding must not steal the exact name of any authored
    // sibling, even when that sibling appears later in source/name order.
    let mut binding_names: BTreeMap<LeafPath, BindingNameAllocator> = BTreeMap::new();
    for (key, _) in &entries {
        let leaf = route(key);
        binding_names
            .entry(leaf.clone())
            .or_default()
            .reserve(key.name().as_str());
        // Child namespaces are sibling exports in each ancestor leaf. A
        // generated projection must not shadow one; exact authored function ↔
        // child collisions remain handled by Object.assign in the renderer.
        for (index, child) in leaf.segments.iter().enumerate() {
            binding_names
                .entry(LeafPath {
                    segments: leaf.segments[..index].to_vec(),
                })
                .or_default()
                .reserve(child.clone());
        }
    }

    let mut out: Vec<(LeafPath, EmittedSymbol, SortKey)> = Vec::new();

    for (key, symbol) in entries {
        let leaf = route(key);
        // Preserve BAML type names verbatim. PPIR partial-output classes keep
        // their `$stream` suffix; function projections are expanded from
        // `Function.operations` below and do not exist as suffixed symbols.
        let bare = key.name().as_str().to_string();

        match symbol {
            Symbol::Class(c) => {
                let sort_key = origin_key(&c.origin);
                let properties = c
                    .properties
                    .iter()
                    .map(|p| TypeScriptClassProperty {
                        name: p.name.as_str().to_string(),
                        ty: p.ty.clone(),
                        docstring: p.docstring.clone(),
                    })
                    .collect();
                let class_fqn_root = key.to_string();
                let static_methods = expand_methods(
                    &c.static_methods,
                    &class_fqn_root,
                    MethodKind::Static,
                    std::iter::empty(),
                );
                let instance_methods = expand_methods(
                    &c.instance_methods,
                    &class_fqn_root,
                    MethodKind::Instance,
                    c.properties
                        .iter()
                        .map(|property| property.name.as_str().to_string()),
                );
                let generic_params = c
                    .generic_params
                    .iter()
                    .map(|n| n.as_str().to_string())
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Class(TypeScriptClass {
                        name: bare,
                        source: key.clone(),
                        generic_params,
                        docstring: c.docstring.clone(),
                        properties,
                        static_methods,
                        instance_methods,
                    }),
                    sort_key,
                ));
            }
            Symbol::Enum(e) => {
                let sort_key = origin_key(&e.origin);
                let variants = e
                    .variants
                    .iter()
                    .map(|v| TypeScriptEnumVariant {
                        ident: v.name.as_str().to_string(),
                        value: v.value.clone(),
                        docstring: v.docstring.clone(),
                    })
                    .collect();
                out.push((
                    leaf,
                    EmittedSymbol::Enum(TypeScriptEnum {
                        name: bare,
                        source: key.clone(),
                        variants,
                        docstring: e.docstring.clone(),
                    }),
                    sort_key,
                ));
            }
            Symbol::TypeAlias(t) => {
                let sort_key = origin_key(&t.origin);
                out.push((
                    leaf,
                    EmittedSymbol::TypeAlias(TypeScriptTypeAlias {
                        name: bare,
                        source: key.clone(),
                        resolves_to: t.resolves_to.clone(),
                        recursive: t.recursive,
                    }),
                    sort_key,
                ));
            }
            Symbol::Function(f) => {
                let sort_key = origin_key(&f.origin);
                let names = binding_names
                    .get_mut(&leaf)
                    .expect("every symbol leaf has a binding-name scope");
                expand_function(&leaf, key, f, &sort_key, names, &mut out);
            }
        }
    }

    out
}

/// Fan out an authored `Symbol::Function` into its available flat host
/// projections. Every projection keeps the same authored FQN.
fn expand_function(
    leaf: &LeafPath,
    key: &Name,
    f: &baml_codegen_types::Function,
    sort_key: &SortKey,
    names: &mut BindingNameAllocator,
    out: &mut Vec<(LeafPath, EmittedSymbol, SortKey)>,
) {
    let fqn_root = key.to_string();
    let bare = key.name().as_str().to_string();
    let func_generic_params: Vec<String> = f
        .generic_params
        .iter()
        .map(|n| n.as_str().to_string())
        .collect();
    let func_docstring = f.docstring.clone();
    let raises_names = collect_raises_names(f.throws.as_ref());
    for (role, return_ty) in operation_bindings(f) {
        let arguments = arguments_for_role(f, role);
        let params = arguments
            .iter()
            .map(|argument| argument.name.as_str().to_string())
            .collect();
        let arg_tys = arguments
            .iter()
            .map(|argument| argument.ty.clone())
            .collect();
        let arg_defaults = arguments
            .iter()
            .map(|argument| argument.default.clone())
            .collect();
        let mode = if role.is_async() {
            SyncAsync::Async
        } else {
            SyncAsync::Sync
        };
        out.push((
            leaf.clone(),
            EmittedSymbol::Function(TypeScriptFunction {
                name: names.binding_name(role, &bare),
                baml_fqn: fqn_root.clone(),
                mode,
                role,
                param_names: params,
                arg_tys,
                arg_defaults,
                return_ty,
                generic_params: func_generic_params.clone(),
                docstring: func_docstring.clone(),
                raises_names: raises_names.clone(),
            }),
            sort_key.clone(),
        ));
    }
}

fn operation_bindings(function: &baml_codegen_types::Function) -> Vec<(BindingRole, Ty)> {
    let mut bindings = vec![
        (BindingRole::DirectSync, function.return_type.clone()),
        (BindingRole::DirectAsync, function.return_type.clone()),
    ];
    if let Some(spec) = &function.operations.spec {
        bindings.extend([
            (BindingRole::SpecSync, spec.return_type.clone()),
            (BindingRole::SpecAsync, spec.return_type.clone()),
        ]);
    }
    if let Some(stream) = &function.operations.stream {
        bindings.extend([
            (BindingRole::StreamSync, stream.return_type.clone()),
            (BindingRole::StreamAsync, stream.return_type.clone()),
        ]);
    }
    bindings
}

fn arguments_for_role(
    function: &baml_codegen_types::Function,
    role: BindingRole,
) -> Vec<&FunctionArgument> {
    match role {
        BindingRole::DirectSync | BindingRole::DirectAsync => function.arguments.iter().collect(),
        BindingRole::SpecSync | BindingRole::SpecAsync => function
            .arguments
            .iter()
            .filter(|argument| !argument.injected)
            .collect(),
        BindingRole::StreamSync | BindingRole::StreamAsync => function
            .arguments
            .iter()
            .filter(|argument| !argument.injected)
            .chain(
                function
                    .operations
                    .stream
                    .as_ref()
                    .expect("a Stream binding has Stream operation metadata")
                    .control_arguments
                    .iter(),
            )
            .collect(),
    }
}

/// Collect the unqualified leaf names of the thrown types in a `throws`
/// `Ty`, in source order, de-duping exact-equal names. Class/Enum/
/// `TypeAlias` contribute their unqualified leaf name; a union contributes
/// each member's; an optional unwraps; anything else contributes nothing.
fn collect_raises_names(throws: Option<&baml_codegen_types::Ty>) -> Vec<String> {
    use baml_codegen_types::Ty;

    fn walk(ty: &Ty, out: &mut Vec<String>) {
        match ty {
            Ty::Class(name, _, _) | Ty::Enum(name, _) | Ty::TypeAlias(name, _) => {
                let n = name.name().as_str().to_string();
                if !out.contains(&n) {
                    out.push(n);
                }
            }
            Ty::Union(members, _) => members.iter().for_each(|m| walk(m, out)),
            _ => {}
        }
    }

    let mut out = Vec::new();
    if let Some(ty) = throws {
        walk(ty, &mut out);
    }
    out
}

/// Fan out source-declared methods into all available operation bindings.
/// Methods are sorted by `(file, span, name)`.
fn expand_methods(
    methods: &[baml_codegen_types::Function],
    class_fqn_root: &str,
    kind: MethodKind,
    additional_authored_names: impl IntoIterator<Item = String>,
) -> Vec<TypeScriptMethodBinding> {
    let mut sorted: Vec<&baml_codegen_types::Function> = methods.iter().collect();
    sorted.sort_by_key(|m| (origin_key(&m.origin), m.name.as_str()));

    let mut names = BindingNameAllocator::default();
    for method in methods {
        names.reserve(method.name.as_str());
    }
    for name in additional_authored_names {
        names.reserve(name);
    }

    let mut out: Vec<TypeScriptMethodBinding> = Vec::new();
    for m in sorted {
        let m_name = m.name.as_str();
        let bare = m_name.to_string();
        let fqn_root = format!("{class_fqn_root}.{m_name}");
        let method_generic_params: Vec<String> = m
            .generic_params
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let method_docstring = m.docstring.clone();
        let raises_names = collect_raises_names(m.throws.as_ref());
        for (role, return_ty) in operation_bindings(m) {
            let arguments = arguments_for_role(m, role);
            let (required_args, optional_args) = split_arguments(&arguments);
            let mode = if role.is_async() {
                SyncAsync::Async
            } else {
                SyncAsync::Sync
            };
            out.push(TypeScriptMethodBinding {
                name: names.binding_name(role, &bare),
                baml_fqn: fqn_root.clone(),
                mode,
                role,
                kind,
                required_args,
                optional_args,
                return_ty,
                generic_params: method_generic_params.clone(),
                docstring: method_docstring.clone(),
                raises_names: raises_names.clone(),
            });
        }
    }
    out
}

fn split_arguments(arguments: &[&FunctionArgument]) -> (Vec<RequiredArg>, Vec<OptionalArg>) {
    let first_optional = arguments
        .iter()
        .position(|arg| arg.default.is_some())
        .unwrap_or(arguments.len());
    (
        arguments[..first_optional]
            .iter()
            .map(|arg| RequiredArg {
                name: arg.name.as_str().to_string(),
                ty: arg.ty.clone(),
            })
            .collect(),
        arguments[first_optional..]
            .iter()
            .map(|arg| OptionalArg {
                name: arg.name.as_str().to_string(),
                ty: arg.ty.clone(),
                default: arg
                    .default
                    .clone()
                    .expect("arguments after the first defaulted method arg must have defaults"),
            })
            .collect(),
    )
}

fn origin_key(origin: &baml_codegen_types::Origin) -> SortKey {
    (origin.source_file_path.clone(), origin.span_start)
}
