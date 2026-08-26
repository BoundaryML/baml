//! Construction-time state for the `ItemTree`.
//!
//! `next_index` is the collision counter used while handing out `LocalItemId`s.
//! It is pure construction state — it has no meaning once the tree is built, and
//! leaving it on `ItemTree` would put mutable bookkeeping inside an immutable
//! value that derives `PartialEq`. It lives here instead.
//!
//! The builder also owns the `ItemTreeSourceMap`, so allocating an item and
//! recording its spans is a single call and the two cannot drift apart.

use baml_base::Name;
use baml_compiler2_ast as ast;
use rustc_hash::FxHashMap;

use crate::{
    ids::{
        ClassMarker, ClientMarker, EnumMarker, FunctionMarker, ImplMarker, InterfaceMarker,
        ItemKind, LetMarker, LocalItemId, RetryPolicyMarker, TemplateStringMarker, TestMarker,
        TypeAliasMarker, hash_impl_key, hash_name,
    },
    item_tree::{
        Attribute, Class, ClassField, Client, DefaultExprRef, Enum, EnumVariant, Function,
        FunctionParam, ImplBlock, ImplSubject, ImplementsBlock, Interface, InterfaceFieldLink,
        ItemSpans, ItemTree, ItemTreeSourceMap, Let, MethodOwner, RetryPolicy, TemplateString,
        Test, TypeAlias,
    },
};

/// Builds an [`ItemTree`] and its [`ItemTreeSourceMap`] together.
#[derive(Debug, Default)]
pub struct ItemTreeBuilder {
    tree: ItemTree,
    source_map: ItemTreeSourceMap,
    /// Collision tracker: `(ItemKind, hash)` -> next available index.
    next_index: FxHashMap<(ItemKind, u16), u16>,
}

impl ItemTreeBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume the builder, dropping `next_index` — it is meaningless once the
    /// tree is complete.
    pub fn finish(self) -> (ItemTree, ItemTreeSourceMap) {
        (self.tree, self.source_map)
    }

    /// Allocate a collision-resistant ID for an item.
    fn alloc_id<T>(&mut self, kind: ItemKind, name: &Name) -> LocalItemId<T> {
        let h = hash_name(name);
        let index = self.next_index.entry((kind, h)).or_insert(0);
        let id = LocalItemId::new(h, *index);
        *index += 1;
        id
    }

    /// Allocate a function, recording its name span in the source map.
    pub fn alloc_function(&mut self, f: &ast::FunctionDef) -> LocalItemId<FunctionMarker> {
        let id = self.alloc_id(ItemKind::Function, &f.name);
        self.source_map.function_name_spans.insert(id, f.name_span);
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
        self.tree.functions.insert(
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
                metadata: f.metadata,
                docstring: f.docstring.clone(),
                is_tagged_template_tag: f.is_tagged_template_tag,
                span: f.span,
            },
        );
        id
    }

    /// Allocate a class, recording its field name spans in the source map.
    pub fn alloc_class(&mut self, c: &ast::ClassDef) -> LocalItemId<ClassMarker> {
        let id = self.alloc_id(ItemKind::Class, &c.name);
        self.source_map.class_name_spans.insert(id, c.name_span);
        self.source_map
            .class_field_spans
            .insert(id, c.fields.iter().map(|f| f.name_span).collect());
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
        self.tree.classes.insert(
            id,
            Class {
                name: c.name.clone(),
                generic_params: c.generic_params.clone(),
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

    /// Record the `implements I { … }` block a method was declared under, so
    /// `default.<name>()` calls in its body can resolve back to the interface.
    /// Both halves are written together — they are always populated as a pair.
    pub fn record_method_interface_target(
        &mut self,
        method: LocalItemId<FunctionMarker>,
        target: ast::TypeExpr,
        associated_type_bindings: Vec<ast::AssociatedTypeBindingDef>,
    ) {
        self.tree.method_to_iface_target.insert(method, target);
        self.tree
            .method_to_iface_associated_type_bindings
            .insert(method, associated_type_bindings);
    }

    /// Attach method IDs to an already-allocated class, recording each method's
    /// owner. Membership and ownership are written by the same call so they
    /// cannot drift apart.
    pub fn set_class_methods(
        &mut self,
        class_id: LocalItemId<ClassMarker>,
        methods: Vec<LocalItemId<FunctionMarker>>,
    ) {
        for method in &methods {
            self.record_method_owner(*method, MethodOwner::Class(class_id));
        }
        if let Some(class) = self.tree.classes.get_mut(&class_id) {
            class.methods = methods;
        }
    }

    /// A method belongs to exactly one item; a second recording is a builder bug
    /// (e.g. one function id handed to two owners).
    fn record_method_owner(&mut self, method: LocalItemId<FunctionMarker>, owner: MethodOwner) {
        let previous = self.tree.method_owners.insert(method, owner);
        debug_assert!(
            previous.is_none(),
            "method owner recorded twice: {previous:?} then {owner:?}"
        );
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
                self.tree.class_to_impls.entry(*class).or_default().push(id);
            }
            ImplSubject::Free { .. } => {
                self.tree.free_impls.push(id);
            }
        }
        // A method belongs to its impl block regardless of where the block
        // is written — the in-class spelling is pure syntax (TYPE_SYSTEM.md:
        // "the implementation should use a unified path for both forms").
        for method in &block.methods {
            self.record_method_owner(*method, MethodOwner::Impl(id));
        }
        self.tree.impls.insert(id, block);
        id
    }

    /// Allocate an enum, recording its variant name spans in the source map.
    pub fn alloc_enum(&mut self, e: &ast::EnumDef) -> LocalItemId<EnumMarker> {
        let id = self.alloc_id(ItemKind::Enum, &e.name);
        self.source_map.enum_name_spans.insert(id, e.name_span);
        self.source_map
            .enum_variant_spans
            .insert(id, e.variants.iter().map(|v| v.name_span).collect());
        let variants = e
            .variants
            .iter()
            .map(|v| EnumVariant {
                name: v.name.clone(),
                attributes: v.attributes.iter().map(Attribute::from).collect(),
                docstring: v.docstring.clone(),
            })
            .collect();
        self.tree.enums.insert(
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

    /// Allocate a REQUIRED (bodyless) interface method as an ordinary
    /// `Function` item - the rust-analyzer shape: one item kind for every
    /// method, `body: None` the only difference. The signature carries no
    /// defaults arena and default metadata; interface signatures declare
    /// `throws` explicitly (spec rule 1), so nothing here needs a body.
    pub fn alloc_function_signature(
        &mut self,
        m: &ast::MethodSigDef,
    ) -> LocalItemId<FunctionMarker> {
        let id = self.alloc_id(ItemKind::Function, &m.name);
        self.source_map.function_name_spans.insert(id, m.name_span);
        let params = m
            .params
            .iter()
            .map(|p| FunctionParam {
                name: p.name.clone(),
                type_expr: p.type_expr.clone(),
                default: None,
                span: p.span,
            })
            .collect();
        self.tree.functions.insert(
            id,
            Function {
                name: m.name.clone(),
                generic_params: m.generic_params.clone(),
                params,
                defaults: m.defaults.clone(),
                return_type: m.return_type.clone(),
                throws: m.throws.clone(),
                body: None,
                declarative_meta: None,
                metadata: ast::FunctionMetadata {
                    origin: ast::FunctionOrigin::UserDefined,
                    is_language_internal: false,
                },
                docstring: m.docstring.clone(),
                is_tagged_template_tag: false,
                span: m.span,
            },
        );
        id
    }

    /// Allocate an interface (BEP-044) in the `ItemTree`.
    ///
    /// `method_ids` are the `FunctionMarker` ids for EVERY method -
    /// defaults allocated via `alloc_function`, required signatures via
    /// `alloc_function_signature` - in declaration-list order.
    pub fn alloc_interface(
        &mut self,
        i: &ast::InterfaceDef,
        method_ids: Vec<LocalItemId<FunctionMarker>>,
    ) -> LocalItemId<InterfaceMarker> {
        let id = self.alloc_id(ItemKind::Interface, &i.name);
        self.source_map.interface_name_spans.insert(id, i.name_span);
        self.source_map
            .interface_field_spans
            .insert(id, i.fields.iter().map(|f| f.name_span).collect());
        self.source_map
            .interface_method_spans
            .insert(id, i.required_methods.iter().map(|m| m.name_span).collect());
        for method in &method_ids {
            self.record_method_owner(*method, MethodOwner::Interface(id));
        }
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
        self.tree.interfaces.insert(
            id,
            Interface {
                name: i.name.clone(),
                generic_params: i.generic_params.clone(),
                requires: i.requires.clone(),
                fields,
                associated_types: i.associated_types.clone(),
                methods: method_ids,
                attributes: i.attributes.iter().map(Attribute::from).collect(),
                docstring: i.docstring.clone(),
                span: i.span,
            },
        );
        id
    }

    pub fn alloc_type_alias(&mut self, ta: &ast::TypeAliasDef) -> LocalItemId<TypeAliasMarker> {
        let id = self.alloc_id(ItemKind::TypeAlias, &ta.name);
        self.source_map
            .type_alias_name_spans
            .insert(id, ta.name_span);
        self.tree.type_aliases.insert(
            id,
            TypeAlias {
                name: ta.name.clone(),
                type_expr: ta.type_expr.clone(),
                span: ta.span,
                docstring: ta.docstring.clone(),
            },
        );
        id
    }

    pub fn alloc_client(&mut self, c: &ast::ClientDef) -> LocalItemId<ClientMarker> {
        let id = self.alloc_id(ItemKind::Client, &c.name);
        self.source_map.client_spans.insert(
            id,
            ItemSpans {
                span: c.span,
                name_span: c.name_span,
            },
        );
        let provider = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "provider")
            .map(|item| Name::new(item.value.trim().trim_matches('"')));
        let sub_client_names = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "options")
            .map(|_| Vec::new()) // sub-clients are not parsed from `options`; left empty (unused downstream)
            .unwrap_or_default();
        let retry_policy_name = c
            .config_items
            .iter()
            .find(|item| item.key.as_str() == "retry_policy")
            .map(|item| Name::new(item.value.trim().trim_matches('"')));
        self.tree.clients.insert(
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
        self.source_map.test_spans.insert(
            id,
            ItemSpans {
                span: t.span,
                name_span: t.name_span,
            },
        );
        self.tree.tests.insert(
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
        self.source_map
            .template_string_name_spans
            .insert(id, ts.name_span);
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
        self.tree.template_strings.insert(
            id,
            TemplateString {
                name: ts.name.clone(),
                params,
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
        self.source_map.retry_policy_spans.insert(
            id,
            ItemSpans {
                span: rp.span,
                name_span: rp.name_span,
            },
        );
        let get_field = |key: &str| -> Option<String> {
            rp.config_items
                .iter()
                .find(|item| item.key.as_str() == key)
                .map(|item| item.value.trim().to_string())
        };
        self.tree.retry_policies.insert(
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
        self.tree.lets.insert(
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
