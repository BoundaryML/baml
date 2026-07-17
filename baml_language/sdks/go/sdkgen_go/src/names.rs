//! Typed projection from BAML identities to Go identifiers.
//!
//! Name allocation is deliberately separate from rendering. The allocator
//! consumes typed FQNs, declaration kinds, and visibility, and produces an
//! opaque [`GoName`]. Raw strings are exposed only when source text is emitted.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
};

use baml_base::MediaKind;
use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::{
    packages::GoPackages,
    rendering::is_protected_go_identifier,
    types::{GoLiteral, GoTy, GoTypeProjection, GoUnionKey},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct BamlFqn {
    symbol: Name,
    members: Vec<baml_base::Name>,
}

impl BamlFqn {
    pub(crate) fn symbol(symbol: &Name) -> Self {
        Self {
            symbol: symbol.clone(),
            members: Vec::new(),
        }
    }

    pub(crate) fn member(&self, member: &baml_base::Name) -> Self {
        let mut result = self.clone();
        result.members.push(member.clone());
        result
    }

    fn parent(&self) -> Option<Self> {
        let mut result = self.clone();
        result.members.pop()?;
        Some(result)
    }

    fn leaf(&self) -> &baml_base::Name {
        self.members.last().unwrap_or(self.symbol.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoNameKind {
    Function,
    Method,
    FunctionOptionType,
    MethodOptionType,
    FunctionOptionSetter,
    MethodOptionSetter,
    Class,
    Enum,
    EnumVariant,
    TypeAlias,
    Parameter,
    Field,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoVisibility {
    Exported,
}

/// The exact BAML identity used by the FFI boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum BamlWireName {
    Symbol(Name),
    Method {
        owner: Name,
        method: baml_base::Name,
    },
    Key(baml_base::Name),
    /// Synthesized Go-only declarations have no independent BAML wire name.
    StructuralUnion,
}

impl fmt::Display for BamlWireName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(name) => write!(f, "{name}"),
            Self::Method { owner, method } => write!(f, "{owner}.{method}"),
            Self::Key(name) => write!(f, "{name}"),
            Self::StructuralUnion => f.write_str("<structural union>"),
        }
    }
}

/// A validated Go package name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoPackageName(Box<str>);

impl GoPackageName {
    pub(crate) fn new(value: &str) -> Self {
        assert!(is_go_identifier(value), "invalid Go package name: {value}");
        assert!(
            !is_go_keyword(value),
            "Go package name is a keyword: {value}"
        );
        Self(value.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated Go identifier together with the package that owns it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct GoIdent {
    package: GoPackageName,
    name: Box<str>,
}

impl GoIdent {
    fn new(package: GoPackageName, value: String) -> Self {
        assert!(
            is_go_identifier(&value),
            "invalid projected Go name: {value}"
        );
        assert!(
            !is_go_keyword(&value),
            "Go keyword escaped too late: {value}"
        );
        Self {
            package,
            name: value.into_boxed_str(),
        }
    }

    fn with_suffix(&self, suffix: &str) -> Self {
        Self::new(self.package.clone(), format!("{}_{suffix}", self.name))
    }

    fn with_trailing_underscore(&self) -> Self {
        Self::new(self.package.clone(), format!("{}{}", self.name, '_'))
    }
}

/// A Go identifier rendered from the perspective of one package.
pub(crate) struct GoIdentifier<'a> {
    identifier: &'a GoIdent,
    current_package: &'a GoPackageName,
}

impl fmt::Display for GoIdentifier<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.identifier.package == *self.current_package {
            f.write_str(&self.identifier.name)
        } else {
            write!(
                f,
                "{}.{}",
                self.identifier.package.as_str(),
                self.identifier.name
            )
        }
    }
}

/// A projected name carries both the canonical Go identifier and the exact
/// wire identity it represents. Neither side is reconstructed from the other.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoName {
    canonical: GoIdent,
    wire: BamlWireName,
}

impl GoName {
    pub(crate) fn identifier<'a>(&'a self, current_package: &'a GoPackageName) -> GoIdentifier<'a> {
        GoIdentifier {
            identifier: &self.canonical,
            current_package,
        }
    }

    pub(crate) fn wire(&self) -> &BamlWireName {
        &self.wire
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct NameRequest {
    fqn: BamlFqn,
    kind: GoNameKind,
    visibility: GoVisibility,
}

impl NameRequest {
    fn new(fqn: BamlFqn, kind: GoNameKind, visibility: GoVisibility) -> Self {
        Self {
            fqn,
            kind,
            visibility,
        }
    }

    fn scope(&self) -> NameScope {
        match self.kind {
            GoNameKind::Function
            | GoNameKind::FunctionOptionType
            | GoNameKind::FunctionOptionSetter
            | GoNameKind::MethodOptionType
            | GoNameKind::MethodOptionSetter
            | GoNameKind::Class
            | GoNameKind::Enum
            | GoNameKind::EnumVariant
            | GoNameKind::TypeAlias => NameScope::Package(self.fqn.symbol.package().clone()),
            GoNameKind::Method => NameScope::Class(
                self.fqn
                    .parent()
                    .expect("method FQN must include its owning class"),
            ),
            GoNameKind::Parameter => NameScope::Function(
                self.fqn
                    .parent()
                    .expect("parameter FQN must include its owning callable"),
            ),
            GoNameKind::Field => NameScope::Class(
                self.fqn
                    .parent()
                    .expect("field FQN must include its owning class"),
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum NameScope {
    Package(baml_base::Name),
    Function(BamlFqn),
    Class(BamlFqn),
}

impl NameScope {
    fn escape_reserved(
        &self,
        mut candidate: GoIdent,
        generated_package_aliases: &BTreeSet<String>,
    ) -> GoIdent {
        while (matches!(self, Self::Function(_))
            && (is_protected_go_identifier(candidate.name.as_ref())
                || generated_package_aliases.contains(candidate.name.as_ref())))
            || (matches!(self, Self::Class(_))
                && is_protected_go_identifier(candidate.name.as_ref()))
        {
            candidate = candidate.with_trailing_underscore();
        }
        candidate
    }
}

struct GoScope {
    used: HashSet<GoIdent>,
}

impl GoScope {
    fn new() -> Self {
        Self {
            used: HashSet::new(),
        }
    }

    fn allocate(&mut self, candidate: GoIdent, request: &NameRequest) -> GoIdent {
        if self.used.insert(candidate.clone()) {
            return candidate;
        }

        let hashed = candidate.with_suffix(&short_hash(request));
        if self.used.insert(hashed.clone()) {
            return hashed;
        }

        for suffix in 2.. {
            let numbered = hashed.with_suffix(&suffix.to_string());
            if self.used.insert(numbered.clone()) {
                return numbered;
            }
        }
        unreachable!()
    }
}

pub(crate) struct GoNames {
    allocations: HashMap<NameRequest, GoName>,
    unions: BTreeMap<(baml_base::Name, GoUnionKey), GoUnionNames>,
}

pub(crate) struct GoUnionNames {
    type_name: GoName,
    variant_name: GoName,
    kind_name: GoName,
    arms: BTreeMap<GoTy, GoUnionArmNames>,
}

pub(crate) struct GoUnionArmNames {
    wrapper: GoName,
    constructor: GoName,
    kind_constant: GoName,
    as_method: GoName,
}

impl GoUnionNames {
    pub(crate) fn type_name(&self) -> &GoName {
        &self.type_name
    }

    pub(crate) fn variant_name(&self) -> &GoName {
        &self.variant_name
    }

    pub(crate) fn kind_name(&self) -> &GoName {
        &self.kind_name
    }

    pub(crate) fn arm(&self, ty: &GoTy) -> &GoUnionArmNames {
        &self.arms[ty]
    }
}

impl GoUnionArmNames {
    pub(crate) fn wrapper(&self) -> &GoName {
        &self.wrapper
    }

    pub(crate) fn constructor(&self) -> &GoName {
        &self.constructor
    }

    pub(crate) fn kind_constant(&self) -> &GoName {
        &self.kind_constant
    }

    pub(crate) fn as_method(&self) -> &GoName {
        &self.as_method
    }
}

impl GoNames {
    /// Build one name table for every generated package. All declarations
    /// reserve names even when their codegen feature has not been implemented.
    pub(crate) fn for_pool(
        pool: &SymbolPool,
        packages: &GoPackages,
        projection: &GoTypeProjection<'_>,
    ) -> Self {
        let mut requests = Vec::new();
        let mut wire_overrides = HashMap::new();
        for (name, symbol) in pool {
            let fqn = BamlFqn::symbol(name);
            let kind = match symbol {
                Symbol::Function(_) => GoNameKind::Function,
                Symbol::Class(_) => GoNameKind::Class,
                Symbol::Enum(_) => GoNameKind::Enum,
                Symbol::TypeAlias(_) => GoNameKind::TypeAlias,
            };
            requests.push(NameRequest::new(fqn.clone(), kind, GoVisibility::Exported));

            if let Symbol::Function(function) = symbol {
                if function
                    .arguments
                    .iter()
                    .any(|argument| argument.default.is_some())
                {
                    requests.push(NameRequest::new(
                        fqn.clone(),
                        GoNameKind::FunctionOptionType,
                        GoVisibility::Exported,
                    ));
                }
                requests.extend(function.arguments.iter().map(|argument| {
                    NameRequest::new(
                        fqn.member(&argument.name),
                        GoNameKind::Parameter,
                        GoVisibility::Exported,
                    )
                }));
                requests.extend(
                    function
                        .arguments
                        .iter()
                        .filter(|argument| argument.default.is_some())
                        .map(|argument| {
                            NameRequest::new(
                                fqn.member(&argument.name),
                                GoNameKind::FunctionOptionSetter,
                                GoVisibility::Exported,
                            )
                        }),
                );
            }
            if let Symbol::Class(class) = symbol {
                requests.extend(class.properties.iter().map(|property| {
                    NameRequest::new(
                        fqn.member(&property.name),
                        GoNameKind::Field,
                        GoVisibility::Exported,
                    )
                }));
                for method in class.static_methods.iter().chain(&class.instance_methods) {
                    let method_fqn = fqn.member(&method.name);
                    requests.push(NameRequest::new(
                        method_fqn.clone(),
                        GoNameKind::Method,
                        GoVisibility::Exported,
                    ));
                    if method
                        .arguments
                        .iter()
                        .any(|argument| argument.default.is_some())
                    {
                        requests.push(NameRequest::new(
                            method_fqn.clone(),
                            GoNameKind::MethodOptionType,
                            GoVisibility::Exported,
                        ));
                    }
                    requests.extend(method.arguments.iter().map(|argument| {
                        NameRequest::new(
                            method_fqn.member(&argument.name),
                            GoNameKind::Parameter,
                            GoVisibility::Exported,
                        )
                    }));
                    requests.extend(
                        method
                            .arguments
                            .iter()
                            .filter(|argument| argument.default.is_some())
                            .map(|argument| {
                                NameRequest::new(
                                    method_fqn.member(&argument.name),
                                    GoNameKind::MethodOptionSetter,
                                    GoVisibility::Exported,
                                )
                            }),
                    );
                }
            }
            if let Symbol::Enum(enum_) = symbol {
                for variant in &enum_.variants {
                    let request = NameRequest::new(
                        fqn.member(&variant.name),
                        GoNameKind::EnumVariant,
                        GoVisibility::Exported,
                    );
                    wire_overrides.insert(
                        request.clone(),
                        BamlWireName::Key(baml_base::Name::new(&variant.value)),
                    );
                    requests.push(request);
                }
            }
        }
        let generated_package_aliases = packages
            .iter()
            .map(|package| package.go_name().as_str().to_string())
            .collect();
        let mut names = Self::allocate(
            requests,
            |request| packages.get(request.fqn.symbol.package()).go_name().clone(),
            &generated_package_aliases,
            &wire_overrides,
        );
        names.allocate_unions(packages, projection);
        names
    }

    /// The canonical naming operation: `(FQN, kind, visibility) -> GoName`.
    pub(crate) fn project(
        &self,
        fqn: &BamlFqn,
        kind: GoNameKind,
        visibility: GoVisibility,
    ) -> &GoName {
        let request = NameRequest::new(fqn.clone(), kind, visibility);
        self.allocations
            .get(&request)
            .expect("name request was not registered during allocation")
    }

    pub(crate) fn union(&self, package: &baml_base::Name, key: &GoUnionKey) -> &GoUnionNames {
        self.unions
            .get(&(package.clone(), key.clone()))
            .expect("typed union name was not registered during allocation")
    }

    #[cfg(test)]
    fn new(package: &GoPackageName, requests: Vec<NameRequest>) -> Self {
        Self::allocate(
            requests,
            |_| package.clone(),
            &BTreeSet::default(),
            &HashMap::new(),
        )
    }

    #[cfg(test)]
    fn new_with_wire_overrides(
        package: &GoPackageName,
        requests: Vec<NameRequest>,
        wire_overrides: &HashMap<NameRequest, BamlWireName>,
    ) -> Self {
        Self::allocate(
            requests,
            |_| package.clone(),
            &BTreeSet::default(),
            wire_overrides,
        )
    }

    fn allocate(
        requests: Vec<NameRequest>,
        package_for: impl Fn(&NameRequest) -> GoPackageName,
        generated_package_aliases: &BTreeSet<String>,
        wire_overrides: &HashMap<NameRequest, BamlWireName>,
    ) -> Self {
        let mut groups = BTreeMap::<NameScope, BTreeMap<GoIdent, Vec<NameRequest>>>::new();
        for request in requests {
            let scope = request.scope();
            let base = scope.escape_reserved(
                project_base(package_for(&request), &request),
                generated_package_aliases,
            );
            groups
                .entry(scope)
                .or_default()
                .entry(base)
                .or_default()
                .push(request);
        }

        let mut allocations = HashMap::new();
        for base_groups in groups.values_mut() {
            let mut scope = GoScope::new();
            for (base, requests) in base_groups {
                requests.sort();
                let collides = requests.len() > 1;
                for request in requests {
                    let candidate = if collides {
                        base.with_suffix(&short_hash(request))
                    } else {
                        base.clone()
                    };
                    let canonical = scope.allocate(candidate, request);
                    allocations.insert(
                        request.clone(),
                        GoName {
                            canonical,
                            wire: wire_overrides
                                .get(request)
                                .cloned()
                                .unwrap_or_else(|| wire_name(request)),
                        },
                    );
                }
            }
        }
        Self {
            allocations,
            unions: BTreeMap::new(),
        }
    }

    fn allocate_unions(&mut self, packages: &GoPackages, projection: &GoTypeProjection<'_>) {
        let mut used = BTreeMap::<GoPackageName, BTreeSet<Box<str>>>::new();
        for name in self.allocations.values() {
            used.entry(name.canonical.package.clone())
                .or_default()
                .insert(name.canonical.name.clone());
        }

        for package in packages.iter() {
            let package_name = package.go_name().clone();
            for key in projection.typed_unions_in(package.baml_name()) {
                let raw_arm_components = key
                    .members()
                    .iter()
                    .map(|member| union_type_component(member, self))
                    .collect::<Vec<_>>();
                let disambiguated = raw_arm_components
                    .iter()
                    .enumerate()
                    .map(|(index, component)| {
                        if raw_arm_components
                            .iter()
                            .filter(|candidate| *candidate == component)
                            .count()
                            == 1
                        {
                            component.clone()
                        } else {
                            disambiguated_union_component(&key.members()[index], component)
                        }
                    })
                    .collect::<Vec<_>>();
                let arm_components = disambiguated
                    .iter()
                    .enumerate()
                    .map(|(index, component)| {
                        if disambiguated
                            .iter()
                            .filter(|candidate| *candidate == component)
                            .count()
                            == 1
                        {
                            component.clone()
                        } else {
                            // A theoretical full identity-hash collision is
                            // still made mathematically unique by structural
                            // order, which is independent of declaration order.
                            format!("{component}Arm{}", index + 1)
                        }
                    })
                    .collect::<Vec<_>>();
                let mut base = arm_components.join("Or");
                if base.is_empty() {
                    base.push_str("BamlUnion");
                }
                loop {
                    let mut family = vec![
                        base.clone(),
                        format!("{base}Variant"),
                        format!("{base}Kind"),
                    ];
                    for component in &arm_components {
                        family.push(format!("{base}{component}"));
                        family.push(format!("New{base}From{component}"));
                        family.push(format!("{base}Kind{component}"));
                    }
                    let unique_family = family.iter().collect::<BTreeSet<_>>();
                    assert_eq!(
                        unique_family.len(),
                        family.len(),
                        "structural union generated duplicate package declarations: {family:?}"
                    );
                    let methods = arm_components
                        .iter()
                        .map(|component| format!("As{component}"))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        methods.iter().collect::<BTreeSet<_>>().len(),
                        methods.len(),
                        "structural union generated duplicate arm methods: {methods:?}"
                    );
                    let package_used = used.entry(package_name.clone()).or_default();
                    if family
                        .iter()
                        .all(|candidate| !package_used.contains(candidate.as_str()))
                    {
                        package_used.extend(family.into_iter().map(Into::into));
                        break;
                    }
                    base.push('_');
                }

                let synthetic = |value: String| GoName {
                    canonical: GoIdent::new(package_name.clone(), value),
                    wire: BamlWireName::StructuralUnion,
                };
                let arms = key
                    .members()
                    .iter()
                    .cloned()
                    .zip(arm_components.iter())
                    .map(|(member, component)| {
                        let arm = GoUnionArmNames {
                            wrapper: synthetic(format!("{base}{component}")),
                            constructor: synthetic(format!("New{base}From{component}")),
                            kind_constant: synthetic(format!("{base}Kind{component}")),
                            as_method: synthetic(format!("As{component}")),
                        };
                        (member, arm)
                    })
                    .collect();
                self.unions.insert(
                    (package.baml_name().clone(), key.clone()),
                    GoUnionNames {
                        type_name: synthetic(base.clone()),
                        variant_name: synthetic(format!("{base}Variant")),
                        kind_name: synthetic(format!("{base}Kind")),
                        arms,
                    },
                );
            }
        }
    }
}

fn union_type_component(ty: &GoTy, names: &GoNames) -> String {
    match ty {
        GoTy::String => "String".into(),
        GoTy::Int => "Int".into(),
        GoTy::Bigint => "Bigint".into(),
        GoTy::Float => "Float".into(),
        GoTy::Bool => "Bool".into(),
        GoTy::Null => "Null".into(),
        GoTy::Uint8Array => "Uint8Array".into(),
        GoTy::Media(kind) => media_component(*kind).to_string(),
        GoTy::Literal(literal) => literal_component(literal),
        GoTy::Class(name) => names
            .project(
                &BamlFqn::symbol(name),
                GoNameKind::Class,
                GoVisibility::Exported,
            )
            .canonical
            .name
            .to_string(),
        GoTy::Enum(name) => names
            .project(
                &BamlFqn::symbol(name),
                GoNameKind::Enum,
                GoVisibility::Exported,
            )
            .canonical
            .name
            .to_string(),
        GoTy::List(inner) => format!("{}List", union_type_component(inner, names)),
        GoTy::Map { key, value } => format!(
            "{}To{}Map",
            union_type_component(key, names),
            union_type_component(value, names)
        ),
        GoTy::Optional(inner) => format!("Optional{}", union_type_component(inner, names)),
        GoTy::TypedUnion(key) | GoTy::DynamicUnion { key, .. } => key
            .members()
            .iter()
            .map(|member| union_type_component(member, names))
            .collect::<Vec<_>>()
            .join("Or"),
        GoTy::Unsupported => "Unsupported".into(),
    }
}

fn disambiguated_union_component(ty: &GoTy, base: &str) -> String {
    match ty {
        GoTy::String => "PrimitiveString".into(),
        GoTy::Int => "PrimitiveInt".into(),
        GoTy::Bigint => "PrimitiveBigint".into(),
        GoTy::Float => "PrimitiveFloat".into(),
        GoTy::Bool => "PrimitiveBool".into(),
        GoTy::Null => "PrimitiveNull".into(),
        GoTy::Uint8Array => "PrimitiveUint8Array".into(),
        GoTy::Media(kind) => format!("Primitive{}", media_component(*kind)),
        GoTy::Class(name) => format!("Class{base}{}", nominal_type_hash(name, GoNameKind::Class)),
        GoTy::Enum(name) => format!("Enum{base}{}", nominal_type_hash(name, GoNameKind::Enum)),
        _ => {
            let mut hash = StableFnv::new();
            hash_go_ty(&mut hash, ty);
            format!("{base}Type{:016x}", hash.finish())
        }
    }
}

fn hash_go_ty(hash: &mut StableFnv, ty: &GoTy) {
    match ty {
        GoTy::String => hash.byte(0),
        GoTy::Int => hash.byte(1),
        GoTy::Bigint => hash.byte(2),
        GoTy::Float => hash.byte(3),
        GoTy::Bool => hash.byte(4),
        GoTy::Null => hash.byte(5),
        GoTy::Uint8Array => hash.byte(6),
        GoTy::Media(kind) => {
            hash.byte(15);
            hash.component(media_component(*kind));
        }
        GoTy::Literal(literal) => {
            hash.byte(7);
            hash.component(&literal_component(literal));
        }
        GoTy::Class(name) => {
            hash.byte(8);
            hash.component(&nominal_type_hash(name, GoNameKind::Class));
        }
        GoTy::Enum(name) => {
            hash.byte(9);
            hash.component(&nominal_type_hash(name, GoNameKind::Enum));
        }
        GoTy::List(inner) => {
            hash.byte(10);
            hash_go_ty(hash, inner);
        }
        GoTy::Map { key, value } => {
            hash.byte(11);
            hash_go_ty(hash, key);
            hash_go_ty(hash, value);
        }
        GoTy::Optional(inner) => {
            hash.byte(12);
            hash_go_ty(hash, inner);
        }
        GoTy::TypedUnion(key) => {
            hash.byte(13);
            for member in key.members() {
                hash_go_ty(hash, member);
            }
        }
        GoTy::DynamicUnion { key, nullable } => {
            hash.byte(14);
            hash.byte(u8::from(*nullable));
            for member in key.members() {
                hash_go_ty(hash, member);
            }
        }
        GoTy::Unsupported => hash.byte(16),
    }
}

fn media_component(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "Image",
        MediaKind::Audio => "Audio",
        MediaKind::Video => "Video",
        MediaKind::Pdf => "Pdf",
        MediaKind::Generic => "Media",
    }
}

fn nominal_type_hash(name: &Name, kind: GoNameKind) -> String {
    let mut hash = StableFnv::new();
    hash.component(name.package().as_str());
    hash.usize(name.namespace().len());
    for segment in name.namespace() {
        hash.component(segment.as_str());
    }
    hash.component(name.name().as_str());
    hash.byte(match kind {
        GoNameKind::Class => 1,
        GoNameKind::Enum => 2,
        _ => unreachable!("only nominal union-arm kinds are hashed"),
    });
    format!("{:016x}", hash.finish())
}

fn literal_component(literal: &GoLiteral) -> String {
    let kind = match literal {
        GoLiteral::String(_) => "String",
        GoLiteral::Int(_) => "Int",
        GoLiteral::Bigint(_) => "Bigint",
        GoLiteral::Float(_) => "Float",
        GoLiteral::Bool(_) => "Bool",
    };
    let value = match literal {
        GoLiteral::String(value) | GoLiteral::Bigint(value) | GoLiteral::Float(value) => {
            value.clone()
        }
        GoLiteral::Int(value) => value.to_string(),
        GoLiteral::Bool(value) => value.to_string(),
    };
    let mut hash = StableFnv::new();
    hash.component(kind);
    hash.component(&value);
    let bytes = hash.finish().to_le_bytes();
    let short = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    format!("{kind}Literal{short:08x}")
}

fn project_base(package: GoPackageName, request: &NameRequest) -> GoIdent {
    let mut value = String::new();
    match request.kind {
        GoNameKind::Function | GoNameKind::Class | GoNameKind::Enum | GoNameKind::TypeAlias => {
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
        }
        GoNameKind::Method => {
            push_upper_component(&mut value, request.fqn.leaf());
        }
        GoNameKind::FunctionOptionType => {
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
            value.push_str("Option");
        }
        GoNameKind::MethodOptionType => {
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
            for member in &request.fqn.members {
                push_upper_component(&mut value, member);
            }
            value.push_str("Option");
        }
        GoNameKind::FunctionOptionSetter => {
            value.push_str("With");
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
            push_upper_component(&mut value, request.fqn.leaf());
        }
        GoNameKind::MethodOptionSetter => {
            value.push_str("With");
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
            for member in &request.fqn.members {
                push_upper_component(&mut value, member);
            }
        }
        GoNameKind::EnumVariant => {
            for segment in request.fqn.symbol.namespace() {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, request.fqn.symbol.name());
            push_upper_component(&mut value, request.fqn.leaf());
        }
        GoNameKind::Parameter | GoNameKind::Field => {
            push_upper_component(&mut value, request.fqn.leaf());
        }
    }

    if value.is_empty() {
        value.push_str("BamlSymbol");
    } else if value.starts_with(|ch: char| ch.is_ascii_digit()) {
        value.insert_str(0, "Baml");
    }
    // Parameters are local identifiers, not declarations with package
    // visibility. Their spelling is controlled by their kind; every generated
    // declaration currently uses exported visibility.
    if matches!(request.kind, GoNameKind::Parameter) {
        lowercase_first(&mut value);
    }

    while is_go_keyword(&value) {
        value.push('_');
    }
    GoIdent::new(package, value)
}

fn wire_name(request: &NameRequest) -> BamlWireName {
    match request.kind {
        GoNameKind::Function
        | GoNameKind::FunctionOptionType
        | GoNameKind::Class
        | GoNameKind::Enum
        | GoNameKind::TypeAlias => BamlWireName::Symbol(request.fqn.symbol.clone()),
        GoNameKind::Method | GoNameKind::MethodOptionType => BamlWireName::Method {
            owner: request.fqn.symbol.clone(),
            method: request.fqn.leaf().clone(),
        },
        GoNameKind::FunctionOptionSetter
        | GoNameKind::MethodOptionSetter
        | GoNameKind::EnumVariant
        | GoNameKind::Parameter
        | GoNameKind::Field => BamlWireName::Key(request.fqn.leaf().clone()),
    }
}

fn push_upper_component(output: &mut String, component: &baml_base::Name) {
    for word in component
        .as_str()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
    {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.push_str(chars.as_str());
        }
    }
}

fn lowercase_first(value: &mut String) {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return;
    };
    let mut lowered = first.to_ascii_lowercase().to_string();
    lowered.push_str(chars.as_str());
    *value = lowered;
}

fn short_hash(request: &NameRequest) -> String {
    let mut hash = StableFnv::new();
    hash.component(request.fqn.symbol.package().as_str());
    hash.usize(request.fqn.symbol.namespace().len());
    for segment in request.fqn.symbol.namespace() {
        hash.component(segment.as_str());
    }
    hash.component(request.fqn.symbol.name().as_str());
    hash.usize(request.fqn.members.len());
    for member in &request.fqn.members {
        hash.component(member.as_str());
    }
    hash.byte(match request.kind {
        GoNameKind::Function => 0,
        GoNameKind::Method => 9,
        GoNameKind::Class => 1,
        GoNameKind::Enum => 2,
        GoNameKind::TypeAlias => 3,
        GoNameKind::Parameter => 4,
        GoNameKind::Field => 5,
        GoNameKind::FunctionOptionType => 6,
        GoNameKind::FunctionOptionSetter => 7,
        GoNameKind::EnumVariant => 8,
        GoNameKind::MethodOptionType => 10,
        GoNameKind::MethodOptionSetter => 11,
    });
    hash.byte(match request.visibility {
        GoVisibility::Exported => 0,
    });
    format!("{:016x}", hash.finish())[..8].to_string()
}

struct StableFnv(u64);

impl StableFnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn byte(&mut self, byte: u8) {
        self.0 ^= u64::from(byte);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn usize(&mut self, value: usize) {
        for byte in (value as u64).to_le_bytes() {
            self.byte(byte);
        }
    }

    fn component(&mut self, value: &str) {
        self.usize(value.len());
        for byte in value.bytes() {
            self.byte(byte);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

fn is_go_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(crate) fn is_go_keyword(value: &str) -> bool {
    matches!(
        value,
        "break"
            | "default"
            | "func"
            | "interface"
            | "select"
            | "case"
            | "defer"
            | "go"
            | "map"
            | "struct"
            | "chan"
            | "else"
            | "goto"
            | "package"
            | "switch"
            | "const"
            | "fallthrough"
            | "if"
            | "range"
            | "type"
            | "continue"
            | "for"
            | "import"
            | "return"
            | "var"
    )
}

#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use baml_codegen_types::Name;

    use super::*;
    use crate::rendering::GeneratorIdent;

    fn symbol(namespace: &[&str], name: &str) -> BamlFqn {
        BamlFqn::symbol(&Name::new(
            BaseName::new("user"),
            namespace
                .iter()
                .map(|value| BaseName::new(*value))
                .collect(),
            BaseName::new(name),
        ))
    }

    fn request(fqn: BamlFqn, kind: GoNameKind, visibility: GoVisibility) -> NameRequest {
        NameRequest::new(fqn, kind, visibility)
    }

    fn generated_package() -> GoPackageName {
        GoPackageName::new("baml_sdk")
    }

    fn identifier(name: &GoName) -> String {
        name.identifier(&generated_package()).to_string()
    }

    #[test]
    fn projects_fqn_kind_and_visibility_without_initialism_rules() {
        let cases = [
            (
                symbol(&["billing_v2"], "lookup_invoice"),
                GoNameKind::Function,
                GoVisibility::Exported,
                "BillingV2LookupInvoice",
            ),
            (
                symbol(&[], "http_client"),
                GoNameKind::Class,
                GoVisibility::Exported,
                "HttpClient",
            ),
            (
                symbol(&[], "HTTP_client"),
                GoNameKind::Class,
                GoVisibility::Exported,
                "HTTPClient",
            ),
        ];
        let names = GoNames::new(
            &generated_package(),
            cases
                .iter()
                .map(|(fqn, kind, visibility, _)| request(fqn.clone(), *kind, *visibility))
                .collect(),
        );

        for (fqn, kind, visibility, expected) in cases {
            let projected = names.project(&fqn, kind, visibility);
            assert_eq!(identifier(projected), expected);
            assert_eq!(projected.wire(), &BamlWireName::Symbol(fqn.symbol.clone()));
        }
    }

    #[test]
    fn companion_names_preserve_wire_identity_and_share_package_collision_scope() {
        let companion = symbol(&["lorem"], "extract_resume$build_request");
        let user_function = symbol(&["lorem"], "extract_resume_build_request");
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(
                    companion.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
                request(
                    user_function.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
            ],
        );

        let companion_name =
            names.project(&companion, GoNameKind::Function, GoVisibility::Exported);
        let user_name = names.project(&user_function, GoNameKind::Function, GoVisibility::Exported);

        assert_ne!(identifier(companion_name), identifier(user_name));
        assert!(identifier(companion_name).starts_with("LoremExtractResumeBuildRequest_"));
        assert!(identifier(user_name).starts_with("LoremExtractResumeBuildRequest_"));
        assert_eq!(
            companion_name.wire(),
            &BamlWireName::Symbol(companion.symbol.clone())
        );
        assert_eq!(
            companion_name.wire().to_string(),
            "user.lorem.extract_resume$build_request"
        );
    }

    #[test]
    fn parse_companion_preserves_wire_identity_and_collides_canonically() {
        let companion = symbol(&["lorem"], "extract_resume$parse");
        let user_function = symbol(&["lorem"], "extract_resume_parse");
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(
                    companion.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
                request(
                    user_function.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
            ],
        );

        let companion_name =
            names.project(&companion, GoNameKind::Function, GoVisibility::Exported);
        let user_name = names.project(&user_function, GoNameKind::Function, GoVisibility::Exported);

        assert_ne!(identifier(companion_name), identifier(user_name));
        assert!(identifier(companion_name).starts_with("LoremExtractResumeParse_"));
        assert!(identifier(user_name).starts_with("LoremExtractResumeParse_"));
        assert_eq!(
            companion_name.wire(),
            &BamlWireName::Symbol(companion.symbol.clone())
        );
        assert_eq!(
            companion_name.wire().to_string(),
            "user.lorem.extract_resume$parse"
        );
    }

    #[test]
    fn build_request_stream_companion_preserves_wire_identity_and_collides_canonically() {
        let companion = symbol(&["lorem"], "extract_resume$build_request_stream");
        let user_function = symbol(&["lorem"], "extract_resume_build_request_stream");
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(
                    companion.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
                request(
                    user_function.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
            ],
        );

        let companion_name =
            names.project(&companion, GoNameKind::Function, GoVisibility::Exported);
        let user_name = names.project(&user_function, GoNameKind::Function, GoVisibility::Exported);

        assert_ne!(identifier(companion_name), identifier(user_name));
        assert!(identifier(companion_name).starts_with("LoremExtractResumeBuildRequestStream_"));
        assert!(identifier(user_name).starts_with("LoremExtractResumeBuildRequestStream_"));
        assert_eq!(
            companion_name.wire(),
            &BamlWireName::Symbol(companion.symbol.clone())
        );
        assert_eq!(
            companion_name.wire().to_string(),
            "user.lorem.extract_resume$build_request_stream"
        );
    }

    #[test]
    fn identifier_is_relative_to_the_current_package() {
        let fqn = symbol(&[], "lookup_invoice");
        let names = GoNames::new(
            &generated_package(),
            vec![request(
                fqn.clone(),
                GoNameKind::Function,
                GoVisibility::Exported,
            )],
        );
        let projected = names.project(&fqn, GoNameKind::Function, GoVisibility::Exported);

        assert_eq!(
            projected.identifier(&generated_package()).to_string(),
            "LookupInvoice"
        );
        assert_eq!(
            projected
                .identifier(&GoPackageName::new("consumer"))
                .to_string(),
            "baml_sdk.LookupInvoice"
        );
        assert_eq!(projected.wire().to_string(), "user.lookup_invoice");
    }

    #[test]
    fn function_options_are_typed_package_declarations_with_wire_identity() {
        let function = symbol(&["billing"], "lookup_invoice");
        let limit = function.member(&BaseName::new("max_rows"));
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(
                    function.clone(),
                    GoNameKind::FunctionOptionType,
                    GoVisibility::Exported,
                ),
                request(
                    limit.clone(),
                    GoNameKind::FunctionOptionSetter,
                    GoVisibility::Exported,
                ),
            ],
        );

        let option_type = names.project(
            &function,
            GoNameKind::FunctionOptionType,
            GoVisibility::Exported,
        );
        assert_eq!(identifier(option_type), "BillingLookupInvoiceOption");
        assert_eq!(
            option_type.wire(),
            &BamlWireName::Symbol(function.symbol.clone())
        );

        let setter = names.project(
            &limit,
            GoNameKind::FunctionOptionSetter,
            GoVisibility::Exported,
        );
        assert_eq!(identifier(setter), "WithBillingLookupInvoiceMaxRows");
        assert_eq!(setter.wire(), &BamlWireName::Key(BaseName::new("max_rows")));
    }

    #[test]
    fn methods_have_exact_wire_identity_and_share_the_class_collision_scope() {
        let class = symbol(&["method_edges"], "edge_box");
        let method = class.member(&BaseName::new("roundTrip"));
        let field = class.member(&BaseName::new("round_trip"));
        let marker_method = class.member(&BaseName::new("BAMLClassName"));
        let value = method.member(&BaseName::new("value"));
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(method.clone(), GoNameKind::Method, GoVisibility::Exported),
                request(field.clone(), GoNameKind::Field, GoVisibility::Exported),
                request(
                    marker_method.clone(),
                    GoNameKind::Method,
                    GoVisibility::Exported,
                ),
                request(
                    method.clone(),
                    GoNameKind::MethodOptionType,
                    GoVisibility::Exported,
                ),
                request(
                    value.clone(),
                    GoNameKind::MethodOptionSetter,
                    GoVisibility::Exported,
                ),
            ],
        );

        let method_name = names.project(&method, GoNameKind::Method, GoVisibility::Exported);
        let field_name = names.project(&field, GoNameKind::Field, GoVisibility::Exported);
        assert_ne!(identifier(method_name), identifier(field_name));
        assert!(identifier(method_name).starts_with("RoundTrip_"));
        assert!(identifier(field_name).starts_with("RoundTrip_"));
        assert_eq!(
            method_name.wire(),
            &BamlWireName::Method {
                owner: method.symbol.clone(),
                method: BaseName::new("roundTrip"),
            }
        );
        assert_eq!(
            identifier(names.project(&marker_method, GoNameKind::Method, GoVisibility::Exported,)),
            "BAMLClassName_"
        );
        assert_eq!(
            identifier(names.project(
                &method,
                GoNameKind::MethodOptionType,
                GoVisibility::Exported,
            )),
            "MethodEdgesEdgeBoxRoundTripOption"
        );
        assert_eq!(
            identifier(names.project(
                &value,
                GoNameKind::MethodOptionSetter,
                GoVisibility::Exported,
            )),
            "WithMethodEdgesEdgeBoxRoundTripValue"
        );
    }

    #[test]
    fn enum_variants_are_type_prefixed_package_declarations_with_wire_identity() {
        let enum_ = symbol(&["review_queue"], "response_state");
        let variant = enum_.member(&BaseName::new("pending_review"));
        let colliding_function = symbol(&[], "review_queue_response_state_pending_review");
        let enum_request = request(
            variant.clone(),
            GoNameKind::EnumVariant,
            GoVisibility::Exported,
        );
        let names = GoNames::new_with_wire_overrides(
            &generated_package(),
            vec![
                request(enum_.clone(), GoNameKind::Enum, GoVisibility::Exported),
                enum_request.clone(),
                request(
                    colliding_function.clone(),
                    GoNameKind::Function,
                    GoVisibility::Exported,
                ),
            ],
            &HashMap::from([(
                enum_request,
                BamlWireName::Key(BaseName::new("pending-review")),
            )]),
        );

        assert_eq!(
            identifier(names.project(&enum_, GoNameKind::Enum, GoVisibility::Exported)),
            "ReviewQueueResponseState"
        );
        let projected = names.project(&variant, GoNameKind::EnumVariant, GoVisibility::Exported);
        assert!(identifier(projected).starts_with("ReviewQueueResponseStatePendingReview_"));
        assert_eq!(
            projected.wire(),
            &BamlWireName::Key(BaseName::new("pending-review"))
        );
        assert!(
            identifier(names.project(
                &colliding_function,
                GoNameKind::Function,
                GoVisibility::Exported,
            ))
            .starts_with("ReviewQueueResponseStatePendingReview_")
        );
    }

    #[test]
    fn parameters_use_their_own_scope_and_escape_generator_locals() {
        let left = symbol(&["left"], "call");
        let right = symbol(&["right"], "call");
        let left_value = left.member(&BaseName::new("user_id"));
        let right_value = right.member(&BaseName::new("user_id"));
        let ctx = left.member(&BaseName::new("ctx"));
        let err_local = left.member(&BaseName::new("err_"));
        let nil = left.member(&BaseName::new("nil"));
        let bootstrap = left.member(&BaseName::new("bootstrap"));
        let type_ = left.member(&BaseName::new("type"));
        let requests = vec![
            request(
                left_value.clone(),
                GoNameKind::Parameter,
                GoVisibility::Exported,
            ),
            request(
                right_value.clone(),
                GoNameKind::Parameter,
                GoVisibility::Exported,
            ),
            request(ctx.clone(), GoNameKind::Parameter, GoVisibility::Exported),
            request(
                err_local.clone(),
                GoNameKind::Parameter,
                GoVisibility::Exported,
            ),
            request(nil.clone(), GoNameKind::Parameter, GoVisibility::Exported),
            request(
                bootstrap.clone(),
                GoNameKind::Parameter,
                GoVisibility::Exported,
            ),
            request(type_.clone(), GoNameKind::Parameter, GoVisibility::Exported),
        ];
        let names = GoNames::new(&generated_package(), requests);

        assert_eq!(
            names
                .project(&left_value, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "userId"
        );
        assert_eq!(
            names
                .project(&left_value, GoNameKind::Parameter, GoVisibility::Exported,)
                .wire(),
            &BamlWireName::Key(BaseName::new("user_id"))
        );
        assert_eq!(
            names
                .project(&right_value, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "userId"
        );
        assert_eq!(
            names
                .project(&ctx, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "ctx"
        );
        assert_eq!(
            names
                .project(&err_local, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "err"
        );
        assert_eq!(
            names
                .project(&nil, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "nil_"
        );
        assert_eq!(
            names
                .project(&bootstrap, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "bootstrap_"
        );
        assert_eq!(
            names
                .project(&type_, GoNameKind::Parameter, GoVisibility::Exported,)
                .identifier(&generated_package())
                .to_string(),
            "type_"
        );
    }

    #[test]
    fn parameters_escape_every_go_keyword() {
        let owner = symbol(&[], "call");
        let keywords = [
            "break",
            "default",
            "func",
            "interface",
            "select",
            "case",
            "defer",
            "go",
            "map",
            "struct",
            "chan",
            "else",
            "goto",
            "package",
            "switch",
            "const",
            "fallthrough",
            "if",
            "range",
            "type",
            "continue",
            "for",
            "import",
            "return",
            "var",
        ];
        let fqns = keywords
            .iter()
            .map(|keyword| owner.member(&BaseName::new(*keyword)))
            .collect::<Vec<_>>();
        let names = GoNames::new(
            &generated_package(),
            fqns.iter()
                .cloned()
                .map(|fqn| request(fqn, GoNameKind::Parameter, GoVisibility::Exported))
                .collect(),
        );

        for (keyword, fqn) in keywords.into_iter().zip(fqns) {
            assert_eq!(
                names
                    .project(&fqn, GoNameKind::Parameter, GoVisibility::Exported)
                    .identifier(&generated_package())
                    .to_string(),
                format!("{keyword}_")
            );
        }
    }

    #[test]
    fn parameters_never_equal_a_generator_owned_identifier() {
        let owner = symbol(&[], "call");
        let fqns = GeneratorIdent::ALL
            .iter()
            .map(|identifier| owner.member(&BaseName::new(identifier.as_str())))
            .collect::<Vec<_>>();
        let names = GoNames::new(
            &generated_package(),
            fqns.iter()
                .cloned()
                .map(|fqn| request(fqn, GoNameKind::Parameter, GoVisibility::Exported))
                .collect(),
        );

        for (identifier, fqn) in GeneratorIdent::ALL.iter().zip(fqns) {
            let projected = names.project(&fqn, GoNameKind::Parameter, GoVisibility::Exported);
            assert_ne!(
                projected.identifier(&generated_package()).to_string(),
                identifier.as_str()
            );
            assert_eq!(
                projected.wire(),
                &BamlWireName::Key(BaseName::new(identifier.as_str()))
            );
        }
    }

    #[test]
    fn collisions_are_typed_and_deterministic_within_a_scope() {
        let function = symbol(&[], "foo_bar");
        let class = symbol(&[], "fooBar");
        let requests = vec![
            request(
                function.clone(),
                GoNameKind::Function,
                GoVisibility::Exported,
            ),
            request(class.clone(), GoNameKind::Class, GoVisibility::Exported),
        ];
        let forward = GoNames::new(&generated_package(), requests.clone());
        let reverse = GoNames::new(&generated_package(), requests.into_iter().rev().collect());

        let function_name =
            forward.project(&function, GoNameKind::Function, GoVisibility::Exported);
        let class_name = forward.project(&class, GoNameKind::Class, GoVisibility::Exported);
        assert!(identifier(function_name).starts_with("FooBar_"));
        assert!(identifier(class_name).starts_with("FooBar_"));
        assert_ne!(identifier(function_name), identifier(class_name));
        assert_eq!(
            function_name,
            reverse.project(&function, GoNameKind::Function, GoVisibility::Exported,)
        );
        assert_eq!(
            class_name,
            reverse.project(&class, GoNameKind::Class, GoVisibility::Exported)
        );
    }

    #[test]
    fn field_collisions_are_local_to_their_class_and_keep_wire_keys() {
        let class = symbol(&[], "record");
        let snake = class.member(&BaseName::new("foo_bar"));
        let camel = class.member(&BaseName::new("fooBar"));
        let names = GoNames::new(
            &generated_package(),
            vec![
                request(snake.clone(), GoNameKind::Field, GoVisibility::Exported),
                request(camel.clone(), GoNameKind::Field, GoVisibility::Exported),
            ],
        );
        let snake_name = names.project(&snake, GoNameKind::Field, GoVisibility::Exported);
        let camel_name = names.project(&camel, GoNameKind::Field, GoVisibility::Exported);

        assert!(identifier(snake_name).starts_with("FooBar_"));
        assert!(identifier(camel_name).starts_with("FooBar_"));
        assert_ne!(identifier(snake_name), identifier(camel_name));
        assert_eq!(
            snake_name.wire(),
            &BamlWireName::Key(BaseName::new("foo_bar"))
        );
        assert_eq!(
            camel_name.wire(),
            &BamlWireName::Key(BaseName::new("fooBar"))
        );
    }
}
