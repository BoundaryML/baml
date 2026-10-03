//! `baml.sap`: schema-aligned parsing (SAP) of an LLM's text into a declared
//! type.
//!
//! VM natives, not sys ops: parsing does no IO, so it runs on the calling VM
//! with the heap at hand. `_new_parse_cache<T>` builds the parse model from the
//! declarations `T` reaches, and the parse methods allocate their result
//! directly on the heap, so no definition table is copied and no value tree is
//! landed by name.

use std::{any::Any, collections::HashMap, sync::Arc};

use baml_type::{DeclarationName, Literal, typetag::TypeTag};
use bex_heap::TlabHolder;
use bex_sap::{
    CompiledSapModel,
    baml_value::BamlValue,
    deserializer::{coercer::ParsingContext, types::BamlValueWithFlags},
    jsonish,
    sap_model::{TyResolvedRef, TypeCtx, TypeRefDb},
    to_baml_ty::ToBamlTy,
};
use bex_str::BexStr;
use bex_vm_types::{
    HeapPtr, RealizedTy, RuntimeTy, TypeHead,
    types::{Instance, Object, Value},
};
use indexmap::IndexMap;
use sys_types::{DefKey, SapTy};

use super::{BamlNamespaceSap, BamlNamespaceSap_ParseCache, PackageBamlImpl};
use crate::{
    BexVm, definitions,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
    reachable,
};

/// The `_data` of a `baml.sap._ParseCache<T>`: the parse model for `T`.
///
/// A `RustData` payload is a GC leaf, so the model holds no heap pointers: it
/// names declarations by tagged name, and each parse maps those names back to
/// the declarations through [`Declarations`].
struct ParseCache {
    model: CompiledSapModel,
}

impl BamlNamespaceSap for PackageBamlImpl {
    fn _new_parse_cache(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
        let target = parse_target(vm)?;
        let model = build_model(vm, &target).map_err(|e| {
            // `throws never`: a `T` the parser cannot model comes from the
            // caller's own `parse<T>`, a program bug rather than bad input.
            VmPanic::UserPanic {
                message: format!("schema-aligned parsing cannot model this type: {e}"),
            }
        })?;
        let data: Arc<dyn Any + Send + Sync> = Arc::new(ParseCache { model });
        let data = Value::object(vm.alloc_rust_data(data));
        let class = vm.resolve_class("baml.sap._ParseCache");
        let cache = vm.tlab.alloc(Object::Instance(Instance::new(
            class,
            Box::new([target]),
            vec![data],
        )));
        Ok(Value::object(cache))
    }
}

impl BamlNamespaceSap_ParseCache for PackageBamlImpl {
    fn _parse_final(vm: &mut BexVm, cache: &Value, json: &BexStr) -> Result<Value, VmRustFnError> {
        let parsed = parse(vm, *cache, json, Completion::Final)?;
        parsed.ok_or_else(|| llm_client("SAP parse returned no value when complete"))
    }

    fn _parse_partial(
        vm: &mut BexVm,
        cache: &Value,
        json: &BexStr,
    ) -> Result<Value, VmRustFnError> {
        match parse(vm, *cache, json, Completion::Partial)? {
            Some(value) => Ok(value),
            None => {
                let no_yield = vm.resolve_class("baml.sap._NoYield");
                Ok(Value::object(vm.alloc_instance(no_yield, Vec::new())))
            }
        }
    }
}

/// Whether the text is the whole reply, or the part a stream has received.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Final,
    Partial,
}

/// The `T` of the current `_new_parse_cache<T>` or `_ParseCache<T>` method
/// call: the first type argument either way.
fn parse_target(vm: &BexVm) -> Result<RealizedTy, VmRustFnError> {
    vm.current_call_type_args().first().cloned().ok_or_else(|| {
        VmInternalError::SapValue {
            message: "a `baml.sap` native was called without its `T`".to_string(),
        }
        .into()
    })
}

/// The parse model for `target`, built from the declarations it reaches.
fn build_model(
    vm: &BexVm,
    target: &RealizedTy,
) -> Result<CompiledSapModel, bex_sap::sap_model::ConvertError> {
    let mut classes = IndexMap::new();
    let mut enums = IndexMap::new();
    let mut aliases = HashMap::new();
    for ptr in reachable::all_declarations(vm, target) {
        match vm.get_object(ptr) {
            Object::Class(class) => {
                classes.insert(
                    DefKey::new(class.type_tag, class.name.clone()),
                    definitions::class_definition(class),
                );
            }
            Object::Enum(enm) => {
                enums.insert(
                    DefKey::new(enm.type_tag, enm.name.clone()),
                    definitions::enum_definition(enm),
                );
            }
            Object::TypeAlias(alias) => {
                aliases.insert(
                    DefKey::new(
                        alias.type_tag,
                        DeclarationName::Declared(alias.name.clone()),
                    ),
                    definitions::lane_ty(&RuntimeTy::from(alias.definition.clone())),
                );
            }
            _ => {}
        }
    }
    let target = definitions::lane_ty(&RuntimeTy::from(target.clone()));
    CompiledSapModel::from_type_ctx(TypeCtx::new(&classes, enums, &aliases), target)
}

/// Parse `text` with the model in `cache` and allocate the result, or `None`
/// for a partial reply that does not parse yet.
fn parse(
    vm: &mut BexVm,
    cache: Value,
    text: &BexStr,
    completion: Completion,
) -> Result<Option<Value>, VmRustFnError> {
    let cache = vm.rust_data_field::<ParseCache>(&cache, 0)?;
    let target_type = parse_target(vm)?;
    let model = &cache.model;
    // An unresolvable target is a schema bug, not an incomplete reply.
    let target = model
        .resolved_target()
        .map_err(|e| llm_client(e.to_string()))?;
    let is_done = completion == Completion::Final;
    let value = match jsonish::parse(text.as_str(), jsonish::ParseOptions::default(), is_done) {
        Ok(value) => value,
        Err(e) if is_done => return Err(llm_client(e.to_string())),
        // A reply that opens with a fence or with prose before the JSON cannot
        // be coerced until the JSON starts: skip the partial, wait for text.
        Err(_) => return Ok(None),
    };
    let context = ParsingContext::new(model.db());
    let parsed = match TyResolvedRef::coerce(&context, target, &value) {
        Ok(parsed) => parsed,
        Err(e) if is_done => return Err(llm_client(e.to_string())),
        Err(_) => return Ok(None),
    };
    // Only a value to allocate needs the declarations: a partial that does not
    // parse yet skips the walk.
    parsed
        .map(|parsed| {
            let declarations = Declarations::reached_by(vm, &target_type);
            to_heap(vm, &parsed, model.db(), &declarations)
        })
        .transpose()
}

/// A parse failure, as `baml.errors.LlmClient`.
fn llm_client(message: impl Into<String>) -> VmRustFnError {
    VmBamlError::LlmClient {
        message: message.into(),
    }
    .into()
}

/// The declarations a parse target reaches, by tag, for mapping the model's
/// tagged names back to the heap. Built for one call: a parse never yields, so
/// no collection can move them meanwhile.
struct Declarations(HashMap<TypeTag, HeapPtr>);

impl Declarations {
    fn reached_by(vm: &BexVm, target: &RealizedTy) -> Self {
        Self(
            reachable::all_declarations(vm, target)
                .into_iter()
                .filter_map(|ptr| {
                    let tag = match vm.get_object(ptr) {
                        Object::Class(class) => class.type_tag,
                        Object::Enum(enm) => enm.type_tag,
                        Object::TypeAlias(alias) => alias.type_tag,
                        _ => return None,
                    };
                    Some((tag, ptr))
                })
                .collect(),
        )
    }

    fn get(&self, name: &DefKey) -> Result<HeapPtr, VmRustFnError> {
        self.0.get(&name.tag()).copied().ok_or_else(|| {
            invariant(format!(
                "the parsed value names `{name}`, which its target does not reach"
            ))
        })
    }

    /// `ty` with each tagged name replaced by the head of its declaration.
    fn realize(&self, ty: &SapTy) -> Result<RealizedTy, VmRustFnError> {
        let ty = ty.try_map_heads(&mut |name: &DefKey| {
            self.get(name).map(|ptr| TypeHead::new(ptr, name.tag()))
        })?;
        RealizedTy::try_from(ty)
            .map_err(|e| invariant(format!("a parsed value's type is not realized: {e}")))
    }
}

fn invariant(message: String) -> VmRustFnError {
    VmInternalError::SapValue { message }.into()
}

/// Allocate `value`: the same value the parser produced, on the heap.
fn to_heap(
    vm: &mut BexVm,
    value: &BamlValueWithFlags<'_, '_, '_, DefKey>,
    db: &TypeRefDb<'_, DefKey>,
    declarations: &Declarations,
) -> Result<Value, VmRustFnError> {
    Ok(match &value.value {
        BamlValue::String(s) => Value::object(vm.alloc_string(s.value.as_ref())),
        BamlValue::Int(i) => Value::try_int(i.value).ok_or_else(|| {
            llm_client(format!(
                "integer {} is outside the BAML integer range [{}, {}]",
                i.value,
                Value::INT_MIN,
                Value::INT_MAX
            ))
        })?,
        BamlValue::Bigint(b) => vm.try_alloc_bigint(Arc::new(b.value.clone()))?,
        BamlValue::Float(f) => Value::object(vm.alloc_float(f.value)),
        BamlValue::Bool(b) => Value::bool(b.value),
        BamlValue::Null(_) => Value::NULL,
        BamlValue::Media(_) => {
            return Err(invariant(
                "the parser produced a media value, which it cannot parse".to_string(),
            ));
        }
        BamlValue::Array(array) => {
            let element = match value.meta.ty {
                TyResolvedRef::Array(array) => array.ty.to_baml_ty(db),
                _ => SapTy::unknown(),
            };
            let element = declarations.realize(&element)?;
            let items = array
                .value
                .iter()
                .map(|item| to_heap(vm, item, db, declarations))
                .collect::<Result<Vec<_>, _>>()?;
            Value::object(vm.alloc_array(element, items))
        }
        BamlValue::Map(map) => {
            let (key, value_ty) = match value.meta.ty {
                TyResolvedRef::Map(map) => (map.key.to_baml_ty(db), map.value.to_baml_ty(db)),
                _ => (SapTy::string(), SapTy::unknown()),
            };
            let (key, value_ty) = (
                declarations.realize(&key)?,
                declarations.realize(&value_ty)?,
            );
            let mut entries = IndexMap::with_capacity(map.value.len());
            for (k, v) in &map.value {
                entries.insert(BexStr::from(k.as_ref()), to_heap(vm, v, db, declarations)?);
            }
            Value::object(vm.alloc_map(key, value_ty, entries))
        }
        BamlValue::Enum(e) => {
            let enm = declarations.get(e.name)?;
            let Object::Enum(declaration) = vm.get_object(enm) else {
                return Err(invariant(format!("`{}` is not an enum", e.name)));
            };
            let index = declaration
                .variants
                .iter()
                .position(|variant| variant.name == e.value)
                .ok_or_else(|| {
                    invariant(format!("enum `{}` has no variant `{}`", e.name, e.value))
                })?;
            Value::object(vm.alloc_variant(enm, index))
        }
        BamlValue::Class(c) => {
            let class = declarations.get(c.name)?;
            let Object::Class(declaration) = vm.get_object(class) else {
                return Err(invariant(format!("`{}` is not a class", c.name)));
            };
            let fields: Vec<(String, bool, RuntimeTy)> = declaration
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.skip, field.field_type.clone()))
                .collect();
            let mut values = Vec::with_capacity(fields.len());
            for (name, skip, field_type) in fields {
                let field_value = match c.value.get(name.as_str()) {
                    Some(parsed) => to_heap(vm, parsed, db, declarations)?,
                    None if skip => skipped_field_default(vm, c.name, &name, &field_type)?,
                    None => {
                        return Err(invariant(format!(
                            "the parsed `{}` has no field `{name}`",
                            c.name
                        )));
                    }
                };
                values.push(field_value);
            }
            Value::object(vm.alloc_instance(class, values))
        }
        BamlValue::StreamState(_) => {
            return Err(invariant(
                "the parser produced a stream state, which no target declares".to_string(),
            ));
        }
    })
}

/// The value a `@skip` field holds in a parsed instance. The parser never reads
/// the field, so it gets its type's empty value.
fn skipped_field_default(
    vm: &mut BexVm,
    class: &DefKey,
    field: &str,
    field_type: &RuntimeTy,
) -> Result<Value, VmRustFnError> {
    let no_default = |ty: &str| -> VmRustFnError {
        VmPanic::UserPanic {
            message: format!(
                "the `@skip` field `{class}.{field}` has type {ty}, which has no default value \
                 for schema-aligned parsing to fill in"
            ),
        }
        .into()
    };
    let realized = RealizedTy::try_from(field_type.clone())
        .map_err(|_| no_default("of a generic parameter"))?;
    empty_value(vm, &realized, &mut Vec::new())?.ok_or_else(|| no_default(&realized.to_string()))
}

/// The empty value of `ty`: `null` for a nullable type, the zero of a number,
/// `false`, `""`, an empty list or map, a literal's own value, an enum's first
/// variant, and a class of its fields' empty values. `None` when `ty` has none
/// (media, functions, a class that contains itself, ...).
///
/// `within` holds the classes and aliases being built, to stop a type that
/// needs its own empty value.
fn empty_value(
    vm: &mut BexVm,
    ty: &RealizedTy,
    within: &mut Vec<HeapPtr>,
) -> Result<Option<Value>, VmRustFnError> {
    Ok(Some(match ty {
        RealizedTy::Null | RealizedTy::Unknown => Value::NULL,
        RealizedTy::Int => Value::int(0),
        RealizedTy::Bigint => vm.try_alloc_bigint(Arc::new(num_bigint::BigInt::default()))?,
        RealizedTy::Float => Value::object(vm.alloc_float(0.0)),
        RealizedTy::Bool => Value::bool(false),
        RealizedTy::String => Value::object(vm.alloc_string("")),
        RealizedTy::Uint8Array => Value::object(vm.alloc_uint8array(Vec::new())),
        RealizedTy::Literal(literal, _) => match literal {
            Literal::Int(n) => Value::int(*n),
            Literal::Bigint(n) => vm.try_alloc_bigint(Arc::new(n.clone()))?,
            Literal::Float(text) => match text.parse::<f64>() {
                Ok(f) => Value::object(vm.alloc_float(f)),
                Err(_) => return Ok(None),
            },
            Literal::String(s) => Value::object(vm.alloc_string(s.as_str())),
            Literal::Bool(b) => Value::bool(*b),
        },
        RealizedTy::List(element) => Value::object(vm.alloc_array((**element).clone(), Vec::new())),
        RealizedTy::Map { key, value } => {
            Value::object(vm.alloc_map((**key).clone(), (**value).clone(), IndexMap::new()))
        }
        RealizedTy::Enum(head) => {
            let Object::Enum(enm) = vm.get_object(head.ptr()) else {
                return Ok(None);
            };
            let Some(index) = enm.variants.iter().position(|variant| !variant.skip) else {
                return Ok(None);
            };
            Value::object(vm.alloc_variant(head.ptr(), index))
        }
        RealizedTy::EnumVariant(head, name) => {
            let Object::Enum(enm) = vm.get_object(head.ptr()) else {
                return Ok(None);
            };
            let Some(index) = enm
                .variants
                .iter()
                .position(|variant| variant.name == name.as_str())
            else {
                return Ok(None);
            };
            Value::object(vm.alloc_variant(head.ptr(), index))
        }
        RealizedTy::Union(members) => {
            if members
                .iter()
                .any(|member| matches!(member, RealizedTy::Null))
            {
                Value::NULL
            } else {
                let mut found = None;
                for member in members {
                    if let Some(value) = empty_value(vm, member, within)? {
                        found = Some(value);
                        break;
                    }
                }
                return Ok(found);
            }
        }
        RealizedTy::Class(head, type_args) => {
            let class = head.ptr();
            if within.contains(&class) {
                return Ok(None);
            }
            let Object::Class(declaration) = vm.get_object(class) else {
                return Ok(None);
            };
            let templates: Vec<_> = declaration
                .fields
                .iter()
                .map(|field| field.field_template.clone())
                .collect();
            within.push(class);
            let mut values = Vec::with_capacity(templates.len());
            for template in &templates {
                let field_ty = vm.realize_field_ty(template, type_args);
                let Some(value) = empty_value(vm, &field_ty, within)? else {
                    within.pop();
                    return Ok(None);
                };
                values.push(value);
            }
            within.pop();
            Value::object(
                vm.tlab
                    .alloc_instance_with_type_args(class, type_args.clone(), values),
            )
        }
        RealizedTy::TypeAlias(head) => {
            let alias = head.ptr();
            if within.contains(&alias) {
                return Ok(None);
            }
            let Object::TypeAlias(declaration) = vm.get_object(alias) else {
                return Ok(None);
            };
            let definition = declaration.definition.clone();
            within.push(alias);
            let value = empty_value(vm, &definition, within)?;
            within.pop();
            return Ok(value);
        }
        RealizedTy::Media(_)
        | RealizedTy::Function { .. }
        | RealizedTy::Future(..)
        | RealizedTy::Interface(..)
        | RealizedTy::RustType
        | RealizedTy::Type
        | RealizedTy::Resource
        | RealizedTy::PromptAst
        | RealizedTy::Void
        | RealizedTy::Never => return Ok(None),
    }))
}
