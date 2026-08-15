//! Typed naming system for the C++ emitter, following the Go generator's
//! reference architecture:
//!
//! - BAML identity stays structured ([`BamlFqn`]), never a preformatted
//!   string: segment boundaries remain available for hashing and scoping.
//! - The canonical C++ identifier and the wire name live together on
//!   [`CppName`] and neither is reconstructed from the other: renaming the
//!   C++ identifier cannot change a runtime argument key.
//! - Collisions resolve within lexical scopes ([`NameScope`]) and receive
//!   deterministic typed-hash suffixes (4 chars, base36) — never
//!   allocation-order numbering.
//! - Generator-owned identifiers ([`GeneratorIdent`]) participate in
//!   reservation, so a BAML parameter named `args` or `w` cannot shadow the
//!   generated body locals.
//! - Emitters consume allocated names; casing, keyword escaping,
//!   qualification, and collision handling all live here.
//!
//! Rendering policy (decision 2a): namespace-scoped identifiers always
//! render fully qualified (`::baml_sdk::ns::X`), which is immune to
//! shadowing and ADL; owner-scoped identifiers (parameters, fields, enum
//! variants) render bare.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt,
};

use baml_codegen_types::Name;

/// The source namespace path a symbol routes to, pkg-aware: `user` symbols
/// live at the generated root, `baml` under `baml/`, any other package under
/// `vendor/<pkg>/` (mirroring the Python generator's routing rules).
pub(crate) fn source_ns(symbol: &Name) -> Vec<Box<str>> {
    let mut out: Vec<Box<str>> = Vec::new();
    match symbol.package().as_str() {
        "user" => {}
        "baml" => out.push(Box::from("baml")),
        vendor => {
            out.push(Box::from("vendor"));
            out.push(Box::from(vendor));
        }
    }
    out.extend(
        symbol
            .namespace()
            .iter()
            .map(|seg| Box::<str>::from(seg.as_str())),
    );
    out
}

// ---------------------------------------------------------------------------
// Typed BAML identity
// ---------------------------------------------------------------------------

/// A BAML identity: the owning symbol plus member path (parameter, field,
/// variant, or synthesized member such as an opts struct).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct BamlFqn {
    pub(crate) symbol: Name,
    pub(crate) members: Vec<Box<str>>,
}

impl BamlFqn {
    pub(crate) fn symbol(symbol: &Name) -> Self {
        Self {
            symbol: symbol.clone(),
            members: Vec::new(),
        }
    }

    pub(crate) fn member(symbol: &Name, member: &str) -> Self {
        Self {
            symbol: symbol.clone(),
            members: vec![member.into()],
        }
    }

    pub(crate) fn child(&self, member: &str) -> Self {
        let mut members = self.members.clone();
        members.push(member.into());
        Self {
            symbol: self.symbol.clone(),
            members,
        }
    }

    /// The scope this identity's *own* name competes in: the parent
    /// namespace for top-level symbols, the owning identity otherwise.
    fn scope(&self) -> NameScope {
        if self.members.is_empty() {
            NameScope::Namespace(source_ns(&self.symbol))
        } else {
            let mut parent = self.clone();
            parent.members.pop();
            NameScope::Owner(parent)
        }
    }

    /// The source token this identity prefers as its C++ name.
    fn source_token(&self) -> &str {
        self.members
            .last()
            .map(|m| &**m)
            .unwrap_or_else(|| self.symbol.bare_name())
    }
}

impl Ord for BamlFqn {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.symbol
            .cmp(&other.symbol)
            .then_with(|| self.members.cmp(&other.members))
    }
}

impl PartialOrd for BamlFqn {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// Name requests
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CppNameKind {
    Namespace,
    Function,
    /// A static or instance method (class-scoped callable). Wire identity
    /// is the class symbol; the emitter appends `.member` for dispatch.
    Method,
    Class,
    Enum,
    EnumVariant,
    TypeAlias,
    Field,
    Parameter,
    /// Synthesized per-callable opts struct (no wire identity).
    OptsStruct,
    /// Synthesized opts-struct setter (no wire identity).
    Setter,
}

impl CppNameKind {
    /// Owner-scoped kinds render bare; everything else renders fully
    /// qualified (decision 2a).
    fn renders_bare(self) -> bool {
        matches!(
            self,
            CppNameKind::EnumVariant
                | CppNameKind::Method
                | CppNameKind::Field
                | CppNameKind::Parameter
                | CppNameKind::Setter
        )
    }

    fn tag(self) -> u8 {
        match self {
            CppNameKind::Namespace => 0,
            CppNameKind::Function => 1,
            CppNameKind::Class => 2,
            CppNameKind::Enum => 3,
            CppNameKind::EnumVariant => 4,
            CppNameKind::TypeAlias => 5,
            CppNameKind::Field => 6,
            CppNameKind::Parameter => 7,
            CppNameKind::OptsStruct => 8,
            CppNameKind::Setter => 9,
            // New kinds append (typed-hash suffixes embed the tag; existing
            // numbers must never move).
            CppNameKind::Method => 10,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct NameRequest {
    pub(crate) fqn: BamlFqn,
    pub(crate) kind: CppNameKind,
    /// Preferred token override for synthesized names (opts structs,
    /// setters); `None` uses the identity's source token.
    pub(crate) preferred: Option<Box<str>>,
}

impl NameRequest {
    pub(crate) fn new(fqn: BamlFqn, kind: CppNameKind) -> Self {
        Self {
            fqn,
            kind,
            preferred: None,
        }
    }

    pub(crate) fn synthesized(fqn: BamlFqn, kind: CppNameKind, preferred: &str) -> Self {
        Self {
            fqn,
            kind,
            preferred: Some(preferred.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Canonical and wire identities
// ---------------------------------------------------------------------------

/// The wire identity a name encodes as at the FFI boundary. Stored, never
/// derived from the canonical C++ identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BamlWireName {
    /// A symbol identity: renders as the full BAML FQN (`user.ns.fn`).
    Symbol(Name),
    /// A member key (parameter name, field name, enum variant value).
    Key(Box<str>),
}

impl fmt::Display for BamlWireName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BamlWireName::Symbol(name) => write!(f, "{name}"),
            BamlWireName::Key(key) => f.write_str(key),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CppIdent {
    /// Allocated namespace path under `baml_sdk` (already collision-free).
    pub(crate) ns: Vec<Box<str>>,
    pub(crate) name: Box<str>,
}

/// An allocated name: the canonical C++ identifier plus the wire identity.
/// There is deliberately no `as_str()`; render sites choose
/// [`CppName::identifier`] (C++ source position) or [`CppName::wire`]
/// (FFI boundary), and declaration sites use [`CppName::declared`].
#[derive(Clone, Debug)]
pub(crate) struct CppName {
    canonical: CppIdent,
    wire: Option<BamlWireName>,
    kind: CppNameKind,
}

impl CppName {
    /// The identifier as spelled at a C++ *use* site: fully qualified for
    /// namespace-scoped kinds, bare for owner-scoped kinds.
    pub(crate) fn identifier(&self) -> CppIdentifier<'_> {
        CppIdentifier(self)
    }

    /// The bare token for a *declaration* position (`struct <here> {`,
    /// field/parameter declarations, enumerators).
    pub(crate) fn declared(&self) -> &str {
        &self.canonical.name
    }

    /// The wire identity for the FFI boundary. Panics on synthesized names,
    /// which must never reach the wire.
    pub(crate) fn wire(&self) -> &BamlWireName {
        self.wire
            .as_ref()
            .expect("synthesized name has no wire identity")
    }
}

pub(crate) struct CppIdentifier<'a>(&'a CppName);

impl fmt::Display for CppIdentifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.kind.renders_bare() {
            return f.write_str(&self.0.canonical.name);
        }
        f.write_str("::baml_sdk")?;
        for seg in &self.0.canonical.ns {
            write!(f, "::{seg}")?;
        }
        write!(f, "::{}", self.0.canonical.name)
    }
}

// ---------------------------------------------------------------------------
// Lexical scopes and reservations
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum NameScope {
    /// Top-level declarations sharing one namespace (source-segment path).
    Namespace(Vec<Box<str>>),
    /// Members sharing one owner (parameters of a callable, fields and
    /// methods of a class, enumerators of an enum, ...).
    Owner(BamlFqn),
}

/// Generator-owned identifiers. Centralized so the reservation sets cannot
/// drift from the tokens the render code actually emits.
#[derive(Clone, Copy, Debug)]
pub(crate) enum GeneratorIdent {
    ArgsLocal,
    WriterParam,
    SetterValueParam,
    OptsParam,
    EnsureRuntime,
    DetailNamespace,
}

impl GeneratorIdent {
    pub(crate) fn token(self) -> &'static str {
        match self {
            GeneratorIdent::ArgsLocal => "args",
            GeneratorIdent::WriterParam => "w",
            GeneratorIdent::SetterValueParam => "v",
            GeneratorIdent::OptsParam => "opts",
            GeneratorIdent::EnsureRuntime => "ensure_runtime",
            GeneratorIdent::DetailNamespace => "detail",
        }
    }
}

/// Tokens reserved in *callable* scopes (parameters share the body with the
/// generated locals).
const CALLABLE_RESERVED: &[GeneratorIdent] = &[
    GeneratorIdent::ArgsLocal,
    GeneratorIdent::WriterParam,
    GeneratorIdent::SetterValueParam,
    GeneratorIdent::OptsParam,
];

/// Tokens reserved in *namespace* scopes.
const NAMESPACE_RESERVED: &[GeneratorIdent] = &[
    GeneratorIdent::EnsureRuntime,
    GeneratorIdent::DetailNamespace,
];

fn reserved_in(scope_kind: CppNameKind, token: &str) -> bool {
    let set: &[GeneratorIdent] = match scope_kind {
        CppNameKind::Parameter => CALLABLE_RESERVED,
        CppNameKind::Namespace
        | CppNameKind::Function
        | CppNameKind::Class
        | CppNameKind::Enum
        | CppNameKind::TypeAlias
        | CppNameKind::OptsStruct => NAMESPACE_RESERVED,
        _ => &[],
    };
    set.iter().any(|ident| ident.token() == token)
}

// ---------------------------------------------------------------------------
// Projection (clean-name normalization)
// ---------------------------------------------------------------------------

const CPP_KEYWORDS: &[&str] = &[
    "alignas",
    "alignof",
    // Alternative operator tokens ([lex.key] — `or` et al. are full keywords;
    // a method named `or` fails to parse, which `baml.env.Ref.or` proved).
    "and",
    "and_eq",
    "bitand",
    "bitor",
    "compl",
    "not",
    "not_eq",
    "or",
    "or_eq",
    "xor",
    "xor_eq",
    "asm",
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "concept",
    "const",
    "constexpr",
    "continue",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "nullptr",
    "operator",
    "private",
    "protected",
    "public",
    "register",
    "requires",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];

/// Projects a BAML token to its preferred C++ identifier: keywords take a
/// trailing underscore, characters C++ cannot spell become underscores.
/// Collision handling happens later in allocation, not here.
pub(crate) fn project(token: &str) -> String {
    let mut out: String = token
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if CPP_KEYWORDS.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

// ---------------------------------------------------------------------------
// Stable hashing (collision suffixes)
// ---------------------------------------------------------------------------

/// Plain FNV-1a-64 of a string. Enum enumerator values derive from this
/// (hash of the variant's wire value), so they are stable across variant
/// reordering; the framed [`ComponentHash`] below shares the constants.
pub(crate) fn fnv1a64(s: &str) -> u64 {
    let mut hash = ComponentHash::new();
    hash.bytes(s.as_bytes());
    hash.0
}

/// FNV-1a with explicit component framing: lengths and tags are hashed so
/// distinct typed requests cannot collapse onto one digest the way a joined
/// display string could.
struct ComponentHash(u64);

impl ComponentHash {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn component(&mut self, s: &str) {
        self.usize(s.len());
        self.bytes(s.as_bytes());
    }

    fn usize(&mut self, v: usize) {
        self.bytes(&(v as u64).to_le_bytes());
    }

    fn byte(&mut self, b: u8) {
        self.bytes(&[b]);
    }
}

/// 4-character base36 suffix from the request's typed identity (decision 1a).
fn collision_suffix(request: &NameRequest) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut hash = ComponentHash::new();
    hash.component(request.fqn.symbol.package().as_str());
    hash.usize(request.fqn.symbol.namespace().len());
    for segment in request.fqn.symbol.namespace() {
        hash.component(segment.as_str());
    }
    hash.component(request.fqn.symbol.name().as_str());
    hash.usize(request.fqn.members.len());
    for member in &request.fqn.members {
        hash.component(member);
    }
    hash.byte(request.kind.tag());

    let mut value = hash.0;
    let mut out = String::with_capacity(4);
    for _ in 0..4 {
        let digit = usize::try_from(value % 36).expect("a mod-36 digit fits in usize");
        out.push(char::from(ALPHABET[digit]));
        value /= 36;
    }
    out
}

// ---------------------------------------------------------------------------
// Allocation
// ---------------------------------------------------------------------------

/// The allocation table: every emitted identifier, keyed by its typed
/// request. Built once per generation from the full request set.
pub(crate) struct CppNames {
    allocations: HashMap<NameRequest, CppName>,
    /// Source namespace path -> allocated C++ namespace path.
    ns_paths: HashMap<Vec<Box<str>>, Vec<Box<str>>>,
    /// Source namespace path -> allocated child namespace segment names.
    /// Namespaces allocate first; phase 2 consults this so a symbol cannot
    /// take the name of a sibling namespace (a C++ redefinition error).
    ns_children: HashMap<Vec<Box<str>>, BTreeSet<Box<str>>>,
}

impl CppNames {
    pub(crate) fn allocate(requests: &BTreeSet<NameRequest>) -> Self {
        let mut names = CppNames {
            allocations: HashMap::new(),
            ns_paths: HashMap::new(),
            ns_children: HashMap::new(),
        };
        names.ns_paths.insert(Vec::new(), Vec::new());

        // Phase 1: namespace segments (every other allocation renders its
        // qualification through these).
        let namespace_requests: BTreeSet<&NameRequest> = requests
            .iter()
            .filter(|r| r.kind == CppNameKind::Namespace)
            .collect();
        for group in group_by_scope_and_preferred(namespace_requests.iter().copied()) {
            names.allocate_group(group);
        }

        // Phase 2: everything else.
        let other_requests: BTreeSet<&NameRequest> = requests
            .iter()
            .filter(|r| r.kind != CppNameKind::Namespace)
            .collect();
        for group in group_by_scope_and_preferred(other_requests.iter().copied()) {
            names.allocate_group(group);
        }
        names
    }

    pub(crate) fn get(&self, request: &NameRequest) -> &CppName {
        self.allocations.get(request).unwrap_or_else(|| {
            panic!("name not allocated (emitter bug): {request:?}");
        })
    }

    /// The allocated C++ namespace path for a source namespace path.
    pub(crate) fn ns_path(&self, source: &[Box<str>]) -> &[Box<str>] {
        self.ns_paths
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or_else(|| {
                panic!("namespace path not allocated (emitter bug): {source:?}");
            })
    }

    fn allocate_group(&mut self, group: Vec<&NameRequest>) {
        let scope_kind = group[0].kind;
        let preferred = preferred_token(group[0]);
        let needs_suffix = group.len() > 1
            || reserved_in(scope_kind, &preferred)
            || self.collides_with_namespace(group[0], &preferred);
        for request in group {
            let name = if needs_suffix {
                format!("{preferred}_{}", collision_suffix(request))
            } else {
                preferred.clone()
            };
            self.insert(request, name.into());
        }
    }

    /// `true` when a non-namespace symbol's preferred name matches an
    /// allocated sibling namespace segment (C++ rejects a namespace and any
    /// other entity sharing a name in one scope).
    fn collides_with_namespace(&self, request: &NameRequest, preferred: &str) -> bool {
        if request.kind == CppNameKind::Namespace {
            return false;
        }
        match request.fqn.scope() {
            NameScope::Namespace(path) => self
                .ns_children
                .get(&path)
                .is_some_and(|children| children.contains(preferred)),
            NameScope::Owner(_) => false,
        }
    }

    fn insert(&mut self, request: &NameRequest, name: Box<str>) {
        let source_ns: Vec<Box<str>> = source_ns(&request.fqn.symbol);
        if request.kind == CppNameKind::Namespace {
            // Record the allocated path for this source path + segment.
            let mut source_path = source_ns.clone();
            source_path.push(Box::from(request.fqn.symbol.name().as_str()));
            let mut allocated = self.ns_paths.get(&source_ns).cloned().unwrap_or_default();
            allocated.push(name.clone());
            self.ns_paths.insert(source_path, allocated);
            self.ns_children
                .entry(source_ns.clone())
                .or_default()
                .insert(name.clone());
        }
        let ns = self
            .ns_paths
            .get(&source_ns)
            .cloned()
            .unwrap_or_else(|| source_ns.iter().map(|s| Box::from(project(s))).collect());
        let wire = match request.kind {
            CppNameKind::OptsStruct | CppNameKind::Setter => None,
            CppNameKind::Parameter | CppNameKind::Field | CppNameKind::EnumVariant => request
                .fqn
                .members
                .last()
                .map(|m| BamlWireName::Key(m.clone())),
            _ => Some(BamlWireName::Symbol(request.fqn.symbol.clone())),
        };
        self.allocations.insert(
            request.clone(),
            CppName {
                canonical: CppIdent { ns, name },
                wire,
                kind: request.kind,
            },
        );
    }
}

fn preferred_token(request: &NameRequest) -> String {
    match &request.preferred {
        Some(preferred) => project(preferred),
        None => project(request.fqn.source_token()),
    }
}

/// Deterministic grouping: (scope, preferred identifier) -> requests, via
/// ordered maps so allocation is independent of hash iteration.
fn group_by_scope_and_preferred<'a>(
    requests: impl Iterator<Item = &'a NameRequest>,
) -> Vec<Vec<&'a NameRequest>> {
    let mut grouped: BTreeMap<(NameScope, String), Vec<&'a NameRequest>> = BTreeMap::new();
    for request in requests {
        grouped
            .entry((request.fqn.scope(), preferred_token(request)))
            .or_default()
            .push(request);
    }
    grouped.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(ns: &[&str], name: &str) -> Name {
        Name::new(
            baml_base::Name::from("user"),
            ns.iter().map(|s| baml_base::Name::from(*s)).collect(),
            baml_base::Name::from(name),
        )
    }

    #[test]
    fn test_keyword_projection() {
        assert_eq!(project("void"), "void_");
        assert_eq!(project("value"), "value");
        assert_eq!(project("9lives"), "_9lives");
    }

    #[test]
    fn test_fnv1a64_known_vectors() {
        // Published FNV-1a 64-bit test vectors.
        assert_eq!(fnv1a64(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn test_single_request_gets_clean_name() {
        let req = NameRequest::new(BamlFqn::symbol(&name(&["a"], "Foo")), CppNameKind::Class);
        let mut set = BTreeSet::new();
        set.insert(req.clone());
        set.insert(NameRequest::new(
            BamlFqn::symbol(&name(&[], "a")),
            CppNameKind::Namespace,
        ));
        let names = CppNames::allocate(&set);
        assert_eq!(names.get(&req).declared(), "Foo");
        assert_eq!(
            names.get(&req).identifier().to_string(),
            "::baml_sdk::a::Foo"
        );
    }

    #[test]
    fn test_colliding_projections_get_stable_typed_suffixes() {
        // `void` and `void_` both project to `void_` within one callable.
        let owner = BamlFqn::symbol(&name(&[], "f"));
        let a = NameRequest::new(owner.child("void"), CppNameKind::Parameter);
        let b = NameRequest::new(owner.child("void_"), CppNameKind::Parameter);
        let mut set = BTreeSet::new();
        set.insert(a.clone());
        set.insert(b.clone());
        let names = CppNames::allocate(&set);
        let named_a = names.get(&a).declared().to_string();
        let named_b = names.get(&b).declared().to_string();
        assert_ne!(named_a, named_b);
        assert!(named_a.starts_with("void__"));
        assert!(named_b.starts_with("void__"));
        // Wire keys keep the source spellings.
        assert_eq!(names.get(&a).wire().to_string(), "void");
        assert_eq!(names.get(&b).wire().to_string(), "void_");
        // Determinism: reallocation yields identical names.
        let again = CppNames::allocate(&set);
        assert_eq!(again.get(&a).declared(), named_a);
        assert_eq!(again.get(&b).declared(), named_b);
    }

    #[test]
    fn test_generator_locals_reserved_in_callable_scope() {
        let owner = BamlFqn::symbol(&name(&[], "f"));
        let req = NameRequest::new(owner.child("args"), CppNameKind::Parameter);
        let mut set = BTreeSet::new();
        set.insert(req.clone());
        let names = CppNames::allocate(&set);
        assert_ne!(names.get(&req).declared(), "args");
        assert_eq!(names.get(&req).wire().to_string(), "args");
    }

    #[test]
    fn test_same_preferred_in_different_scopes_no_collision() {
        let f = BamlFqn::symbol(&name(&[], "f"));
        let g = BamlFqn::symbol(&name(&[], "g"));
        let a = NameRequest::new(f.child("value"), CppNameKind::Parameter);
        let b = NameRequest::new(g.child("value"), CppNameKind::Parameter);
        let mut set = BTreeSet::new();
        set.insert(a.clone());
        set.insert(b.clone());
        let names = CppNames::allocate(&set);
        assert_eq!(names.get(&a).declared(), "value");
        assert_eq!(names.get(&b).declared(), "value");
    }
}
