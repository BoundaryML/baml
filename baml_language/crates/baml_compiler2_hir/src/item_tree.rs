//! Position-independent item storage for `compiler2_hir`.
//!
//! `ItemTree` stores minimal item representations keyed by name-based IDs,
//! following the same scheme as `baml_compiler_hir::item_tree`.
//! Items are indexed by name (not source position) for position-independence.

use std::ops::Index;

use baml_base::Name;
use baml_compiler2_ast as ast;
use rustc_hash::FxHashMap;
use text_size::TextRange;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, GeneratorMarker, ItemKind, LetMarker,
    LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker, hash_name,
};

// ── Span-free attribute representation ───────────────────────────────────────

/// A span-free attribute for position-independent storage in the `ItemTree`.
/// Derived from `ast::RawAttribute` with all `TextRange`s stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: Name,
    pub args: Vec<AttributeArg>,
}

/// A span-free attribute argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeArg {
    pub key: Option<Name>,
    pub value: String,
}

impl From<&ast::RawAttribute> for Attribute {
    fn from(raw: &ast::RawAttribute) -> Self {
        Self {
            name: raw.name.clone(),
            args: raw.args.iter().map(AttributeArg::from).collect(),
        }
    }
}

impl From<&ast::RawAttributeArg> for AttributeArg {
    fn from(raw: &ast::RawAttributeArg) -> Self {
        Self {
            key: raw.key.clone(),
            value: raw.value.clone(),
        }
    }
}

// ── Minimal item data structs ────────────────────────────────────────────────

/// Full function data stored in the `ItemTree`.
/// Params and return type are stored for signature queries.
/// Body is stored for body queries (no CST re-parsing needed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: Name,
    /// Generic type parameters (e.g., `["T", "U"]`).
    /// Empty for non-generic functions.
    pub generic_params: Vec<Name>,
    /// Function parameters with optional type annotations and spans.
    pub params: Vec<FunctionParam>,
    /// Function parameter default expression arena.
    pub defaults: ast::FunctionDefaults,
    /// Return type with its source span.
    pub return_type: Option<ast::SpannedTypeExpr>,
    /// Throws contract type with its source span.
    pub throws: Option<ast::SpannedTypeExpr>,
    /// Function body — either an expression or a builtin.
    pub body: Option<ast::FunctionBodyDef>,
    /// Declarative metadata, if this function was declared with declarative syntax.
    pub declarative_meta: Option<ast::DeclarativeMeta>,
    pub origin: ast::FunctionOrigin,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the function.
    pub span: TextRange,
}

/// A function parameter entry in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: Name,
    pub type_expr: Option<ast::SpannedTypeExpr>,
    pub default: Option<DefaultExprRef>,
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultExprRef {
    pub function: LocalItemId<FunctionMarker>,
    pub expr: ast::DefaultExprId,
}

/// A class field stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassField {
    pub name: Name,
    pub type_expr: Option<ast::SpannedTypeExpr>,
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub name: Name,
    /// Generic type parameters (e.g., `["T"]` for `Array<T>`).
    /// Empty for non-generic classes.
    pub generic_params: Vec<Name>,
    /// Fields of the class, in declaration order.
    pub fields: Vec<ClassField>,
    /// Methods defined inside this class, referencing their `Function` entries
    /// in the same `ItemTree`.
    pub methods: Vec<LocalItemId<FunctionMarker>>,
    /// Block-level attributes (@@description, @@alias, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the class declaration.
    pub span: TextRange,
}

/// An enum variant stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,
    /// Field-level attributes (@description, @alias, @skip, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    /// Variants of the enum, in declaration order.
    pub variants: Vec<EnumVariant>,
    /// Block-level attributes (@@description, @@alias, etc.).
    pub attributes: Vec<Attribute>,
    /// Joined `///` doc-comment lines preceding this declaration.
    pub docstring: Option<String>,
    /// Full source span of the enum declaration.
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    /// The type expression on the RHS of the alias, if present.
    pub type_expr: Option<ast::SpannedTypeExpr>,
    /// Full source span of the type alias declaration.
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub name: Name,
    /// Provider name (e.g., "openai", "anthropic", "fallback", "round-robin").
    pub provider: Option<Name>,
    /// Sub-client names for fallback/round-robin clients.
    pub sub_client_names: Vec<Name>,
    /// Retry policy name, if configured.
    pub retry_policy_name: Option<Name>,
    /// Starting index for round-robin clients.
    pub round_robin_start: Option<usize>,
}

/// A test argument value stored in the `ItemTree`.
///
/// Floats are stored as bit patterns (via `f64::to_bits`) to allow `Eq` and `Hash`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestArgValue {
    Null,
    Int(i64),
    /// Float stored as raw bits (`f64::to_bits(value)`).
    FloatBits(u64),
    Bool(bool),
    String(String),
    Array(Vec<TestArgValue>),
    Map(Vec<(String, TestArgValue)>),
}

impl TestArgValue {
    pub fn float(v: f64) -> Self {
        Self::FloatBits(v.to_bits())
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::FloatBits(bits) => Some(f64::from_bits(*bits)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: Name,
    /// The function(s) this test exercises.
    pub function_refs: Vec<Name>,
    /// Test arguments as key-value pairs.
    pub args: Vec<(Name, TestArgValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorConfigItem {
    pub key: Name,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    pub name: Name,
    pub config_items: Vec<GeneratorConfigItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateString {
    pub name: Name,
    /// Template parameters with optional type annotations and spans.
    pub params: Vec<FunctionParam>,
    /// Template body text (Jinja template).
    pub body: Option<String>,
    /// Full source span of the template string declaration.
    pub span: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub name: Name,
    /// Raw string value of `max_retries` (parsed at emit time).
    pub max_retries: Option<String>,
    /// Raw string value of `initial_delay_ms`.
    pub initial_delay_ms: Option<String>,
    /// Raw string value of multiplier.
    pub multiplier: Option<String>,
    /// Raw string value of `max_delay_ms`.
    pub max_delay_ms: Option<String>,
}

/// A top-level let binding stored in the `ItemTree`.
/// Carries the optional initializer `ExprBody` for body queries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Let {
    pub name: Name,
    pub initializer: Option<(ast::ExprBody, ast::AstSourceMap)>,
    pub origin: ast::LetOrigin,
    pub span: TextRange,
    pub name_span: TextRange,
}

// ── ItemTreeSourceMap ─────────────────────────────────────────────────────────

/// Parallel source map for `ItemTree` — stores name spans that are
/// deliberately excluded from the semantic `ItemTree` to avoid polluting
/// Salsa's early-cutoff comparisons with position data.
///
/// Follows the same body/signature source-map pattern used by
/// `function_body` / `function_body_source_map`.
#[derive(Debug, Clone, Default)]
pub struct ItemTreeSourceMap {
    /// `name_span` for each class's fields, parallel to `Class::fields`.
    pub class_field_spans: FxHashMap<LocalItemId<ClassMarker>, Vec<TextRange>>,
    /// `name_span` for each enum's variants, parallel to `Enum::variants`.
    pub enum_variant_spans: FxHashMap<LocalItemId<EnumMarker>, Vec<TextRange>>,
    /// `name_span` for each function.
    pub function_name_spans: FxHashMap<LocalItemId<FunctionMarker>, TextRange>,
    /// Whole-block span for each generator (the `generator … { … }` node).
    pub generator_block_spans: FxHashMap<LocalItemId<GeneratorMarker>, TextRange>,
    /// Per-config-item span for each generator, parallel to
    /// `Generator::config_items`.
    pub generator_config_item_spans: FxHashMap<LocalItemId<GeneratorMarker>, Vec<TextRange>>,
}

// ── ItemTree ─────────────────────────────────────────────────────────────────

/// Position-independent item storage for a single file.
///
/// Items are stored in hash maps keyed by name-based IDs.
/// The `next_index` map tracks the next available collision index
/// per `(ItemKind, hash)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemTree {
    pub functions: FxHashMap<LocalItemId<FunctionMarker>, Function>,
    pub classes: FxHashMap<LocalItemId<ClassMarker>, Class>,
    pub enums: FxHashMap<LocalItemId<EnumMarker>, Enum>,
    pub type_aliases: FxHashMap<LocalItemId<TypeAliasMarker>, TypeAlias>,
    pub clients: FxHashMap<LocalItemId<ClientMarker>, Client>,
    pub tests: FxHashMap<LocalItemId<TestMarker>, Test>,
    pub generators: FxHashMap<LocalItemId<GeneratorMarker>, Generator>,
    pub template_strings: FxHashMap<LocalItemId<TemplateStringMarker>, TemplateString>,
    pub retry_policies: FxHashMap<LocalItemId<RetryPolicyMarker>, RetryPolicy>,
    pub lets: FxHashMap<LocalItemId<LetMarker>, Let>,

    /// Collision tracker: `(ItemKind, hash)` → next available index.
    next_index: FxHashMap<(ItemKind, u16), u16>,
}

impl Default for ItemTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ItemTree {
    pub fn new() -> Self {
        Self {
            functions: FxHashMap::default(),
            classes: FxHashMap::default(),
            enums: FxHashMap::default(),
            type_aliases: FxHashMap::default(),
            clients: FxHashMap::default(),
            tests: FxHashMap::default(),
            generators: FxHashMap::default(),
            template_strings: FxHashMap::default(),
            retry_policies: FxHashMap::default(),
            lets: FxHashMap::default(),
            next_index: FxHashMap::default(),
        }
    }

    /// Allocate a collision-resistant ID for an item.
    fn alloc_id<T>(&mut self, kind: ItemKind, name: &Name) -> LocalItemId<T> {
        let h = hash_name(name);
        let index = self.next_index.entry((kind, h)).or_insert(0);
        let id = LocalItemId::new(h, *index);
        *index += 1;
        id
    }

    /// Allocate a function in the `ItemTree` with full AST data.
    pub fn alloc_function(&mut self, f: &ast::FunctionDef) -> LocalItemId<FunctionMarker> {
        let id = self.alloc_id(ItemKind::Function, &f.name);
        let params = f
            .params
            .iter()
            .map(|p| FunctionParam {
                name: p.name.clone(),
                type_expr: p.type_expr.clone(),
                default: p.default.map(|expr| DefaultExprRef { function: id, expr }),
                span: p.span,
            })
            .collect();
        self.functions.insert(
            id,
            Function {
                name: f.name.clone(),
                generic_params: f.generic_params.clone(),
                params,
                defaults: f.defaults.clone(),
                return_type: f.return_type.clone(),
                throws: f.throws.clone(),
                body: f.body.clone(),
                declarative_meta: f.declarative_meta.clone(),
                origin: f.origin,
                docstring: f.docstring.clone(),
                span: f.span,
            },
        );
        id
    }

    pub fn alloc_class(&mut self, c: &ast::ClassDef) -> LocalItemId<ClassMarker> {
        let id = self.alloc_id(ItemKind::Class, &c.name);
        let fields = c
            .fields
            .iter()
            .map(|f| ClassField {
                name: f.name.clone(),
                type_expr: f.type_expr.clone(),
                attributes: f.attributes.iter().map(Attribute::from).collect(),
                docstring: f.docstring.clone(),
            })
            .collect();
        self.classes.insert(
            id,
            Class {
                name: c.name.clone(),
                generic_params: c.generic_params.clone(),
                fields,
                methods: Vec::new(),
                attributes: c.attributes.iter().map(Attribute::from).collect(),
                docstring: c.docstring.clone(),
                span: c.span,
            },
        );
        id
    }

    /// Attach method IDs to an already-allocated class.
    pub fn set_class_methods(
        &mut self,
        class_id: LocalItemId<ClassMarker>,
        methods: Vec<LocalItemId<FunctionMarker>>,
    ) {
        if let Some(class) = self.classes.get_mut(&class_id) {
            class.methods = methods;
        }
    }

    pub fn alloc_enum(&mut self, e: &ast::EnumDef) -> LocalItemId<EnumMarker> {
        let id = self.alloc_id(ItemKind::Enum, &e.name);
        let variants = e
            .variants
            .iter()
            .map(|v| EnumVariant {
                name: v.name.clone(),
                attributes: v.attributes.iter().map(Attribute::from).collect(),
                docstring: v.docstring.clone(),
            })
            .collect();
        self.enums.insert(
            id,
            Enum {
                name: e.name.clone(),
                variants,
                attributes: e.attributes.iter().map(Attribute::from).collect(),
                docstring: e.docstring.clone(),
                span: e.span,
            },
        );
        id
    }

    /// Populate source map spans for a class that was allocated via `alloc_class`.
    pub fn collect_class_spans(
        source_map: &mut ItemTreeSourceMap,
        id: LocalItemId<ClassMarker>,
        class_def: &ast::ClassDef,
    ) {
        let spans: Vec<TextRange> = class_def.fields.iter().map(|f| f.name_span).collect();
        source_map.class_field_spans.insert(id, spans);
    }

    /// Populate source map name span for a function.
    pub fn collect_function_span(
        source_map: &mut ItemTreeSourceMap,
        id: LocalItemId<FunctionMarker>,
        func_def: &ast::FunctionDef,
    ) {
        source_map
            .function_name_spans
            .insert(id, func_def.name_span);
    }

    /// Populate source map spans for an enum that was allocated via `alloc_enum`.
    pub fn collect_enum_spans(
        source_map: &mut ItemTreeSourceMap,
        id: LocalItemId<EnumMarker>,
        enum_def: &ast::EnumDef,
    ) {
        let spans: Vec<TextRange> = enum_def.variants.iter().map(|v| v.name_span).collect();
        source_map.enum_variant_spans.insert(id, spans);
    }

    pub fn alloc_type_alias(&mut self, ta: &ast::TypeAliasDef) -> LocalItemId<TypeAliasMarker> {
        let id = self.alloc_id(ItemKind::TypeAlias, &ta.name);
        self.type_aliases.insert(
            id,
            TypeAlias {
                name: ta.name.clone(),
                type_expr: ta.type_expr.clone(),
                span: ta.span,
            },
        );
        id
    }

    pub fn alloc_client(&mut self, c: &ast::ClientDef) -> LocalItemId<ClientMarker> {
        let id = self.alloc_id(ItemKind::Client, &c.name);
        let provider = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "provider")
            .map(|item| Name::new(item.value.trim().trim_matches('"')));
        let sub_client_names = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "options")
            .map(|_| Vec::new()) // complex to parse; clients field is more relevant
            .unwrap_or_default();
        let retry_policy_name = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "retry_policy")
            .map(|item| Name::new(item.value.trim().trim_matches('"')));
        self.clients.insert(
            id,
            Client {
                name: c.name.clone(),
                provider,
                sub_client_names,
                retry_policy_name,
                round_robin_start: None,
            },
        );
        id
    }

    pub fn alloc_test(&mut self, t: &ast::TestDef) -> LocalItemId<TestMarker> {
        let id = self.alloc_id(ItemKind::Test, &t.name);
        // Extract function_refs from config_items (key "functions" or "function")
        let function_refs = t
            .config_items
            .iter()
            .filter(|item| item.key.as_str() == "functions" || item.key.as_str() == "function")
            .flat_map(|item| {
                // Values may be comma-separated or a single name
                item.value
                    .split(',')
                    .map(|s| Name::new(s.trim().trim_matches('"')))
                    .collect::<Vec<_>>()
            })
            .collect();
        // Args come from config_items with key "args" — store raw; complex parsing skipped
        let args = Vec::new();
        self.tests.insert(
            id,
            Test {
                name: t.name.clone(),
                function_refs,
                args,
            },
        );
        id
    }

    pub fn alloc_generator(&mut self, g: &ast::GeneratorDef) -> LocalItemId<GeneratorMarker> {
        let id = self.alloc_id(ItemKind::Generator, &g.name);
        let config_items = g
            .config_items
            .iter()
            .map(|item| GeneratorConfigItem {
                key: item.key.clone(),
                value: item.value.clone(),
            })
            .collect();
        self.generators.insert(
            id,
            Generator {
                name: g.name.clone(),
                config_items,
            },
        );
        id
    }

    /// Populate source map spans for a generator that was allocated via
    /// `alloc_generator`. Mirrors `collect_class_spans` / `collect_enum_spans`.
    pub fn collect_generator_spans(
        source_map: &mut ItemTreeSourceMap,
        id: LocalItemId<GeneratorMarker>,
        gen_def: &ast::GeneratorDef,
    ) {
        source_map.generator_block_spans.insert(id, gen_def.span);
        let item_spans: Vec<TextRange> =
            gen_def.config_items.iter().map(|item| item.span).collect();
        source_map
            .generator_config_item_spans
            .insert(id, item_spans);
    }

    pub fn alloc_template_string(
        &mut self,
        ts: &ast::TemplateStringDef,
    ) -> LocalItemId<TemplateStringMarker> {
        let id = self.alloc_id(ItemKind::TemplateString, &ts.name);
        let params = ts
            .params
            .iter()
            .map(|p| FunctionParam {
                name: p.name.clone(),
                type_expr: p.type_expr.clone(),
                default: None,
                span: p.span,
            })
            .collect();
        let body = ts.body.as_ref().map(|b| b.text.clone());
        self.template_strings.insert(
            id,
            TemplateString {
                name: ts.name.clone(),
                params,
                body,
                span: ts.span,
            },
        );
        id
    }

    pub fn alloc_retry_policy(
        &mut self,
        rp: &ast::RetryPolicyDef,
    ) -> LocalItemId<RetryPolicyMarker> {
        let id = self.alloc_id(ItemKind::RetryPolicy, &rp.name);
        let get_field = |key: &str| -> Option<String> {
            rp.config_items
                .iter()
                .find(|item| item.key.as_str() == key)
                .map(|item| item.value.trim().to_string())
        };
        self.retry_policies.insert(
            id,
            RetryPolicy {
                name: rp.name.clone(),
                max_retries: get_field("max_retries"),
                initial_delay_ms: get_field("initial_delay_ms"),
                multiplier: get_field("multiplier"),
                max_delay_ms: get_field("max_delay_ms"),
            },
        );
        id
    }

    pub fn alloc_let(&mut self, l: &ast::LetDef) -> LocalItemId<LetMarker> {
        let id = self.alloc_id(ItemKind::Let, &l.name);
        self.lets.insert(
            id,
            Let {
                name: l.name.clone(),
                initializer: l.initializer.clone(),
                origin: l.origin,
                span: l.span,
                name_span: l.name_span,
            },
        );
        id
    }
}

// ── Index impls ───────────────────────────────────────────────────────────────

impl Index<LocalItemId<FunctionMarker>> for ItemTree {
    type Output = Function;
    fn index(&self, id: LocalItemId<FunctionMarker>) -> &Function {
        &self.functions[&id]
    }
}

impl Index<LocalItemId<ClassMarker>> for ItemTree {
    type Output = Class;
    fn index(&self, id: LocalItemId<ClassMarker>) -> &Class {
        &self.classes[&id]
    }
}

impl Index<LocalItemId<EnumMarker>> for ItemTree {
    type Output = Enum;
    fn index(&self, id: LocalItemId<EnumMarker>) -> &Enum {
        &self.enums[&id]
    }
}

impl Index<LocalItemId<TypeAliasMarker>> for ItemTree {
    type Output = TypeAlias;
    fn index(&self, id: LocalItemId<TypeAliasMarker>) -> &TypeAlias {
        &self.type_aliases[&id]
    }
}

impl Index<LocalItemId<ClientMarker>> for ItemTree {
    type Output = Client;
    fn index(&self, id: LocalItemId<ClientMarker>) -> &Client {
        &self.clients[&id]
    }
}

impl Index<LocalItemId<TestMarker>> for ItemTree {
    type Output = Test;
    fn index(&self, id: LocalItemId<TestMarker>) -> &Test {
        &self.tests[&id]
    }
}

impl Index<LocalItemId<GeneratorMarker>> for ItemTree {
    type Output = Generator;
    fn index(&self, id: LocalItemId<GeneratorMarker>) -> &Generator {
        &self.generators[&id]
    }
}

impl Index<LocalItemId<TemplateStringMarker>> for ItemTree {
    type Output = TemplateString;
    fn index(&self, id: LocalItemId<TemplateStringMarker>) -> &TemplateString {
        &self.template_strings[&id]
    }
}

impl Index<LocalItemId<RetryPolicyMarker>> for ItemTree {
    type Output = RetryPolicy;
    fn index(&self, id: LocalItemId<RetryPolicyMarker>) -> &RetryPolicy {
        &self.retry_policies[&id]
    }
}

impl Index<LocalItemId<LetMarker>> for ItemTree {
    type Output = Let;
    fn index(&self, id: LocalItemId<LetMarker>) -> &Let {
        &self.lets[&id]
    }
}
