// ============================================================================
// IO pipeline (generated from .baml files via baml_builtins2_codegen)
// ============================================================================

// SysOps struct, IO traits (IoClassFsFile, IoNamespaceFs, etc.),
// view/owned types, from_impl, all_host_unavailable — all generated from
// `.baml` `$rust_io_function` definitions by `baml_builtins2_codegen`.
#[allow(
    dead_code,
    non_snake_case,
    unreachable_pub,
    unused_imports,
    unused_variables,
    unused_parens,
    clippy::all,
    clippy::wildcard_imports,
    clippy::pub_underscore_fields,
    clippy::used_underscore_binding,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_clone,
    clippy::used_underscore_items,
    clippy::implicit_clone
)]
pub mod io {
    use std::sync::Arc;

    pub use bex_heap::{AccessError, BexClass, BexValue, BuiltinClass, PermitProof};
    pub use bex_vm_types::SysOp;
    // Owned structs are generated once in sys_types and re-exported here
    // so that `io::owned::ai::*` paths continue to work.
    pub use sys_types::generated::owned;
    pub use sys_types::{
        AsBexExternalValue, BexExternalValue, BexHeap, CallId, OpError, SysOpContext, SysOpFn,
        SysOpOutput, SysOpResult, VmBamlError, VmPanic, VmRustFnError,
    };

    include!(concat!(env!("OUT_DIR"), "/io_generated.rs"));
}

// ============================================================================
// RuntimeIo adapter (generated from .baml files via baml_builtins2_codegen)
// ============================================================================
// Every `$rust_io_function` in the ns_* .baml files is used by
// `baml_builtins2_codegen` to generate the `RuntimeIo` trait (in
// `sys_types::runtime_io`). RuntimeIo is a flat, typed async interface to all
// sys-ops -- no VM plumbing (BexHeap, SysOpContext, CallId) in its signatures.
// Crates like `sys_auth` take `&dyn RuntimeIo` to call into the runtime IO
// layer (HTTP, env, filesystem, shell) without coupling to the VM.
//
// The generated `RuntimeIoAdapter` below bridges the trait to the underlying
// `SysOpFn` pointers by marshaling typed args through `BexExternalValue`.
//
// The trait carries `UnwindSafe + RefUnwindSafe` bounds because the AWS SDK's
// `HttpConnector` trait requires them on provider objects that store
// `Arc<dyn RuntimeIo>`. The adapter has a manual `impl UnwindSafe` -- this is
// safe because it holds only `Arc` clones (no interior mutability of its own)
// and we never catch panics across the SysOpFn boundary.
// ============================================================================

#[allow(
    dead_code,
    unreachable_pub,
    non_snake_case,
    unused_imports,
    unused_variables,
    clippy::all,
    clippy::redundant_closure_for_method_calls,
    clippy::used_underscore_binding,
    clippy::used_underscore_items
)]
mod io_adapter {
    use std::{future::Future, pin::Pin, sync::Arc};

    #[allow(unused_imports)]
    pub use bex_external_types::BexExternalAdt;
    pub use bex_heap::{BexValue, HeapPermitManager};
    pub use sys_types::{
        AsBexExternalValue, BexExternalValue, BexHeap, CallId, SysOpContext, SysOpFn, SysOpResult,
        runtime_io::*,
    };

    use super::io::SysOps;

    include!(concat!(env!("OUT_DIR"), "/io_adapter.rs"));
}
pub use io_adapter::build_runtime_io;

// ============================================================================
// Prompt schema rendering + SAP parsing
// ============================================================================
// Relocated verbatim from the (now deleted) `sys_llm` crate, whose provider
// stack was replaced by native BAML client implementations. `sys_ops` was the
// only remaining caller of these two pieces.
// ============================================================================

pub mod output_format;
pub mod sap;

// ============================================================================
// Blanket IO LLM implementation
// ============================================================================

/// Look up an LLM function by name via the canonical
/// [`sys_types::resolve_name`] rule. The suffix-scan step handles
/// functions declared inside a user namespace (e.g. `ns_lorem/`) — the
/// synthesized companion passes the bare BAML identifier, not the FQN,
/// so without it a namespaced LLM function fails to resolve. Returns the
/// full `ResolveOutcome` (rather than collapsing to `Option`) so callers
/// can distinguish ambiguity from a true not-found in their error
/// messages: both still abort the sysop as an `InvalidArgument`, but the
/// distinction matters for diagnosing synthesis / name-resolution bugs.
fn lookup_llm_function<'a>(
    function_name: &str,
    llm_functions: &'a std::collections::HashMap<String, LlmFunctionInfo>,
) -> sys_types::ResolveOutcome<'a, LlmFunctionInfo> {
    sys_types::resolve_name(llm_functions, function_name)
}

/// Format a `lookup_llm_function` miss as a sysop error message,
/// distinguishing ambiguous from not-found.
fn llm_function_lookup_error(
    function_name: &str,
    outcome: &sys_types::ResolveOutcome<'_, LlmFunctionInfo>,
) -> VmRustFnError {
    match outcome {
        sys_types::ResolveOutcome::Found(_, _) => {
            // Unreachable in practice — caller only invokes this on a miss.
            // We still produce a coherent error rather than panicking so
            // a future refactor can't accidentally trip on this.
            VmInternalError::BridgeFailure {
                message: format!(
                    "llm_function_lookup_error called with a Found \
                     outcome for `{function_name}`"
                ),
            }
            .into()
        }
        sys_types::ResolveOutcome::NotFound => VmBamlError::InvalidArgument {
            message: format!("LLM function not found: {function_name}"),
        }
        .into(),
        sys_types::ResolveOutcome::Ambiguous => VmBamlError::InvalidArgument {
            message: format!(
                "LLM function name `{function_name}` is ambiguous: two or more \
                 namespaced functions end with `.{function_name}`. Pass a fully \
                 qualified name (e.g. `<pkg>.<ns>.{function_name}`) to disambiguate."
            ),
        }
        .into(),
    }
}

/// Blanket impl — schema-aligned parsing, backing both public `baml.sap.parse`
/// and incremental `ai.stream.Stream` parsing.
///
impl<T> io::IoClassAiOutputFormat for T {
    #[allow(clippy::too_many_arguments)]
    fn _render(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        output_format: io::owned::ai::OutputFormat,
        prefix: io::BexExternalValue,
        or_splitter: io::BexExternalValue,
        enum_value_prefix: io::BexExternalValue,
        hoisted_class_prefix: io::BexExternalValue,
        always_hoist_enums: io::BexExternalValue,
        quote_class_fields: io::BexExternalValue,
        hoist_classes: io::BexExternalValue,
        map_style: io::BexExternalValue,
        render_null_as: io::BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        render_output_format_with_op(
            &output_format,
            prefix,
            or_splitter,
            enum_value_prefix,
            hoisted_class_prefix,
            always_hoist_enums,
            quote_class_fields,
            hoist_classes,
            map_style,
            render_null_as,
        )
    }
}

// ============================================================================
// `baml.json.schema` — provider-neutral JSON Schema lowering
// ============================================================================

impl<T> io::IoNamespaceJson for T {
    fn schema(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        t: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        match schema::json_schema(&t, ctx) {
            Ok(value) => SysOpOutput::ok(schema::json_to_bex(value)),
            Err(message) => SysOpOutput::err(VmBamlError::Unsupported { message }),
        }
    }
}

mod schema {
    use std::collections::HashSet;

    use ::sys_types::SapTy;
    use bex_external_types::BexExternalValue;
    use serde_json::{Value, json};
    use sys_types::SysOpContext;

    /// The stdlib `baml.json.json` alias as value metadata.
    ///
    /// `BexExternalValue` tags its containers with a declared type rather than
    /// a lane type; `baml.json.json` is compiled, so it has a real qualified
    /// name and this conversion is total.
    fn json_alias_ty() -> baml_type::RuntimeTy {
        baml_type::RuntimeTy::TypeAlias(
            baml_type::TypeName::from_dotted_path(baml_base::qualified_name::BAML_JSON_JSON),
            baml_type::TyAttr::default(),
        )
    }

    pub(super) fn json_to_bex(value: Value) -> BexExternalValue {
        match value {
            Value::Null => BexExternalValue::Null,
            Value::Bool(value) => BexExternalValue::Bool(value),
            Value::Number(value) => match value.as_i64() {
                Some(value) => BexExternalValue::Int(value),
                None => BexExternalValue::Float(value.as_f64().unwrap_or_default()),
            },
            Value::String(value) => BexExternalValue::String(value.into()),
            Value::Array(items) => BexExternalValue::Array {
                element_type: json_alias_ty(),
                items: items.into_iter().map(json_to_bex).collect(),
            },
            Value::Object(entries) => BexExternalValue::Map {
                key_type: baml_type::RuntimeTy::string(),
                value_type: json_alias_ty(),
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, json_to_bex(value)))
                    .collect(),
            },
        }
    }

    pub(super) fn json_schema(ty: &SapTy, ctx: &SysOpContext) -> Result<Value, String> {
        let mut builder = SchemaBuilder {
            ctx,
            definitions: serde_json::Map::new(),
            building: HashSet::new(),
            referenced: HashSet::new(),
        };

        let (mut root, root_class_key) = match ty {
            SapTy::Class(name, _, _) => {
                let key = definition_key(name);
                builder.building.insert(key.clone());
                let schema = builder.class_object(name)?;
                builder.building.remove(&key);
                (schema, Some(key))
            }
            _ => (builder.ty_schema(ty)?, None),
        };

        if let Some(key) = root_class_key
            && builder.referenced.contains(&key)
        {
            builder.definitions.insert(key, root.clone());
        }

        if !builder.definitions.is_empty() {
            let Value::Object(root_object) = &mut root else {
                return Err("json_schema: schema root must be a JSON object".to_string());
            };
            root_object.insert("$defs".to_string(), Value::Object(builder.definitions));
        }
        Ok(root)
    }

    struct SchemaBuilder<'a> {
        ctx: &'a SysOpContext,
        definitions: serde_json::Map<String, Value>,
        building: HashSet<String>,
        referenced: HashSet<String>,
    }

    impl SchemaBuilder<'_> {
        fn ty_schema(&mut self, ty: &SapTy) -> Result<Value, String> {
            match ty {
                SapTy::Int { .. } | SapTy::Bigint { .. } => Ok(json!({ "type": "integer" })),
                SapTy::Float { .. } => Ok(json!({ "type": "number" })),
                SapTy::String { .. } => Ok(json!({ "type": "string" })),
                SapTy::Bool { .. } => Ok(json!({ "type": "boolean" })),
                SapTy::Null { .. } => Ok(json!({ "type": "null" })),
                SapTy::Uint8Array { .. } => Ok(json!({ "type": "string" })),
                SapTy::Literal(lit, _, _) => Ok(Self::literal_schema(lit)),
                SapTy::List(inner, _) => Ok(json!({
                    "type": "array",
                    "items": self.ty_schema(inner)?,
                })),
                SapTy::Map { value, .. } => Ok(json!({
                    "type": "object",
                    "additionalProperties": self.ty_schema(value)?,
                })),
                SapTy::Union(members, _) => self.union_schema(members),
                SapTy::Enum(name, _) => Self::enum_schema(name, self.ctx),
                SapTy::Class(name, _, _) => self.class_ref(name),
                SapTy::TypeAlias(name, _) => self.type_alias_ref(name),
                SapTy::Unknown { .. } => Ok(json!({})),
                other => Err(format!(
                    "json_schema: no JSON Schema representation for `{other}`"
                )),
            }
        }

        fn literal_schema(lit: &baml_base::Literal) -> Value {
            use baml_base::Literal;
            match lit {
                Literal::Int(i) => json!({ "type": "integer", "const": i }),
                Literal::Bigint(n) => json!({ "type": "integer", "const": n.to_string() }),
                Literal::Float(s) => {
                    json!({ "type": "number", "const": s.parse::<f64>().unwrap_or(0.0) })
                }
                Literal::String(s) => json!({ "type": "string", "const": s }),
                Literal::Bool(b) => json!({ "type": "boolean", "const": b }),
            }
        }

        fn union_schema(&mut self, members: &[SapTy]) -> Result<Value, String> {
            let has_null = members.iter().any(SapTy::is_null);
            let non_null: Vec<&SapTy> = members.iter().filter(|m| !m.is_null()).collect();
            if non_null.is_empty() {
                return Ok(json!({ "type": "null" }));
            }
            let mut schemas = non_null
                .iter()
                .map(|member| self.ty_schema(member))
                .collect::<Result<Vec<_>, _>>()?;
            if schemas.len() == 1 {
                let base = schemas.pop().unwrap_or_else(|| json!({}));
                return Ok(if has_null {
                    Self::with_null(base)
                } else {
                    base
                });
            }
            if has_null {
                schemas.push(json!({ "type": "null" }));
            }
            Ok(json!({ "anyOf": schemas }))
        }

        fn with_null(base: Value) -> Value {
            if let Value::Object(mut object) = base {
                if let Some(Value::String(kind)) = object.get("type") {
                    let widened = json!([kind, "null"]);
                    object.insert("type".to_string(), widened);
                    return Value::Object(object);
                }
                return json!({ "anyOf": [Value::Object(object), { "type": "null" }] });
            }
            json!({ "anyOf": [base, { "type": "null" }] })
        }

        fn enum_schema(head: &::sys_types::DefKey, ctx: &SysOpContext) -> Result<Value, String> {
            let enum_def = find_enum_definition(ctx, head)
                .ok_or_else(|| format!("json_schema: unknown enum `{}`", head.display_name()))?;
            let variants: Vec<Value> = enum_def
                .variants
                .iter()
                .map(|variant| {
                    json!(
                        variant
                            .alias
                            .clone()
                            .unwrap_or_else(|| variant.name.clone())
                    )
                })
                .collect();
            Ok(json!({ "type": "string", "enum": variants }))
        }

        fn class_ref(&mut self, head: &::sys_types::DefKey) -> Result<Value, String> {
            let key = definition_key(head);
            self.referenced.insert(key.clone());

            if !self.definitions.contains_key(&key) && !self.building.contains(&key) {
                self.building.insert(key.clone());
                let definition = self.class_object(head)?;
                self.building.remove(&key);
                self.definitions.insert(key.clone(), definition);
            }

            Ok(json!({ "$ref": format!("#/$defs/{}", json_pointer_escape(&key)) }))
        }

        fn type_alias_ref(&mut self, head: &::sys_types::DefKey) -> Result<Value, String> {
            let key = definition_key(head);
            self.referenced.insert(key.clone());

            if !self.definitions.contains_key(&key) && !self.building.contains(&key) {
                let target = find_type_alias_definition(self.ctx, head)
                    .cloned()
                    .ok_or_else(|| {
                        format!("json_schema: unknown type alias `{}`", head.display_name())
                    })?;
                self.building.insert(key.clone());
                let definition = self.ty_schema(&target)?;
                self.building.remove(&key);
                self.definitions.insert(key.clone(), definition);
            }

            Ok(json!({ "$ref": format!("#/$defs/{}", json_pointer_escape(&key)) }))
        }

        fn class_object(&mut self, head: &::sys_types::DefKey) -> Result<Value, String> {
            let class_def = find_class_definition(self.ctx, head)
                .ok_or_else(|| format!("json_schema: unknown class `{}`", head.display_name()))?
                .clone();

            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for field in &class_def.fields {
                if field.skip {
                    continue;
                }
                let prop_name = field.alias.clone().unwrap_or_else(|| field.name.clone());
                properties.insert(prop_name.clone(), self.ty_schema(&field.field_type)?);
                if !(field.field_type.is_nullable_union() || field.field_type.is_null()) {
                    required.push(json!(prop_name));
                }
            }

            Ok(json!({
                "type": "object",
                "properties": properties,
                "required": required,
            }))
        }
    }

    /// The `$defs` key a type is published under in the emitted JSON schema.
    ///
    /// An output label, so it is the head's *name* — the schema is read by a
    /// model, not by us. Identity lookups go through the definition tables,
    /// which are keyed by the head itself.
    fn definition_key(head: &::sys_types::DefKey) -> String {
        head.display_name().to_string()
    }

    fn json_pointer_escape(value: &str) -> String {
        value.replace('~', "~0").replace('/', "~1")
    }

    /// Look up a class definition by declaration identity.
    ///
    /// Exact, with nothing to fall back to: the key's equality is its tag, so
    /// this finds the declaration the type actually names or nothing at all.
    /// The old fallback scanned for a unique matching `display_name`, which
    /// could return a *different* declaration that merely shared a spelling —
    /// unrepresentable once the table is keyed by identity.
    fn find_class_definition<'a>(
        ctx: &'a SysOpContext,
        head: &::sys_types::DefKey,
    ) -> Option<&'a sys_types::ClassDefinition> {
        ctx.class_definitions.get(head)
    }

    /// See [`find_class_definition`] — same contract, for enums.
    fn find_enum_definition<'a>(
        ctx: &'a SysOpContext,
        head: &::sys_types::DefKey,
    ) -> Option<&'a sys_types::EnumDefinition> {
        ctx.enum_definitions.get(head)
    }

    /// See [`find_class_definition`] — same contract, for recursive aliases.
    fn find_type_alias_definition<'a>(
        ctx: &'a SysOpContext,
        head: &::sys_types::DefKey,
    ) -> Option<&'a SapTy> {
        ctx.type_alias_definitions.get(head)
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use baml_type::{TyAttr, TypeName};
        use serde_json::json;
        use sys_types::{
            ClassDefinition, ClassFieldDefinition, DefKey, EnumDefinition, EnumVariantDefinition,
            SapTy as RuntimeTy, SysOpContext,
        };

        use super::json_schema;

        fn type_name(name: &str) -> TypeName {
            TypeName::from_dotted_path(name)
        }

        fn class_ty(name: &TypeName) -> RuntimeTy {
            RuntimeTy::Class(key(name), Box::new([]), TyAttr::default())
        }

        /// A lane key for a compiled test declaration.
        fn key(name: &TypeName) -> DefKey {
            DefKey::new(
                baml_type::typetag::TypeTag::of_head(&name.render_dotted(false)),
                baml_type::DeclarationName::Declared(name.clone()),
            )
        }

        fn alias_ty(name: &TypeName) -> RuntimeTy {
            RuntimeTy::TypeAlias(key(name), TyAttr::default())
        }

        fn field(name: &str, field_type: RuntimeTy) -> ClassFieldDefinition {
            ClassFieldDefinition {
                name: name.to_string(),
                field_type,
                field_template: None,
                description: None,
                docstring: None,
                alias: None,
                skip: false,
            }
        }

        fn class_definition(name: &TypeName, fields: Vec<ClassFieldDefinition>) -> ClassDefinition {
            ClassDefinition {
                name: name.display_name().to_string(),
                description: None,
                docstring: None,
                alias: None,
                fields,
            }
        }

        #[test]
        fn self_referential_class_uses_defs_ref() {
            let node = type_name("pkg.Node");
            let mut classes = indexmap::IndexMap::new();
            classes.insert(
                key(&node),
                class_definition(
                    &node,
                    vec![field("next", RuntimeTy::optional(class_ty(&node)))],
                ),
            );
            let mut ctx = SysOpContext::empty();
            ctx.class_definitions = Arc::new(classes);

            let schema = json_schema(&class_ty(&node), &ctx).expect("schema should lower");
            assert_eq!(schema["type"], "object");
            assert_eq!(
                schema["properties"]["next"]["anyOf"][0]["$ref"],
                "#/$defs/pkg.Node"
            );
            assert_eq!(schema["$defs"]["pkg.Node"]["type"], "object");
        }

        #[test]
        fn mutually_recursive_classes_share_defs() {
            let a = type_name("pkg.A");
            let b = type_name("pkg.B");
            let mut classes = indexmap::IndexMap::new();
            classes.insert(
                key(&a),
                class_definition(&a, vec![field("b", class_ty(&b))]),
            );
            classes.insert(
                key(&b),
                class_definition(&b, vec![field("a", RuntimeTy::optional(class_ty(&a)))]),
            );
            let mut ctx = SysOpContext::empty();
            ctx.class_definitions = Arc::new(classes);

            let schema = json_schema(&class_ty(&a), &ctx).expect("schema should lower");
            assert_eq!(schema["properties"]["b"]["$ref"], "#/$defs/pkg.B");
            assert_eq!(
                schema["$defs"]["pkg.B"]["properties"]["a"]["anyOf"][0]["$ref"],
                "#/$defs/pkg.A"
            );
            assert_eq!(schema["$defs"]["pkg.A"]["type"], "object");
        }

        #[test]
        fn nullable_union_widens_primitive_type() {
            let schema = json_schema(
                &RuntimeTy::optional(RuntimeTy::string()),
                &SysOpContext::empty(),
            )
            .expect("schema should lower");
            assert_eq!(schema, json!({ "type": ["string", "null"] }));
        }

        #[test]
        fn class_refs_escape_json_pointer_tokens() {
            let holder = type_name("pkg.Holder");
            let escaped = type_name("pkg.A/B~C");
            let mut classes = indexmap::IndexMap::new();
            classes.insert(
                key(&holder),
                class_definition(&holder, vec![field("value", class_ty(&escaped))]),
            );
            classes.insert(
                key(&escaped),
                class_definition(&escaped, vec![field("value", RuntimeTy::int())]),
            );
            let mut ctx = SysOpContext::empty();
            ctx.class_definitions = Arc::new(classes);

            let schema = json_schema(&class_ty(&holder), &ctx).expect("schema should lower");
            assert_eq!(schema["properties"]["value"]["$ref"], "#/$defs/pkg.A~1B~0C");
            assert_eq!(schema["$defs"]["pkg.A/B~C"]["type"], "object");
        }

        #[test]
        fn enum_variant_aliases_become_schema_values() {
            let status = type_name("pkg.Status");
            let mut enums = indexmap::IndexMap::new();
            enums.insert(
                key(&status),
                EnumDefinition {
                    name: "Status".to_string(),
                    description: None,
                    docstring: None,
                    alias: None,
                    variants: vec![
                        EnumVariantDefinition {
                            name: "Ready".to_string(),
                            description: None,
                            docstring: None,
                            alias: Some("ready-now".to_string()),
                        },
                        EnumVariantDefinition {
                            name: "Done".to_string(),
                            description: None,
                            docstring: None,
                            alias: None,
                        },
                    ],
                },
            );
            let mut ctx = SysOpContext::empty();
            ctx.enum_definitions = Arc::new(enums);

            let schema = json_schema(&RuntimeTy::Enum(key(&status), TyAttr::default()), &ctx)
                .expect("schema should lower");
            assert_eq!(
                schema,
                json!({ "type": "string", "enum": ["ready-now", "Done"] })
            );
        }

        #[test]
        fn recursive_json_alias_uses_a_self_ref() {
            let json_name = type_name(baml_base::qualified_name::BAML_JSON_JSON);
            let json_alias = alias_ty(&json_name);
            let target = RuntimeTy::union([
                RuntimeTy::null(),
                RuntimeTy::bool(),
                RuntimeTy::int(),
                RuntimeTy::float(),
                RuntimeTy::string(),
                RuntimeTy::list(json_alias.clone()),
                RuntimeTy::map(RuntimeTy::string(), json_alias.clone()),
            ]);
            let mut aliases = indexmap::IndexMap::new();
            aliases.insert(key(&json_name), target);
            let mut ctx = SysOpContext::empty();
            ctx.type_alias_definitions = Arc::new(aliases);

            let schema = json_schema(&json_alias, &ctx).expect("schema should lower");
            assert_eq!(schema["$ref"], "#/$defs/baml.json.json");
            let definition = &schema["$defs"]["baml.json.json"];
            assert_eq!(
                definition["anyOf"][4]["items"]["$ref"],
                "#/$defs/baml.json.json"
            );
            assert_eq!(
                definition["anyOf"][5]["additionalProperties"]["$ref"],
                "#/$defs/baml.json.json"
            );
        }
    }
}

/// The `ai` package root has no free IO functions — its IO surface is the
/// `ai.Context` class methods (`IoClassAiContext`) — but the generated package
/// trait still requires the (method-only) namespace trait.
impl<T> io::IoNamespaceAi for T {}

// The `ai.internal` prompt-rendering sys-ops are pure (no platform IO), so
// both `DefaultIoOps` and `NativeSysOps` delegate their `IoNamespaceAiInternal`
// prompt methods to these shared implementations.

/// BEP-049 section 10 (M5b): the `ctx.output_format()` schema string.
pub fn render_output_format_op(
    return_type: &::sys_types::SapTy,
    ctx: &SysOpContext,
) -> SysOpOutput<String> {
    SysOpOutput::ok(crate::output_format::render_output_format(return_type, ctx))
}

/// BEP-049 section 10 (M5b.2): build the opaque schema handle `Context._output_format`
/// carries; `output_format(...)` renders it with caller options.
pub fn build_output_format_op(
    return_type: &::sys_types::SapTy,
    ctx: &SysOpContext,
) -> SysOpOutput<io::owned::ai::OutputFormat> {
    let content = crate::output_format::build_output_format_content(return_type, ctx);
    SysOpOutput::ok(wrap_output_format(std::sync::Arc::new(content)))
}

fn output_format_option_value(value: io::BexExternalValue) -> io::BexExternalValue {
    match value {
        io::BexExternalValue::Union { value, .. } => output_format_option_value(*value),
        value => value,
    }
}

/// The `output_format` options are declared as literal unions in
/// `ai/context.baml`, so the type checker already rejects every value this
/// would report. Reaching it means the wire value disagrees with the declared
/// parameter type — an engine inconsistency, not a caller error.
fn invalid_output_format_option(name: &str, value: &io::BexExternalValue) -> VmRustFnError {
    VmInternalError::BridgeFailure {
        message: format!("invalid internal value for output_format option `{name}`: {value:?}"),
    }
    .into()
}

fn is_output_format_default(value: &io::BexExternalValue) -> bool {
    matches!(
        value,
        io::BexExternalValue::Variant {
            variant_name,
            ..
        } if variant_name == "Auto"
    )
}

fn output_format_string_setting(
    name: &str,
    value: io::BexExternalValue,
    null_is_never: bool,
) -> Result<crate::output_format::RenderSetting<String>, VmRustFnError> {
    use crate::output_format::RenderSetting;

    let value = output_format_option_value(value);
    match value {
        io::BexExternalValue::String(value) => Ok(RenderSetting::Always(value.to_string())),
        io::BexExternalValue::Null if null_is_never => Ok(RenderSetting::Never),
        io::BexExternalValue::Null => Ok(RenderSetting::Auto),
        value if is_output_format_default(&value) => Ok(RenderSetting::Auto),
        value => Err(invalid_output_format_option(name, &value)),
    }
}

fn output_format_bool_setting(
    name: &str,
    value: io::BexExternalValue,
) -> Result<crate::output_format::RenderSetting<bool>, VmRustFnError> {
    use crate::output_format::RenderSetting;

    let value = output_format_option_value(value);
    match value {
        io::BexExternalValue::Bool(value) => Ok(RenderSetting::Always(value)),
        value if is_output_format_default(&value) => Ok(RenderSetting::Auto),
        value => Err(invalid_output_format_option(name, &value)),
    }
}

fn output_format_hoist_classes(
    value: io::BexExternalValue,
) -> Result<crate::output_format::HoistClasses, VmRustFnError> {
    use crate::output_format::HoistClasses;

    let value = output_format_option_value(value);
    match value {
        io::BexExternalValue::Bool(true) => Ok(HoistClasses::All),
        io::BexExternalValue::Bool(false) => Ok(HoistClasses::Auto),
        io::BexExternalValue::String(value) if value.as_str() == "auto" => Ok(HoistClasses::Auto),
        io::BexExternalValue::Array { items, .. } => items
            .into_iter()
            .map(|item| match output_format_option_value(item) {
                io::BexExternalValue::String(value) => Ok(value.to_string()),
                value => Err(invalid_output_format_option("hoist_classes", &value)),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(HoistClasses::Subset),
        value if is_output_format_default(&value) => Ok(HoistClasses::Auto),
        value => Err(invalid_output_format_option("hoist_classes", &value)),
    }
}

fn output_format_map_style(
    value: io::BexExternalValue,
) -> Result<crate::output_format::MapStyle, VmRustFnError> {
    use crate::output_format::MapStyle;

    let value = output_format_option_value(value);
    match value {
        io::BexExternalValue::String(value) if value.as_str() == "angle" => {
            Ok(MapStyle::TypeParameters)
        }
        io::BexExternalValue::String(value) if value.as_str() == "object" => {
            Ok(MapStyle::ObjectLiteral)
        }
        value if is_output_format_default(&value) => Ok(MapStyle::default()),
        value => Err(invalid_output_format_option("map_style", &value)),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_output_format_with_op(
    output_format: &io::owned::ai::OutputFormat,
    prefix: io::BexExternalValue,
    or_splitter: io::BexExternalValue,
    enum_value_prefix: io::BexExternalValue,
    hoisted_class_prefix: io::BexExternalValue,
    always_hoist_enums: io::BexExternalValue,
    quote_class_fields: io::BexExternalValue,
    hoist_classes: io::BexExternalValue,
    map_style: io::BexExternalValue,
    render_null_as: io::BexExternalValue,
) -> SysOpOutput<String> {
    let options: Result<crate::output_format::RenderOptions, VmRustFnError> = (|| {
        Ok(crate::output_format::RenderOptions {
            prefix: output_format_string_setting("prefix", prefix, true)?,
            or_splitter: output_format_string_setting("or_splitter", or_splitter, false)?,
            enum_value_prefix: output_format_string_setting(
                "enum_value_prefix",
                enum_value_prefix,
                true,
            )?,
            hoisted_class_prefix: output_format_string_setting(
                "hoisted_class_prefix",
                hoisted_class_prefix,
                true,
            )?,
            hoist_classes: output_format_hoist_classes(hoist_classes)?,
            always_hoist_enums: output_format_bool_setting(
                "always_hoist_enums",
                always_hoist_enums,
            )?,
            map_style: output_format_map_style(map_style)?,
            quote_class_fields: output_format_bool_setting(
                "quote_class_fields",
                quote_class_fields,
            )?,
            render_null_as: output_format_string_setting("render_null_as", render_null_as, false)?,
        })
    })();

    let options = match options {
        Ok(options) => options,
        Err(error) => return SysOpOutput::err(error),
    };
    let content = unwrap_output_format(output_format);
    match crate::output_format::render_output_format_content(&content, &options) {
        Ok(rendered) => SysOpOutput::ok(rendered),
        Err(error) => SysOpOutput::err(VmBamlError::RenderPrompt {
            message: error.to_string(),
        }),
    }
}

/// Look up an LLM function's declared return type by name.
pub fn get_return_type_op(
    function_name: &str,
    ctx: &SysOpContext,
) -> SysOpOutput<::sys_types::SapTy> {
    let outcome = lookup_llm_function(function_name, &ctx.llm_functions);
    let sys_types::ResolveOutcome::Found(_, info) = outcome else {
        return SysOpOutput::err(llm_function_lookup_error(function_name, &outcome));
    };
    SysOpOutput::ok(info.return_type.clone())
}

/// Blanket impl — schema-aligned parsing, backing both public `baml.sap.parse`
/// and incremental `ai.stream.Stream` parsing. All three are free functions
/// (see `ns_sap/sap.baml`), so each carries its own `TStream`/`TFinal`
/// type-arg operands; the cache already holds the compiled model, so the
/// two parse entry points ignore theirs.
impl<T> io::IoClassSapParseCache for T {
    fn _parse_final(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        cache: io::owned::sap::ParseCache,
        json: String,
        _type_arg_0: ::sys_types::SapTy,
        _type_arg_1: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.clone().downcast::<crate::sap::SapParseCache>() else {
            return SysOpOutput::err(VmInternalError::RustTypeError {
                expected: std::any::TypeId::of::<crate::sap::SapParseCache>(),
                got: cache._data.type_id(),
            });
        };
        SysOpOutput::Ready(
            crate::sap::execute_sap_parse_final(&json, &sap, ctx).map_err(VmRustFnError::from),
        )
    }

    fn _parse_partial(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        cache: io::owned::sap::ParseCache,
        json: String,
        _type_arg_0: ::sys_types::SapTy,
        _type_arg_1: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.clone().downcast::<crate::sap::SapParseCache>() else {
            return SysOpOutput::err(VmInternalError::RustTypeError {
                expected: std::any::TypeId::of::<crate::sap::SapParseCache>(),
                got: cache._data.type_id(),
            });
        };
        let result = match crate::sap::execute_sap_parse_partial(&json, &sap, ctx) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(BexExternalValue::instance(
                "baml.sap._NoYield",
                ::indexmap::IndexMap::new(),
            )),
            Err(e) => Err(VmRustFnError::from(e)),
        };
        SysOpOutput::Ready(result)
    }
}

impl<T> io::IoNamespaceSap for T {
    fn _new_parse_cache(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        stream_target: ::sys_types::SapTy,
        target: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sap::ParseCache> {
        let compiled =
            match ::bex_sap::CompiledSapModel::from_sys_op_context(ctx, target, stream_target) {
                Ok(compiled) => compiled,
                Err(e) => {
                    // `_new_parse_cache` declares `throws never`, and the type
                    // arguments that reach it come from the caller's own
                    // `parse<T>` — a `T` schema-aligned parsing cannot model is
                    // a program bug, not a recoverable condition, so it panics.
                    return SysOpOutput::err(VmPanic::UserPanic {
                        message: format!("schema-aligned parsing cannot model this type: {e}"),
                    });
                }
            };
        let sap = crate::sap::SapParseCache::new(compiled);
        let data: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(sap);
        SysOpOutput::ok(io::owned::sap::ParseCache { _data: data })
    }
}

/// Wrap an `OutputFormatContent` into the generated `owned::ai::OutputFormat` handle.
fn wrap_output_format(
    content: std::sync::Arc<crate::output_format::OutputFormatContent>,
) -> io::owned::ai::OutputFormat {
    io::owned::ai::OutputFormat {
        _data: content as std::sync::Arc<dyn std::any::Any + Send + Sync>,
    }
}

/// Unwrap a generated `owned::ai::OutputFormat` handle back to its `OutputFormatContent`.
#[allow(clippy::used_underscore_binding)]
fn unwrap_output_format(
    owned: &io::owned::ai::OutputFormat,
) -> std::sync::Arc<crate::output_format::OutputFormatContent> {
    owned
        ._data
        .clone()
        .downcast::<crate::output_format::OutputFormatContent>()
        .expect("OutputFormat._data downcast failed: expected Arc<OutputFormatContent>. This indicates a bug in wrap_output_format or a type mismatch.")
}

// ============================================================================
// IoSysOpsBuilder — Compose an io::SysOps table by overriding namespaces
// ============================================================================

/// Default provider for the IO pipeline. Prompt AST, output-format, and SAP
/// operations use the implementations above; unsupported platform operations
/// return their resource-specific errors.
struct DefaultIoOps;

fn host_unavailable(resource: &str) -> VmPanic {
    VmPanic::HostUnavailable {
        resource: resource.to_string(),
        message: "Operation not supported on this platform".to_string(),
    }
}

impl io::IoClassReflectPackage for DefaultIoOps {
    io::io_error_methods!(
        IoClassReflectPackage,
        VmPanic::HostUnavailable {
            resource: "runtime-compiler".to_string(),
            message: "runtime compiler is not installed".to_string()
        }
    );
}

impl io::IoClassReflectSession for DefaultIoOps {
    io::io_error_methods!(
        IoClassReflectSession,
        VmPanic::HostUnavailable {
            resource: "runtime-compiler".to_string(),
            message: "runtime compiler is not installed".to_string()
        }
    );
}

impl io::IoNamespaceReflect for DefaultIoOps {}

impl io::IoClassFsFile for DefaultIoOps {
    io::io_error_methods!(IoClassFsFile, host_unavailable("filesystem"));
}

impl io::IoNamespaceFs for DefaultIoOps {
    io::io_error_methods!(IoNamespaceFs::open, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::exists, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::remove, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::remove_dir, host_unavailable("filesystem"));

    io::io_error_methods!(
        IoNamespaceFs::remove_dir_all,
        host_unavailable("filesystem")
    );

    io::io_error_methods!(IoNamespaceFs::size, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::read, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::write, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::write_bytes, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::read_dir, host_unavailable("filesystem"));

    io::io_error_methods!(IoNamespaceFs::mkdir, host_unavailable("filesystem"));

    io::io_error_methods!(
        IoNamespaceFs::chmod,
        VmPanic::HostUnavailable {
            resource: "filesystem".to_string(),
            message: "File permissions are not supported on this platform".to_string(),
        }
    );

    io::io_error_methods!(
        IoNamespaceFs::symlink,
        VmPanic::HostUnavailable {
            resource: "filesystem".to_string(),
            message: "Symbolic links are not supported on this platform".to_string(),
        }
    );
}

impl io::IoClassHttpResponse for DefaultIoOps {
    io::io_error_methods!(IoClassHttpResponse, host_unavailable("http"));
}

impl io::IoClassHttpTlsConfig for DefaultIoOps {
    io::io_error_methods!(IoClassHttpTlsConfig, host_unavailable("http"));
}

impl io::IoClassHttpServer for DefaultIoOps {
    io::io_error_methods!(IoClassHttpServer, host_unavailable("http"));
}

impl io::IoClassHttpSseStream for DefaultIoOps {
    io::io_error_methods!(IoClassHttpSseStream, host_unavailable("http"));
}

impl io::IoNamespaceHttp for DefaultIoOps {
    io::io_error_methods!(IoNamespaceHttp, host_unavailable("http"));
}

impl io::IoClassWsWsStream for DefaultIoOps {
    io::io_error_methods!(IoClassWsWsStream, host_unavailable("websocket"));
}

impl io::IoNamespaceWs for DefaultIoOps {
    io::io_error_methods!(IoNamespaceWs, host_unavailable("websocket"));
}

impl io::IoClassNetTcpStream for DefaultIoOps {
    io::io_error_methods!(IoClassNetTcpStream, host_unavailable("network"));
}

impl io::IoClassNetTcpListener for DefaultIoOps {
    io::io_error_methods!(IoClassNetTcpListener, host_unavailable("network"));
}

impl io::IoClassNetUdpSocket for DefaultIoOps {
    io::io_error_methods!(IoClassNetUdpSocket, host_unavailable("network"));
}

impl io::IoNamespaceNet for DefaultIoOps {}

impl io::IoNamespaceEnv for DefaultIoOps {
    io::io_error_methods!(IoNamespaceEnv, host_unavailable("environment"));
}

impl io::IoNamespaceIo for DefaultIoOps {
    io::io_error_methods!(IoNamespaceIo, host_unavailable("stdio"));
}

impl io::IoClassSysProcess for DefaultIoOps {
    io::io_error_methods!(IoClassSysProcess::wait, host_unavailable("process"));

    io::io_error_methods!(IoClassSysProcess::kill, host_unavailable("process"));

    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::ok(())
    }
}

impl io::IoClassSysReadPipe for DefaultIoOps {
    io::io_error_methods!(IoClassSysReadPipe, host_unavailable("process"));
}

impl io::IoClassSysWritePipe for DefaultIoOps {
    io::io_error_methods!(IoClassSysWritePipe, host_unavailable("process"));
}

impl io::IoNamespaceSys for DefaultIoOps {
    fn collect_garbage(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // The engine handles this intrinsic before consulting the platform IO
        // table. Returning success is the safe fallback for other runtimes.
        SysOpOutput::ok(())
    }

    io::io_error_methods!(IoNamespaceSys::exec, host_unavailable("process"));

    io::io_error_methods!(IoNamespaceSys::start_process, host_unavailable("process"));

    io::io_error_methods!(IoNamespaceSys::shell, host_unavailable("process"));

    io::io_error_methods!(IoNamespaceSys::sleep, host_unavailable("timer"));

    io::io_error_methods!(IoNamespaceSys::pid, host_unavailable("process-id"));
}

impl io::IoClassGlobGlob for DefaultIoOps {
    io::io_error_methods!(IoClassGlobGlob, host_unavailable("filesystem"));
}

impl io::IoNamespaceGlob for DefaultIoOps {
    io::io_error_methods!(IoNamespaceGlob, host_unavailable("filesystem"));
}

impl io::IoNamespaceHost for DefaultIoOps {
    io::io_error_methods!(IoNamespaceHost, host_unavailable("host-callable"));
}

impl io::IoClassTimeInstant for DefaultIoOps {
    io::io_error_methods!(IoClassTimeInstant, host_unavailable("clock"));
}

impl io::IoNamespaceTime for DefaultIoOps {
    io::io_error_methods!(IoNamespaceTime, host_unavailable("timezone-database"));
}

impl io::IoClassRandomSystemRandom for DefaultIoOps {
    io::io_error_methods!(IoClassRandomSystemRandom, host_unavailable("randomness"));
}

impl io::IoNamespaceRandom for DefaultIoOps {}

impl io::IoNamespaceAiInternal for DefaultIoOps {
    fn render_output_format(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        return_type: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        render_output_format_op(&return_type, ctx)
    }
    fn build_output_format(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        return_type: ::sys_types::SapTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::ai::OutputFormat> {
        build_output_format_op(&return_type, ctx)
    }
    fn get_return_type(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        function_name: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<::sys_types::SapTy> {
        get_return_type_op(&function_name, ctx)
    }
    io::io_error_methods!(
        IoNamespaceAiInternal::_gcp_access_token,
        host_unavailable("gcp-credentials")
    );
    io::io_error_methods!(
        IoNamespaceAiInternal::_gcp_project_id,
        host_unavailable("gcp-credentials")
    );
    io::io_error_methods!(
        IoNamespaceAiInternal::_gcp_quota_project_id,
        host_unavailable("gcp-credentials")
    );
    io::io_error_methods!(
        IoNamespaceAiInternal::_aws_sign_request,
        host_unavailable("aws-credentials")
    );
    io::io_error_methods!(
        IoNamespaceAiInternal::_aws_resolve_region,
        host_unavailable("aws-credentials")
    );
}

impl io::IoPackageBaml for DefaultIoOps {}

/// Builder for composing an [`io::SysOps`] table by overriding namespaces.
///
/// Starts with built-in operations and unsupported platform fallbacks,
/// then allows overriding namespaces:
///
/// ```ignore
/// let ops = IoSysOpsBuilder::new()
///     .with_http_instance(Arc::new(my_http_impl))
///     .with_env_instance(Arc::new(my_env_impl))
///     .build();
/// ```
pub struct IoSysOpsBuilder {
    inner: io::SysOps,
}

impl IoSysOpsBuilder {
    /// Use built-in operations and unsupported platform fallbacks.
    ///
    /// To preserve an existing platform's implementations while overriding
    /// selected operations, start with [`Self::from_ops`].
    pub fn new() -> Self {
        Self {
            inner: io::SysOps::from_impl(DefaultIoOps),
        }
    }

    /// Start from an existing table, preserving operations that are not overridden.
    #[must_use]
    pub fn from_ops(ops: io::SysOps) -> Self {
        Self { inner: ops }
    }

    /// Consume the builder and return the composed [`io::SysOps`] table.
    pub fn build(self) -> io::SysOps {
        self.inner
    }

    /// Override the `env` namespace with a pre-built instance.
    #[must_use]
    pub fn with_env_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceEnv + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_env(instance);
        self
    }

    /// Override the `io` namespace with a pre-built instance.
    #[must_use]
    pub fn with_io_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceIo + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_io(instance);
        self
    }

    /// Override the `fs` namespace (including `fs.File` methods) with a pre-built instance.
    #[must_use]
    pub fn with_fs_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceFs + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_fs(instance);
        self
    }

    /// Override only `baml.fs.read`, leaving other filesystem operations unchanged.
    #[must_use]
    pub fn with_fs_read_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceFs + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_fs_read = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_read(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `fs` namespace with a default-constructible type.
    #[must_use]
    pub fn with_fs<T: io::IoNamespaceFs + Default + Send + Sync + 'static>(self) -> Self {
        self.with_fs_instance(Arc::new(T::default()))
    }

    /// Override the `glob` namespace (including `glob.Glob` methods) with a pre-built instance.
    #[must_use]
    pub fn with_glob_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceGlob + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_glob(instance);
        self
    }

    /// Override the `http` namespace (including `http.Response` methods) with a pre-built instance.
    #[must_use]
    pub fn with_http_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHttp + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_http(instance);
        self
    }

    /// Override the non-streaming HTTP client operations only.
    ///
    /// Installs `_fetch`, `_send`, `Response.text`, and `Response.bytes` while
    /// leaving SSE, server, TLS, and response-construction slots unchanged.
    #[must_use]
    pub fn with_http_fetch_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHttp + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_http__fetch = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http__fetch(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http__send = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http__send(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_text = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_text(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_bytes = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `net` namespace (`TcpStream` / `TcpListener` / `UdpSocket`
    /// methods) with a pre-built instance.
    #[must_use]
    pub fn with_net_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceNet + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_net(instance);
        self
    }

    /// Override the `net` namespace with a default-constructible type.
    #[must_use]
    pub fn with_net<T: io::IoNamespaceNet + Default + Send + Sync + 'static>(self) -> Self {
        self.with_net_instance(Arc::new(T::default()))
    }

    /// Override the `sys` namespace with a pre-built instance.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_sys_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceSys + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_sys(instance);
        self
    }

    /// Override the `sys` namespace with a default-constructible type.
    #[must_use]
    pub fn with_sys<T: io::IoNamespaceSys + Default + Send + Sync + 'static>(self) -> Self {
        self.with_sys_instance(Arc::new(T::default()))
    }

    /// Override the `host` namespace (host-callable dispatch) with a pre-built instance.
    ///
    /// Only the WASM bridge uses this builder method: it composes its `SysOps`
    /// here and injects its JS dispatch impl explicitly, wiring the
    /// [`io::IoNamespaceHost::call_host_value`] sysop to a bridge-specific
    /// dispatch implementation that fires the host-language callable. The
    /// native bridges (Python, Node, Go) instead wire dispatch through
    /// `sys_native::NativeSysOps` (passed to [`SysOps::from_impl`]) and never
    /// call this method.
    #[must_use]
    pub fn with_host_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceHost + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_host(instance);
        self
    }

    /// Override the `time` namespace, including timezone operations, with a pre-built instance.
    #[must_use]
    pub fn with_time_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceTime + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_time(instance);
        self
    }

    /// Override the `random` namespace (`SystemRandom.random` / `random_int`)
    /// with a pre-built instance.
    #[must_use]
    pub fn with_random_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceRandom + Send + Sync + 'static>,
    ) -> Self {
        self.inner.set_random(instance);
        self
    }
}

impl Default for IoSysOpsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

use ::bex_heap::{BexExternalValue, BexHeap};
use ::std::sync::Arc;
// Re-export io::SysOps as the primary SysOps type.
use ::sys_types::{
    CallId, LlmFunctionInfo, SysOpContext, SysOpOutput, VmBamlError, VmInternalError, VmPanic,
    VmRustFnError,
};
pub use io::SysOps;

/// Builder for composing a [`SysOps`] table by overriding namespaces.
pub type SysOpsBuilder = IoSysOpsBuilder;

#[cfg(test)]
mod tests {
    use bex_heap::HeapPermit;
    use bex_vm_types::SysOp;

    use super::*;

    fn test_heap() -> Arc<BexHeap> {
        BexHeap::new(vec![])
    }

    fn test_ctx() -> SysOpContext {
        SysOpContext::empty()
    }

    async fn test_permit() -> bex_heap::ActiveHeapPermit<()> {
        bex_heap::HeapPermitManager::new()
            .new_permit(())
            .await
            .acquire()
            .await
    }

    #[test]
    fn namespace_overrides_include_recent_operations() {
        let original = IoSysOpsBuilder::new().build();
        let time = IoSysOpsBuilder::from_ops(original.clone())
            .with_time_instance(Arc::new(DefaultIoOps))
            .build();
        for (before, after) in [
            (&original.baml_time_instant_now, &time.baml_time_instant_now),
            (
                &original.baml_time_system_timezone,
                &time.baml_time_system_timezone,
            ),
            (
                &original.baml_time__tz_offset_at,
                &time.baml_time__tz_offset_at,
            ),
            (
                &original.baml_time__tz_to_instant,
                &time.baml_time__tz_to_instant,
            ),
        ] {
            assert!(!Arc::ptr_eq(before, after));
        }
        assert!(Arc::ptr_eq(&original.baml_fs_read, &time.baml_fs_read));

        let fs = IoSysOpsBuilder::from_ops(original.clone())
            .with_fs_instance(Arc::new(DefaultIoOps))
            .build();
        assert!(!Arc::ptr_eq(
            &original.baml_fs_remove_dir,
            &fs.baml_fs_remove_dir
        ));
        assert!(!Arc::ptr_eq(
            &original.baml_fs_remove_dir_all,
            &fs.baml_fs_remove_dir_all
        ));
        assert!(Arc::ptr_eq(
            &original.baml_time_instant_now,
            &fs.baml_time_instant_now
        ));
    }

    #[test]
    fn partial_overrides_preserve_other_operations() {
        let original = IoSysOpsBuilder::new().build();
        let fs = IoSysOpsBuilder::from_ops(original.clone())
            .with_fs_read_instance(Arc::new(DefaultIoOps))
            .build();
        assert!(!Arc::ptr_eq(&original.baml_fs_read, &fs.baml_fs_read));
        assert!(Arc::ptr_eq(&original.baml_fs_write, &fs.baml_fs_write));
        assert!(Arc::ptr_eq(
            &original.baml_fs_remove_dir,
            &fs.baml_fs_remove_dir
        ));

        let http = IoSysOpsBuilder::from_ops(original.clone())
            .with_http_fetch_instance(Arc::new(DefaultIoOps))
            .build();
        assert!(!Arc::ptr_eq(
            &original.baml_http__fetch,
            &http.baml_http__fetch
        ));
        assert!(Arc::ptr_eq(
            &original.baml_http__fetch_sse,
            &http.baml_http__fetch_sse
        ));
        assert!(Arc::ptr_eq(
            &original.baml_http_server_bind,
            &http.baml_http_server_bind
        ));
    }

    #[tokio::test]
    async fn test_host_unavailable_returns_panic() {
        use bex_vm_types::errors::{VmPanic, VmRustFnError};
        use sys_types::SysOpResult;

        let heap = test_heap();
        let ctx = test_ctx();
        let op = SysOps::host_unavailable(SysOp::BamlSysShell);
        let permit = test_permit().await;
        let result = op(&heap, permit.proof(), vec![], &ctx, CallId::next());
        match result {
            SysOpResult::Ready(Err(e)) => {
                assert!(matches!(
                    e.payload,
                    sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(
                        VmPanic::HostUnavailable { .. }
                    ))
                ));
                assert_eq!(e.fn_name, SysOp::BamlSysShell);
            }
            _ => panic!("Expected HostUnavailable panic"),
        }
    }

    #[tokio::test]
    async fn test_all_host_unavailable() {
        use bex_vm_types::errors::{VmPanic, VmRustFnError};
        use sys_types::{OpError, SysOpResult};

        let heap = test_heap();
        let ctx = test_ctx();
        let ops = SysOps::all_host_unavailable();
        let permit = test_permit().await;

        // Test fs_open panics with HostUnavailable
        let result = (ops.baml_fs_open)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlFsOpen,
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(
                    VmPanic::HostUnavailable { .. }
                )),
            }))
        ));

        // Test shell panics with HostUnavailable
        let result = (ops.baml_sys_shell)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlSysShell,
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::Panic(
                    VmPanic::HostUnavailable { .. }
                )),
            }))
        ));
    }

    #[tokio::test]
    async fn test_sys_ops_get() {
        use sys_types::SysOpResult;

        let ops = SysOps::all_host_unavailable();
        let heap = test_heap();
        let ctx = test_ctx();
        let permit = test_permit().await;

        // Test that get() returns the correct function pointer
        let fn_ptr = ops.get(SysOp::BamlFsOpen);
        let result = fn_ptr(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(result, SysOpResult::Ready(Err(_))));
    }
}
