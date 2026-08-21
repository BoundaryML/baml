// ============================================================================
// IO pipeline (generated from .baml files via baml_builtins2_codegen)
// ============================================================================

// SysOps struct, IO traits (IoClassFsFile, IoNamespaceFs, etc.),
// view/owned types, from_impl, all_unsupported — all generated from
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
/// messages: both still abort the sysop as a `DevOther`, but the
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
) -> VmBamlError {
    match outcome {
        sys_types::ResolveOutcome::Found(_, _) => {
            // Unreachable in practice — caller only invokes this on a miss.
            // We still produce a coherent message rather than panicking so
            // a future refactor can't accidentally trip on this.
            VmBamlError::DevOther {
                message: format!(
                    "internal: llm_function_lookup_error called with a Found \
                     outcome for `{function_name}`"
                ),
            }
        }
        sys_types::ResolveOutcome::NotFound => VmBamlError::DevOther {
            message: format!("LLM function not found: {function_name}"),
        },
        sys_types::ResolveOutcome::Ambiguous => VmBamlError::DevOther {
            message: format!(
                "LLM function name `{function_name}` is ambiguous: two or more \
                 namespaced functions end with `.{function_name}`. Pass a fully \
                 qualified name (e.g. `<pkg>.<ns>.{function_name}`) to disambiguate."
            ),
        },
    }
}

/// Blanket impl — `ParseCache.new()` creates a SAP cache from a type descriptor.
/// Parameter order follows the BAML decl (`new(streaming, target)` — stream
/// type first, mirroring `ParseCache<TStream, TFinal>`).
impl<T> io::IoClassSapParseCache for T {
    fn new(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        stream_target: baml_type::RuntimeTy,
        target: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sap::ParseCache> {
        let compiled =
            match ::bex_sap::CompiledSapModel::from_sys_op_context(ctx, target, stream_target) {
                Ok(compiled) => compiled,
                Err(e) => {
                    return SysOpOutput::err(VmBamlError::InvalidArgument {
                        message: e.to_string(),
                    });
                }
            };
        let sap = crate::sap::SapParseCache::new(compiled);
        let data: std::sync::Arc<dyn std::any::Any + Send + Sync> = std::sync::Arc::new(sap);
        SysOpOutput::ok(io::owned::sap::ParseCache { _data: data })
    }
}

/// Blanket impl — `Context.output_format_with(...)` re-renders the return
/// type's schema with caller options (BEP-049 §10 / M5b.2). `Context._output_format`
/// carries the prebuilt schema as an opaque handle, so this only re-renders it.
impl<T> io::IoClassAiContext for T {
    #[allow(clippy::too_many_arguments)]
    fn output_format_with(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        context: io::owned::ai::Context,
        prefix: Option<String>,
        or_splitter: Option<String>,
        enum_value_prefix: Option<String>,
        hoisted_class_prefix: Option<String>,
        always_hoist_enums: Option<bool>,
        quote_class_fields: Option<bool>,
        hoist_classes: Option<Vec<String>>,
        map_style: Option<String>,
        render_null_as: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        // Render the prebuilt schema handle with the caller's options. The
        // `Option → RenderOptions` mapping lives inside `output_format` (those
        // option types are module-internal there).
        let content = unwrap_output_format(&context._output_format);
        let rendered = crate::output_format::render_output_format_content(
            &content,
            prefix,
            or_splitter,
            enum_value_prefix,
            hoisted_class_prefix,
            always_hoist_enums,
            quote_class_fields,
            hoist_classes,
            map_style,
            render_null_as,
        );
        match rendered {
            Ok(rendered) => SysOpOutput::ok(rendered),
            // Keep the structured render failure at the VM boundary instead of
            // turning it into a successful empty schema. The public BAML method
            // retains its existing effect signature for prompt-tag compatibility.
            Err(error) => SysOpOutput::err(VmBamlError::RenderPrompt {
                message: error.to_string(),
            }),
        }
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
        t: baml_type::RuntimeTy,
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

    use baml_type::RuntimeTy;
    use bex_external_types::BexExternalValue;
    use serde_json::{Value, json};
    use sys_types::SysOpContext;

    fn json_alias_ty() -> RuntimeTy {
        RuntimeTy::TypeAlias(
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
                key_type: RuntimeTy::string(),
                value_type: json_alias_ty(),
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, json_to_bex(value)))
                    .collect(),
            },
        }
    }

    pub(super) fn json_schema(ty: &RuntimeTy, ctx: &SysOpContext) -> Result<Value, String> {
        let mut builder = SchemaBuilder {
            ctx,
            definitions: serde_json::Map::new(),
            building: HashSet::new(),
            referenced: HashSet::new(),
        };

        let (mut root, root_class_key) = match ty {
            RuntimeTy::Class(name, _, _) => {
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
        fn ty_schema(&mut self, ty: &RuntimeTy) -> Result<Value, String> {
            match ty {
                RuntimeTy::Int { .. } | RuntimeTy::Bigint { .. } => {
                    Ok(json!({ "type": "integer" }))
                }
                RuntimeTy::Float { .. } => Ok(json!({ "type": "number" })),
                RuntimeTy::String { .. } => Ok(json!({ "type": "string" })),
                RuntimeTy::Bool { .. } => Ok(json!({ "type": "boolean" })),
                RuntimeTy::Null { .. } => Ok(json!({ "type": "null" })),
                RuntimeTy::Uint8Array { .. } => Ok(json!({ "type": "string" })),
                RuntimeTy::Literal(lit, _, _) => Ok(Self::literal_schema(lit)),
                RuntimeTy::List(inner, _) => Ok(json!({
                    "type": "array",
                    "items": self.ty_schema(inner)?,
                })),
                RuntimeTy::Map { value, .. } => Ok(json!({
                    "type": "object",
                    "additionalProperties": self.ty_schema(value)?,
                })),
                RuntimeTy::Union(members, _) => self.union_schema(members),
                RuntimeTy::Enum(name, _) => Self::enum_schema(name, self.ctx),
                RuntimeTy::Class(name, _, _) => self.class_ref(name),
                RuntimeTy::TypeAlias(name, _) => self.type_alias_ref(name),
                RuntimeTy::BuiltinUnknown { .. } => Ok(json!({})),
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

        fn union_schema(&mut self, members: &[RuntimeTy]) -> Result<Value, String> {
            let has_null = members.iter().any(RuntimeTy::is_null);
            let non_null: Vec<&RuntimeTy> = members.iter().filter(|m| !m.is_null()).collect();
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

        fn enum_schema(name: &baml_type::TypeName, ctx: &SysOpContext) -> Result<Value, String> {
            let enum_def = find_enum_definition(ctx, name)
                .ok_or_else(|| format!("json_schema: unknown enum `{}`", name.display_name()))?;
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

        fn class_ref(&mut self, name: &baml_type::TypeName) -> Result<Value, String> {
            let key = definition_key(name);
            self.referenced.insert(key.clone());

            if !self.definitions.contains_key(&key) && !self.building.contains(&key) {
                self.building.insert(key.clone());
                let definition = self.class_object(name)?;
                self.building.remove(&key);
                self.definitions.insert(key.clone(), definition);
            }

            Ok(json!({ "$ref": format!("#/$defs/{}", json_pointer_escape(&key)) }))
        }

        fn type_alias_ref(&mut self, name: &baml_type::TypeName) -> Result<Value, String> {
            let key = definition_key(name);
            self.referenced.insert(key.clone());

            if !self.definitions.contains_key(&key) && !self.building.contains(&key) {
                let target = find_type_alias_definition(self.ctx, name)
                    .cloned()
                    .ok_or_else(|| {
                        format!("json_schema: unknown type alias `{}`", name.display_name())
                    })?;
                self.building.insert(key.clone());
                let definition = self.ty_schema(&target)?;
                self.building.remove(&key);
                self.definitions.insert(key.clone(), definition);
            }

            Ok(json!({ "$ref": format!("#/$defs/{}", json_pointer_escape(&key)) }))
        }

        fn class_object(&mut self, name: &baml_type::TypeName) -> Result<Value, String> {
            let class_def = find_class_definition(self.ctx, name)
                .ok_or_else(|| format!("json_schema: unknown class `{}`", name.display_name()))?
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

    fn definition_key(name: &baml_type::TypeName) -> String {
        name.display_name().to_string()
    }

    fn json_pointer_escape(value: &str) -> String {
        value.replace('~', "~0").replace('/', "~1")
    }

    fn find_class_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a sys_types::ClassDefinition> {
        ctx.class_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .class_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, definition)| definition);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
    }

    fn find_enum_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a sys_types::EnumDefinition> {
        ctx.enum_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .enum_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, definition)| definition);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
    }

    fn find_type_alias_definition<'a>(
        ctx: &'a SysOpContext,
        type_name: &baml_type::TypeName,
    ) -> Option<&'a RuntimeTy> {
        ctx.type_alias_definitions.get(type_name).or_else(|| {
            let mut matches = ctx
                .type_alias_definitions
                .iter()
                .filter(|(name, _)| name.display_name() == type_name.display_name())
                .map(|(_, ty)| ty);
            let first = matches.next()?;
            matches.next().is_none().then_some(first)
        })
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use baml_type::{RuntimeTy, TyAttr, TypeName};
        use serde_json::json;
        use sys_types::{
            ClassDefinition, ClassFieldDefinition, EnumDefinition, EnumVariantDefinition,
            SysOpContext,
        };

        use super::json_schema;

        fn type_name(name: &str) -> TypeName {
            TypeName::from_dotted_path(name)
        }

        fn class_ty(name: &TypeName) -> RuntimeTy {
            RuntimeTy::Class(name.clone(), Vec::new(), TyAttr::default())
        }

        fn alias_ty(name: &TypeName) -> RuntimeTy {
            RuntimeTy::TypeAlias(name.clone(), TyAttr::default())
        }

        fn field(name: &str, field_type: RuntimeTy) -> ClassFieldDefinition {
            ClassFieldDefinition {
                name: name.to_string(),
                field_type,
                field_template: None,
                description: None,
                alias: None,
                skip: false,
            }
        }

        fn class_definition(name: &TypeName, fields: Vec<ClassFieldDefinition>) -> ClassDefinition {
            ClassDefinition {
                name: name.display_name().to_string(),
                description: None,
                alias: None,
                fields,
            }
        }

        #[test]
        fn self_referential_class_uses_defs_ref() {
            let node = type_name("pkg.Node");
            let mut classes = indexmap::IndexMap::new();
            classes.insert(
                node.clone(),
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
                a.clone(),
                class_definition(&a, vec![field("b", class_ty(&b))]),
            );
            classes.insert(
                b.clone(),
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
                holder.clone(),
                class_definition(&holder, vec![field("value", class_ty(&escaped))]),
            );
            classes.insert(
                escaped.clone(),
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
                status.clone(),
                EnumDefinition {
                    name: "Status".to_string(),
                    description: None,
                    alias: None,
                    variants: vec![
                        EnumVariantDefinition {
                            name: "Ready".to_string(),
                            description: None,
                            alias: Some("ready-now".to_string()),
                        },
                        EnumVariantDefinition {
                            name: "Done".to_string(),
                            description: None,
                            alias: None,
                        },
                    ],
                },
            );
            let mut ctx = SysOpContext::empty();
            ctx.enum_definitions = Arc::new(enums);

            let schema = json_schema(&RuntimeTy::Enum(status, TyAttr::default()), &ctx)
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
            aliases.insert(json_name, target);
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

/// BEP-049 §10 (M5b): the `ctx.output_format` schema string.
pub fn render_output_format_op(
    return_type: &baml_type::RuntimeTy,
    ctx: &SysOpContext,
) -> SysOpOutput<String> {
    SysOpOutput::ok(crate::output_format::render_output_format(return_type, ctx))
}

/// BEP-049 §10 (M5b.2): build the opaque schema handle `Context._output_format`
/// carries; `output_format_with(...)` renders it with caller options.
pub fn build_output_format_op(
    return_type: &baml_type::RuntimeTy,
    ctx: &SysOpContext,
) -> SysOpOutput<io::owned::ai::OutputFormat> {
    let content = crate::output_format::build_output_format_content(return_type, ctx);
    SysOpOutput::ok(wrap_output_format(std::sync::Arc::new(content)))
}

/// Look up an LLM function's declared return type by name.
pub fn get_return_type_op(
    function_name: &str,
    ctx: &SysOpContext,
) -> SysOpOutput<baml_type::RuntimeTy> {
    let outcome = lookup_llm_function(function_name, &ctx.llm_functions);
    let sys_types::ResolveOutcome::Found(_, info) = outcome else {
        return SysOpOutput::err(llm_function_lookup_error(function_name, &outcome));
    };
    SysOpOutput::ok(info.return_type.clone())
}

/// Schema-aligned parsing operations back both public `baml.sap.parse` and
/// incremental `ai.stream.Stream` parsing.
impl<T> io::IoNamespaceSap for T {
    fn __parse_final(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::sap::ParseCache,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<crate::sap::SapParseCache>() else {
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid ParseCache: expected SapParseCache".into(),
            });
        };
        SysOpOutput::Ready(
            crate::sap::execute_sap_parse_final(&json, &sap, ctx).map_err(VmRustFnError::from),
        )
    }

    fn __parse_partial(
        &self,
        _heap: &std::sync::Arc<BexHeap>,
        _call_id: CallId,
        json: String,
        cache: io::owned::sap::ParseCache,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        let Ok(sap) = cache._data.downcast::<crate::sap::SapParseCache>() else {
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Invalid ParseCache: expected SapParseCache".into(),
            });
        };
        let result = match crate::sap::execute_sap_parse_partial(&json, &sap, ctx) {
            Ok(Some(value)) => Ok(value),
            Ok(None) => Ok(BexExternalValue::instance(
                "baml.sap.NoYield",
                ::indexmap::IndexMap::new(),
            )),
            Err(e) => Err(VmRustFnError::from(e)),
        };
        SysOpOutput::Ready(result)
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
/// retain the generated defaults.
struct DefaultIoOps;

impl io::IoClassReflectPackage for DefaultIoOps {
    fn _compile(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _files: indexmap::IndexMap<String, String>,
        _packages: indexmap::IndexMap<String, io::owned::reflect::Package>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::reflect::Package> {
        // BexEngine intercepts this operation and delegates to its injected
        // RuntimeCompiler before the provider table is consulted.
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "runtime compiler is not installed".to_string(),
        })
    }
}

impl io::IoClassReflectSession for DefaultIoOps {
    fn _compile(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _session: io::owned::reflect::Session,
        _source: String,
        _type_arg_0: baml_type::RuntimeTy,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::reflect::Package> {
        // BexEngine intercepts Session compilation for the same reason as
        // Package.compile: the concrete compiler is injected above sys_ops.
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "runtime compiler is not installed".to_string(),
        })
    }
}

impl io::IoNamespaceReflect for DefaultIoOps {}

impl io::IoClassFsFile for DefaultIoOps {
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn seek_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _whence: BexExternalValue,
        _offset: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: io::owned::fs::File,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceFs for DefaultIoOps {
    fn open(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::fs::File> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn exists(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn remove_dir_all(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn size(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<io::owned::fs::DirEntry>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn mkdir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _options: io::owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    // `chmod` and `symlink` declare `throws root.errors.Io`, so — unlike their
    // siblings above — they report an absent platform facility as `Io` rather
    // than `Unsupported`. An `Unsupported` here would be off-contract: nothing
    // in the declared throw set can hold it, so it would escape every typed
    // `catch` arm the caller can write.
    fn chmod(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "File permissions are not supported on this platform".to_string(),
        })
    }

    fn symlink(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _target: String,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "Symbolic links are not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpResponse for DefaultIoOps {
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn new_streaming(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn end(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _r: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpTlsConfig for DefaultIoOps {
    fn _new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::TlsConfig> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpServer for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Server> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _serve(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _server: io::owned::http::Server,
        _handler: bex_external_types::Handle,
        _tls_config: Option<io::owned::http::TlsConfig>,
        _allow_http1: bool,
        _allow_http2: bool,
        _max_body_size: i64,
        _max_connections: i64,
        _header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassHttpSseStream for DefaultIoOps {
    fn next(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceHttp for DefaultIoOps {
    fn _fetch(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _url: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _send(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _fetch_sse(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _req: io::owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _first_event_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::SseStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassWsWsStream for DefaultIoOps {
    fn send(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _stream: io::owned::ws::WsStream,
        _text: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn next(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _stream: io::owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _stream: io::owned::ws::WsStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceWs for DefaultIoOps {
    fn _connect(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _url: String,
        _headers: indexmap::IndexMap<String, String>,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::ws::WsStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetTcpStream for DefaultIoOps {
    fn _connect(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _data: Vec<u8>,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::TcpStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetTcpListener for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpListener> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn accept(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::TcpStream> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _l: io::owned::net::TcpListener,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassNetUdpSocket for DefaultIoOps {
    fn bind(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::UdpSocket> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _send_to(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _data: Vec<u8>,
        _addr: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _recv_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::net::Datagram> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: io::owned::net::UdpSocket,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceNet for DefaultIoOps {}

impl io::IoNamespaceEnv for DefaultIoOps {
    fn get(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _key: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceIo for DefaultIoOps {
    fn input(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _prompt: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn print(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn println(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprint(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn eprintln(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _s: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassSysProcess for DefaultIoOps {
    fn write_stdin(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _process: io::owned::sys::Process,
        _data: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn close_stdin(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn wait(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ProcessExit> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn kill(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _process: io::owned::sys::Process,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

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

impl io::IoClassSysProcessLineStream for DefaultIoOps {
    fn _next(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _processlinestream: io::owned::sys::ProcessLineStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _processlinestream: io::owned::sys::ProcessLineStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::ok(())
    }
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

    fn exec(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _program: String,
        _args: Option<Vec<String>>,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn start_process(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _program: String,
        _args: Option<Vec<String>>,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::Process> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn shell(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _command: String,
        _options: Option<io::owned::sys::ProcessOptions>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::sys::ShellOutput> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn sleep(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _delay: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    // `baml.sys.pid` declares `throws never`, so a platform without process
    // IDs cannot report `Unsupported` as a catchable error — it panics with
    // `baml.panics.HostUnavailable`, exactly as `SystemRandom` does when no
    // entropy source is reachable.
    fn pid(&self, _h: &Arc<BexHeap>, _c: CallId, _ctx: &SysOpContext) -> SysOpOutput<i64> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "process-id".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassGlobGlob for DefaultIoOps {
    fn scan(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _glob: io::owned::glob::Glob,
        _root: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn matches(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _glob: io::owned::glob::Glob,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceGlob for DefaultIoOps {
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _pattern: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::glob::Glob> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceHost for DefaultIoOps {
    fn call_host_value(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _handle: BexExternalValue,
        _args: Vec<BexExternalValue>,
        _type_arg_0: baml_type::RuntimeTy,
        _type_arg_1: baml_type::RuntimeTy,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassTimeInstant for DefaultIoOps {
    fn now(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::time::Instant> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceTime for DefaultIoOps {
    fn system_timezone(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _tz_offset_at(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _timezone: String,
        _at_ns: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<i64>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _tz_to_instant(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _timezone: String,
        _civil_ns: Arc<num_bigint::BigInt>,
        _disambiguation: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<Arc<num_bigint::BigInt>>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoClassRandomSystemRandom for DefaultIoOps {
    fn random(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _bytes: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "randomness".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn random_int(&self, _h: &Arc<BexHeap>, _c: CallId, _ctx: &SysOpContext) -> SysOpOutput<i64> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "randomness".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceRandom for DefaultIoOps {}

impl io::IoNamespaceAiInternal for DefaultIoOps {
    fn render_output_format(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        return_type: baml_type::RuntimeTy,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        render_output_format_op(&return_type, ctx)
    }
    fn build_output_format(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        return_type: baml_type::RuntimeTy,
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
    ) -> SysOpOutput<baml_type::RuntimeTy> {
        get_return_type_op(&function_name, ctx)
    }
    fn _gcp_access_token(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _credentials_json: Option<String>,
        _scope: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _gcp_project_id(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _credentials_json: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _gcp_quota_project_id(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _credentials_json: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _aws_sign_request(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _request: BexExternalValue,
        _service: String,
        _region: Option<String>,
        _profile: Option<String>,
        _access_key_id: Option<String>,
        _secret_access_key: Option<String>,
        _session_token: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<BexExternalValue> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
    fn _aws_resolve_region(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _region: Option<String>,
        _profile: Option<String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoPackageBaml for DefaultIoOps {}

/// Builder for composing an [`io::SysOps`] table by overriding namespaces.
///
/// Starts with all operations returning `Unsupported` (except LLM, which uses
/// the blanket implementation), and allows selectively overriding namespaces:
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
    /// Create a new builder with all operations defaulting to `Unsupported`,
    /// except LLM ops which use the real blanket implementation.
    ///
    /// Every operation not overridden afterwards throws — including ones a
    /// host may never think about (`baml.time.Instant.now`, `random`, the
    /// per-class `fs::File`/`http::Response` readers). A host that wants a
    /// working platform with a few operations *intercepted* wants
    /// [`IoSysOpsBuilder::from_ops`] instead.
    pub fn new() -> Self {
        Self {
            inner: io::SysOps::from_impl(DefaultIoOps),
        }
    }

    /// Start from an existing table — typically `SysOps::native()` — and
    /// override individual namespaces on top of it.
    ///
    /// This is the right base for an *interposing* host (the playground
    /// intercepts HTTP, env and IO to route them through its UI, and wants
    /// the platform's real behavior for everything else): the set of
    /// operations it must not break is open-ended and grows with the
    /// standard library, so it cannot be enumerated at the call site.
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
        self.inner.baml_env_get = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_env_get(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `io` namespace with a pre-built instance.
    #[must_use]
    pub fn with_io_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceIo + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_io_input = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_input(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_print = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_print(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_println = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_println(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_eprint = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_eprint(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_io_eprintln = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_io_eprintln(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `fs` namespace (including `fs.File` methods) with a pre-built instance.
    #[must_use]
    pub fn with_fs_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceFs + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_fs_open = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_open(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_exists = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_exists(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_remove = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_remove(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_size = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_size(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_write_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_write_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_text = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_text(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_read_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_read_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_seek_from = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_seek_from(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_file_write_bytes = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_file_write_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_read_dir = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_read_dir(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_mkdir = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_mkdir(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_chmod = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_chmod(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_fs_symlink = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_fs_symlink(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override only `baml.fs.read`, leaving every other filesystem operation unsupported.
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
        self.inner.baml_glob_new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_glob_glob_scan = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_glob_scan(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_glob_glob_matches = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_glob_glob_matches(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `http` namespace (including `http.Response` methods) with a pre-built instance.
    #[must_use]
    pub fn with_http_instance(
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
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_bytes(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http__fetch_sse = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http__fetch_sse(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_ssestream_next = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_ssestream_next(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_ssestream_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_ssestream_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_new_streaming = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_new_streaming(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_response_end = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_response_end(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_tlsconfig__new = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_tlsconfig__new(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_server_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_server_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_http_server__serve = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_http_server__serve(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the non-streaming HTTP client operations only.
    ///
    /// Installs `_fetch`, `_send`, `Response.text`, and `Response.bytes` while
    /// leaving SSE, server, TLS, and response-construction slots unsupported.
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
        self.inner.baml_net_tcpstream__connect = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__connect(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream__read = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__read(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream__write = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream__write(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcpstream_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcpstream_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_accept = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_accept(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_tcplistener_close = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_tcplistener_close(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket_bind = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket_bind(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket__send_to = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket__send_to(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket__recv_from = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket__recv_from(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_net_udpsocket_close = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_net_udpsocket_close(heap, permit, args, ctx, call_id)
            })
        };
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
        self.inner.baml_sys_exec = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_exec(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_shell = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_shell(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_sleep = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_sleep(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_sys_pid = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_sys_pid(heap, permit, args, ctx, call_id)
            })
        };
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
        self.inner.baml_host_call_host_value = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_host_call_host_value(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `time` namespace (`Instant.now`) with a pre-built instance.
    #[must_use]
    pub fn with_time_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceTime + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_time_instant_now = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_time_instant_now(heap, permit, args, ctx, call_id)
            })
        };
        self
    }

    /// Override the `random` namespace (`SystemRandom.random` / `random_int`)
    /// with a pre-built instance.
    #[must_use]
    pub fn with_random_instance(
        mut self,
        instance: Arc<dyn io::IoNamespaceRandom + Send + Sync + 'static>,
    ) -> Self {
        self.inner.baml_random_systemrandom_rng_random = {
            let t = instance.clone();
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_random_systemrandom_rng_random(heap, permit, args, ctx, call_id)
            })
        };
        self.inner.baml_random_systemrandom_rng_random_int = {
            let t = instance;
            Arc::new(move |heap, permit, args, ctx, call_id| {
                t.__glue_baml_random_systemrandom_rng_random_int(heap, permit, args, ctx, call_id)
            })
        };
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
    CallId, LlmFunctionInfo, SysOpContext, SysOpOutput, VmBamlError, VmPanic, VmRustFnError,
};
pub use io::SysOps;

/// Builder for composing a [`SysOps`] table by overriding namespaces.
///
/// Starts with the built-in prompt/SAP implementations and otherwise returns
/// `Unsupported`, then allows selectively overriding namespaces.
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

    #[tokio::test]
    async fn test_unsupported_returns_error() {
        use bex_vm_types::errors::{VmBamlError, VmRustFnError};
        use sys_types::SysOpResult;

        let heap = test_heap();
        let ctx = test_ctx();
        let op = SysOps::unsupported(SysOp::BamlSysShell);
        let permit = test_permit().await;
        let result = op(&heap, permit.proof(), vec![], &ctx, CallId::next());
        match result {
            SysOpResult::Ready(Err(e)) => {
                assert!(matches!(
                    e.payload,
                    sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                        VmBamlError::Unsupported { .. }
                    ))
                ));
                assert_eq!(e.fn_name, SysOp::BamlSysShell);
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    #[tokio::test]
    async fn test_all_unsupported() {
        use bex_vm_types::errors::{VmBamlError, VmRustFnError};
        use sys_types::{OpError, SysOpResult};

        let heap = test_heap();
        let ctx = test_ctx();
        let ops = SysOps::all_unsupported();
        let permit = test_permit().await;

        // Test fs_open returns Unsupported
        let result = (ops.baml_fs_open)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlFsOpen,
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                    VmBamlError::Unsupported { .. }
                )),
            }))
        ));

        // Test shell returns Unsupported
        let result = (ops.baml_sys_shell)(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError {
                fn_name: SysOp::BamlSysShell,
                payload: sys_types::OpErrorPayload::Vm(VmRustFnError::BamlError(
                    VmBamlError::Unsupported { .. }
                )),
            }))
        ));
    }

    #[tokio::test]
    async fn test_sys_ops_get() {
        use sys_types::SysOpResult;

        let ops = SysOps::all_unsupported();
        let heap = test_heap();
        let ctx = test_ctx();
        let permit = test_permit().await;

        // Test that get() returns the correct function pointer
        let fn_ptr = ops.get(SysOp::BamlFsOpen);
        let result = fn_ptr(&heap, permit.proof(), vec![], &ctx, CallId::next());
        assert!(matches!(result, SysOpResult::Ready(Err(_))));
    }
}
