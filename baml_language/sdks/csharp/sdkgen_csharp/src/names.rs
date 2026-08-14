//! Typed, deterministic projection from BAML identities to C# identifiers.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fmt::Write as _,
};

use baml_base::Name as BaseName;
use baml_codegen_types::Name;

use crate::model::{CallableIdentity, CallableVariant};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BamlFqn {
    symbol: Name,
    members: Vec<BaseName>,
}

impl BamlFqn {
    #[must_use]
    pub fn symbol(symbol: &Name) -> Self {
        Self {
            symbol: symbol.clone(),
            members: Vec::new(),
        }
    }

    #[must_use]
    pub fn member(&self, member: &BaseName) -> Self {
        let mut result = self.clone();
        result.members.push(member.clone());
        result
    }

    #[must_use]
    pub fn symbol_name(&self) -> &Name {
        &self.symbol
    }

    pub(crate) fn stable_identity(&self) -> String {
        let mut result = String::new();
        push_identity_part(&mut result, self.symbol.package().as_str());
        for segment in self.symbol.namespace() {
            push_identity_part(&mut result, segment.as_str());
        }
        push_identity_part(&mut result, self.symbol.name().as_str());
        for member in &self.members {
            push_identity_part(&mut result, member.as_str());
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CSharpNameKind {
    NamespaceSegment,
    FunctionsHolder,
    Function,
    Class,
    Enum,
    TypeAlias,
    Property,
    EnumMember,
    TypeParameter,
    Parameter,
    FileStem,
    GeneratedLocal,
    HelperType,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CSharpVisibility {
    Public,
    Internal,
    Private,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CSharpNameOrigin {
    Source,
    CompilerGenerated,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CSharpScope {
    Namespace {
        package: BaseName,
        path: Vec<BaseName>,
    },
    Type(BamlFqn),
    Enum(BamlFqn),
    Callable(BamlFqn),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BamlWireName {
    Symbol(Name),
    Key(BaseName),
    Generated,
}

impl fmt::Display for BamlWireName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(name) => write!(f, "{name}"),
            Self::Key(name) => f.write_str(name.as_str()),
            Self::Generated => f.write_str("<generated>"),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CSharpNameRequest {
    pub fqn: BamlFqn,
    pub wire: BamlWireName,
    pub source_identity: String,
    pub kind: CSharpNameKind,
    pub visibility: CSharpVisibility,
    pub origin: CSharpNameOrigin,
    pub scope: CSharpScope,
}

impl CSharpNameRequest {
    #[must_use]
    pub fn new(
        fqn: BamlFqn,
        wire: BamlWireName,
        source_identity: impl Into<String>,
        kind: CSharpNameKind,
        visibility: CSharpVisibility,
        origin: CSharpNameOrigin,
        scope: CSharpScope,
    ) -> Self {
        Self {
            fqn,
            wire,
            source_identity: source_identity.into(),
            kind,
            visibility,
            origin,
            scope,
        }
    }

    fn stable_identity(&self) -> String {
        let mut result = self.fqn.stable_identity();
        push_identity_part(&mut result, &self.kind.to_string());
        push_identity_part(&mut result, &self.visibility.to_string());
        push_identity_part(&mut result, &self.origin.to_string());
        push_identity_part(&mut result, &self.source_identity);
        push_identity_part(&mut result, &self.wire.to_string());
        result
    }
}

impl fmt::Display for CSharpNameKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Display for CSharpVisibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Display for CSharpNameOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CSharpName {
    logical: String,
    source: String,
    wire: BamlWireName,
    kind: CSharpNameKind,
    request: CSharpNameRequest,
}

impl CSharpName {
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn wire(&self) -> &BamlWireName {
        &self.wire
    }

    #[must_use]
    pub fn kind(&self) -> CSharpNameKind {
        self.kind
    }

    pub(crate) fn logical(&self) -> &str {
        &self.logical
    }

    #[must_use]
    pub fn global_qualified<'a>(
        &'a self,
        namespace: impl IntoIterator<Item = &'a CSharpName>,
    ) -> String {
        let mut result = String::from("global::");
        for segment in namespace {
            result.push_str(segment.source());
            result.push('.');
        }
        result.push_str(self.source());
        result
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CSharpNames {
    allocations: BTreeMap<CSharpNameRequest, CSharpName>,
}

impl CSharpNames {
    #[must_use]
    pub fn allocate(requests: impl IntoIterator<Item = CSharpNameRequest>) -> Self {
        Self::allocate_with_primary_hasher(requests, stable_primary_hash)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn allocate_with_primary_hasher(
        requests: impl IntoIterator<Item = CSharpNameRequest>,
        primary_hasher: impl Fn(&[u8]) -> u64,
    ) -> Self {
        Self::allocate_with_hashers(requests, primary_hasher, stable_secondary_hash)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn allocate_with_hashers(
        requests: impl IntoIterator<Item = CSharpNameRequest>,
        primary_hasher: impl Fn(&[u8]) -> u64,
        secondary_hasher: impl Fn(&[u8]) -> u64,
    ) -> Self {
        let requests = requests.into_iter().collect::<BTreeSet<_>>();
        let mut by_scope = BTreeMap::<CSharpScope, Vec<CSharpNameRequest>>::new();
        for request in requests {
            by_scope
                .entry(request.scope.clone())
                .or_default()
                .push(request);
        }

        let mut allocations = BTreeMap::new();
        for scoped_requests in by_scope.values_mut() {
            scoped_requests.sort();
            let mut by_base = BTreeMap::<String, Vec<CSharpNameRequest>>::new();
            for request in scoped_requests.drain(..) {
                by_base
                    .entry(project_base(&request))
                    .or_default()
                    .push(request);
            }

            let mut occupied = BTreeSet::new();
            let mut deferred = Vec::new();
            for (base, requests) in &mut by_base {
                requests.sort_by_key(|request| (request_priority(request), request.clone()));
                let winner = requests.remove(0);
                occupied.insert(base.clone());
                insert_allocation(&mut allocations, winner, base.clone());
                deferred.extend(requests.drain(..).map(|request| (base.clone(), request)));
            }

            deferred.sort_by(|left, right| left.1.cmp(&right.1));
            for (base, request) in deferred {
                let identity = request.stable_identity();
                let primary = primary_hasher(identity.as_bytes());
                let secondary = secondary_hasher(identity.as_bytes());
                let logical = collision_name(&base, primary, secondary, &identity, &occupied);
                occupied.insert(logical.clone());
                insert_allocation(&mut allocations, request, logical);
            }
        }
        Self { allocations }
    }

    #[must_use]
    pub fn get(&self, request: &CSharpNameRequest) -> Option<&CSharpName> {
        self.allocations.get(request)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&CSharpNameRequest, &CSharpName)> {
        self.allocations.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allocations.is_empty()
    }

    #[must_use]
    pub fn contains_allocated(&self, name: &CSharpName) -> bool {
        self.allocations
            .get(&name.request)
            .is_some_and(|allocated| allocated == name)
    }
}

fn insert_allocation(
    allocations: &mut BTreeMap<CSharpNameRequest, CSharpName>,
    request: CSharpNameRequest,
    logical: String,
) {
    debug_assert!(is_csharp_identifier(&logical));
    let source = escape_keyword(&logical);
    let wire = request.wire.clone();
    let kind = request.kind;
    let name_request = request.clone();
    allocations.insert(
        request,
        CSharpName {
            logical,
            source,
            wire,
            kind,
            request: name_request,
        },
    );
}

fn collision_name(
    base: &str,
    primary: u64,
    secondary: u64,
    identity: &str,
    occupied: &BTreeSet<String>,
) -> String {
    let primary = format!("{primary:016x}");
    for width in [8, 12, 16] {
        let candidate = format!("{base}_{}", &primary[..width]);
        if !occupied.contains(&candidate) {
            return candidate;
        }
    }

    let secondary = format!("{secondary:016x}");
    for width in [8, 12, 16] {
        let candidate = format!("{base}_{primary}_{}", &secondary[..width]);
        if !occupied.contains(&candidate) {
            return candidate;
        }
    }

    let encoded_identity = identity.bytes().fold(
        String::with_capacity(identity.len() * 2),
        |mut encoded, byte| {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
            encoded
        },
    );
    let candidate = format!("{base}_{primary}_{secondary}_{encoded_identity}");
    assert!(
        !occupied.contains(&candidate),
        "distinct typed identities must allocate uniquely"
    );
    candidate
}

fn project_base(request: &CSharpNameRequest) -> String {
    match request.kind {
        CSharpNameKind::Parameter | CSharpNameKind::GeneratedLocal => {
            to_camel_case(&request.source_identity)
        }
        CSharpNameKind::NamespaceSegment
        | CSharpNameKind::FunctionsHolder
        | CSharpNameKind::Function
        | CSharpNameKind::Class
        | CSharpNameKind::Enum
        | CSharpNameKind::TypeAlias
        | CSharpNameKind::Property
        | CSharpNameKind::EnumMember
        | CSharpNameKind::TypeParameter
        | CSharpNameKind::FileStem
        | CSharpNameKind::HelperType => to_pascal_case(&request.source_identity),
    }
}

fn request_priority(request: &CSharpNameRequest) -> u8 {
    match (request.origin, request.kind) {
        (CSharpNameOrigin::Source, _) => 0,
        (CSharpNameOrigin::CompilerGenerated, CSharpNameKind::GeneratedLocal) => 2,
        (CSharpNameOrigin::CompilerGenerated, _) => 1,
    }
}

#[must_use]
pub(crate) fn callable_source_identity(identity: &CallableIdentity) -> String {
    let family = identity.family_name.as_str();
    match identity.variant {
        CallableVariant::Execute => family.to_string(),
        CallableVariant::RenderPrompt => format!("{family}_render_prompt"),
        CallableVariant::BuildRequest => format!("{family}_build_request"),
        CallableVariant::BuildRequestStream => format!("{family}_build_stream_request"),
        CallableVariant::Stream => format!("{family}_stream"),
        CallableVariant::Parse => format!("{family}_parse_response"),
        CallableVariant::ParseStream => format!("{family}_parse_stream_response"),
    }
}

pub(crate) fn stable_primary_hash(bytes: &[u8]) -> u64 {
    fnv1a(bytes, 0xcbf2_9ce4_8422_2325)
}

pub(crate) fn stable_secondary_hash(bytes: &[u8]) -> u64 {
    fnv1a(bytes, 0x8422_2325_cbf2_9ce4)
}

fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn push_identity_part(target: &mut String, value: &str) {
    target.push_str(&value.len().to_string());
    target.push(':');
    target.push_str(value);
    target.push(';');
}

pub(crate) fn to_pascal_case(value: &str) -> String {
    let words = identifier_words(value);
    let mut result = String::new();
    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            for character in chars {
                result.extend(character.to_lowercase());
            }
        }
    }
    finish_identifier(result)
}

pub(crate) fn to_camel_case(value: &str) -> String {
    let mut result = to_pascal_case(value);
    if let Some(first) = result.chars().next() {
        let lower = first.to_lowercase().collect::<String>();
        result.replace_range(..first.len_utf8(), &lower);
    }
    result
}

fn identifier_words(value: &str) -> Vec<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut words = Vec::new();
    let mut current = String::new();
    for (index, character) in chars.iter().copied().enumerate() {
        if !character.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }
        let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let next = chars.get(index + 1).copied();
        let boundary = !current.is_empty()
            && character.is_uppercase()
            && (previous.is_some_and(char::is_lowercase)
                || next.is_some_and(char::is_lowercase)
                    && previous.is_some_and(char::is_uppercase));
        if boundary {
            words.push(std::mem::take(&mut current));
        }
        current.push(character);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn finish_identifier(mut value: String) -> String {
    if value.is_empty() {
        value.push('_');
    }
    if value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        value.insert(0, '_');
    }
    value
}

fn escape_keyword(logical: &str) -> String {
    if is_csharp_keyword(logical) {
        format!("@{logical}")
    } else {
        logical.to_string()
    }
}

fn is_csharp_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character == '_' || character.is_alphabetic())
        && chars.all(|character| character == '_' || character.is_alphanumeric())
}

fn is_csharp_keyword(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "add"
            | "alias"
            | "and"
            | "args"
            | "as"
            | "ascending"
            | "async"
            | "await"
            | "base"
            | "bool"
            | "break"
            | "by"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "checked"
            | "class"
            | "const"
            | "continue"
            | "decimal"
            | "default"
            | "delegate"
            | "descending"
            | "do"
            | "double"
            | "dynamic"
            | "else"
            | "enum"
            | "equals"
            | "event"
            | "explicit"
            | "extern"
            | "false"
            | "file"
            | "finally"
            | "fixed"
            | "float"
            | "for"
            | "foreach"
            | "from"
            | "get"
            | "global"
            | "goto"
            | "group"
            | "if"
            | "implicit"
            | "in"
            | "init"
            | "int"
            | "interface"
            | "internal"
            | "into"
            | "is"
            | "join"
            | "let"
            | "lock"
            | "long"
            | "managed"
            | "nameof"
            | "namespace"
            | "new"
            | "nint"
            | "not"
            | "notnull"
            | "null"
            | "nuint"
            | "object"
            | "on"
            | "operator"
            | "or"
            | "orderby"
            | "out"
            | "override"
            | "params"
            | "partial"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "record"
            | "ref"
            | "remove"
            | "required"
            | "return"
            | "sbyte"
            | "scoped"
            | "sealed"
            | "select"
            | "set"
            | "short"
            | "sizeof"
            | "stackalloc"
            | "static"
            | "string"
            | "struct"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "uint"
            | "ulong"
            | "unchecked"
            | "unmanaged"
            | "unsafe"
            | "ushort"
            | "using"
            | "value"
            | "var"
            | "virtual"
            | "void"
            | "volatile"
            | "when"
            | "where"
            | "while"
            | "with"
            | "yield"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(name: &str) -> Name {
        Name::new(BaseName::new("user"), vec![], BaseName::new(name))
    }

    fn request(
        owner: &str,
        source: &str,
        kind: CSharpNameKind,
        scope: CSharpScope,
    ) -> CSharpNameRequest {
        let symbol = symbol(owner);
        CSharpNameRequest::new(
            BamlFqn::symbol(&symbol).member(&BaseName::new(source)),
            if kind == CSharpNameKind::GeneratedLocal {
                BamlWireName::Generated
            } else {
                BamlWireName::Key(BaseName::new(source))
            },
            source,
            kind,
            CSharpVisibility::Public,
            if kind == CSharpNameKind::GeneratedLocal {
                CSharpNameOrigin::CompilerGenerated
            } else {
                CSharpNameOrigin::Source
            },
            scope,
        )
    }

    #[test]
    fn projection_collisions_keywords_and_generated_local_priority() {
        let callable = BamlFqn::symbol(&symbol("classify"));
        let scope = CSharpScope::Callable(callable);
        let parameter = request(
            "classify",
            "result",
            CSharpNameKind::Parameter,
            scope.clone(),
        );
        let local = request(
            "classify",
            "result",
            CSharpNameKind::GeneratedLocal,
            scope.clone(),
        );
        let keyword = request(
            "classify",
            "class",
            CSharpNameKind::Parameter,
            scope.clone(),
        );
        let contextual = request(
            "classify",
            "record",
            CSharpNameKind::Parameter,
            scope.clone(),
        );
        let collision_a = request(
            "classify",
            "foo_bar",
            CSharpNameKind::Parameter,
            scope.clone(),
        );
        let collision_b = request(
            "classify",
            "fooBar",
            CSharpNameKind::Parameter,
            scope.clone(),
        );
        let collision_c = request("classify", "FooBar", CSharpNameKind::Parameter, scope);
        let names = CSharpNames::allocate([
            local.clone(),
            keyword.clone(),
            collision_c.clone(),
            parameter.clone(),
            collision_a.clone(),
            contextual.clone(),
            collision_b.clone(),
        ]);

        assert_eq!(names.get(&parameter).unwrap().source(), "result");
        assert_eq!(
            names.get(&parameter).unwrap().wire(),
            &BamlWireName::Key(BaseName::new("result"))
        );
        assert!(names.get(&local).unwrap().source().starts_with("result_"));
        assert_eq!(names.get(&keyword).unwrap().source(), "@class");
        assert_eq!(names.get(&contextual).unwrap().source(), "@record");
        let projected = [&collision_a, &collision_b, &collision_c]
            .map(|request| names.get(request).unwrap().source())
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(projected.len(), 3);
    }

    #[test]
    fn namespace_type_and_member_collision_domains_are_typed() {
        let namespace = CSharpScope::Namespace {
            package: BaseName::new("user"),
            path: vec![BaseName::new("acme")],
        };
        let holder = request(
            "functions_holder",
            "Functions",
            CSharpNameKind::FunctionsHolder,
            namespace.clone(),
        );
        let same_named_type = request("Functions", "Functions", CSharpNameKind::Class, namespace);

        let owner = BamlFqn::symbol(&symbol("invoice"));
        let first_property = request(
            "invoice",
            "invoice_id",
            CSharpNameKind::Property,
            CSharpScope::Type(owner.clone()),
        );
        let second_property = request(
            "invoice",
            "invoiceId",
            CSharpNameKind::Property,
            CSharpScope::Type(owner),
        );
        let names = CSharpNames::allocate([
            second_property.clone(),
            same_named_type.clone(),
            holder.clone(),
            first_property.clone(),
        ]);

        assert_ne!(
            names.get(&holder).unwrap().source(),
            names.get(&same_named_type).unwrap().source()
        );
        assert_ne!(
            names.get(&first_property).unwrap().source(),
            names.get(&second_property).unwrap().source()
        );
        assert!(
            [
                names.get(&holder).unwrap().source(),
                names.get(&same_named_type).unwrap().source(),
            ]
            .contains(&"Functions")
        );
    }

    #[test]
    fn source_declarations_outrank_generated_companion_names() {
        let scope = CSharpScope::Namespace {
            package: BaseName::new("user"),
            path: vec![],
        };
        let source = request(
            "zz_source",
            "extract_stream",
            CSharpNameKind::Function,
            scope.clone(),
        );
        let mut companion = request(
            "aa_generated",
            "extract_stream",
            CSharpNameKind::Function,
            scope,
        );
        companion.origin = CSharpNameOrigin::CompilerGenerated;
        let names = CSharpNames::allocate([companion.clone(), source.clone()]);
        assert_eq!(names.get(&source).unwrap().source(), "ExtractStream");
        assert!(
            names
                .get(&companion)
                .unwrap()
                .source()
                .starts_with("ExtractStream_")
        );
    }

    #[test]
    fn allocation_is_identical_across_one_hundred_orders_and_scopes_are_independent() {
        let callable_a = BamlFqn::symbol(&symbol("first"));
        let callable_b = BamlFqn::symbol(&symbol("second"));
        let requests = vec![
            request(
                "first",
                "value",
                CSharpNameKind::Parameter,
                CSharpScope::Callable(callable_a.clone()),
            ),
            request(
                "first",
                "foo_bar",
                CSharpNameKind::Parameter,
                CSharpScope::Callable(callable_a.clone()),
            ),
            request(
                "first",
                "fooBar",
                CSharpNameKind::Parameter,
                CSharpScope::Callable(callable_a),
            ),
            request(
                "second",
                "value",
                CSharpNameKind::Parameter,
                CSharpScope::Callable(callable_b),
            ),
        ];
        let expected = CSharpNames::allocate(requests.clone());
        for rank in 0..100 {
            let mut ordered = requests.clone();
            ordered.rotate_left(rank % requests.len());
            if rank % 3 == 0 {
                ordered.reverse();
            }
            assert_eq!(CSharpNames::allocate(ordered), expected);
        }
        assert_eq!(expected.get(&requests[0]).unwrap().source(), "@value");
        assert_eq!(expected.get(&requests[3]).unwrap().source(), "@value");
    }

    #[test]
    fn hash_prefix_and_full_primary_collisions_extend_deterministically() {
        let scope = CSharpScope::Callable(BamlFqn::symbol(&symbol("hashes")));
        let requests = [
            request(
                "hashes",
                "foo_bar",
                CSharpNameKind::Parameter,
                scope.clone(),
            ),
            request("hashes", "fooBar", CSharpNameKind::Parameter, scope.clone()),
            request("hashes", "FooBar", CSharpNameKind::Parameter, scope.clone()),
            request(
                "hashes",
                "foo-bar",
                CSharpNameKind::Parameter,
                scope.clone(),
            ),
            request(
                "hashes",
                "foo bar",
                CSharpNameKind::Parameter,
                scope.clone(),
            ),
            request(
                "hashes",
                "foo.bar",
                CSharpNameKind::Parameter,
                scope.clone(),
            ),
            request(
                "hashes",
                "foo$bar",
                CSharpNameKind::Parameter,
                scope.clone(),
            ),
            request("hashes", "foo__bar", CSharpNameKind::Parameter, scope),
        ];
        let prefix = CSharpNames::allocate_with_primary_hasher(requests.clone(), |bytes| {
            0xdead_beef_0000_0000 | u64::from(bytes.last().copied().unwrap_or_default())
        });
        assert_eq!(
            prefix
                .iter()
                .map(|(_, name)| &name.logical)
                .collect::<BTreeSet<_>>()
                .len(),
            8
        );

        let full = CSharpNames::allocate_with_hashers(
            requests,
            |_| 0xdead_beef_dead_beef,
            |_| 0x0123_4567_89ab_cdef,
        );
        assert_eq!(
            full.iter()
                .map(|(_, name)| &name.logical)
                .collect::<BTreeSet<_>>()
                .len(),
            8
        );
        assert!(
            full.iter()
                .any(|(_, name)| name.logical.matches('_').count() >= 3)
        );
    }

    #[test]
    fn qualification_is_global_and_callable_variants_are_typed() {
        let namespace_scope = CSharpScope::Namespace {
            package: BaseName::new("user"),
            path: vec![],
        };
        let ns_request = request(
            "billing",
            "acme_billing",
            CSharpNameKind::NamespaceSegment,
            namespace_scope,
        );
        let type_fqn = BamlFqn::symbol(&symbol("invoice"));
        let type_request = request(
            "invoice",
            "invoice",
            CSharpNameKind::Class,
            CSharpScope::Namespace {
                package: BaseName::new("user"),
                path: vec![BaseName::new("acme_billing")],
            },
        );
        let names = CSharpNames::allocate([ns_request.clone(), type_request.clone()]);
        assert_eq!(
            names
                .get(&type_request)
                .unwrap()
                .global_qualified([names.get(&ns_request).unwrap()]),
            "global::AcmeBilling.Invoice",
        );

        let identity = CallableIdentity {
            family_name: BaseName::new("classify_text"),
            wire_name: BaseName::new("classify_text$parse_stream"),
            variant: CallableVariant::ParseStream,
            receiver: None,
        };
        assert_eq!(
            callable_source_identity(&identity),
            "classify_text_parse_stream_response"
        );
        assert_eq!(
            to_pascal_case(&callable_source_identity(&identity)),
            "ClassifyTextParseStreamResponse"
        );
        assert_eq!(type_fqn.symbol_name().name().as_str(), "invoice");
    }
}
