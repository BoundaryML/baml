//! Pool-wide analysis the emitters consume: which nominal types can be
//! emitted, where recursive class fields need `Box`, and which module
//! path segments must be renamed away from type-name collisions.

use std::collections::{BTreeMap, HashMap, HashSet};

use baml_codegen_types::{Name, Symbol, SymbolPool, Ty};

use crate::{SkipWarning, routing};

/// Results of [`analyze`]. Emitters treat this as read-only context.
pub(crate) struct Analysis {
    /// Nominal types (classes and enums) that will be emitted. A
    /// reference to anything else in a signature or field makes the
    /// referencing symbol unsupported.
    pub(crate) emitted: HashSet<Name>,
    /// Strongly-connected-component id per emitted class. Two classes in
    /// the same SCC form a containment cycle through non-heap fields;
    /// class references between them are boxed at the field site.
    scc: HashMap<Name, usize>,
    /// Module-path renames: a namespace segment that collides with a type
    /// name emitted in its parent module gets a trailing underscore
    /// (Rust puts `mod` and type names in one namespace). Keyed by the
    /// original routed path; values are the fully renamed path.
    renames: HashMap<Vec<String>, Vec<String>>,
}

impl Analysis {
    pub(crate) fn is_emitted(&self, name: &Name) -> bool {
        self.emitted.contains(name)
    }

    /// Whether a field of `owner` referencing `target` sits on a
    /// containment cycle (and therefore needs `Box`).
    pub(crate) fn needs_box(&self, owner: &Name, target: &Name) -> bool {
        match (self.scc.get(owner), self.scc.get(target)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// The on-disk / in-crate module path for a routed path, with
    /// collision renames applied.
    pub(crate) fn renamed<'a>(&'a self, path: &'a [String]) -> &'a [String] {
        match self.renames.get(path) {
            Some(renamed) => renamed,
            None => path,
        }
    }
}

/// Analyze the pool. Returns the analysis plus one warning per skipped
/// class or type alias (functions warn separately at emission time).
pub(crate) fn analyze(pool: &SymbolPool) -> (Analysis, Vec<SkipWarning>) {
    let mut warnings = Vec::new();

    // Deterministic iteration everywhere: sort by Name.
    let mut classes: Vec<(&Name, &baml_codegen_types::Class)> = Vec::new();
    let mut aliases: Vec<(&Name, &baml_codegen_types::TypeAlias)> = Vec::new();
    let mut enums: HashSet<Name> = HashSet::new();
    for (name, symbol) in pool {
        match symbol {
            Symbol::Class(class) => classes.push((name, class)),
            Symbol::Enum(_) => {
                if name.name.as_str().contains('$') {
                    warnings.push(SkipWarning {
                        fqn: name.to_string(),
                        reason: "companion types ($stream, …) are not emitted yet".to_string(),
                    });
                } else {
                    enums.insert(name.clone());
                }
            }
            Symbol::TypeAlias(alias) => aliases.push((name, alias)),
            Symbol::Function(_) => {}
        }
    }
    classes.sort_by(|(a, _), (b, _)| a.cmp(b));
    aliases.sort_by(|(a, _), (b, _)| a.cmp(b));

    // Per-class field requirements: either a list of nominal types the
    // fields reference, or the reason the class is structurally
    // unsupported.
    let mut deps: BTreeMap<&Name, Vec<Name>> = BTreeMap::new();
    let mut alive: HashSet<Name> = HashSet::new();
    for (name, class) in &classes {
        // `$`-suffixed companion types ($stream partials, …) are not
        // representable as Rust identifiers and are not emitted yet —
        // same filter the function emitter applies.
        if name.name.as_str().contains('$') {
            warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason: "companion types ($stream, …) are not emitted yet".to_string(),
            });
            continue;
        }
        if !class.generic_params.is_empty() {
            warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason: "generic classes are not emitted yet".to_string(),
            });
            continue;
        }
        let mut class_deps = Vec::new();
        let unsupported = class.properties.iter().find_map(|prop| {
            field_deps(&prop.ty, &mut class_deps)
                .err()
                .map(|reason| (prop.name.as_str(), reason))
        });
        match unsupported {
            Some((field, reason)) => warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason: format!("field `{field}`: {reason}"),
            }),
            None => {
                deps.insert(name, class_deps);
                alive.insert((*name).clone());
            }
        }
    }

    // Type aliases join the same fixpoint. Non-recursive alias
    // *references* are inlined by the pool builder, so the alias items
    // themselves are the SDK-surface representation of the user's named
    // types; only recursive aliases (unrepresentable as a plain Rust
    // `type`) and structurally unsupported right-hand sides skip.
    for (name, alias) in &aliases {
        if name.name.as_str().contains('$') {
            warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason: "companion types ($stream, …) are not emitted yet".to_string(),
            });
            continue;
        }
        if alias.recursive {
            warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason: "recursive type aliases are not representable as a plain Rust `type` yet"
                    .to_string(),
            });
            continue;
        }
        let mut alias_deps = Vec::new();
        match field_deps(&alias.resolves_to, &mut alias_deps) {
            Err(reason) => warnings.push(SkipWarning {
                fqn: name.to_string(),
                reason,
            }),
            Ok(()) => {
                deps.insert(name, alias_deps);
                alive.insert((*name).clone());
            }
        }
    }

    // Fixpoint: a class or alias referencing a skipped (or absent)
    // nominal type is itself skipped. Enums never skip, so only
    // class/alias deps can fail.
    loop {
        let mut removed = Vec::new();
        for (name, class_deps) in &deps {
            if !alive.contains(*name) {
                continue;
            }
            if let Some(dead) = class_deps
                .iter()
                .find(|dep| !alive.contains(dep) && !enums.contains(dep))
            {
                warnings.push(SkipWarning {
                    fqn: name.to_string(),
                    reason: format!("references skipped or unknown type `{dead}`"),
                });
                removed.push((*name).clone());
            }
        }
        if removed.is_empty() {
            break;
        }
        for name in removed {
            alive.remove(&name);
        }
    }

    let mut emitted = alive;
    emitted.extend(enums.iter().cloned());

    // Containment graph over emitted classes: an edge per class reference
    // reachable without crossing a heap-indirected container (`Vec`,
    // `Map`); those already break cycles, so only direct/optional
    // references participate.
    let class_edges: BTreeMap<&Name, Vec<&Name>> = classes
        .iter()
        .filter(|(name, _)| emitted.contains(*name))
        .map(|(name, class)| {
            let mut targets = Vec::new();
            for prop in &class.properties {
                non_heap_class_refs(&prop.ty, &emitted, &enums, &mut targets);
            }
            (*name, targets)
        })
        .collect();
    let scc = compute_sccs(&class_edges);

    let renames = compute_renames(pool, &emitted);

    (
        Analysis {
            emitted,
            scc,
            renames,
        },
        warnings,
    )
}

/// Collect the nominal types a field type references, or the reason the
/// type cannot be represented. Mirrors the supported subset of
/// `translate_ty` — the two must agree, and the emission path re-checks
/// through `translate_ty` so a disagreement fails codegen loudly rather
/// than emitting broken code.
fn field_deps(ty: &Ty, deps: &mut Vec<Name>) -> Result<(), String> {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Unit
        | Ty::Literal(_)
        | Ty::Uint8Array => Ok(()),
        Ty::List(inner) => field_deps(inner, deps),
        Ty::Map { key, value } => {
            match key.as_ref() {
                Ty::String => {}
                other => return Err(format!("unsupported map key type: {other}")),
            }
            field_deps(value, deps)
        }
        Ty::Union(items) => {
            if let [a, b] = items.as_slice() {
                let inner = match (a, b) {
                    (Ty::Null, other) | (other, Ty::Null) => Some(other),
                    _ => None,
                };
                if let Some(inner) = inner {
                    return field_deps(inner, deps);
                }
            }
            Err("unsupported type: union".to_string())
        }
        Ty::Class(name, args) => {
            if args.is_empty() {
                deps.push(name.clone());
                Ok(())
            } else {
                Err("unsupported type: generic class".to_string())
            }
        }
        Ty::Enum(name) => {
            deps.push(name.clone());
            Ok(())
        }
        // Opaque alias references (the pool builder inlines in-package
        // non-recursive aliases, so these are recursive or cross-package
        // ones): representable iff the alias item itself is emitted.
        Ty::TypeAlias(name) => {
            deps.push(name.clone());
            Ok(())
        }
        Ty::TypeVar(_) => Err("unsupported type: type variable (generics)".to_string()),
        Ty::Media(kind) => Err(format!("unsupported type: media ({kind})")),
        Ty::BuiltinUnknown => Err("unsupported type: unknown".to_string()),
        Ty::Callable { .. } => Err("unsupported type: callable".to_string()),
        Ty::BamlOptions => Err("unsupported type: baml.Options".to_string()),
        Ty::RustType => Err("unsupported type: $rust_type handle".to_string()),
    }
}

/// Collect emitted-class references reachable without crossing `Vec` /
/// `Map` (heap indirection already breaks the containment cycle there).
fn non_heap_class_refs<'a>(
    ty: &'a Ty,
    emitted: &HashSet<Name>,
    enums: &HashSet<Name>,
    out: &mut Vec<&'a Name>,
) {
    match ty {
        Ty::Class(name, args) if args.is_empty() => {
            if emitted.contains(name) && !enums.contains(name) {
                out.push(name);
            }
        }
        Ty::Union(items) => {
            for item in items {
                non_heap_class_refs(item, emitted, enums, out);
            }
        }
        // Heap-indirected containers end the walk.
        Ty::List(_) | Ty::Map { .. } => {}
        // Opaque alias references contribute no containment edges: an
        // in-package non-recursive alias is inlined before it reaches
        // codegen, and a cross-package alias cannot sit on a class cycle
        // because package dependencies are acyclic.
        Ty::TypeAlias(_) => {}
        _ => {}
    }
}

/// Iterative Tarjan SCC over the containment graph. Only classes on an
/// actual cycle matter, but assigning every node an SCC id keeps
/// `needs_box` a plain id comparison (a lone class is its own singleton
/// component, and a self-loop shares its id with itself trivially — the
/// self-loop case is why edges to the owner itself must also be present).
fn compute_sccs(edges: &BTreeMap<&Name, Vec<&Name>>) -> HashMap<Name, usize> {
    // Tarjan needs an explicit stack to avoid recursing on user-sized
    // graphs. State per node: index, lowlink, on-stack flag.
    struct NodeState {
        index: usize,
        lowlink: usize,
        on_stack: bool,
    }

    let mut states: HashMap<&Name, NodeState> = HashMap::new();
    let mut stack: Vec<&Name> = Vec::new();
    let mut next_index = 0;
    let mut next_scc = 0;
    let mut sccs: HashMap<Name, usize> = HashMap::new();
    // A singleton component only counts as a cycle when it has a
    // self-loop; give non-self-looping singletons unique ids by default
    // (which `needs_box` compares as inequality anyway) — the id equality
    // test then exactly answers "is there a containment cycle".
    let mut self_loops: HashSet<&Name> = HashSet::new();
    for (name, targets) in edges {
        if targets.contains(name) {
            self_loops.insert(name);
        }
    }

    for root in edges.keys() {
        if states.contains_key(*root) {
            continue;
        }
        // Explicit DFS frame: (node, next child index to visit).
        let mut dfs: Vec<(&Name, usize)> = vec![(*root, 0)];
        while let Some((node, child_idx)) = dfs.pop() {
            if child_idx == 0 {
                states.insert(
                    node,
                    NodeState {
                        index: next_index,
                        lowlink: next_index,
                        on_stack: true,
                    },
                );
                next_index += 1;
                stack.push(node);
            }
            let children = edges.get(node).map(Vec::as_slice).unwrap_or_default();
            if let Some(child) = children.get(child_idx) {
                dfs.push((node, child_idx + 1));
                match states.get(*child) {
                    None => dfs.push((child, 0)),
                    Some(child_state) if child_state.on_stack => {
                        let child_index = child_state.index;
                        let state = states.get_mut(node).unwrap();
                        state.lowlink = state.lowlink.min(child_index);
                    }
                    Some(_) => {}
                }
            } else {
                // Node finished: propagate lowlink to the parent frame and
                // pop a component when this node is its root.
                let state = &states[node];
                let (index, lowlink) = (state.index, state.lowlink);
                if let Some((parent, _)) = dfs.last() {
                    let parent_state = states.get_mut(*parent).unwrap();
                    parent_state.lowlink = parent_state.lowlink.min(lowlink);
                }
                if index == lowlink {
                    let mut members = Vec::new();
                    while let Some(member) = stack.pop() {
                        states.get_mut(member).unwrap().on_stack = false;
                        members.push(member);
                        if member == node {
                            break;
                        }
                    }
                    // Multi-member components are cycles; singletons only
                    // with a self-loop. Non-cyclic nodes get no entry, so
                    // `needs_box` is false for them.
                    if members.len() > 1 || self_loops.contains(node) {
                        for member in members {
                            sccs.insert(member.clone(), next_scc);
                        }
                        next_scc += 1;
                    }
                }
            }
        }
    }
    sccs
}

/// Rust puts modules and types in one namespace: a namespace segment
/// whose name equals a type emitted in its parent module is renamed with
/// a trailing underscore (deterministic, and cascading to descendants).
fn compute_renames(
    pool: &SymbolPool,
    emitted: &HashSet<Name>,
) -> HashMap<Vec<String>, Vec<String>> {
    // Types per module path (original routed segments).
    let mut types_in: HashMap<Vec<String>, HashSet<String>> = HashMap::new();
    let mut all_paths: HashSet<Vec<String>> = HashSet::new();
    for (name, symbol) in pool {
        let path = routing::route(name).segments;
        for depth in 0..=path.len() {
            all_paths.insert(path[..depth].to_vec());
        }
        if matches!(
            symbol,
            Symbol::Class(_) | Symbol::Enum(_) | Symbol::TypeAlias(_)
        ) && emitted.contains(name)
        {
            types_in
                .entry(path.clone())
                .or_default()
                .insert(name.name.as_str().to_string());
        }
    }

    // Rename each colliding segment, deepest paths inheriting their
    // ancestors' renames.
    let mut renames: HashMap<Vec<String>, Vec<String>> = HashMap::new();
    let mut sorted_paths: Vec<Vec<String>> = all_paths.into_iter().collect();
    sorted_paths.sort();
    for path in sorted_paths {
        let Some((child, parent)) = path.split_last() else {
            continue;
        };
        let renamed_parent = renames
            .get(parent)
            .cloned()
            .unwrap_or_else(|| parent.to_vec());
        let collides = types_in
            .get(parent)
            .is_some_and(|types| types.contains(child));
        let renamed_child = if collides {
            format!("{child}_")
        } else {
            child.clone()
        };
        if collides || renames.contains_key(parent) {
            let mut renamed = renamed_parent;
            renamed.push(renamed_child);
            renames.insert(path, renamed);
        }
    }
    renames
}
