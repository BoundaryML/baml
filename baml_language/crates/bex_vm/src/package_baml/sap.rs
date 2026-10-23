//! `baml.sap`: schema-aligned parsing (SAP) of an LLM's text into a declared
//! type.
//!
//! VM natives, not sys ops: parsing does no IO, so it runs on the calling VM
//! with the heap at hand. `_new_parse_cache<T>` builds the parse model from the
//! class instantiations and declarations `T` reaches, and the parse methods
//! allocate their result directly on the heap, so no definition table is
//! copied and no value tree is landed by name.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

use baml_type::{Literal, typetag::TypeTag};
use bex_heap::TlabHolder;
use bex_sap::{
    CompiledSapModel,
    baml_value::BamlValue,
    deserializer::{coercer::ParsingContext, types::BamlValueWithFlags},
    jsonish,
    sap_model::{ConvertError, NamedTy, NamedTyKind, ParseTy, TyResolvedRef, TypeCtx},
    to_baml_ty::ToBamlTy,
};
use bex_str::BexStr;
use bex_vm_types::{
    HeapPtr, RealizedTy, TyTemplate, TypeHead,
    types::{Class, Instance, Object, Value},
};
use indexmap::IndexMap;
use sys_types::DefKey;

use super::{BamlNamespaceSap, BamlNamespaceSap_ParseCache, PackageBamlImpl};
use crate::{
    BexVm, definitions,
    errors::{VmBamlError, VmInternalError, VmPanic, VmRustFnError},
};

/// The `_data` of a `baml.sap._ParseCache<T>`: the parse model for `T`.
///
/// A `RustData` payload is a GC leaf, so the model holds no heap pointers: it
/// names declarations by tagged name, and each parse maps those names back to
/// the declarations through [`Declarations`].
struct ParseCache {
    model: CompiledSapModel,
}

impl bex_vm_types::BexRustData for ParseCache {
    fn measure(&self, _: &mut bex_vm_types::Meter) {
        // The model is bounded by the class instantiations the target type
        // reaches, not by data, and the parser crate does not expose its size.
    }
}

impl BamlNamespaceSap for PackageBamlImpl {
    fn _new_parse_cache(vm: &mut BexVm) -> Result<Value, VmRustFnError> {
        let target = parse_target(vm)?;
        let model = build_model(vm, &target).map_err(|e| {
            parse_error(format!(
                "schema-aligned parsing cannot model this type: {e}"
            ))
        })?;
        let data: Arc<dyn bex_vm_types::BexRustData> = Arc::new(ParseCache { model });
        let data = Value::object(vm.alloc_rust_data(data));
        let class = vm.lang_object(crate::lang_items::BAML_SAP_PARSE_CACHE);
        let cache = vm.tlab.alloc(Object::Instance(Instance::new(
            class,
            Box::new([target]),
            vec![data],
        )));
        Ok(Value::object(cache))
    }

    fn coerce(vm: &mut BexVm, value: &Value) -> Result<Value, VmRustFnError> {
        let target = parse_target(vm)?;
        // The type of a coercion is often data, so one the parser cannot
        // model is an answer for the caller, not a bug of the program.
        let model = build_model(vm, &target).map_err(|e| {
            parse_error(format!(
                "schema-aligned parsing cannot model this type: {e}"
            ))
        })?;
        let json = super::json::json_value_to_serde(vm, *value);
        let aligned = align(
            vm,
            &model,
            &target,
            &parsed_tree(&json),
            Completion::Final,
            parse_error,
        )?;
        aligned.ok_or_else(|| parse_error("a complete value did not align to a value"))
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
                let no_yield = vm.lang_object(crate::lang_items::BAML_SAP_NO_YIELD);
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

/// What a parse of `target` can build, and every declaration it can name.
///
/// The parser builds each class at every instantiation the target reaches
/// through the fields it reads (not `@skip` ones). A generic class is a type
/// constructor, so `Box<int>` and `Box<string>` are reached as two classes,
/// each with its field types realized at its own arguments. Realization goes
/// through the VM, so a field typed by an associated-type projection resolves
/// to its implementation's binding.
///
/// An instantiation is identified by its arguments with each union spelled
/// flat and without repeats ([`RealizedTy::flatten_union`]), the spelling the
/// parse model keys it by. So `class Opt<T> { inner: Opt<T?>? }` at `int`
/// reaches `Opt<int?>` and then `Opt<int?>` again, not a new `Opt<(int?)?>`.
struct ParseReach {
    /// Each class instantiation reached, in the order its walk finished.
    classes: Vec<ReachedClass>,
    /// The enums reached, in the order first reached.
    enums: Vec<TypeHead>,
    /// The type aliases reached, in the order first reached.
    aliases: Vec<TypeHead>,
    /// The interface-existential types reached, in the order first reached. Nothing parses
    /// as one, but the model names each, so a container of one is typed
    /// exactly.
    interfaces: Vec<RealizedTy>,
    /// Every nominal head, of any kind, that a type of the parse model names:
    /// those of the target, of the read fields' types (type arguments
    /// included), and of alias bodies. A parsed value's types name no other.
    heads: Vec<TypeHead>,
}

struct ReachedClass {
    head: TypeHead,
    /// The instantiation's arguments, each union spelled flat.
    args: Box<[RealizedTy]>,
    /// The type of every field, `@skip` ones included, realized at `args`.
    fields: Vec<RealizedTy>,
}

/// Why a target has no parse model.
enum ReachError {
    /// A generic class that reaches itself at ever-growing arguments, as
    /// `class Nest<T> { inner: Nest<T[]>? }` does: it has infinitely many
    /// instantiations, so no finite parse model (nor output format).
    NonRegular {
        class: String,
        from: String,
        to: String,
    },
    /// A class type with the wrong number of type arguments, such as a
    /// generic class reflected without its arguments: it names no type.
    Arity {
        class: String,
        expected: usize,
        found: usize,
    },
}

impl fmt::Display for ReachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonRegular { class, from, to } => write!(
                f,
                "non-regular recursive generic class `{class}` expands from `{from}` to `{to}`"
            ),
            Self::Arity {
                class,
                expected,
                found,
            } => {
                let arguments = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
                write!(
                    f,
                    "class `{class}` takes {expected} type {arguments}, but is given {found}"
                )
            }
        }
    }
}

/// Origins of the type being walked relative to each class instantiation on
/// the ancestry stack, to tell a recursive class that closes from one whose
/// arguments grow forever. The output-format renderer and the render validator
/// apply the same test to the same flat spelling of arguments.
type ReachOrigins = baml_type::template::TyTemplateOrigins<TypeHead>;

impl ParseReach {
    fn of(vm: &BexVm, target: &RealizedTy) -> Result<Self, ReachError> {
        let mut walk = ReachWalk {
            vm,
            seen_classes: HashSet::new(),
            seen_declarations: HashSet::new(),
            seen_heads: HashSet::new(),
            ancestry: Vec::new(),
            reach: ParseReach {
                classes: Vec::new(),
                enums: Vec::new(),
                aliases: Vec::new(),
                interfaces: Vec::new(),
                heads: Vec::new(),
            },
        };
        walk.record_heads(target);
        walk.visit(target, &ReachOrigins::root())?;
        Ok(walk.reach)
    }
}

struct ReachWalk<'vm> {
    vm: &'vm BexVm,
    seen_classes: HashSet<(TypeHead, Box<[RealizedTy]>)>,
    seen_declarations: HashSet<TypeHead>,
    seen_heads: HashSet<TypeHead>,
    /// The class instantiations whose fields are being walked, outermost first.
    ancestry: Vec<(TypeHead, Box<[RealizedTy]>)>,
    reach: ParseReach,
}

impl ReachWalk<'_> {
    /// Records every head `ty` names (see [`ParseReach::heads`]).
    fn record_heads(&mut self, ty: &RealizedTy) {
        let Self {
            seen_heads, reach, ..
        } = self;
        ty.visit_heads(&mut |head: &TypeHead| {
            if seen_heads.insert(*head) {
                reach.heads.push(*head);
            }
        });
    }

    fn visit(&mut self, ty: &RealizedTy, origins: &ReachOrigins) -> Result<(), ReachError> {
        match ty {
            RealizedTy::Class(head, args) => self.visit_class(*head, args, origins),
            RealizedTy::Enum(head) | RealizedTy::EnumVariant(head, _) => {
                if self.seen_declarations.insert(*head) {
                    self.reach.enums.push(*head);
                }
                Ok(())
            }
            RealizedTy::TypeAlias(head) => {
                if !self.seen_declarations.insert(*head) {
                    return Ok(());
                }
                self.reach.aliases.push(*head);
                let Object::TypeAlias(alias) = self.vm.get_object(head.ptr()) else {
                    unreachable!("a type alias's head is a type alias: {head:?}")
                };
                self.record_heads(&alias.definition);
                // An alias takes no type arguments, so its body is relative to
                // no enclosing class.
                self.visit(
                    &alias.definition,
                    &ReachOrigins::opaque(self.ancestry.len()),
                )
            }
            RealizedTy::Interface(..) => {
                if !self.reach.interfaces.contains(ty) {
                    self.reach.interfaces.push(ty.clone());
                }
                Ok(())
            }
            RealizedTy::List(element) => self.visit(element, &origins.list_element()),
            RealizedTy::Map { key, value } => {
                self.visit(key, &origins.map_key())?;
                self.visit(value, &origins.map_value())
            }
            RealizedTy::Union(members) => {
                for (index, member) in members.iter().enumerate() {
                    self.visit(member, &origins.union_member(index))?;
                }
                Ok(())
            }
            // Nothing the parser builds is named below these; whether it can
            // model them at all is the converter's call.
            RealizedTy::Null
            | RealizedTy::Unknown
            | RealizedTy::Int
            | RealizedTy::Bigint
            | RealizedTy::Float
            | RealizedTy::Bool
            | RealizedTy::String
            | RealizedTy::Uint8Array
            | RealizedTy::Literal(..)
            | RealizedTy::Media(_)
            | RealizedTy::Function { .. }
            | RealizedTy::Future(..)
            | RealizedTy::RustType
            | RealizedTy::Type
            | RealizedTy::Resource
            | RealizedTy::PromptAst
            | RealizedTy::Never => Ok(()),
        }
    }

    fn visit_class(
        &mut self,
        head: TypeHead,
        args: &[RealizedTy],
        origins: &ReachOrigins,
    ) -> Result<(), ReachError> {
        let class = reached_class(self.vm, head);
        if args.len() != class.generic_param_count {
            return Err(ReachError::Arity {
                class: baml_type::HeadDisplay::head_display_name(&head),
                expected: class.generic_param_count,
                found: args.len(),
            });
        }
        let args: Box<[RealizedTy]> = args
            .iter()
            .cloned()
            .map(RealizedTy::flatten_union)
            .collect();
        let identity = (head, args);
        if self.seen_classes.contains(&identity) {
            return Ok(());
        }
        for (index, (ancestor, ancestor_args)) in self.ancestry.iter().enumerate() {
            if *ancestor == head
                && origins.class_transform_expands(index, &head, ancestor_args.len())
            {
                return Err(ReachError::NonRegular {
                    class: baml_type::HeadDisplay::head_display_name(&head),
                    from: RealizedTy::Class(*ancestor, ancestor_args.clone()).to_string(),
                    to: RealizedTy::Class(head, identity.1).to_string(),
                });
            }
        }
        let fields: Vec<RealizedTy> = class
            .fields
            .iter()
            .map(|field| self.vm.realize_field_ty(&field.field_template, &identity.1))
            .collect();
        self.seen_classes.insert(identity.clone());
        self.ancestry.push(identity);
        for (field, field_ty) in class.fields.iter().zip(&fields) {
            if field.skip {
                continue;
            }
            self.record_heads(field_ty);
            // NOTE: an associated-type projection in a field template has no
            // origin through its realized binding, so an instantiation that
            // grows only through an impl's binding is not caught as non-regular
            // (the output-format renderer shares the gap); the walk would not
            // terminate. No such program compiles today: a projection on a
            // class's own type parameter crashes emit (see the `BUG:` in
            // `baml_compiler2_emit::items`), and one with another base reduces
            // at compile time, so the walk sees its binding's template.
            let field_origins =
                origins.through_field(&head, class.generic_param_count, &field.field_template);
            self.visit(field_ty, &field_origins)?;
        }
        let (head, args) = self
            .ancestry
            .pop()
            .unwrap_or_else(|| unreachable!("this visit pushed its own frame"));
        self.reach.classes.push(ReachedClass { head, args, fields });
        Ok(())
    }
}

/// The declaration of a reached class. Total by invariant: a class type's head
/// is a live class declaration.
fn reached_class(vm: &BexVm, head: TypeHead) -> &Class {
    let Object::Class(class) = vm.get_object(head.ptr()) else {
        unreachable!("a class type's head is a class: {head:?}")
    };
    class
}

/// The parse model's name for `class` instantiated at `args`: also how a
/// failure about one of its fields names it, on every path.
fn model_name(class: &Class, args: &[RealizedTy]) -> NamedTy {
    NamedTy::class(
        DefKey::new(class.type_tag, class.name()),
        args.iter().map(definitions::lane_realized_ty),
    )
}

/// Why the parser cannot model a target.
enum ModelError {
    Reach(ReachError),
    Convert(Box<ConvertError>),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reach(e) => e.fmt(f),
            Self::Convert(e) => e.fmt(f),
        }
    }
}

/// The parse model for `target`, built from what it reaches.
///
/// A union member the parser cannot build is dropped, as `baml.json.schema`
/// drops it: one with no JSON form or no values, and a class with a `@skip`
/// field whose type has no empty value to hold.
fn build_model(vm: &BexVm, target: &RealizedTy) -> Result<CompiledSapModel, ModelError> {
    let reach = ParseReach::of(vm, target).map_err(ModelError::Reach)?;
    let defs = crate::json_defs::HeapDefs { vm };
    let classes = reach.classes.iter().map(|reached| {
        let class = reached_class(vm, reached.head);
        let unfillable = class
            .fields
            .iter()
            .zip(&reached.fields)
            .find(|(field, field_type)| {
                field.skip
                    && !baml_type::json_inhabit::has_empty_value(&defs, field_type.as_runtime_ty())
                        .unwrap_or(false)
            })
            .map(|(field, _)| field.name.clone());
        (
            model_name(class, &reached.args),
            definitions::parse_class(class, &reached.fields, unfillable),
        )
    });
    let mut enums = IndexMap::new();
    let mut aliases = HashMap::new();
    for head in &reach.enums {
        let Object::Enum(enm) = vm.get_object(head.ptr()) else {
            unreachable!("an enum type's head is an enum: {head:?}")
        };
        enums.insert(
            DefKey::new(enm.type_tag, enm.name()),
            definitions::enum_definition(enm),
        );
    }
    for head in &reach.aliases {
        let Object::TypeAlias(alias) = vm.get_object(head.ptr()) else {
            unreachable!("a type alias's head is a type alias: {head:?}")
        };
        aliases.insert(
            DefKey::new(alias.type_tag, alias.name()),
            definitions::lane_realized_ty(&alias.definition),
        );
    }
    let target = definitions::lane_realized_ty(target);
    let interfaces = reach.interfaces.iter().map(|interface| {
        NamedTy::of(&definitions::lane_realized_ty(interface))
            .unwrap_or_else(|| unreachable!("an interface type is named: {interface}"))
    });
    CompiledSapModel::from_type_ctx(TypeCtx::new(classes, enums, &aliases, interfaces), target)
        .map_err(|e| ModelError::Convert(Box::new(e)))
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
    // An unresolvable target is a schema bug, not an incomplete reply: it
    // reports before the text is looked at.
    cache
        .model
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
    align(
        vm,
        &cache.model,
        &target_type,
        &value,
        completion,
        llm_client,
    )
}

/// Align `value` to the target of `model` and allocate the result, or `None`
/// for a partial value that does not align yet: the second stage of a parse,
/// and the whole of a coercion. `mismatch` raises a value that does not align.
fn align(
    vm: &mut BexVm,
    model: &CompiledSapModel,
    target_type: &RealizedTy,
    value: &jsonish::Value<'_>,
    completion: Completion,
    mismatch: impl Fn(String) -> VmRustFnError,
) -> Result<Option<Value>, VmRustFnError> {
    let target = model
        .resolved_target()
        .map_err(|e| mismatch(e.to_string()))?;
    let is_done = completion == Completion::Final;
    let context = ParsingContext::new(model.db());
    let parsed = match TyResolvedRef::coerce(&context, target, value) {
        Ok(parsed) => parsed,
        Err(e) if is_done => return Err(mismatch(e.to_string())),
        Err(_) => return Ok(None),
    };
    // Only a value to allocate needs the declarations: a partial that does not
    // parse yet skips the walk.
    parsed
        .map(|parsed| {
            let declarations = Declarations::reached_by(vm, target_type)?;
            to_heap(vm, &parsed, &declarations, &mismatch)
        })
        .transpose()
}

/// The tree the text stage would have produced for `json`, had it been
/// written out: every value complete, nothing repaired.
fn parsed_tree(json: &serde_json::Value) -> jsonish::Value<'_> {
    use jsonish::CompletionState::Complete;
    match json {
        serde_json::Value::Null => jsonish::Value::Null,
        serde_json::Value::Bool(value) => jsonish::Value::Boolean(*value),
        serde_json::Value::Number(number) => jsonish::Value::Number(number.clone(), Complete),
        serde_json::Value::String(text) => jsonish::Value::String(text.as_str().into(), Complete),
        serde_json::Value::Array(items) => {
            jsonish::Value::Array(items.iter().map(parsed_tree).collect(), Complete)
        }
        serde_json::Value::Object(entries) => jsonish::Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.as_str().into(), parsed_tree(value)))
                .collect(),
            Complete,
        ),
    }
}

/// A parse failure, as `baml.errors.LlmClient`.
fn llm_client(message: impl Into<String>) -> VmRustFnError {
    VmBamlError::LlmClient {
        message: message.into(),
    }
    .into()
}

/// A coercion failure, as `baml.errors.ParseError`.
fn parse_error(message: impl Into<String>) -> VmRustFnError {
    VmBamlError::ParseError {
        message: message.into(),
    }
    .into()
}

/// The declarations a parse target reaches, by tag, for mapping the model's
/// tagged names back to the heap. Built for one call: a parse never yields, so
/// no collection can move them meanwhile.
struct Declarations(HashMap<TypeTag, HeapPtr>);

impl Declarations {
    fn reached_by(vm: &BexVm, target: &RealizedTy) -> Result<Self, VmRustFnError> {
        // The target's model was built, so its reach closes.
        let reach = ParseReach::of(vm, target).map_err(|e| invariant(e.to_string()))?;
        Ok(Self(
            reach
                .heads
                .iter()
                .map(|head| (head.tag(), head.ptr()))
                .collect(),
        ))
    }

    fn get(&self, name: &DefKey) -> Result<HeapPtr, VmRustFnError> {
        self.0.get(&name.tag()).copied().ok_or_else(|| {
            invariant(format!(
                "the parsed value names `{name}`, which its target does not reach"
            ))
        })
    }

    /// `ty` with each tagged name replaced by the head of its declaration.
    fn realize(&self, ty: &ParseTy) -> Result<RealizedTy, VmRustFnError> {
        ty.try_map_heads(&mut |name: &DefKey| {
            self.get(name).map(|ptr| TypeHead::new(ptr, name.tag()))
        })
    }
}

fn invariant(message: String) -> VmRustFnError {
    VmInternalError::SapValue { message }.into()
}

/// Allocate `value`: the same value the parser produced, on the heap.
/// `mismatch` raises a value the heap cannot hold: the parser's integer is
/// wider than the VM's.
fn to_heap(
    vm: &mut BexVm,
    value: &BamlValueWithFlags<'_, '_, '_, NamedTy>,
    declarations: &Declarations,
    mismatch: &impl Fn(String) -> VmRustFnError,
) -> Result<Value, VmRustFnError> {
    Ok(match &value.value {
        BamlValue::String(s) => Value::object(vm.alloc_string(s.value.as_ref())),
        BamlValue::Int(i) => Value::try_int(i.value).ok_or_else(|| {
            mismatch(format!(
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
            let TyResolvedRef::Array(array_ty) = value.meta.ty else {
                return Err(invariant(format!(
                    "a parsed list is typed `{}`, not as a list",
                    value.meta.ty.to_baml_ty()
                )));
            };
            let element = array_ty.ty.to_baml_ty();
            let element = declarations.realize(&element)?;
            let items = array
                .value
                .iter()
                .map(|item| to_heap(vm, item, declarations, mismatch))
                .collect::<Result<Vec<_>, _>>()?;
            Value::object(vm.alloc_array(element, items))
        }
        BamlValue::Map(map) => {
            let TyResolvedRef::Map(map_ty) = value.meta.ty else {
                return Err(invariant(format!(
                    "a parsed map is typed `{}`, not as a map",
                    value.meta.ty.to_baml_ty()
                )));
            };
            let (key, value_ty) = (map_ty.key.to_baml_ty(), map_ty.value.to_baml_ty());
            let (key, value_ty) = (
                declarations.realize(&key)?,
                declarations.realize(&value_ty)?,
            );
            let mut entries = IndexMap::with_capacity(map.value.len());
            for (k, v) in &map.value {
                entries.insert(
                    BexStr::from(k.as_ref()),
                    to_heap(vm, v, declarations, mismatch)?,
                );
            }
            Value::object(vm.alloc_map(key, value_ty, entries))
        }
        BamlValue::Enum(e) => {
            let decl = e
                .name
                .decl()
                .ok_or_else(|| invariant(format!("`{}` is not an enum", e.name)))?;
            let enm = declarations.get(decl)?;
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
            let NamedTyKind::Class(decl, args) = c.name.kind() else {
                return Err(invariant(format!("`{}` is not a class", c.name)));
            };
            let class = declarations.get(decl)?;
            let type_args = args
                .iter()
                .map(|arg| declarations.realize(arg))
                .collect::<Result<Box<[_]>, _>>()?;
            let Object::Class(declaration) = vm.get_object(class) else {
                return Err(invariant(format!("`{}` is not a class", c.name)));
            };
            let fields: Vec<(String, bool, TyTemplate)> = declaration
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.skip, field.field_template.clone()))
                .collect();
            let mut values = Vec::with_capacity(fields.len());
            for (name, skip, field_template) in fields {
                let field_value = match c.value.get(name.as_str()) {
                    Some(parsed) => to_heap(vm, parsed, declarations, mismatch)?,
                    None if skip => {
                        let field_type = vm.realize_field_ty(&field_template, &type_args);
                        skipped_field_default(vm, c.name, &name, &field_type)?
                    }
                    None => {
                        return Err(invariant(format!(
                            "the parsed `{}` has no field `{name}`",
                            c.name
                        )));
                    }
                };
                values.push(field_value);
            }
            Value::object(
                vm.tlab
                    .alloc_instance_with_type_args(class, type_args, values),
            )
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
    class: &NamedTy,
    field: &str,
    field_type: &RealizedTy,
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
    empty_value(vm, field_type, &mut Vec::new())?.ok_or_else(|| no_default(&field_type.to_string()))
}

/// The empty value of `ty`: `null` for a nullable type, the zero of a number,
/// `false`, `""`, an empty list or map, a literal's own value, an enum's first
/// variant, and a class of its fields' empty values. `None` when `ty` has none
/// (media, functions, a class that contains itself, ...).
///
/// `within` holds the classes and aliases being built, to stop a type that
/// needs its own empty value.
///
/// This is the value a `@skip` field holds whenever a model's answer is read
/// under the names a model is shown: here, and in `baml.json.decode`
/// with `Naming.Aliased`. [`baml_type::json_inhabit::has_empty_value`] is the
/// same rule as a predicate.
pub(super) fn empty_value(
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
        | RealizedTy::Never => return Ok(None),
    }))
}
