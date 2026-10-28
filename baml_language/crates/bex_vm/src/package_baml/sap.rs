//! `baml.sap`: schema-aligned parsing (SAP) of an LLM's text into a declared
//! type.
//!
//! VM natives, not sys ops: parsing does no IO, so it runs on the calling VM
//! with the heap at hand. `_new_parse_cache<T>` builds the parse model from the
//! class instantiations and declarations `T` reaches, and the parse methods
//! allocate their result directly on the heap, so no definition table is
//! copied and no value tree is landed by name.
//!
//! A type that implements `baml.FromLlmOutput` is parsed as its `Shape`. The
//! parse then builds it from the shape value read, with its `from_llm_output`,
//! which is BAML code: the native yields to call it ([`ShapedRead`]). A value
//! `from_llm_output` refuses does not fit the type, so the parse runs again
//! without the values refused, and a union moves on to its next member.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
};

use baml_type::{Literal, typetag::TypeTag};
use bex_heap::TlabHolder;
use bex_sap::{
    CompiledSapModel,
    baml_value::BamlValue,
    deserializer::{
        coercer::{ParsingContext, ParsingError, Rejections, shape_key},
        types::BamlValueWithFlags,
    },
    jsonish,
    sap_model::{ConvertError, NamedTy, NamedTyKind, ParseTy, TyResolvedRef, TypeCtx},
    to_baml_ty::ToBamlTy,
};
use bex_str::BexStr;
use bex_vm_types::{
    HeapPtr, RealizedTy, RootHaver, RootVisitor, TyTemplate, TypeHead,
    types::{Class, Instance, Object, Value},
};
use indexmap::IndexMap;
use sys_types::DefKey;

use super::{
    BamlNamespaceSap, BamlNamespaceSap_ParseCache, Continuation, NativeCallResult, PackageBamlImpl,
};
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
    /// Whether the target reaches a type a model writes as its
    /// `baml.FromLlmOutput` shape. A parse of one that does not never yields.
    has_shapes: bool,
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
        let (model, has_shapes) = build_model(vm, &target).map_err(|e| {
            parse_error(format!(
                "schema-aligned parsing cannot model this type: {e}"
            ))
        })?;
        let data: Arc<dyn bex_vm_types::BexRustData> = Arc::new(ParseCache { model, has_shapes });
        let data = Value::object(vm.alloc_rust_data(data));
        let class = vm.lang_object(crate::lang_items::BAML_SAP_PARSE_CACHE);
        let cache = vm.tlab.alloc(Object::Instance(Instance::new(
            class,
            Box::new([target]),
            vec![data],
        )));
        Ok(Value::object(cache))
    }

    fn _reads_shapes(vm: &BexVm) -> Result<bool, VmRustFnError> {
        Ok(!shapes_reached(vm, &parse_target(vm)?).is_empty())
    }

    fn coerce(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        let target = match parse_target(vm) {
            Ok(target) => target,
            Err(error) => return NativeCallResult::Error(error),
        };
        // The type of a coercion is often data, so one the parser cannot
        // model is an answer for the caller, not a bug of the program.
        let (model, has_shapes) = match build_model(vm, &target) {
            Ok(built) => built,
            Err(e) => {
                return NativeCallResult::Error(parse_error(format!(
                    "schema-aligned parsing cannot model this type: {e}"
                )));
            }
        };
        let model = Arc::new(model);
        let json = match super::json::json_value_to_serde(vm, *value) {
            Ok(json) => json,
            Err(unrepresentable) => {
                return NativeCallResult::Error(parse_error(format!(
                    "schema-aligned parsing cannot read this value: {unrepresentable}"
                )));
            }
        };
        read(
            vm,
            &model,
            Input::Json(&json),
            has_shapes,
            target,
            Mode::Coerce,
            || Source::Json {
                model: Arc::clone(&model),
                json: json.clone(),
            },
        )
    }
}

impl BamlNamespaceSap_ParseCache for PackageBamlImpl {
    fn _parse_final(vm: &mut BexVm, cache: &Value, json: &BexStr) -> NativeCallResult {
        text_read(vm, *cache, json, Mode::Final)
    }

    fn _parse_partial(vm: &mut BexVm, cache: &Value, json: &BexStr) -> NativeCallResult {
        text_read(vm, *cache, json, Mode::Partial)
    }
}

/// Parse `text` with the model in `cache`.
fn text_read(vm: &mut BexVm, cache: Value, text: &BexStr, mode: Mode) -> NativeCallResult {
    let cache = vm
        .rust_data_field::<ParseCache>(&cache, 0)
        .map_err(VmRustFnError::from)
        .and_then(|cache| {
            // An unresolvable target is a schema bug, not an incomplete
            // reply: it reports before the text is looked at.
            cache
                .model
                .resolved_target()
                .map_err(|e| llm_client(e.to_string()))?;
            Ok(cache)
        });
    match (cache, parse_target(vm)) {
        (Ok(cache), Ok(target)) => read(
            vm,
            &cache.model,
            Input::Text(text.as_str()),
            cache.has_shapes,
            target,
            mode,
            || Source::Text {
                cache: Arc::clone(&cache),
                text: text.as_str().to_string(),
            },
        ),
        (Err(error), _) | (_, Err(error)) => NativeCallResult::Error(error),
    }
}

/// What a read is for: the final parse of a reply, the parse of the part a
/// stream has received, or the coercion of a value that arrived parsed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Final,
    Partial,
    Coerce,
}

impl Mode {
    fn is_done(self) -> bool {
        self != Mode::Partial
    }

    /// The error for a value that does not align: `baml.errors.ParseError`
    /// for a coercion, `baml.errors.LlmClient` for a parse.
    fn mismatch(self, message: String) -> VmRustFnError {
        match self {
            Mode::Coerce => parse_error(message),
            Mode::Final | Mode::Partial => llm_client(message),
        }
    }

    /// [`Self::mismatch`] for the parser's failure. A coercion names where the
    /// value that does not align is, as `baml.json` decoding does: `... at
    /// .meeting.at`.
    fn misaligned(self, error: &ParsingError) -> VmRustFnError {
        match self {
            Mode::Coerce => parse_error(located_message(error)),
            Mode::Final | Mode::Partial => llm_client(error.to_string()),
        }
    }

    /// The result of a read that produced no value: a partial reply that
    /// does not parse yet.
    fn no_value(self, vm: &mut BexVm) -> NativeCallResult {
        match self {
            Mode::Partial => {
                let no_yield = vm.lang_object(crate::lang_items::BAML_SAP_NO_YIELD);
                NativeCallResult::Done(Value::object(vm.alloc_instance(no_yield, Vec::new())))
            }
            Mode::Final => {
                NativeCallResult::Error(llm_client("SAP parse returned no value when complete"))
            }
            Mode::Coerce => {
                NativeCallResult::Error(parse_error("a complete value did not align to a value"))
            }
        }
    }
}

/// What a read's first parse came to.
enum FirstRead {
    /// The value, allocated: nothing in it is a shape value.
    Value(Value),
    /// The parsed tree, which has shape values for `from_llm_output` to
    /// build from.
    Shaped(Box<Parsed<'static>>),
}

/// Read `input` with `model` as `target`. A value with no shaped part is
/// allocated at once, straight from the parser's tree. Otherwise the read
/// calls `from_llm_output` for each shaped part ([`ShapedRead`]), and keeps
/// `source()`, the input it may parse again.
fn read(
    vm: &mut BexVm,
    model: &CompiledSapModel,
    input: Input<'_>,
    has_shapes: bool,
    target: RealizedTy,
    mode: Mode,
    source: impl FnOnce() -> Source,
) -> NativeCallResult {
    let rejections = Rejections::new();
    let first = with_tree(model, input, mode, &rejections, |tree| {
        if has_shapes && has_shaped(tree) {
            return Ok(FirstRead::Shaped(Box::new(Parsed::of(tree)?.into_owned())));
        }
        let declarations = Declarations::reached_by(vm, &target)?;
        Ok(FirstRead::Value(to_heap(vm, tree, &declarations, mode)?))
    });
    let tree = match first {
        Ok(Some(FirstRead::Value(value))) => return NativeCallResult::Done(value),
        Ok(Some(FirstRead::Shaped(tree))) => *tree,
        Ok(None) => return mode.no_value(vm),
        Err(error) => return NativeCallResult::Error(error),
    };
    let pending = tree.shaped_paths();
    ShapedRead {
        source: source(),
        target,
        mode,
        rejections,
        tree,
        pending,
        refused: Vec::new(),
        building: None,
        declarations: None,
    }
    .run(vm)
}

/// What a read parses: a model's text, or a value that arrived parsed.
#[derive(Clone, Copy)]
enum Input<'a> {
    Text(&'a str),
    Json(&'a serde_json::Value),
}

/// Parse `input` with `model` and hand the value, aligned to the model's
/// target, to `f`; `None` for a partial reply that does not parse yet. A
/// shape value in `rejections` does not parse as its type.
fn with_tree<R>(
    model: &CompiledSapModel,
    input: Input<'_>,
    mode: Mode,
    rejections: &Rejections<NamedTy>,
    f: impl FnOnce(&BamlValueWithFlags<'_, '_, '_, NamedTy>) -> Result<R, VmRustFnError>,
) -> Result<Option<R>, VmRustFnError> {
    let value = match input {
        Input::Text(text) => {
            match jsonish::parse(text, jsonish::ParseOptions::default(), mode.is_done()) {
                Ok(value) => value,
                Err(e) if mode.is_done() => return Err(mode.mismatch(e.to_string())),
                // A reply that opens with a fence or with prose before the
                // JSON cannot be coerced until the JSON starts: skip the
                // partial, wait for text.
                Err(_) => return Ok(None),
            }
        }
        Input::Json(json) => parsed_tree(json),
    };
    let target = model
        .resolved_target()
        .map_err(|e| mode.mismatch(e.to_string()))?;
    let context = ParsingContext::new(model.db()).with_rejections(rejections);
    match TyResolvedRef::coerce(&context, target, &value) {
        Ok(Some(parsed)) => f(&parsed).map(Some),
        Ok(None) => Ok(None),
        Err(e) if mode.is_done() => Err(mode.misaligned(&e)),
        Err(_) => Ok(None),
    }
}

/// The parser's failure as a sentence that says where: the innermost reason
/// that alone explains it, then ` at <path>` unless it is at the root. When
/// several readings failed there (each member of a union), each follows on
/// its own line.
fn located_message(error: &ParsingError) -> String {
    use std::fmt::Write;
    let (error, path) = error.located();
    let mut message = error.reason.trim_end_matches('.').to_string();
    if !path.is_empty() {
        let _ = write!(message, " at {path}");
    }
    for cause in &error.causes {
        let _ = write!(message, "\n  - {}", cause.to_string().replace('\n', "\n  "));
    }
    message
}

/// Whether any part of `value` is a shape value.
fn has_shaped(value: &BamlValueWithFlags<'_, '_, '_, NamedTy>) -> bool {
    match &value.value {
        BamlValue::Shaped(_) => true,
        BamlValue::Array(array) => array.value.iter().any(has_shaped),
        BamlValue::Map(map) => map.value.values().any(has_shaped),
        BamlValue::Class(class) => class.value.values().any(has_shaped),
        _ => false,
    }
}

/// The input a [`ShapedRead`] parses again when `from_llm_output` refuses a
/// value, with its parse model.
enum Source {
    /// A model's text, with the parse model for its target.
    Text {
        cache: Arc<ParseCache>,
        text: String,
    },
    /// A value that arrived parsed, with the parse model for its target.
    Json {
        model: Arc<CompiledSapModel>,
        json: serde_json::Value,
    },
}

impl Source {
    fn model(&self) -> &CompiledSapModel {
        match self {
            Source::Text { cache, .. } => &cache.model,
            Source::Json { model, .. } => model,
        }
    }

    fn input(&self) -> Input<'_> {
        match self {
            Source::Text { text, .. } => Input::Text(text),
            Source::Json { json, .. } => Input::Json(json),
        }
    }
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
///
/// A class instantiation or enum that implements `baml.FromLlmOutput` is read
/// as its `Shape`: the walk follows the shape, not the fields or variants.
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
    /// Each class instantiation (its arguments flat) or enum that a model
    /// writes as its `baml.FromLlmOutput` shape, with the shape, in the order
    /// first reached.
    shapes: Vec<(RealizedTy, RealizedTy)>,
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
    /// The `baml.FromLlmOutput` shape of a type cannot be read, or shapes
    /// reach ever more types.
    Shape(String),
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
            Self::Shape(message) => f.write_str(message),
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
            shape_checked: HashMap::new(),
            ancestry: Vec::new(),
            reach: ParseReach {
                classes: Vec::new(),
                enums: Vec::new(),
                aliases: Vec::new(),
                interfaces: Vec::new(),
                heads: Vec::new(),
                shapes: Vec::new(),
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
    /// Each class instantiation and enum asked for its `baml.FromLlmOutput`
    /// shape, and whether it has one.
    shape_checked: HashMap<RealizedTy, bool>,
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
            RealizedTy::Enum(_) if self.visit_shaped(ty)? => Ok(()),
            // A variant type stays a variant of its enum, shaped or not.
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
        if self.visit_shaped(&RealizedTy::Class(head, identity.1.clone()))? {
            return Ok(());
        }
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

/// Distinct shaped types one walk follows before it refuses: a shape that
/// names its own type at ever-growing arguments would reach new ones forever.
const MAX_SHAPED_TYPES: usize = 1024;

impl ReachWalk<'_> {
    /// Whether `ty`, a class instantiation (its arguments flat) or an enum, is
    /// read as its `baml.FromLlmOutput` shape. The first time one is, the
    /// walk records it and follows the shape.
    fn visit_shaped(&mut self, ty: &RealizedTy) -> Result<bool, ReachError> {
        if let Some(&shaped) = self.shape_checked.get(ty) {
            return Ok(shaped);
        }
        let shape = crate::llm_shape::llm_shape(self.vm, ty)
            .map_err(|e| ReachError::Shape(e.to_string()))?;
        self.shape_checked.insert(ty.clone(), shape.is_some());
        let Some(shape) = shape else {
            return Ok(false);
        };
        if self.reach.shapes.len() >= MAX_SHAPED_TYPES {
            return Err(ReachError::Shape(format!(
                "the `baml.FromLlmOutput` shapes reached from `{ty}` reach more than \
                 {MAX_SHAPED_TYPES} types"
            )));
        }
        self.reach.shapes.push((ty.clone(), shape.clone()));
        self.record_heads(&shape);
        // A shape is a type of its own, not relative to any enclosing class.
        self.visit(&shape, &ReachOrigins::opaque(self.ancestry.len()))?;
        Ok(true)
    }
}

/// The shaped types `ty` reaches, with their shapes ([`ParseReach::shapes`]);
/// none when the parser cannot model `ty`.
pub(crate) fn shapes_reached(vm: &BexVm, ty: &RealizedTy) -> Vec<(RealizedTy, RealizedTy)> {
    ParseReach::of(vm, ty)
        .map(|reach| reach.shapes)
        .unwrap_or_default()
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
fn build_model(vm: &BexVm, target: &RealizedTy) -> Result<(CompiledSapModel, bool), ModelError> {
    let reach = ParseReach::of(vm, target).map_err(ModelError::Reach)?;
    let has_shapes = !reach.shapes.is_empty();
    let defs = crate::json_defs::HeapDefs::output(vm);
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
    let shapes = reach.shapes.iter().map(|(ty, shape)| {
        (
            NamedTy::of(&definitions::lane_realized_ty(ty))
                .unwrap_or_else(|| unreachable!("a shaped type is a class or enum: {ty}")),
            definitions::lane_realized_ty(shape),
        )
    });
    CompiledSapModel::from_type_ctx(
        TypeCtx::new(classes, enums, &aliases, interfaces, shapes),
        target,
    )
    .map(|model| (model, has_shapes))
    .map_err(|e| ModelError::Convert(Box::new(e)))
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
/// tagged names back to the heap. Built and used between two yields, so no
/// collection moves them meanwhile.
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

/// A parsed value, before it is allocated: what the parser produced, with the
/// types its lists and maps are allocated at. Its strings borrow the text
/// they were read from until [`Parsed::into_owned`].
enum Parsed<'s> {
    String(Cow<'s, str>),
    Int(i64),
    Bigint(num_bigint::BigInt),
    Float(f64),
    Bool(bool),
    Null,
    List {
        element: ParseTy,
        items: Vec<Parsed<'s>>,
    },
    Map {
        key: ParseTy,
        value: ParseTy,
        entries: Vec<(Cow<'s, str>, Parsed<'s>)>,
    },
    Enum {
        decl: DefKey,
        variant: String,
    },
    Class {
        name: NamedTy,
        /// The fields read, by declared name.
        fields: IndexMap<String, Parsed<'s>>,
    },
    /// A shape value, which its type's `from_llm_output` builds from.
    Shaped {
        name: NamedTy,
        /// The value's [`shape_key`], for a refusal to name it.
        key: String,
        shape: Box<Parsed<'s>>,
    },
    /// What `from_llm_output` built.
    Built(Value),
}

impl<'s> Parsed<'s> {
    /// The parser's value, as the parsed tree.
    fn of(value: &BamlValueWithFlags<'s, '_, '_, NamedTy>) -> Result<Self, VmRustFnError> {
        Ok(match &value.value {
            BamlValue::String(s) => Parsed::String(s.value.clone()),
            BamlValue::Int(i) => Parsed::Int(i.value),
            BamlValue::Bigint(b) => Parsed::Bigint(b.value.clone()),
            BamlValue::Float(f) => Parsed::Float(f.value),
            BamlValue::Bool(b) => Parsed::Bool(b.value),
            BamlValue::Null(_) => Parsed::Null,
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
                Parsed::List {
                    element: array_ty.ty.to_baml_ty(),
                    items: array
                        .value
                        .iter()
                        .map(Parsed::of)
                        .collect::<Result<_, _>>()?,
                }
            }
            BamlValue::Map(map) => {
                let TyResolvedRef::Map(map_ty) = value.meta.ty else {
                    return Err(invariant(format!(
                        "a parsed map is typed `{}`, not as a map",
                        value.meta.ty.to_baml_ty()
                    )));
                };
                Parsed::Map {
                    key: map_ty.key.to_baml_ty(),
                    value: map_ty.value.to_baml_ty(),
                    entries: map
                        .value
                        .iter()
                        .map(|(key, value)| Ok((key.clone(), Parsed::of(value)?)))
                        .collect::<Result<_, VmRustFnError>>()?,
                }
            }
            BamlValue::Enum(e) => Parsed::Enum {
                decl: e
                    .name
                    .decl()
                    .ok_or_else(|| invariant(format!("`{}` is not an enum", e.name)))?
                    .clone(),
                variant: e.value.to_string(),
            },
            BamlValue::Class(c) => Parsed::Class {
                name: c.name.clone(),
                fields: c
                    .value
                    .iter()
                    .map(|(name, value)| Ok(((*name).to_string(), Parsed::of(value)?)))
                    .collect::<Result<_, VmRustFnError>>()?,
            },
            BamlValue::Shaped(shaped) => Parsed::Shaped {
                name: shaped.name.clone(),
                key: shape_key(&shaped.value.value),
                shape: Box::new(Parsed::of(&shaped.value)?),
            },
            BamlValue::StreamState(_) => {
                return Err(invariant(
                    "the parser produced a stream state, which no target declares".to_string(),
                ));
            }
        })
    }

    /// The tree, holding its own strings, to keep across a yield.
    fn into_owned(self) -> Parsed<'static> {
        match self {
            Parsed::String(s) => Parsed::String(Cow::Owned(s.into_owned())),
            Parsed::Int(i) => Parsed::Int(i),
            Parsed::Bigint(b) => Parsed::Bigint(b),
            Parsed::Float(f) => Parsed::Float(f),
            Parsed::Bool(b) => Parsed::Bool(b),
            Parsed::Null => Parsed::Null,
            Parsed::List { element, items } => Parsed::List {
                element,
                items: items.into_iter().map(Parsed::into_owned).collect(),
            },
            Parsed::Map {
                key,
                value,
                entries,
            } => Parsed::Map {
                key,
                value,
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (Cow::Owned(key.into_owned()), value.into_owned()))
                    .collect(),
            },
            Parsed::Enum { decl, variant } => Parsed::Enum { decl, variant },
            Parsed::Class { name, fields } => Parsed::Class {
                name,
                fields: fields
                    .into_iter()
                    .map(|(name, value)| (name, value.into_owned()))
                    .collect(),
            },
            Parsed::Shaped { name, key, shape } => Parsed::Shaped {
                name,
                key,
                shape: Box::new(shape.into_owned()),
            },
            Parsed::Built(value) => Parsed::Built(value),
        }
    }

    /// The children of this node, in order.
    fn children(&self) -> Vec<&Parsed<'s>> {
        match self {
            Parsed::List { items, .. } => items.iter().collect(),
            Parsed::Map { entries, .. } => entries.iter().map(|(_, value)| value).collect(),
            Parsed::Class { fields, .. } => fields.values().collect(),
            Parsed::Shaped { shape, .. } => vec![shape],
            _ => Vec::new(),
        }
    }

    /// The `index`-th child of this node, in the order of [`Self::children`].
    fn child_mut(&mut self, index: usize) -> Option<&mut Parsed<'s>> {
        match self {
            Parsed::List { items, .. } => items.get_mut(index),
            Parsed::Map { entries, .. } => entries.get_mut(index).map(|(_, value)| value),
            Parsed::Class { fields, .. } => fields.get_index_mut(index).map(|(_, value)| value),
            Parsed::Shaped { shape, .. } if index == 0 => Some(shape),
            _ => None,
        }
    }

    /// The node at `path`, a list of child indexes from this one.
    fn at_mut(&mut self, path: &[usize]) -> Option<&mut Parsed<'s>> {
        path.iter()
            .try_fold(self, |node, &index| node.child_mut(index))
    }

    /// The path of every shape value in the tree, the one to build last
    /// first: a shape value inside another is built before it.
    fn shaped_paths(&self) -> Vec<Vec<usize>> {
        fn walk(node: &Parsed<'_>, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            for (index, child) in node.children().into_iter().enumerate() {
                path.push(index);
                walk(child, path, out);
                path.pop();
            }
            if matches!(node, Parsed::Shaped { .. }) {
                out.push(path.clone());
            }
        }
        let mut out = Vec::new();
        walk(self, &mut Vec::new(), &mut out);
        out.reverse();
        out
    }

    /// Hand every value `from_llm_output` built to a collection's visitor.
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        if let Parsed::Built(value) = self {
            value.visit_roots(visitor);
        }
        let mut index = 0;
        while let Some(child) = self.child_mut(index) {
            child.visit_roots(visitor);
            index += 1;
        }
    }
}

/// Allocate `value`: the same value the parser produced, on the heap. A
/// value the heap cannot hold (the parser's integer is wider than the VM's)
/// raises `mode`'s mismatch.
fn to_heap(
    vm: &mut BexVm,
    value: &BamlValueWithFlags<'_, '_, '_, NamedTy>,
    declarations: &Declarations,
    mode: Mode,
) -> Result<Value, VmRustFnError> {
    Ok(match &value.value {
        BamlValue::String(s) => Value::object(vm.alloc_string(s.value.as_ref())),
        BamlValue::Int(i) => heap_int(i.value, mode)?,
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
            let element = declarations.realize(&array_ty.ty.to_baml_ty())?;
            let items = array
                .value
                .iter()
                .map(|item| to_heap(vm, item, declarations, mode))
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
            let key = declarations.realize(&map_ty.key.to_baml_ty())?;
            let value_ty = declarations.realize(&map_ty.value.to_baml_ty())?;
            let mut entries = IndexMap::with_capacity(map.value.len());
            for (k, v) in &map.value {
                entries.insert(
                    BexStr::from(k.as_ref()),
                    to_heap(vm, v, declarations, mode)?,
                );
            }
            Value::object(vm.alloc_map(key, value_ty, entries))
        }
        BamlValue::Enum(e) => {
            let decl = e
                .name
                .decl()
                .ok_or_else(|| invariant(format!("`{}` is not an enum", e.name)))?;
            heap_variant(vm, declarations, decl, e.value)?
        }
        BamlValue::Class(c) => heap_instance(vm, declarations, c.name, |vm, field| {
            c.value
                .get(field)
                .map(|parsed| to_heap(vm, parsed, declarations, mode))
        })?,
        BamlValue::Shaped(shaped) => {
            return Err(invariant(format!(
                "a `{}` shape value was allocated before `from_llm_output` built it",
                shaped.name
            )));
        }
        BamlValue::StreamState(_) => {
            return Err(invariant(
                "the parser produced a stream state, which no target declares".to_string(),
            ));
        }
    })
}

/// [`to_heap`] for a parsed tree a [`ShapedRead`] keeps: every shape value in
/// it is built.
fn parsed_to_heap(
    vm: &mut BexVm,
    value: &Parsed<'_>,
    declarations: &Declarations,
    mode: Mode,
) -> Result<Value, VmRustFnError> {
    Ok(match value {
        Parsed::String(s) => Value::object(vm.alloc_string(s.as_ref())),
        Parsed::Int(i) => heap_int(*i, mode)?,
        Parsed::Bigint(b) => vm.try_alloc_bigint(Arc::new(b.clone()))?,
        Parsed::Float(f) => Value::object(vm.alloc_float(*f)),
        Parsed::Bool(b) => Value::bool(*b),
        Parsed::Null => Value::NULL,
        Parsed::List { element, items } => {
            let element = declarations.realize(element)?;
            let items = items
                .iter()
                .map(|item| parsed_to_heap(vm, item, declarations, mode))
                .collect::<Result<Vec<_>, _>>()?;
            Value::object(vm.alloc_array(element, items))
        }
        Parsed::Map {
            key,
            value,
            entries,
        } => {
            let (key, value_ty) = (declarations.realize(key)?, declarations.realize(value)?);
            let mut values = IndexMap::with_capacity(entries.len());
            for (k, v) in entries {
                values.insert(
                    BexStr::from(k.as_ref()),
                    parsed_to_heap(vm, v, declarations, mode)?,
                );
            }
            Value::object(vm.alloc_map(key, value_ty, values))
        }
        Parsed::Enum { decl, variant } => heap_variant(vm, declarations, decl, variant)?,
        Parsed::Class { name, fields } => heap_instance(vm, declarations, name, |vm, field| {
            fields
                .get(field)
                .map(|parsed| parsed_to_heap(vm, parsed, declarations, mode))
        })?,
        Parsed::Shaped { name, .. } => {
            return Err(invariant(format!(
                "a `{name}` shape value was allocated before `from_llm_output` built it"
            )));
        }
        Parsed::Built(value) => *value,
    })
}

/// The parser's integer, if the VM's `int` holds it.
fn heap_int(value: i64, mode: Mode) -> Result<Value, VmRustFnError> {
    Value::try_int(value).ok_or_else(|| {
        mode.mismatch(format!(
            "integer {value} is outside the BAML integer range [{}, {}]",
            Value::INT_MIN,
            Value::INT_MAX
        ))
    })
}

/// The variant `variant` of the enum `decl`.
fn heap_variant(
    vm: &mut BexVm,
    declarations: &Declarations,
    decl: &DefKey,
    variant: &str,
) -> Result<Value, VmRustFnError> {
    let enm = declarations.get(decl)?;
    let Object::Enum(declaration) = vm.get_object(enm) else {
        return Err(invariant(format!("`{decl}` is not an enum")));
    };
    let index = declaration
        .variants
        .iter()
        .position(|candidate| candidate.name == variant)
        .ok_or_else(|| invariant(format!("enum `{decl}` has no variant `{variant}`")))?;
    Ok(Value::object(vm.alloc_variant(enm, index)))
}

/// An instance of the class instantiation `name`, each field allocated by
/// `read` from the value the parser read for it. A `@skip` field the parser
/// did not read holds its type's empty value.
fn heap_instance(
    vm: &mut BexVm,
    declarations: &Declarations,
    name: &NamedTy,
    mut read: impl FnMut(&mut BexVm, &str) -> Option<Result<Value, VmRustFnError>>,
) -> Result<Value, VmRustFnError> {
    let NamedTyKind::Class(decl, args) = name.kind() else {
        return Err(invariant(format!("`{name}` is not a class")));
    };
    let class = declarations.get(decl)?;
    let type_args = args
        .iter()
        .map(|arg| declarations.realize(arg))
        .collect::<Result<Box<[_]>, _>>()?;
    let Object::Class(declaration) = vm.get_object(class) else {
        return Err(invariant(format!("`{name}` is not a class")));
    };
    let fields: Vec<(String, bool, TyTemplate)> = declaration
        .fields
        .iter()
        .map(|field| (field.name.clone(), field.skip, field.field_template.clone()))
        .collect();
    let mut values = Vec::with_capacity(fields.len());
    for (field, skip, field_template) in fields {
        let value = match read(vm, &field) {
            Some(value) => value?,
            None if skip => {
                let field_type = vm.realize_field_ty(&field_template, &type_args);
                skipped_field_default(vm, name, &field, &field_type)?
            }
            None => {
                return Err(invariant(format!(
                    "the parsed `{name}` has no field `{field}`"
                )));
            }
        };
        values.push(value);
    }
    Ok(Value::object(
        vm.tlab
            .alloc_instance_with_type_args(class, type_args, values),
    ))
}

/// A read whose value has shaped parts: it calls `from_llm_output` for each,
/// innermost first, through `baml.sap._Rejected._build`, which turns a thrown
/// `baml.json.DecodeError` into a `_Rejected`.
///
/// A refusal means the shape value does not fit its type. The read builds
/// every other shape value of the tree first, and collects every refusal.
/// Then it parses its source again once, with all of them refused, so a
/// union tries its next member, and the parse fails where no member is left.
/// The new tree is built the same way. A pass that refuses nothing ends the
/// read. Each refusal is new, and there are finitely many shape values in a
/// source, so the passes end.
struct ShapedRead {
    source: Source,
    target: RealizedTy,
    mode: Mode,
    rejections: Rejections<NamedTy>,
    tree: Parsed<'static>,
    /// The shape values this pass still has to build, the next one last.
    pending: Vec<Vec<usize>>,
    /// The shape values this pass refused, or did not build because a shape
    /// value inside them was refused: the next tree reads them anew.
    refused: Vec<Vec<usize>>,
    /// The shape value `from_llm_output` is building now.
    building: Option<Vec<usize>>,
    /// The declarations the target reaches, until a collection moves them.
    declarations: Option<Declarations>,
}

impl ShapedRead {
    /// Build the next shape value. Once the pass has built them all, parse
    /// again if it refused any, or allocate the whole value.
    fn run(mut self, vm: &mut BexVm) -> NativeCallResult {
        let next = match self.next_call(vm) {
            Ok(next) => next,
            Err(error) => return NativeCallResult::Error(error),
        };
        match next {
            Some((callee, args, type_args)) => NativeCallResult::YieldToCall {
                callee,
                args,
                type_args,
                continuation: Box::new(self),
            },
            None if !self.refused.is_empty() => self.reparse(vm),
            None => {
                let declarations = self
                    .declarations
                    .as_ref()
                    .unwrap_or_else(|| unreachable!("the declarations were just read"));
                match parsed_to_heap(vm, &self.tree, declarations, self.mode) {
                    Ok(value) => NativeCallResult::Done(value),
                    Err(error) => NativeCallResult::Error(error),
                }
            }
        }
    }

    /// The `_build` call for the next shape value this pass builds, or `None`
    /// once the pass is done. A shape value that holds a refused one is not
    /// built, and one this read already refused is refused again without a
    /// call.
    #[allow(clippy::type_complexity)]
    fn next_call(
        &mut self,
        vm: &mut BexVm,
    ) -> Result<Option<(HeapPtr, Vec<Value>, Vec<RealizedTy>)>, VmRustFnError> {
        if self.declarations.is_none() {
            self.declarations = Some(Declarations::reached_by(vm, &self.target)?);
        }
        let declarations = self
            .declarations
            .as_ref()
            .unwrap_or_else(|| unreachable!("the declarations were just read"));
        while let Some(path) = self.pending.pop() {
            if self
                .refused
                .iter()
                .any(|refused| refused.len() > path.len() && refused.starts_with(&path))
            {
                self.refused.push(path);
                continue;
            }
            let Some(Parsed::Shaped { name, key, shape }) = self.tree.at_mut(&path) else {
                return Err(invariant(
                    "a pending shape value is not in the parsed tree".to_string(),
                ));
            };
            // The same shape value twice in one answer: `from_llm_output`
            // already refused it.
            if self.rejections.contains_key(&(name.clone(), key.clone())) {
                self.refused.push(path);
                continue;
            }
            let ty = declarations.realize(&name.to_ty())?;
            let shape = parsed_to_heap(vm, shape, declarations, self.mode)?;
            let build = build_function(vm)?;
            self.building = Some(path);
            return Ok(Some((build, vec![shape], vec![ty])));
        }
        Ok(None)
    }

    /// `built` is what `_build` returned for the shape value being built.
    fn built(mut self, vm: &mut BexVm, built: Value) -> NativeCallResult {
        let Some(path) = self.building.take() else {
            return NativeCallResult::Error(invariant(
                "a shaped read resumed with no shape value being built".to_string(),
            ));
        };
        let Some(node) = self.tree.at_mut(&path) else {
            return NativeCallResult::Error(invariant(
                "the shape value being built is not in the parsed tree".to_string(),
            ));
        };
        match refusal(vm, built) {
            None => *node = Parsed::Built(built),
            Some(message) => {
                let Parsed::Shaped { name, key, .. } = node else {
                    return NativeCallResult::Error(invariant(
                        "the shape value being built is not a shape value".to_string(),
                    ));
                };
                self.rejections.insert((name.clone(), key.clone()), message);
                self.refused.push(path);
            }
        }
        self.run(vm)
    }

    /// Parse the source again, with every refusal so far, and start a pass
    /// over the new tree.
    fn reparse(mut self, vm: &mut BexVm) -> NativeCallResult {
        let tree = match with_tree(
            self.source.model(),
            self.source.input(),
            self.mode,
            &self.rejections,
            |tree| Ok(Parsed::of(tree)?.into_owned()),
        ) {
            Ok(Some(tree)) => tree,
            Ok(None) => return self.mode.no_value(vm),
            Err(error) => return NativeCallResult::Error(error),
        };
        self.pending = tree.shaped_paths();
        self.refused.clear();
        self.tree = tree;
        self.run(vm)
    }
}

impl RootHaver for ShapedRead {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            source: _,
            target,
            mode: _,
            rejections: _,
            tree,
            pending: _,
            refused: _,
            building: _,
            declarations,
        } = self;
        // A cache of the declarations the target reaches: forgotten at every
        // collection rather than walked, and worked out again when needed.
        *declarations = None;
        tree.visit_roots(visitor);
        target.visit_roots(visitor);
    }
}

impl Continuation for ShapedRead {
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.built(vm, value)
    }
}

/// `baml.sap._Rejected._build`, the wrapper a shaped read calls
/// `from_llm_output` through.
fn build_function(vm: &BexVm) -> Result<HeapPtr, VmRustFnError> {
    let rejected = vm.lang_object(crate::lang_items::BAML_SAP_REJECTED);
    let Object::Class(class) = vm.get_object(rejected) else {
        return Err(invariant("`baml.sap._Rejected` is not a class".to_string()));
    };
    class
        .methods
        .iter()
        .find(|(name, _)| name.as_str() == "_build")
        .map(|(_, method)| method.function_ptr)
        .filter(|ptr| !ptr.is_null())
        .ok_or_else(|| invariant("`baml.sap._Rejected` has no `_build`".to_string()))
}

/// The message of `value` when it is a `baml.sap._Rejected`: what
/// `from_llm_output` threw.
fn refusal(vm: &BexVm, value: Value) -> Option<String> {
    let ptr = value.as_object_ptr()?;
    let Object::Instance(instance) = vm.get_object(ptr) else {
        return None;
    };
    if instance.class != vm.lang_object(crate::lang_items::BAML_SAP_REJECTED) {
        return None;
    }
    let message = instance.load_field(0);
    Some(
        vm.as_string(&message)
            .map(|message| message.as_str().to_string())
            .unwrap_or_default(),
    )
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
