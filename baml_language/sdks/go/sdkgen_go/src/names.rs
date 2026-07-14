//! Typed projection from BAML identities to Go identifiers.
//!
//! Name allocation is deliberately separate from rendering. The allocator
//! consumes typed FQNs, declaration kinds, and visibility, and produces an
//! opaque [`GoName`]. Raw strings are exposed only when source text is emitted.

use std::collections::{BTreeMap, HashMap, HashSet};

use baml_codegen_types::{Name, Symbol, SymbolPool};

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
    Class,
    Enum,
    TypeAlias,
    Parameter,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum GoVisibility {
    Exported,
    Unexported,
}

/// A validated Go identifier. Its textual representation is intentionally
/// available only through an explicit render-boundary method.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GoName(Box<str>);

impl GoName {
    fn new(value: String) -> Self {
        assert!(
            is_go_identifier(&value),
            "invalid projected Go name: {value}"
        );
        assert!(
            !is_go_keyword(&value),
            "Go keyword escaped too late: {value}"
        );
        Self(value.into_boxed_str())
    }

    fn with_suffix(&self, suffix: &str) -> Self {
        Self::new(format!("{}_{suffix}", self.0))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
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
            GoNameKind::Function | GoNameKind::Class | GoNameKind::Enum | GoNameKind::TypeAlias => {
                NameScope::Package(self.fqn.symbol.pkg.clone())
            }
            GoNameKind::Parameter => NameScope::Owner(
                self.fqn
                    .parent()
                    .expect("parameter FQN must include its owning callable"),
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum NameScope {
    Package(baml_base::Name),
    Owner(BamlFqn),
}

pub(crate) struct GoNames {
    allocations: HashMap<NameRequest, GoName>,
}

impl GoNames {
    /// Build the name table for the generated `user` package. All top-level
    /// symbols reserve their package-scope identifiers even when their codegen
    /// feature has not been implemented yet.
    pub(crate) fn for_user_package(pool: &SymbolPool) -> Self {
        let mut requests = Vec::new();
        for (name, symbol) in pool {
            if name.pkg.as_str() != "user" {
                continue;
            }

            let fqn = BamlFqn::symbol(name);
            let kind = match symbol {
                Symbol::Function(_) => GoNameKind::Function,
                Symbol::Class(_) => GoNameKind::Class,
                Symbol::Enum(_) => GoNameKind::Enum,
                Symbol::TypeAlias(_) => GoNameKind::TypeAlias,
            };
            requests.push(NameRequest::new(fqn.clone(), kind, GoVisibility::Exported));

            if let Symbol::Function(function) = symbol {
                requests.extend(function.arguments.iter().map(|argument| {
                    NameRequest::new(
                        fqn.member(&argument.name),
                        GoNameKind::Parameter,
                        GoVisibility::Unexported,
                    )
                }));
            }
        }
        Self::new(requests)
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

    fn new(requests: Vec<NameRequest>) -> Self {
        let mut groups = BTreeMap::<NameScope, BTreeMap<GoName, Vec<NameRequest>>>::new();
        for request in requests {
            let scope = request.scope();
            let base = project_base(&request);
            groups
                .entry(scope)
                .or_default()
                .entry(base)
                .or_default()
                .push(request);
        }

        let mut allocations = HashMap::new();
        for base_groups in groups.values_mut() {
            let mut used = HashSet::new();
            for (base, requests) in base_groups {
                requests.sort();
                let collides = requests.len() > 1;
                for request in requests {
                    let candidate = if collides {
                        base.with_suffix(&short_hash(request))
                    } else {
                        base.clone()
                    };
                    let allocated = allocate_unique(candidate, request, &mut used);
                    allocations.insert(request.clone(), allocated);
                }
            }
        }
        Self { allocations }
    }
}

fn project_base(request: &NameRequest) -> GoName {
    let mut value = String::new();
    match request.kind {
        GoNameKind::Function | GoNameKind::Class | GoNameKind::Enum | GoNameKind::TypeAlias => {
            for segment in &request.fqn.symbol.namespace_path {
                push_upper_component(&mut value, segment);
            }
            push_upper_component(&mut value, &request.fqn.symbol.name);
        }
        GoNameKind::Parameter => push_upper_component(&mut value, request.fqn.leaf()),
    }

    if value.is_empty() {
        value.push_str("BamlSymbol");
    } else if value.starts_with(|ch: char| ch.is_ascii_digit()) {
        value.insert_str(0, "Baml");
    }
    if matches!(request.visibility, GoVisibility::Unexported) {
        lowercase_first(&mut value);
    }

    while is_go_keyword(&value) || is_generator_local(request.kind, &value) {
        value.push('_');
    }
    GoName::new(value)
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

fn is_generator_local(kind: GoNameKind, value: &str) -> bool {
    matches!(kind, GoNameKind::Parameter) && matches!(value, "ctx" | "err" | "result" | "zero")
}

fn allocate_unique(candidate: GoName, request: &NameRequest, used: &mut HashSet<GoName>) -> GoName {
    if used.insert(candidate.clone()) {
        return candidate;
    }

    let hashed = candidate.with_suffix(&short_hash(request));
    if used.insert(hashed.clone()) {
        return hashed;
    }

    for suffix in 2.. {
        let numbered = hashed.with_suffix(&suffix.to_string());
        if used.insert(numbered.clone()) {
            return numbered;
        }
    }
    unreachable!()
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
    });
    hash.byte(match request.visibility {
        GoVisibility::Exported => 0,
        GoVisibility::Unexported => 1,
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

fn is_go_keyword(value: &str) -> bool {
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
            cases
                .iter()
                .map(|(fqn, kind, visibility, _)| request(fqn.clone(), *kind, *visibility))
                .collect(),
        );

        for (fqn, kind, visibility, expected) in cases {
            assert_eq!(names.project(&fqn, kind, visibility).as_str(), expected);
        }
    }

    #[test]
    fn parameters_use_their_own_scope_and_escape_generator_locals() {
        let left = symbol(&["left"], "call");
        let right = symbol(&["right"], "call");
        let left_value = left.member(&BaseName::new("user_id"));
        let right_value = right.member(&BaseName::new("user_id"));
        let ctx = left.member(&BaseName::new("ctx"));
        let type_ = left.member(&BaseName::new("type"));
        let requests = vec![
            request(
                left_value.clone(),
                GoNameKind::Parameter,
                GoVisibility::Unexported,
            ),
            request(
                right_value.clone(),
                GoNameKind::Parameter,
                GoVisibility::Unexported,
            ),
            request(ctx.clone(), GoNameKind::Parameter, GoVisibility::Unexported),
            request(
                type_.clone(),
                GoNameKind::Parameter,
                GoVisibility::Unexported,
            ),
        ];
        let names = GoNames::new(requests);

        assert_eq!(
            names
                .project(&left_value, GoNameKind::Parameter, GoVisibility::Unexported,)
                .as_str(),
            "userId"
        );
        assert_eq!(
            names
                .project(
                    &right_value,
                    GoNameKind::Parameter,
                    GoVisibility::Unexported,
                )
                .as_str(),
            "userId"
        );
        assert_eq!(
            names
                .project(&ctx, GoNameKind::Parameter, GoVisibility::Unexported,)
                .as_str(),
            "ctx_"
        );
        assert_eq!(
            names
                .project(&type_, GoNameKind::Parameter, GoVisibility::Unexported,)
                .as_str(),
            "type_"
        );
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
        let forward = GoNames::new(requests.clone());
        let reverse = GoNames::new(requests.into_iter().rev().collect());

        let function_name =
            forward.project(&function, GoNameKind::Function, GoVisibility::Exported);
        let class_name = forward.project(&class, GoNameKind::Class, GoVisibility::Exported);
        assert!(function_name.as_str().starts_with("FooBar_"));
        assert!(class_name.as_str().starts_with("FooBar_"));
        assert_ne!(function_name, class_name);
        assert_eq!(
            function_name,
            reverse.project(&function, GoNameKind::Function, GoVisibility::Exported,)
        );
        assert_eq!(
            class_name,
            reverse.project(&class, GoNameKind::Class, GoVisibility::Exported)
        );
    }
}
