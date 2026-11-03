//! BAML `Ty` → Swift type expression translation.
//!
//! Phase 2 supports primitives, lists/maps/optionals, and references
//! to *supported* user classes, enums, and (non-recursive) type
//! aliases. [`translate_ty`] returns `None` for anything not yet
//! supported, and the emitter skips whole symbols whose signature
//! contains an unsupported type — the generated package must always
//! compile, and capabilities widen phase by phase.

use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

use baml_base::MediaKind;
use baml_sdkgen_types::{ClassCrossing, Name, ParamTy, Symbol, SymbolPool, Ty};

/// Which named types the emitter decided it can emit this run (the
/// fixpoint result from `lib.rs`), plus how to spell their Swift path.
pub(crate) struct TranslateCtx {
    /// FQNs of user classes that will be emitted.
    pub supported_classes: BTreeSet<String>,
    /// FQNs of user enums that will be emitted.
    pub supported_enums: BTreeSet<String>,
    /// FQNs of user type aliases that will be emitted.
    pub supported_aliases: BTreeSet<String>,
    /// Recursive union aliases whose union is null-bearing (stdlib
    /// `json`): the nominal enum holds the non-null arms, so every
    /// reference site spells `Name?`.
    pub nullable_aliases: BTreeSet<String>,
    /// Bodies of the supported non-recursive aliases. Swift spells such an
    /// alias as a `typealias`, and a statement spells it by its body: no
    /// alias but a recursive one exists at run time.
    pub alias_bodies: BTreeMap<String, Ty>,
    /// How each class of the pool crosses the boundary, as the compiler
    /// decided it.
    pub crossings: BTreeMap<String, ClassCrossing>,
    /// The name of each class of the pool, by FQN.
    pub class_names: BTreeMap<String, Name>,
    /// The generated struct name of each tuple type too wide for the
    /// `BamlTupleN` family. Fixed by the pool alone, so it is shared by every
    /// round of the support fixpoint.
    pub tuple_names: Rc<TupleNames>,
}

impl TranslateCtx {
    /// How the class `name` crosses, as the compiler decided it.
    pub(crate) fn crossing(&self, name: &Name) -> Option<ClassCrossing> {
        self.crossings.get(&name.to_string()).copied()
    }

    /// The Swift type of media of kind `kind`: the generated struct of the
    /// class that crosses as that media.
    fn media_wrapper(&self, kind: MediaKind) -> Option<String> {
        let (fqn, _) = self
            .crossings
            .iter()
            .find(|(_, crossing)| **crossing == ClassCrossing::Media(kind))?;
        if !self.supported_classes.contains(fqn) {
            return None;
        }
        self.class_names.get(fqn).map(swift_type_path)
    }

    fn named_ref(name: &Name, pool: &BTreeSet<String>) -> Option<String> {
        let fqn = name.to_string();
        if pool.contains(&fqn) {
            Some(swift_type_path(name))
        } else {
            None
        }
    }
}

/// Swift namespace segments for a symbol, mirroring Python's routing:
/// pkg `user` → the namespace path as-is; pkg `baml` (stdlib) → under
/// `baml`; any other package → under `vendor.<pkg>`.
pub(crate) fn namespace_for(name: &Name) -> Vec<String> {
    let mut ns: Vec<String> = Vec::new();
    match name.package().as_str() {
        "user" => {}
        "baml" => ns.push("baml".to_string()),
        other => {
            ns.push("vendor".to_string());
            ns.push(other.to_string());
        }
    }
    ns.extend(name.namespace().iter().map(|s| s.as_str().to_string()));
    ns
}

/// Fully-qualified Swift spelling of a generated type: the `Baml`
/// namespace-enum tree mirrors the BAML namespace path. Always
/// qualified so cross-namespace references need no imports.
pub(crate) fn swift_type_path(name: &Name) -> String {
    let mut out = String::from("Baml");
    for seg in namespace_for(name) {
        out.push('.');
        out.push_str(&crate::escape_ident(&seg));
    }
    out.push('.');
    out.push_str(&crate::escape_ident(name.name().as_str()));
    out
}

/// Swift spelling of `ty`, or `None` if the type is not yet supported.
pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    match ty {
        Ty::Int => Some("Swift.Int".to_string()),
        Ty::Bigint => Some("BamlBigInt".to_string()),
        // BAML float is f64.
        Ty::Float => Some("Swift.Double".to_string()),
        Ty::String => Some("Swift.String".to_string()),
        Ty::Bool => Some("Swift.Bool".to_string()),
        // Standalone `null` type: Swift has no untyped nil, so it gets
        // a unit-like runtime type that encodes/decodes as BAML null.
        Ty::Null => Some("BamlNull".to_string()),
        Ty::Uint8Array => Some("Foundation.Data".to_string()),
        // Provider-neutral prompts cross as a copied protobuf tree. They are
        // portable values, not generated `$rust_type` handle wrappers.
        Ty::PromptAst => Some("BamlPrompt".to_string()),
        // Media is spelled by the generated struct of the class the
        // compiler says crosses as that media. The generic media shape has
        // no such class, so no Swift type.
        Ty::Media(kind) => ctx.media_wrapper(*kind),
        // A literal type is spelled by its base type: Swift has no literal
        // types. The slot (`slot_expr`) still carries the literal, which is
        // what a statement enclosing it spells.
        Ty::Literal(lit, ..) => Some(
            match lit {
                baml_base::Literal::String(_) => "Swift.String",
                baml_base::Literal::Int(_) => "Swift.Int",
                baml_base::Literal::Bigint(_) => "BamlBigInt",
                baml_base::Literal::Bool(_) => "Swift.Bool",
                // Float literals are unsupported for now.
                baml_base::Literal::Float(_) => return None,
            }
            .to_string(),
        ),
        Ty::List(inner) => Some(format!("[{}]", translate_ty(inner, ctx)?)),
        Ty::Map { key, value, .. } => {
            // BAML map keys are stringified engine-side; only string
            // keys are supported host-side for now (mirrors Python's
            // dict[str, Any] posture on the decode path).
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("[Swift.String: {}]", translate_ty(value, ctx)?))
        }
        Ty::Union(members) => translate_union(members, ctx),
        Ty::Class(name, args) => match ctx.crossing(name)? {
            // The runtime-owned capabilities and the rendered prompt are
            // BamlBridge types, never generated structs.
            ClassCrossing::FunctionSpec => {
                let [final_ty] = &**args else {
                    return None;
                };
                Some(format!(
                    "BamlFunctionSpec<{}>",
                    translate_ty(final_ty, ctx)?
                ))
            }
            ClassCrossing::Stream => {
                let [value] = &**args else {
                    return None;
                };
                Some(format!("BamlStream<{}>", translate_ty(value, ctx)?))
            }
            ClassCrossing::Prompt => args.is_empty().then(|| "BamlPrompt".to_string()),
            ClassCrossing::Fields | ClassCrossing::Opaque | ClassCrossing::Media(_) => {
                let path = TranslateCtx::named_ref(name, &ctx.supported_classes)?;
                if args.is_empty() {
                    return Some(path);
                }
                // Parameterized generic reference: `Wrapper<int>` →
                // `Baml.ns.Wrapper<Swift.Int>`.
                let translated: Vec<String> = args
                    .iter()
                    .map(|a| translate_ty(a, ctx))
                    .collect::<Option<_>>()?;
                Some(format!("{path}<{}>", translated.join(", ")))
            }
        },
        // A generic parameter reference (`T`) — spelled bare; only
        // valid inside the generic declaration that binds it, which is
        // the only place the pool produces it.
        Ty::TypeVar(name) => Some(crate::escape_ident(name.as_str())),
        Ty::Enum(name) => TranslateCtx::named_ref(name, &ctx.supported_enums),
        Ty::TypeAlias(name) => {
            let path = TranslateCtx::named_ref(name, &ctx.supported_aliases)?;
            if ctx.nullable_aliases.contains(&name.to_string()) {
                Some(format!("{path}?"))
            } else {
                Some(path)
            }
        }
        // Opaque engine-owned state (`$rust_type` fields on stdlib
        // resource classes: File._handle, Response._body, media _data).
        Ty::RustType => Some("BamlHandle?".to_string()),
        Ty::Tuple(items) => translate_tuple(items, ctx),
        // Unit is only meaningful in return position; the emitter
        // special-cases it. Everything else lands in later phases.
        _ => None,
    }
}

/// Largest `BamlTupleN` shipped in the `BamlBridge` runtime. A wider tuple
/// is a generated struct (see [`TupleNames`]); bumping the cap is an
/// additive runtime-library change.
pub(crate) const MAX_TUPLE_ARITY: usize = 12;

/// Swift spelling of a tuple. A bare Swift tuple conforms to no protocol, so
/// it cannot fill a position (each crosses through `BamlEncodable`,
/// `BamlDecodable` and `BamlTyped`, and a generated struct's fields through
/// `Equatable`, `Hashable` and `Sendable`): every tuple position is a
/// `BamlTupleN` from the runtime family, which carries the native tuple as
/// its `values`. Like a union, its identity is structural -- one Swift type
/// per tuple type, slots in declaration order with repeats kept.
///
/// A tuple wider than the family takes the first of, in order:
///
/// 1. its generated `TupleN_...` struct ([`TupleNames::declared`]), one per
///    tuple type, whose slot types are its identity;
/// 2. where none can be declared (a slot mentions a type parameter, which a
///    struct declared once per tuple type cannot name), the host's
///    `(T1 | T2 | ...)[]`: an array of the join of its slot types, as
///    [`translate_union`] spells it;
/// 3. where that join has no Swift spelling either (more than
///    [`MAX_UNION_ARITY`] cases), the host's `unknown[]`: an array of the
///    dynamic `BamlOutboundValue`.
///
/// Both arrays keep exactly one element per slot, in order, and state the
/// tuple of the declared slot types, so the engine still checks every slot.
fn translate_tuple(items: &[Ty], ctx: &TranslateCtx) -> Option<String> {
    match items.len() {
        // No BAML syntax spells an empty tuple.
        0 => None,
        n if n <= MAX_TUPLE_ARITY => {
            let slots = items
                .iter()
                .map(|item| translate_ty(item, ctx))
                .collect::<Option<Vec<_>>>()?;
            Some(format!("BamlTuple{n}<{}>", slots.join(", ")))
        }
        _ => {
            if let Some(name) = ctx.tuple_names.declared(items, ctx) {
                return Some(format!("Baml.{name}"));
            }
            // Every stage states each slot, so each must have a spelling.
            if !items.iter().all(|item| translate_ty(item, ctx).is_some()) {
                return None;
            }
            Some(format!(
                "[{}]",
                translate_ty(&tuple_join(items), ctx).unwrap_or_else(|| DYNAMIC_VALUE.to_string())
            ))
        }
    }
}

/// The SDK's dynamic value: the element of the `unknown[]` stage of a tuple.
pub(crate) const DYNAMIC_VALUE: &str = "BamlOutboundValue";

/// The generated struct names of the tuple types too wide for the
/// `BamlTupleN` family: `TupleN_Type1Type2...`, `N` the arity and one
/// [`TupleNames::token`] per slot in slot order, repeats kept. The one helper
/// behind both a struct's declaration and every reference to it.
///
/// A tuple type's identity is its wire identity: its slots with every
/// non-recursive alias resolved (Swift states such an alias by its body), so
/// every spelling of one tuple type names one struct. Two identities whose
/// tokens coincide (`a.Item` and `b.Item`), or one that coincides with a
/// declaration at the root of the `Baml` namespace, are told apart by a
/// numeric suffix, in identity order.
#[derive(Default)]
pub(crate) struct TupleNames {
    /// Bodies of every non-recursive alias of the pool.
    aliases: BTreeMap<String, Ty>,
    /// Struct name by identity key.
    names: BTreeMap<String, String>,
    /// The resolved slots of each named identity, by identity key.
    shapes: BTreeMap<String, Vec<Ty>>,
}

impl TupleNames {
    pub(crate) fn build(pool: &SymbolPool) -> Self {
        let mut this = Self {
            aliases: pool
                .iter()
                .filter_map(|(key, symbol)| match symbol {
                    Symbol::TypeAlias(alias) if !alias.recursive => {
                        Some((key.to_string(), alias.resolves_to.clone()))
                    }
                    _ => None,
                })
                .collect(),
            ..Self::default()
        };
        // Every wide tuple type the pool mentions, by identity.
        let mut found: BTreeMap<String, Vec<Ty>> = BTreeMap::new();
        let mut visit = |ty: &Ty| {
            if let Ty::Tuple(items) = ty
                && items.len() > MAX_TUPLE_ARITY
            {
                let resolved = this.resolve_items(items);
                found.insert(identity_key(&resolved), resolved);
            }
        };
        for symbol in pool.values() {
            for_each_symbol_type(symbol, &mut |ty| {
                baml_sdkgen_types::visit_type(ty, &mut visit);
            });
        }
        // What already occupies the root of the `Baml` namespace.
        let mut taken: BTreeSet<String> = pool
            .keys()
            .map(|key| match namespace_for(key).first() {
                Some(top) => top.clone(),
                None => key.name().as_str().to_string(),
            })
            .collect();
        for (key, items) in &found {
            this.assign(key, items, &found, &mut taken);
        }
        this
    }

    /// Name `key` (and, first, every wide tuple nested in it).
    fn assign(
        &mut self,
        key: &str,
        items: &[Ty],
        found: &BTreeMap<String, Vec<Ty>>,
        taken: &mut BTreeSet<String>,
    ) {
        if self.names.contains_key(key) {
            return;
        }
        for item in items {
            baml_sdkgen_types::visit_type(item, &mut |ty| {
                if let Ty::Tuple(inner) = ty
                    && inner.len() > MAX_TUPLE_ARITY
                {
                    let inner_key = identity_key(inner);
                    if let Some(inner_items) = found.get(&inner_key) {
                        self.assign(&inner_key, inner_items, found, taken);
                    }
                }
            });
        }
        let mut base = format!("Tuple{}_", items.len());
        for item in items {
            match self.token(item) {
                Some(token) => base.push_str(&token),
                // A slot with no token (a type parameter, a callable): the
                // tuple takes the list stages instead.
                None => return,
            }
        }
        let mut name = base.clone();
        let mut suffix = 2;
        while !taken.insert(name.clone()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        self.names.insert(key.to_string(), name);
        self.shapes.insert(key.to_string(), items.to_vec());
    }

    /// `items` with every non-recursive alias resolved, at any depth.
    fn resolve_items(&self, items: &[Ty]) -> Vec<Ty> {
        items.iter().map(|item| self.resolve(item)).collect()
    }

    fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::TypeAlias(name) => match self.aliases.get(&name.to_string()) {
                Some(body) => self.resolve(body),
                None => ty.clone(),
            },
            Ty::List(inner) => Ty::List(Box::new(self.resolve(inner))),
            Ty::Map { key, value } => Ty::Map {
                key: Box::new(self.resolve(key)),
                value: Box::new(self.resolve(value)),
            },
            Ty::Union(members) => Ty::Union(members.iter().map(|m| self.resolve(m)).collect()),
            Ty::Tuple(items) => Ty::Tuple(self.resolve_items(items).into_boxed_slice()),
            Ty::Class(name, args) => Ty::Class(
                name.clone(),
                args.iter().map(|arg| self.resolve(arg)).collect(),
            ),
            other => other.clone(),
        }
    }

    /// The struct name of the wide tuple of `items`, if its slots all have a
    /// token.
    pub(crate) fn name(&self, items: &[Ty]) -> Option<&str> {
        self.names
            .get(&identity_key(&self.resolve_items(items)))
            .map(String::as_str)
    }

    /// The struct name of the wide tuple of `items` where its struct is
    /// declared under `ctx`: it has a name and every slot a Swift type.
    pub(crate) fn declared(&self, items: &[Ty], ctx: &TranslateCtx) -> Option<&str> {
        let name = self.name(items)?;
        items
            .iter()
            .all(|item| translate_ty(item, ctx).is_some())
            .then_some(name)
    }

    /// Every named tuple type: its struct name and resolved slots, in name
    /// order.
    pub(crate) fn named(&self) -> Vec<(&str, &[Ty])> {
        let mut out: Vec<(&str, &[Ty])> = self
            .names
            .iter()
            .map(|(key, name)| (name.as_str(), self.shapes[key].as_slice()))
            .collect();
        out.sort_by_key(|(name, _)| *name);
        out
    }

    /// The `PascalCase` token one (resolved) slot type contributes to a
    /// struct name. `None` for a slot no struct declared once per tuple type
    /// can hold by name: a type parameter, a callable, an open type.
    fn token(&self, ty: &Ty) -> Option<String> {
        Some(match ty {
            Ty::Int => "Int".to_string(),
            Ty::Bigint => "BigInt".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::String => "String".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Null => "Null".to_string(),
            Ty::Uint8Array => "Bytes".to_string(),
            Ty::PromptAst => "Prompt".to_string(),
            Ty::RustType => "Handle".to_string(),
            Ty::Media(kind) => format!("{kind:?}"),
            Ty::Literal(literal, ..) => match literal {
                baml_base::Literal::String(value) => format!("K{}", pascal_words(value)),
                baml_base::Literal::Int(value) => {
                    format!("IntK{}", value.to_string().replace('-', "Neg"))
                }
                baml_base::Literal::Bigint(value) => {
                    format!("BigIntK{}", value.to_string().replace('-', "Neg"))
                }
                baml_base::Literal::Bool(true) => "BoolKTrue".to_string(),
                baml_base::Literal::Bool(false) => "BoolKFalse".to_string(),
                baml_base::Literal::Float(_) => return None,
            },
            Ty::Class(name, args) => {
                let mut token = pascal_words(name.name().as_str());
                for arg in args {
                    token.push_str(&self.token(arg)?);
                }
                token
            }
            Ty::Enum(name) | Ty::TypeAlias(name) => pascal_words(name.name().as_str()),
            Ty::List(inner) => format!("{}List", self.token(inner)?),
            Ty::Map { value, .. } => format!("{}Map", self.token(value)?),
            Ty::Union(members) => {
                let (non_null, nullable) = normalize_union(members);
                let inner = if non_null.is_empty() {
                    "Null".to_string()
                } else {
                    non_null
                        .iter()
                        .map(|member| self.token(member))
                        .collect::<Option<Vec<_>>>()?
                        .join("Or")
                };
                if nullable && !non_null.is_empty() {
                    format!("Optional{inner}")
                } else {
                    inner
                }
            }
            Ty::Tuple(items) if items.len() > MAX_TUPLE_ARITY => {
                self.names.get(&identity_key(items))?.clone()
            }
            Ty::Tuple(items) => {
                let mut token = String::new();
                for item in items {
                    token.push_str(&self.token(item)?);
                }
                token.push_str("Tuple");
                token
            }
            _ => return None,
        })
    }
}

/// The key of a tuple identity: its resolved slots, structurally.
fn identity_key(items: &[Ty]) -> String {
    format!("{items:?}")
}

/// Every type a symbol's declaration spells.
fn for_each_symbol_type(symbol: &Symbol, f: &mut impl FnMut(&Ty)) {
    fn function(function: &baml_sdkgen_types::Function, f: &mut impl FnMut(&Ty)) {
        for argument in &function.arguments {
            f(&argument.ty);
        }
        f(&function.return_type);
        if let Some(throws) = &function.throws {
            f(throws);
        }
    }
    match symbol {
        Symbol::Function(fun) => function(fun, f),
        Symbol::Class(class) => {
            for property in &class.properties {
                f(&property.ty);
            }
            for method in class.static_methods.iter().chain(&class.instance_methods) {
                function(method, f);
            }
        }
        Symbol::Enum(_) => {}
        Symbol::TypeAlias(alias) => f(&alias.resolves_to),
    }
}

/// `text` as one identifier-safe `PascalCase` run: each run of ASCII
/// alphanumerics starts upper-case, everything else is dropped.
fn pascal_words(text: &str) -> String {
    let mut out = String::new();
    let mut boundary = true;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if boundary {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
            boundary = false;
        } else {
            boundary = true;
        }
    }
    out
}

/// The join of a tuple's slot types, `T1 | T2 | ...`: the element type of
/// the list fallback. A union slot contributes its members.
pub(crate) fn tuple_join(items: &[Ty]) -> Ty {
    let mut members: Vec<Ty> = Vec::new();
    for item in items {
        match item {
            Ty::Union(inner) => members.extend(inner.iter().cloned()),
            other => members.push(other.clone()),
        }
    }
    Ty::Union(members.into_boxed_slice())
}

/// Largest `BamlUnionN` shipped in the `BamlBridge` runtime. Wider
/// unions are unsupported (symbol skipped, soft) until the family is
/// extended — an additive runtime-library change.
pub(crate) const MAX_UNION_ARITY: usize = 8;

/// Normalize a union: drop nulls (remembering they were there), dedup
/// structurally equal members, preserve declaration order — the
/// canonical arm order every bridge shares.
pub(crate) fn normalize_union(members: &[Ty]) -> (Vec<Ty>, bool) {
    let mut nullable = false;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut non_null: Vec<Ty> = Vec::new();
    for member in members {
        if matches!(member, Ty::Null) {
            nullable = true;
            continue;
        }
        if seen.insert(format!("{member:?}")) {
            non_null.push(member.clone());
        }
    }
    (non_null, nullable)
}

/// One Swift case of a union and the BAML members it stands for.
pub(crate) struct UnionArm {
    pub swift: String,
    pub members: Vec<Ty>,
}

/// Group a union's non-null members into Swift cases, in declaration order.
/// Members share a case only when they are leaves of one Swift type
/// (`"draft" | "sent"` is one `String` case): a leaf states nothing, so the
/// merged case loses nothing a statement needs. Members of one Swift type
/// that are not leaves (`"a"[] | "b"[]`) keep a case each, because a list
/// states its own exact type and only the case says which one.
pub(crate) fn union_arms(non_null: &[Ty], ctx: &TranslateCtx) -> Option<Vec<UnionArm>> {
    let mut arms: Vec<UnionArm> = Vec::new();
    for member in non_null {
        let swift = translate_ty(member, ctx)?;
        let leaf = is_leaf(member, ctx);
        match arms
            .iter_mut()
            .find(|arm| arm.swift == swift && leaf && arm.members.iter().all(|m| is_leaf(m, ctx)))
        {
            Some(arm) => arm.members.push(member.clone()),
            None => arms.push(UnionArm {
                swift,
                members: vec![member.clone()],
            }),
        }
    }
    Some(arms)
}

/// Does a value of `ty` encode as a node that implies its own type, so its
/// slot contributes nothing but that type?
fn is_leaf(ty: &Ty, ctx: &TranslateCtx) -> bool {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::Literal(..)
        | Ty::Enum(_)
        | Ty::PromptAst => true,
        // A node that implies its type (media, a prompt) or a handle, which
        // states nothing.
        Ty::Class(name, _) => match ctx.crossing(name) {
            Some(
                ClassCrossing::Opaque
                | ClassCrossing::Stream
                | ClassCrossing::FunctionSpec
                | ClassCrossing::Media(_)
                | ClassCrossing::Prompt,
            ) => true,
            Some(ClassCrossing::Fields) | None => false,
        },
        Ty::TypeAlias(name) => ctx
            .alias_bodies
            .get(&name.to_string())
            .is_some_and(|body| is_leaf(body, ctx)),
        _ => false,
    }
}

/// Swift spelling of a union: `BamlUnionN<...>` from the runtime
/// family — never a generated named type. Null members strip into a
/// trailing `?`; a single surviving arm collapses to that member.
fn translate_union(members: &[Ty], ctx: &TranslateCtx) -> Option<String> {
    let (non_null, nullable) = normalize_union(members);
    let suffix = if nullable { "?" } else { "" };
    let arms = translate_union_arms(&non_null, ctx)?;
    match arms.len() {
        0 => Some("BamlNull".to_string()),
        1 => Some(format!("{}{suffix}", arms[0])),
        n if n <= MAX_UNION_ARITY => Some(format!("BamlUnion{n}<{}>{suffix}", arms.join(", "))),
        _ => None,
    }
}

/// The Swift spelling of each case of [`union_arms`].
pub(crate) fn translate_union_arms(non_null: &[Ty], ctx: &TranslateCtx) -> Option<Vec<String>> {
    Some(
        union_arms(non_null, ctx)?
            .into_iter()
            .map(|arm| arm.swift)
            .collect(),
    )
}

/// For an optional-argument slot: the `T` in `BamlOptional<T>`. A
/// nullable declared type contributes its non-null part (the `.null`
/// case covers the rest); a non-nullable defaulted type is used as-is.
pub(crate) fn translate_optional_arg_inner(ty: &Ty, ctx: &TranslateCtx) -> Option<String> {
    if let Ty::Union(members) = ty {
        let (non_null, nullable) = normalize_union(members);
        if nullable {
            let arms = translate_union_arms(&non_null, ctx)?;
            return match arms.len() {
                0 => Some("BamlNull".to_string()),
                1 => Some(arms[0].clone()),
                n if n <= MAX_UNION_ARITY => Some(format!("BamlUnion{n}<{}>", arms.join(", "))),
                _ => None,
            };
        }
    }
    translate_ty(ty, ctx)
}

/// How a type variable's slot is spelled where the slot is emitted: a
/// function's `T` is the Swift type's own slot, a class's `T` the type
/// argument its instance was stated with.
pub(crate) type TypeVarSlot<'a> = &'a dyn Fn(&ParamTy) -> String;

/// A function or method: each `T` states its Swift type's own type.
pub(crate) fn generic_param_slot(param: &ParamTy) -> String {
    own_slot(&crate::escape_ident(param.as_str()))
}

/// The own slot of the Swift type `swift`. Spelled with the type in a
/// generic argument, a type position: a type named `Type` cannot be followed
/// by a member in an expression.
pub(crate) fn own_slot(swift: &str) -> String {
    format!("BamlSlot<{swift}>.own")
}

/// The Swift expression of the [`BamlSlot`] for a value of declared type
/// `ty`: a `BamlSlot<V>` where `V` is exactly `translate_ty(ty)`, carrying the
/// exact BAML type a statement spells. `None` exactly where `translate_ty`
/// is `None`.
pub(crate) fn slot_expr(ty: &Ty, ctx: &TranslateCtx, vars: TypeVarSlot<'_>) -> Option<String> {
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::PromptAst
        | Ty::Media(_)
        | Ty::Enum(_) => Some(own_slot(&translate_ty(ty, ctx)?)),
        Ty::Literal(lit, ..) => Some(match lit {
            // Rust's `Debug` escapes are valid Swift string escapes.
            baml_base::Literal::String(value) => {
                format!("BamlSlot<Swift.String>.literal({value:?})")
            }
            baml_base::Literal::Int(value) => format!("BamlSlot<Swift.Int>.literal({value})"),
            baml_base::Literal::Bool(value) => format!("BamlSlot<Swift.Bool>.literal({value})"),
            baml_base::Literal::Bigint(value) => {
                format!("BamlSlot<BamlBigInt>.literal(bigint: \"{value}\")")
            }
            baml_base::Literal::Float(_) => return None,
        }),
        Ty::List(inner) => Some(format!("BamlSlots.list({})", slot_expr(inner, ctx, vars)?)),
        Ty::Map { key, value, .. } => {
            if !matches!(**key, Ty::String) {
                return None;
            }
            Some(format!("BamlSlots.map({})", slot_expr(value, ctx, vars)?))
        }
        Ty::Union(members) => {
            let (non_null, nullable) = normalize_union(members);
            let inner = union_slot(&non_null, ctx, vars)?;
            Some(if nullable && !non_null.is_empty() {
                format!("BamlSlots.optional({inner})")
            } else {
                inner
            })
        }
        Ty::Class(name, args) => {
            let fqn = name.to_string();
            let swift = translate_ty(ty, ctx)?;
            // Runtime-owned handle wrappers: a handle states nothing, so the
            // slot carries only the type an enclosing statement spells.
            if matches!(
                ctx.crossing(name),
                Some(ClassCrossing::FunctionSpec | ClassCrossing::Stream)
            ) {
                let arguments = args
                    .iter()
                    .map(|arg| Some(format!("({}).type", slot_expr(arg, ctx, vars)?)))
                    .collect::<Option<Vec<_>>>()?;
                return Some(format!(
                    "BamlSlot<{swift}>._leaf(.classType(\"{fqn}\", typeArguments: [{}]))",
                    arguments.join(", ")
                ));
            }
            if args.is_empty() {
                return Some(own_slot(&swift));
            }
            let arguments = args
                .iter()
                .map(|arg| slot_expr(arg, ctx, vars))
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "BamlSlot<{swift}>._classType(\"{fqn}\", {})",
                arguments.join(", ")
            ))
        }
        Ty::TypeVar(param) => Some(vars(param)),
        Ty::TypeAlias(name) => {
            let fqn = name.to_string();
            if let Some(body) = ctx.alias_bodies.get(&fqn) {
                return slot_expr(body, ctx, vars);
            }
            let path = TranslateCtx::named_ref(name, &ctx.supported_aliases)?;
            Some(if ctx.nullable_aliases.contains(&fqn) {
                format!("BamlSlots.nullable({})", own_slot(&path))
            } else {
                own_slot(&path)
            })
        }
        Ty::RustType => {
            Some("BamlSlots.optional(BamlSlot<BamlHandle>._leaf(.rustType))".to_string())
        }
        Ty::Tuple(items) => match items.len() {
            0 => None,
            n if n <= MAX_TUPLE_ARITY => {
                let slots = items
                    .iter()
                    .map(|item| slot_expr(item, ctx, vars))
                    .collect::<Option<Vec<_>>>()?;
                Some(format!("BamlSlots.tuple({})", slots.join(", ")))
            }
            _ => {
                // A generated tuple struct is its tuple type: its own slot.
                if let Some(name) = ctx.tuple_names.declared(items, ctx) {
                    return Some(own_slot(&format!("Baml.{name}")));
                }
                // The list stages, as `translate_tuple` picks them: an array
                // of the join, else of the dynamic value, stating the tuple of
                // the declared slot types so the engine still checks each slot.
                let slots = items
                    .iter()
                    .map(|item| Some(format!("({}).type", slot_expr(item, ctx, vars)?)))
                    .collect::<Option<Vec<_>>>()?;
                let element = slot_expr(&tuple_join(items), ctx, vars)
                    .unwrap_or_else(|| own_slot(DYNAMIC_VALUE));
                Some(format!(
                    "BamlSlots.tupleList({element}, slots: {})",
                    slots.join(", ")
                ))
            }
        },
        _ => None,
    }
}

/// The slot of a union with nulls already stripped, matching
/// [`translate_union`]'s spelling of the non-null part.
fn union_slot(non_null: &[Ty], ctx: &TranslateCtx, vars: TypeVarSlot<'_>) -> Option<String> {
    let arms = union_arms(non_null, ctx)?
        .iter()
        .map(|arm| arm_slot(arm, ctx, vars))
        .collect::<Option<Vec<_>>>()?;
    match arms.len() {
        0 => Some(own_slot("BamlNull")),
        1 => arms.into_iter().next(),
        n if n <= MAX_UNION_ARITY => Some(format!("BamlSlots.union({})", arms.join(", "))),
        _ => None,
    }
}

/// The slot of one union case: its member's, or the union of its merged
/// leaf members'.
pub(crate) fn arm_slot(
    arm: &UnionArm,
    ctx: &TranslateCtx,
    vars: TypeVarSlot<'_>,
) -> Option<String> {
    match arm.members.as_slice() {
        [member] => slot_expr(member, ctx, vars),
        members => {
            let slots = members
                .iter()
                .map(|member| slot_expr(member, ctx, vars))
                .collect::<Option<Vec<_>>>()?;
            Some(format!(
                "BamlSlot<{}>.oneOf({})",
                arm.swift,
                slots.join(", ")
            ))
        }
    }
}

/// The slot of an optional argument's value, matching
/// [`translate_optional_arg_inner`]: the declared type without its null.
pub(crate) fn optional_arg_inner_slot(
    ty: &Ty,
    ctx: &TranslateCtx,
    vars: TypeVarSlot<'_>,
) -> Option<String> {
    if let Ty::Union(members) = ty {
        let (non_null, nullable) = normalize_union(members);
        if nullable {
            return union_slot(&non_null, ctx, vars);
        }
    }
    slot_expr(ty, ctx, vars)
}
