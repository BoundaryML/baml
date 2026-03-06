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
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, GeneratorMarker, ItemKind, LocalItemId,
    RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker, hash_name,
};

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
    /// Return type with its source span.
    pub return_type: Option<ast::SpannedTypeExpr>,
    /// Throws contract type with its source span.
    pub throws: Option<ast::SpannedTypeExpr>,
    /// Function body — either LLM or expression.
    pub body: Option<ast::FunctionBodyDef>,
    /// Full source span of the function.
    pub span: TextRange,
}

/// A function parameter entry in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParam {
    pub name: Name,
    pub type_expr: Option<ast::SpannedTypeExpr>,
    pub span: TextRange,
}

/// A class field stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassField {
    pub name: Name,
    pub type_expr: Option<ast::SpannedTypeExpr>,
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
}

/// An enum variant stored in the `ItemTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enum {
    pub name: Name,
    /// Variants of the enum, in declaration order.
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAlias {
    pub name: Name,
    /// The type expression on the RHS of the alias, if present.
    pub type_expr: Option<ast::SpannedTypeExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generator {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateString {
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub name: Name,
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
                span: p.span,
            })
            .collect();
        self.functions.insert(
            id,
            Function {
                name: f.name.clone(),
                generic_params: f.generic_params.clone(),
                params,
                return_type: f.return_type.clone(),
                throws: f.throws.clone(),
                body: f.body.clone(),
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
            })
            .collect();
        self.classes.insert(
            id,
            Class {
                name: c.name.clone(),
                generic_params: c.generic_params.clone(),
                fields,
                methods: Vec::new(),
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
            })
            .collect();
        self.enums.insert(
            id,
            Enum {
                name: e.name.clone(),
                variants,
            },
        );
        id
    }

    pub fn alloc_type_alias(&mut self, ta: &ast::TypeAliasDef) -> LocalItemId<TypeAliasMarker> {
        let id = self.alloc_id(ItemKind::TypeAlias, &ta.name);
        self.type_aliases.insert(
            id,
            TypeAlias {
                name: ta.name.clone(),
                type_expr: ta.type_expr.clone(),
            },
        );
        id
    }

    pub fn alloc_client(&mut self, name: &Name) -> LocalItemId<ClientMarker> {
        let id = self.alloc_id(ItemKind::Client, name);
        self.clients.insert(id, Client { name: name.clone() });
        id
    }

    pub fn alloc_test(&mut self, name: &Name) -> LocalItemId<TestMarker> {
        let id = self.alloc_id(ItemKind::Test, name);
        self.tests.insert(id, Test { name: name.clone() });
        id
    }

    pub fn alloc_generator(&mut self, name: &Name) -> LocalItemId<GeneratorMarker> {
        let id = self.alloc_id(ItemKind::Generator, name);
        self.generators.insert(id, Generator { name: name.clone() });
        id
    }

    pub fn alloc_template_string(&mut self, name: &Name) -> LocalItemId<TemplateStringMarker> {
        let id = self.alloc_id(ItemKind::TemplateString, name);
        self.template_strings
            .insert(id, TemplateString { name: name.clone() });
        id
    }

    pub fn alloc_retry_policy(&mut self, name: &Name) -> LocalItemId<RetryPolicyMarker> {
        let id = self.alloc_id(ItemKind::RetryPolicy, name);
        self.retry_policies
            .insert(id, RetryPolicy { name: name.clone() });
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
