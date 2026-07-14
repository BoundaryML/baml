//! Typed projection from BAML identities to Go identifiers.
//!
//! Name allocation is deliberately separate from rendering. The allocator
//! consumes typed FQNs, declaration kinds, and visibility, and produces an
//! opaque [`GoName`]. Raw strings are exposed only when source text is emitted.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
};

use baml_codegen_types::{Name, Symbol, SymbolPool};

use crate::{packages::GoPackages, rendering::GeneratorIdent};

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
        self.members.last().unwrap_or(&self.symbol.name)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoNameKind {
    Function,
    FunctionOptionType,
    FunctionOptionSetter,
    Class,
    Enum,
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
    Key(baml_base::Name),
}

impl fmt::Display for BamlWireName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symbol(name) => write!(f, "{name}"),
            Self::Key(name) => write!(f, "{name}"),
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
            | GoNameKind::Class
            | GoNameKind::Enum
            | GoNameKind::TypeAlias => NameScope::Package(self.fqn.symbol.pkg.clone()),
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
        let reserved = match self {
            Self::Function(_) => GeneratorIdent::FUNCTION_SCOPE,
            Self::Package(_) | Self::Class(_) => &[],
        };
        while reserved
            .iter()
            .any(|identifier| identifier.as_str() == candidate.name.as_ref())
            || matches!(self, Self::Function(_))
                && generated_package_aliases.contains(candidate.name.as_ref())
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
}

impl GoNames {
    /// Build one name table for every generated package. All declarations
    /// reserve names even when their codegen feature has not been implemented.
    pub(crate) fn for_pool(pool: &SymbolPool, packages: &GoPackages) -> Self {
        let mut requests = Vec::new();
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
            }
        }
        let generated_package_aliases = packages
            .iter()
            .map(|package| package.go_name().as_str().to_string())
            .collect();
        Self::allocate(
            requests,
            |request| packages.get(&request.fqn.symbol.pkg).go_name().clone(),
            &generated_package_aliases,
        )
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

    #[cfg(test)]
    fn new(package: &GoPackageName, requests: Vec<NameRequest>) -> Self {
        Self::allocate(requests, |_| package.clone(), &BTreeSet::default())
    }

    fn allocate(
        requests: Vec<NameRequest>,
        package_for: impl Fn(&NameRequest) -> GoPackageName,
        generated_package_aliases: &BTreeSet<String>,
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
                            wire: wire_name(request),
                        },
                    );
                }
            }
        }
        Self { allocations }
    }
}

fn project_base(package: GoPackageName, request: &NameRequest) -> GoIdent {
    let mut value = String::new();
    match request.kind {
        GoNameKind::Function | GoNameKind::Class | GoNameKind::Enum | GoNameKind::TypeAlias => {
            for segment in &request.fqn.symbol.namespace_path {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, &request.fqn.symbol.name);
        }
        GoNameKind::FunctionOptionType => {
            for segment in &request.fqn.symbol.namespace_path {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, &request.fqn.symbol.name);
            value.push_str("Option");
        }
        GoNameKind::FunctionOptionSetter => {
            value.push_str("With");
            for segment in &request.fqn.symbol.namespace_path {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, &request.fqn.symbol.name);
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
        GoNameKind::FunctionOptionSetter | GoNameKind::Parameter | GoNameKind::Field => {
            BamlWireName::Key(request.fqn.leaf().clone())
        }
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
    hash.component(request.fqn.symbol.pkg.as_str());
    hash.usize(request.fqn.symbol.namespace_path.len());
    for segment in &request.fqn.symbol.namespace_path {
        hash.component(segment.as_str());
    }
    hash.component(request.fqn.symbol.name.as_str());
    hash.usize(request.fqn.members.len());
    for member in &request.fqn.members {
        hash.component(member.as_str());
    }
    hash.byte(match request.kind {
        GoNameKind::Function => 0,
        GoNameKind::Class => 1,
        GoNameKind::Enum => 2,
        GoNameKind::TypeAlias => 3,
        GoNameKind::Parameter => 4,
        GoNameKind::Field => 5,
        GoNameKind::FunctionOptionType => 6,
        GoNameKind::FunctionOptionSetter => 7,
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
    fn parameters_use_their_own_scope_and_escape_generator_locals() {
        let left = symbol(&["left"], "call");
        let right = symbol(&["right"], "call");
        let left_value = left.member(&BaseName::new("user_id"));
        let right_value = right.member(&BaseName::new("user_id"));
        let ctx = left.member(&BaseName::new("ctx"));
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
            "ctx_"
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
        let fqns = GeneratorIdent::FUNCTION_SCOPE
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

        for (identifier, fqn) in GeneratorIdent::FUNCTION_SCOPE.iter().zip(fqns) {
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
