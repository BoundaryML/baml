//! Position-independent item storage for `compiler2_hir`.
//!
//! `ItemTree` stores minimal item representations keyed by name-based IDs,
//! following the same scheme as `baml_compiler_hir::item_tree`.
//! Items are indexed by name (not source position) for position-independence.

mod classes;
mod clients;
mod common;
mod enums;
mod functions;
mod interfaces;
mod lets;
mod retry_policies;
mod source_map;
mod template_strings;
mod test_items;
mod type_aliases;

use std::ops::Index;

use baml_base::Name;
use baml_compiler2_ast as ast;
pub use classes::*;
pub use clients::*;
pub use common::*;
pub use enums::*;
pub use functions::*;
pub use interfaces::*;
pub use lets::*;
pub use retry_policies::*;
use rustc_hash::FxHashMap;
pub use source_map::*;
pub use template_strings::*;
pub use test_items::*;
use text_size::TextRange;
pub use type_aliases::*;

use crate::ids::{
    ClassMarker, ClientMarker, EnumMarker, FunctionMarker, ImplMarker, InterfaceMarker, ItemKind,
    LetMarker, LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker, TypeAliasMarker,
    hash_impl_key, hash_name,
};

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
    pub interfaces: FxHashMap<LocalItemId<InterfaceMarker>, Interface>,
    pub type_aliases: FxHashMap<LocalItemId<TypeAliasMarker>, TypeAlias>,
    pub clients: FxHashMap<LocalItemId<ClientMarker>, Client>,
    pub tests: FxHashMap<LocalItemId<TestMarker>, Test>,
    pub template_strings: FxHashMap<LocalItemId<TemplateStringMarker>, TemplateString>,
    pub retry_policies: FxHashMap<LocalItemId<RetryPolicyMarker>, RetryPolicy>,
    pub lets: FxHashMap<LocalItemId<LetMarker>, Let>,

    /// Unified store for every `implements` block (both in-body and
    /// out-of-body), keyed by a stable `ImplMarker` id. Downstream queries
    /// (`impl_data`) read this map; `class_to_impls` / `free_impls` index it.
    pub impls: FxHashMap<LocalItemId<ImplMarker>, ImplBlock>,
    /// Index from a class to the impls whose subject is that class
    /// (`ImplSubject::InClass`), in source order. Lets "impls for class C" be
    /// answered without a scan; parallel to `Class::implements`.
    pub class_to_impls: FxHashMap<LocalItemId<ClassMarker>, Vec<LocalItemId<ImplMarker>>>,
    /// Out-of-body (`ImplSubject::Free`) impl ids in source order. Gives consumers a
    /// deterministic iteration order over free impls (the unified `impls` map is unordered) —
    /// e.g. resolving the enclosing out-of-body impl of a method.
    pub free_impls: Vec<LocalItemId<ImplMarker>>,

    /// BEP-044: for a class method declared inside an `implements I {}`
    /// block, record the unresolved interface target path. Empty for
    /// methods declared at the class level (not inside any `implements`
    /// block) and for interface default-methods themselves. Consumers
    /// resolve the path to an `InterfaceLoc` lazily so HIR construction
    /// stays independent of name resolution.
    pub method_to_iface_target: FxHashMap<LocalItemId<FunctionMarker>, ast::TypeExpr>,
    pub method_to_iface_associated_type_bindings:
        FxHashMap<LocalItemId<FunctionMarker>, Vec<ast::AssociatedTypeBindingDef>>,

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
            interfaces: FxHashMap::default(),
            type_aliases: FxHashMap::default(),
            clients: FxHashMap::default(),
            tests: FxHashMap::default(),
            template_strings: FxHashMap::default(),
            retry_policies: FxHashMap::default(),
            lets: FxHashMap::default(),
            impls: FxHashMap::default(),
            class_to_impls: FxHashMap::default(),
            free_impls: Vec::new(),
            method_to_iface_target: FxHashMap::default(),
            method_to_iface_associated_type_bindings: FxHashMap::default(),
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
                generic_param_bounds: f.generic_param_bounds.clone(),
                params,
                defaults: f.defaults.clone(),
                return_type: f.return_type.clone(),
                throws: f.throws.clone(),
                body: f.body.clone(),
                declarative_meta: f.declarative_meta.clone(),
                origin: f.origin,
                docstring: f.docstring.clone(),
                is_tagged_template_tag: f.is_tagged_template_tag,
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
        let implements = c
            .implements
            .iter()
            .map(|b| ImplementsBlock {
                target: b.target.clone(),
                field_links: b
                    .field_links
                    .iter()
                    .map(InterfaceFieldLink::from_ast)
                    .collect(),
                associated_type_bindings: b.associated_type_bindings.clone(),
                is_out_of_body: b.is_out_of_body,
                span: b.span,
            })
            .collect();
        self.classes.insert(
            id,
            Class {
                name: c.name.clone(),
                generic_params: c.generic_params.clone(),
                generic_param_bounds: c.generic_param_bounds.clone(),
                fields,
                methods: Vec::new(),
                implements,
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

    /// Allocate a stable id for an `implements` block and store it in the
    /// unified `impls` map. `iface_head`/`for_head` seed the position-independent
    /// id (impls have no declared name); the collision index disambiguates impls
    /// that share both heads. Records the `class_to_impls` edge for `InClass`.
    pub fn alloc_impl(
        &mut self,
        iface_head: &Name,
        for_head: &Name,
        block: ImplBlock,
    ) -> LocalItemId<ImplMarker> {
        let h = hash_impl_key(iface_head, for_head);
        let index = self.next_index.entry((ItemKind::Impl, h)).or_insert(0);
        let id = LocalItemId::new(h, *index);
        *index += 1;
        match &block.subject {
            ImplSubject::InClass { class, .. } => {
                self.class_to_impls.entry(*class).or_default().push(id);
            }
            ImplSubject::Free { .. } => self.free_impls.push(id),
        }
        self.impls.insert(id, block);
        id
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

    /// Populate source map spans for an interface allocated via `alloc_interface`.
    pub fn collect_interface_spans(
        source_map: &mut ItemTreeSourceMap,
        id: LocalItemId<InterfaceMarker>,
        iface_def: &ast::InterfaceDef,
    ) {
        let field_spans: Vec<TextRange> = iface_def.fields.iter().map(|f| f.name_span).collect();
        source_map.interface_field_spans.insert(id, field_spans);
        let method_spans: Vec<TextRange> = iface_def
            .required_methods
            .iter()
            .map(|m| m.name_span)
            .collect();
        source_map.interface_method_spans.insert(id, method_spans);
    }

    /// Allocate an interface (BEP-044) in the `ItemTree`.
    ///
    /// `default_method_ids` are the `FunctionMarker` ids for any default
    /// methods in the interface — those should be allocated separately via
    /// `alloc_function` before this is called.
    pub fn alloc_interface(
        &mut self,
        i: &ast::InterfaceDef,
        default_method_ids: Vec<LocalItemId<FunctionMarker>>,
    ) -> LocalItemId<InterfaceMarker> {
        let id = self.alloc_id(ItemKind::Interface, &i.name);
        let fields = i
            .fields
            .iter()
            .map(|f| ClassField {
                name: f.name.clone(),
                type_expr: f.type_expr.clone(),
                attributes: f.attributes.iter().map(Attribute::from).collect(),
                docstring: f.docstring.clone(),
            })
            .collect();
        let required_methods = i
            .required_methods
            .iter()
            .map(|m| InterfaceMethodSig {
                name: m.name.clone(),
                generic_params: m.generic_params.clone(),
                generic_param_bounds: m.generic_param_bounds.clone(),
                params: m
                    .params
                    .iter()
                    .map(|p| FunctionParam {
                        name: p.name.clone(),
                        type_expr: p.type_expr.clone(),
                        default: None,
                        span: p.span,
                    })
                    .collect(),
                return_type: m.return_type.clone(),
                throws: m.throws.clone(),
                attributes: m.attributes.iter().map(Attribute::from).collect(),
                docstring: m.docstring.clone(),
                span: m.span,
            })
            .collect();
        self.interfaces.insert(
            id,
            Interface {
                name: i.name.clone(),
                generic_params: i.generic_params.clone(),
                generic_param_bounds: i.generic_param_bounds.clone(),
                requires: i.requires.clone(),
                fields,
                associated_types: i.associated_types.clone(),
                default_methods: default_method_ids,
                required_methods,
                attributes: i.attributes.iter().map(Attribute::from).collect(),
                docstring: i.docstring.clone(),
                span: i.span,
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
        self.tests.insert(
            id,
            Test {
                name: t.name.clone(),
                function_refs: t.function_refs.clone(),
                args: t.args.clone(),
            },
        );
        id
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

impl Index<LocalItemId<InterfaceMarker>> for ItemTree {
    type Output = Interface;
    fn index(&self, id: LocalItemId<InterfaceMarker>) -> &Interface {
        &self.interfaces[&id]
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
