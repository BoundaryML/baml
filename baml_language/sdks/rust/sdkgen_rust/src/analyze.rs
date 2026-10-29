//! Pool-wide analysis the emitters consume: which nominal types can be
//! emitted, where recursive class fields need `Box`, which module path
//! segments must be renamed away from type-name collisions, and which
//! types get serde impls under the generated crate's `serde` feature.

use std::collections::{BTreeMap, HashMap, HashSet};

use baml_sdkgen_types::{Class, ClassCrossing, ClassProperty, Name, Symbol, SymbolPool, Ty};

use crate::{SkipWarning, routing};

/// Results of [`analyze`]. Emitters treat this as read-only context.
pub(crate) struct Analysis {
    /// Nominal types (classes and enums) that will be emitted. A
    /// reference to anything else in a signature or field makes the
    /// referencing symbol unsupported.
    pub(crate) emitted: HashSet<Name>,
    /// Strongly-connected-component id per emitted class and type alias.
    /// Two nodes in the same SCC form a containment cycle through non-heap
    /// fields; class and alias references between them are boxed at the
    /// field site.
    scc: HashMap<Name, usize>,
    /// Module-path renames: a namespace segment that collides with a type
    /// name emitted in its parent module gets a trailing underscore
    /// (Rust puts `mod` and type names in one namespace). Keyed by the
    /// original routed path; values are the fully renamed path.
    renames: HashMap<Vec<String>, Vec<String>>,
    /// Emitted type names per (renamed) leaf — the namespace synthesized
    /// union enums must not collide with.
    type_names_by_leaf: HashMap<Vec<String>, HashSet<String>>,
    /// The right-hand side of every emitted type alias that a statement
    /// spells by its body. The one recursive alias the SDK emits, the stdlib's
    /// `json`, exists as a declaration at run time and is spelled by name, so
    /// it is not here.
    alias_bodies: HashMap<Name, Ty>,
    /// Emitted classes and aliases whose values all have a JSON form
    /// serde can match to BAML's own (`baml.json`), so their Rust types
    /// get serde impls under the generated crate's `serde` feature.
    serde: HashSet<Name>,
    /// How each class in the pool crosses the boundary, decided by the
    /// compiler once per declaration.
    crossings: HashMap<Name, ClassCrossing>,
    /// `baml.json.json` and the aliases whose chain ends at it: their Rust
    /// type is `baml_bridge::Json`, which reads a missing key as `null`
    /// only with `serde(default)`.
    json_aliases: HashSet<Name>,
}

impl Analysis {
    pub(crate) fn is_emitted(&self, name: &Name) -> bool {
        self.emitted.contains(name)
    }

    /// Whether the alias `name` is `baml.json.json` or resolves to it.
    pub(crate) fn is_json_alias(&self, name: &Name) -> bool {
        self.json_aliases.contains(name)
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

    /// The right-hand side of an emitted type alias a statement spells by
    /// its body.
    pub(crate) fn alias_body(&self, name: &Name) -> Option<&Ty> {
        self.alias_bodies.get(name)
    }

    /// Emitted type names in a (renamed) leaf.
    pub(crate) fn type_names_in(&self, leaf: &[String]) -> HashSet<String> {
        self.type_names_by_leaf
            .get(leaf)
            .cloned()
            .unwrap_or_default()
    }

    /// How the class `name` crosses the boundary, when the pool declares it.
    pub(crate) fn crossing(&self, name: &Name) -> Option<ClassCrossing> {
        self.crossings.get(name).copied()
    }

    /// Whether the emitted class `name` derives serde's traits.
    pub(crate) fn derives_serde(&self, name: &Name) -> bool {
        self.serde.contains(name)
    }

    /// Whether the Rust type for `ty` implements serde's traits under the
    /// `serde` feature.
    pub(crate) fn has_serde(&self, ty: &Ty) -> bool {
        has_serde(ty, &self.serde)
    }
}

/// How a class's instances appear to a Rust host, decided by the class's
/// [`ClassCrossing`].
pub(crate) enum ClassShape<'a> {
    /// Plain data: a struct the host reads and builds by these fields.
    Fields(&'a [ClassProperty]),
    /// The engine's instance, held by reference: its declared fields are the
    /// runtime's own internals and never reach the host.
    Opaque,
    /// A capability or value with a crossing of its own (`ai.stream.Stream`,
    /// `ai.FunctionSpec`, `ai.Prompt`): a `baml_bridge` type stands in
    /// wherever a type names the class, so the class itself emits nothing.
    Bridged,
    /// A media wrapper, carried by its own wire arm.
    Media,
}

impl<'a> ClassShape<'a> {
    /// The fields a host sees: none for a class that does not cross by its
    /// fields.
    pub(crate) fn properties(&self) -> &'a [ClassProperty] {
        match self {
            Self::Fields(properties) => properties,
            Self::Opaque | Self::Bridged | Self::Media => &[],
        }
    }

    pub(crate) fn of(class: &'a Class) -> Self {
        match class.crossing {
            ClassCrossing::Fields => Self::Fields(&class.properties),
            ClassCrossing::Opaque => Self::Opaque,
            ClassCrossing::Stream | ClassCrossing::FunctionSpec | ClassCrossing::Prompt => {
                Self::Bridged
            }
            ClassCrossing::Media(_) => Self::Media,
        }
    }
}

/// Analyze the pool. Returns the analysis plus one warning per skipped
/// class or type alias (functions warn separately at emission time).
pub(crate) fn analyze(pool: &SymbolPool) -> (Analysis, Vec<SkipWarning>) {
    let mut warnings = Vec::new();

    // Deterministic iteration everywhere: sort by Name.
    let mut classes: Vec<(&Name, &baml_sdkgen_types::Class)> = Vec::new();
    let mut aliases: Vec<(&Name, &baml_sdkgen_types::TypeAlias)> = Vec::new();
    let mut enums: HashSet<Name> = HashSet::new();
    for (name, symbol) in pool {
        match symbol {
            Symbol::Class(class) => classes.push((name, class)),
            Symbol::Enum(_) => {
                if name.name().as_str().contains('$') {
                    warnings.push(SkipWarning::new(
                        name,
                        "`$`-named types are not representable as Rust identifiers",
                    ));
                } else {
                    enums.insert(name.clone());
                }
            }
            Symbol::TypeAlias(alias) => aliases.push((name, alias)),
            Symbol::Function(_) => {}
        }
    }
    classes.sort_by_key(|(name, _)| *name);
    aliases.sort_by_key(|(name, _)| *name);

    // Per-class field requirements: either a list of nominal types the
    // fields reference, or the reason the class is structurally
    // unsupported.
    let mut deps: BTreeMap<&Name, Vec<Name>> = BTreeMap::new();
    let mut alive: HashSet<Name> = HashSet::new();
    // Classes a `baml_bridge` type represents: a reference to one is
    // representable though the class itself emits nothing.
    let mut bridged: HashSet<Name> = HashSet::new();
    for (name, class) in &classes {
        // `$`-named types are not representable as Rust identifiers —
        // same filter the function emitter applies.
        if name.name().as_str().contains('$') {
            warnings.push(SkipWarning::new(
                name,
                "`$`-named types are not representable as Rust identifiers",
            ));
            continue;
        }
        // A generic class emits as a `<T: BamlValue>` struct: its own type
        // params come into scope, so a `Ty::TypeVar` naming one is a
        // representable field (it resolves to that Rust generic parameter).
        let class_params: Vec<&str> = class
            .generic_params
            .iter()
            .map(baml_base::Name::as_str)
            .collect();
        let shape = ClassShape::of(class);
        let properties = match shape {
            ClassShape::Fields(properties) => properties,
            ClassShape::Opaque => &[],
            ClassShape::Bridged => {
                bridged.insert((*name).clone());
                continue;
            }
            ClassShape::Media => {
                // BUG: a media value has no Rust representation yet (the
                // `media_value` wire arm has no `baml_bridge` type), so a
                // media wrapper emits nothing and any signature naming one
                // skips.
                warnings.push(SkipWarning::new(
                    name,
                    "media values have no Rust representation yet",
                ));
                continue;
            }
        };
        let mut class_deps = Vec::new();
        let unsupported = properties.iter().find_map(|prop| {
            field_deps(&prop.ty, &class_params, &mut class_deps)
                .err()
                .map(|reason| (prop.name.as_str(), reason))
        });
        match unsupported {
            Some((field, reason)) => {
                warnings.push(SkipWarning::new(name, format!("field `{field}`: {reason}")));
            }
            None => {
                // Every type parameter must appear in some field in a
                // *non-recursive* position. An entirely unused param is an
                // `E0392`; a param used only inside the class's own recursive
                // references (`GNode<T> { children: GNode<T>[] }`) is the
                // "type parameter is only used recursively" error — Rust
                // cannot determine its variance. Either way the struct would
                // not compile, and PhantomData would leak into the public
                // struct literal, so skip (fail closed). An opaque class's
                // parameters say what its instance is, through a private
                // marker, so it needs no field to use them.
                let mut used = HashSet::new();
                for prop in properties {
                    collect_non_recursive_type_vars(&prop.ty, name, &mut used);
                }
                if let ClassShape::Fields(_) = shape
                    && let Some(unused) = class_params.iter().find(|param| !used.contains(**param))
                {
                    warnings.push(SkipWarning::new(
                        name,
                        format!(
                            "generic type parameter `{unused}` is not used in a non-recursive \
                             field position (not representable as a Rust struct)"
                        ),
                    ));
                    continue;
                }
                deps.insert(name, class_deps);
                alive.insert((*name).clone());
            }
        }
    }

    // Type aliases join the same fixpoint. Alias references stay nominal in
    // the pool, so the alias items are the SDK-surface representation of
    // the user's named types; only recursive aliases (unrepresentable as a
    // plain Rust `type`) and structurally unsupported right-hand sides skip.
    for (name, alias) in &aliases {
        if name.name().as_str().contains('$') {
            warnings.push(SkipWarning::new(
                name,
                "`$`-named types are not representable as Rust identifiers",
            ));
            continue;
        }
        // The stdlib's recursive `json` alias maps onto `baml_bridge::Json`
        // rather than a Rust `type` of its expansion, so it is always
        // representable and depends on nothing.
        if crate::translate_ty::is_canonical_json(name) {
            deps.insert(name, Vec::new());
            alive.insert((*name).clone());
            continue;
        }
        if alias.recursive {
            warnings.push(SkipWarning::new(
                name,
                "recursive type aliases are not representable as a plain Rust `type` yet",
            ));
            continue;
        }
        let mut alias_deps = Vec::new();
        // Aliases carry no type parameters of their own in the current
        // scope, so no TypeVar is ever in scope for their right-hand side.
        match field_deps(&alias.resolves_to, &[], &mut alias_deps) {
            Err(reason) => warnings.push(SkipWarning::new(name, reason)),
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
                .find(|dep| !alive.contains(dep) && !enums.contains(dep) && !bridged.contains(dep))
            {
                warnings.push(SkipWarning::new(
                    name,
                    format!("references skipped or unknown type `{dead}`"),
                ));
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

    // Greatest fixpoint, so recursive types keep their impls: start from
    // every emitted class and alias, then drop each one holding a type
    // without serde impls until nothing changes. Classes from BAML's
    // bundled packages never start in: many render through a
    // `baml.ToJson` override (`baml.time.PlainDate` is a date string),
    // which a derive cannot follow, and some are `baml_bridge` handles.
    let serde_candidates: Vec<(&Name, Vec<&Ty>)> = classes
        .iter()
        .filter(|(name, _)| !baml_base::LangPackage::is_manifest_name(name.package().as_str()))
        .map(|(name, class)| (*name, class.properties.iter().map(|p| &p.ty).collect()))
        .chain(
            aliases
                .iter()
                .map(|(name, alias)| (*name, vec![&alias.resolves_to])),
        )
        .filter(|(name, _)| emitted.contains(*name))
        .collect();
    let mut serde: HashSet<Name> = serde_candidates
        .iter()
        .map(|(name, _)| (*name).clone())
        .collect();
    loop {
        let lacking: Vec<&Name> = serde_candidates
            .iter()
            .filter(|(name, tys)| {
                serde.contains(*name) && !tys.iter().all(|ty| has_serde(ty, &serde))
            })
            .map(|(name, _)| *name)
            .collect();
        if lacking.is_empty() {
            break;
        }
        for name in lacking {
            serde.remove(name);
        }
    }

    // Containment graph over emitted classes and type aliases: an edge per
    // class or alias reference reachable without crossing a heap-indirected
    // container (`Vec`, `Map`); those already break cycles, so only
    // direct/optional references participate. A Rust `type` alias holds its
    // target inline, so `type Part = Text | Extra` with `class Extra { part
    // Part }` is a cycle through the alias.
    let class_edges: BTreeMap<&Name, Vec<&Name>> = classes
        .iter()
        .filter(|(name, _)| emitted.contains(*name))
        .map(|(name, class)| {
            let mut targets = Vec::new();
            for prop in ClassShape::of(class).properties() {
                non_heap_class_refs(&prop.ty, &emitted, &enums, &mut targets);
            }
            (*name, targets)
        })
        .chain(
            aliases
                .iter()
                .filter(|(name, _)| emitted.contains(*name))
                .map(|(name, alias)| {
                    let mut targets = Vec::new();
                    non_heap_class_refs(&alias.resolves_to, &emitted, &enums, &mut targets);
                    (*name, targets)
                }),
        )
        .collect();
    let scc = compute_sccs(&class_edges);

    let renames = compute_renames(pool, &emitted);

    // Emitted type names per renamed leaf, for synthesized-name
    // de-collision.
    let mut type_names_by_leaf: HashMap<Vec<String>, HashSet<String>> = HashMap::new();
    for (name, symbol) in pool {
        if matches!(
            symbol,
            Symbol::Class(_) | Symbol::Enum(_) | Symbol::TypeAlias(_)
        ) && emitted.contains(name)
        {
            let routed = routing::route(name).segments;
            let leaf = renames.get(&routed).cloned().unwrap_or(routed);
            type_names_by_leaf
                .entry(leaf)
                .or_default()
                .insert(name.name().to_string());
        }
    }

    let alias_bodies = aliases
        .iter()
        .filter(|(name, _)| {
            emitted.contains(*name) && !crate::translate_ty::is_canonical_json(name)
        })
        .map(|(name, alias)| ((*name).clone(), alias.resolves_to.clone()))
        .collect();
    // Aliases of `json`, through any chain of aliases.
    let mut json_aliases: HashSet<Name> = HashSet::from([Name::from_dotted_path(
        baml_base::qualified_name::BAML_JSON_JSON,
    )]);
    loop {
        let before = json_aliases.len();
        for (name, alias) in &aliases {
            if let Ty::TypeAlias(target) = &alias.resolves_to
                && json_aliases.contains(target)
            {
                json_aliases.insert((*name).clone());
            }
        }
        if json_aliases.len() == before {
            break;
        }
    }

    (
        Analysis {
            emitted,
            scc,
            renames,
            type_names_by_leaf,
            alias_bodies,
            serde,
            crossings: classes
                .iter()
                .map(|(name, class)| ((*name).clone(), class.crossing))
                .collect(),
            json_aliases,
        },
        warnings,
    )
}

/// Collect the nominal types a field type references, or the reason the
/// type cannot be represented. `generic_params` are the enclosing generic
/// class's own type parameters, in scope for its fields (a `Ty::TypeVar`
/// naming one is representable). Mirrors the supported subset of
/// `translate_ty` — the two must agree, and the emission path re-checks
/// through `translate_ty` so a disagreement fails codegen loudly rather
/// than emitting broken code.
fn field_deps(ty: &Ty, generic_params: &[&str], deps: &mut Vec<Name>) -> Result<(), String> {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(..)
        | Ty::Uint8Array => Ok(()),
        Ty::List(inner) => field_deps(inner, generic_params, deps),
        Ty::Map { key, value, .. } => {
            match key.as_ref() {
                Ty::String => {}
                other => return Err(format!("unsupported map key type: {other}")),
            }
            field_deps(value, generic_params, deps)
        }
        Ty::Union(items) => {
            let (arms, _) = crate::unions::strip_null(items);
            match arms.as_slice() {
                // Pure optionality (or the degenerate all-null union).
                [] => Ok(()),
                [only] => field_deps(only, generic_params, deps),
                arms => {
                    if let Some(reason) = crate::unions::shape_error(arms) {
                        return Err(reason);
                    }
                    for arm in arms {
                        field_deps(arm, generic_params, deps)?;
                    }
                    Ok(())
                }
            }
        }
        // A class reference depends on the class item; its concrete type
        // arguments are themselves types that must be representable (a
        // generic argument referencing a skipped type poisons this field).
        Ty::Class(name, args) => {
            deps.push(name.clone());
            for arg in args {
                field_deps(arg, generic_params, deps)?;
            }
            Ok(())
        }
        // An enum-variant type (`Sentiment.Positive`) drops its tag to the
        // enum, so it depends on the same enum item.
        Ty::Enum(name) | Ty::EnumVariant(name, _) => {
            deps.push(name.clone());
            Ok(())
        }
        // `baml.json.json` is `baml_bridge::Json`, with or without its
        // alias item in the pool.
        Ty::TypeAlias(name) if crate::translate_ty::is_canonical_json(name) => Ok(()),
        // Alias references: representable iff the alias item itself is
        // emitted.
        Ty::TypeAlias(name) => {
            deps.push(name.clone());
            Ok(())
        }
        // A TypeVar naming one of the enclosing generic class's own params
        // is representable (it becomes that Rust generic parameter); any
        // other TypeVar is not.
        Ty::TypeVar(var) => {
            if generic_params.contains(&var.as_str()) {
                Ok(())
            } else {
                Err(format!(
                    "unsupported type: type variable `{}` (not a class type parameter)",
                    var.as_str()
                ))
            }
        }
        Ty::Media(kind) => Err(format!("unsupported type: media ({kind})")),
        Ty::Unknown => Err("unsupported type: unknown".to_string()),
        Ty::Function { .. } => Err("unsupported type: function".to_string()),
        Ty::Future(..) => Err("unsupported type: future handle".to_string()),
        Ty::Interface(..) => Err("unsupported type: interface".to_string()),
        Ty::Type => Err("unsupported type: reflect.Type metatype".to_string()),
        Ty::Resource => Err("unsupported type: resource handle".to_string()),
        Ty::PromptAst => Err("unsupported type: prompt AST".to_string()),
        Ty::Never => Err("unsupported type: never".to_string()),
        Ty::RustType => Ok(()),
    }
}

/// Whether the Rust type `translate_ty` produces for `ty` implements
/// serde's traits in the JSON shape BAML's own `baml.json` uses, given the
/// classes and aliases that do (`serde`).
fn has_serde(ty: &Ty, serde: &HashSet<Name>) -> bool {
    match ty {
        Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Null | Ty::TypeVar(_) => true,
        Ty::Enum(_) | Ty::EnumVariant(..) => true,
        // A literal widens to its base primitive.
        Ty::Literal(literal, ..) => !matches!(literal, baml_base::Literal::Bigint(_)),
        // `uint8array` has no JSON form in BAML. A `bigint` is a JSON
        // number of any size there, but `BigInt`'s own serde form is a
        // `[sign, digits]` pair.
        Ty::Bigint | Ty::Uint8Array => false,
        Ty::List(inner) => has_serde(inner, serde),
        Ty::Map { value, .. } => has_serde(value, serde),
        Ty::Union(items) => items.iter().all(|item| has_serde(item, serde)),
        Ty::Class(name, args) => {
            serde.contains(name) && args.iter().all(|arg| has_serde(arg, serde))
        }
        // `baml_bridge::Json` is `serde_json::Value`: its serde form is the
        // JSON value itself.
        Ty::TypeAlias(name) if crate::translate_ty::is_canonical_json(name) => true,
        Ty::TypeAlias(name) => serde.contains(name),
        // Functions, handles and opaque values have no JSON form.
        Ty::Media(_)
        | Ty::Unknown
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::Interface(..)
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::Never
        | Ty::RustType => false,
    }
}

/// Collect every `TypeVar` name used in a *non-recursive* position — one
/// not reached solely as a type argument of a `self_name` self-reference.
/// Rust requires a struct type parameter to appear at least once outside
/// the struct's own recursive occurrences to fix its variance; a param
/// seen only inside `self_name<…>` fails that rule.
fn collect_non_recursive_type_vars<'a>(ty: &'a Ty, self_name: &Name, used: &mut HashSet<&'a str>) {
    match ty {
        Ty::TypeVar(var) => {
            used.insert(var.as_str());
        }
        Ty::List(inner) => collect_non_recursive_type_vars(inner, self_name, used),
        Ty::Map { key, value, .. } => {
            collect_non_recursive_type_vars(key, self_name, used);
            collect_non_recursive_type_vars(value, self_name, used);
        }
        Ty::Union(items) => items
            .iter()
            .for_each(|item| collect_non_recursive_type_vars(item, self_name, used)),
        // A recursive self-reference does not constrain its args' variance,
        // so type vars appearing only there do not count as used. Arguments
        // of a *different* generic class do count (that class's definition
        // pins their variance).
        Ty::Class(name, args) if name != self_name => args
            .iter()
            .for_each(|arg| collect_non_recursive_type_vars(arg, self_name, used)),
        _ => {}
    }
}

/// Collect emitted class and type-alias references reachable without
/// crossing `Vec` / `Map` (heap indirection already breaks the containment
/// cycle there).
fn non_heap_class_refs<'a>(
    ty: &'a Ty,
    emitted: &HashSet<Name>,
    enums: &HashSet<Name>,
    out: &mut Vec<&'a Name>,
) {
    match ty {
        // A class reference is a containment edge whether or not it is
        // generic; a generic instance stores its arguments inline, so those
        // arguments continue the walk (an inline same-SCC argument, e.g.
        // `GenericBox<Self>`, is a cycle too).
        Ty::Class(name, args) => {
            if emitted.contains(name) && !enums.contains(name) {
                out.push(name);
            }
            for arg in args {
                non_heap_class_refs(arg, emitted, enums, out);
            }
        }
        Ty::Union(items) => {
            for item in items {
                non_heap_class_refs(item, emitted, enums, out);
            }
        }
        // Heap-indirected containers end the walk.
        Ty::List(..) | Ty::Map { .. } => {}
        // An alias is a node of its own; its edges are what its target
        // holds inline.
        Ty::TypeAlias(name) if emitted.contains(name) => out.push(name),
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
                .insert(name.name().to_string());
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
