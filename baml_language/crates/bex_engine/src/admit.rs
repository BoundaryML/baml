//! Admission: the one judgment of a value that a host hands the engine.
//!
//! A host value arrives as a [`BexExternalValue`] and is bound for a slot with
//! a declared type: a function argument, the return value of a host callback,
//! a class field or a list item inside either. [`admit`] decides whether the
//! value inhabits that type. Its result is an [`Admitted`] tree in which every
//! decision is made, so landing it on the heap ([`BexEngine::land`]) decides
//! nothing.
//!
//! The judgment is directed by the declared type, and a value that no arm
//! takes is a [`Mismatch`]: nothing enters a slot as it arrived because no
//! rule spoke for it.
//!
//! # The engine converts nothing
//!
//! A value inhabits its slot as it is, or it is refused. What a host means
//! by a value that is not a BAML value (a JavaScript number, a Python `int`
//! where a `float` is declared, an object where a class is) is the host's
//! bridge to say, before the value reaches the engine. The engine has no
//! reading of its own, so the judgment is the same at every depth and under
//! a union, an optional or an alias.
//!
//! # Unions
//!
//! A value is admitted into each member of a union, and one of the members
//! that took it is chosen: the type the host named for the value, else a
//! member that asks something of the value before one that takes every
//! value. What still ties is the same value in more than one type. Where
//! every member left would land the same value, which was meant makes no
//! difference; otherwise only the host knows, and it has to name the type.

use baml_type::{Literal, MediaKind, TypeName};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, HostValueKind, RuntimeTy, UnionMetadata,
    is_canonical_json_alias, runtime_ty_structurally_equal, selected_arm_equal,
    value_satisfies_json,
};
use bex_heap::{HeapPermit, TlabHolder};
use bex_vm_types::{Object, Value};

use crate::{
    BexEngine, EngineError,
    conversion::{
        self, InboundDeclarationKind, InboundRuntimeOverlay, class_annotation_can_refine,
        class_type_args_compatible, first_unbound_type_var, float_literal_matches,
        inbound_annotation_resolves_to_root_union, is_structural_host_only, peel_to_rust_type,
        refine_class_annotation_args, runtime_ty_assignable_with_aliases,
        runtime_ty_resolves_to_exact_null, stdlib_media_wrapper_kind, stdlib_media_wrapper_matches,
        stdlib_media_wrapper_payload_type, type_name_matches_external_name,
    },
    thread::BexThread,
};

/// How deep a judgment may nest before it is given up. A value is a finite
/// tree, so only a type can exceed this: an alias cycle that consumes no value
/// on the way round.
const MAX_DEPTH: usize = 512;

/// Why a host value does not inhabit the type it was bound for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mismatch {
    pub(crate) message: String,
}

impl Mismatch {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// `value` is of another kind than `ty` holds.
    fn kind(value: &BexExternalValue, ty: &RuntimeTy) -> Self {
        Self::new(format!(
            "Value of type '{}' does not match the declared type `{ty}`",
            conversion::described_value_type(value),
        ))
    }
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<Mismatch> for EngineError {
    fn from(mismatch: Mismatch) -> Self {
        EngineError::TypeMismatch {
            message: mismatch.message,
        }
    }
}

/// The fields of a class, as a value is judged against them.
pub(crate) enum ClassFacts {
    /// The program has no class of that name.
    Missing,
    /// The fields in declaration order, each with its type for the type
    /// arguments that were asked.
    Fields(Vec<(String, RuntimeTy)>),
}

/// Whether an enum declares a variant.
pub(crate) enum VariantFacts {
    /// The program has no enum of that name.
    NoEnum,
    NoVariant,
    /// The variant, at this index among the enum's variants.
    Declared(usize),
}

/// What the judgment reads of the program and of its live values. The engine
/// answers from the heap ([`Heap`]).
pub(crate) trait Program {
    /// The fields of the class a host calls `name`, for `type_args`.
    fn class(&self, name: &str, type_args: &[RuntimeTy]) -> Result<ClassFacts, Mismatch>;

    fn variant(&self, enum_name: &str, variant: &str) -> VariantFacts;

    /// Whether the object behind `handle` is a member of `ty`. A live value
    /// is judged as `is` judges it, since its type is on the heap and not in
    /// what the host sent.
    fn live_value_fits(
        &self,
        handle: &bex_external_types::Handle,
        ty: &RuntimeTy,
    ) -> Result<bool, Mismatch>;

    /// Whether every value of `value_ty` is a member of `ty`.
    fn is_subtype(&self, value_ty: &RuntimeTy, ty: &RuntimeTy) -> Result<bool, Mismatch>;
}

/// What one judgment is made against.
pub(crate) struct Cx<'c> {
    pub(crate) program: &'c dyn Program,
    /// The bodies of the program's aliases. An alias is transparent: a value
    /// is judged against what it stands for.
    pub(crate) aliases: &'c indexmap::IndexMap<TypeName, RuntimeTy>,
}

/// The kind of type that took a value, where a union reads it to choose
/// between two members that both took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Leaf {
    /// A type that takes every value it is asked about: `unknown`, a type
    /// variable, `json`.
    CatchAll,
    /// A type that asks something of the value.
    Other,
}

/// A value that inhabits the type it was judged against, with every decision
/// made. It borrows from the value it was made of.
#[derive(Debug, PartialEq)]
pub(crate) enum Admitted<'a> {
    Null,
    Int(VmInt),
    Bigint(std::borrow::Cow<'a, num_bigint::BigInt>),
    Float(f64),
    Bool(bool),
    String(&'a bex_str::BexStr),
    Bytes(&'a [u8]),
    List {
        element: RuntimeTy,
        items: Vec<Admitted<'a>>,
    },
    Map {
        /// The key type the host sent, and the one that is declared: a key is
        /// checked against both where the map lands.
        sent_key: &'a RuntimeTy,
        key: RuntimeTy,
        value: RuntimeTy,
        entries: Vec<(&'a str, Admitted<'a>)>,
    },
    Instance {
        /// The class, by the name the program resolves.
        class: String,
        type_args: Vec<RuntimeTy>,
        /// One value for each field, in declaration order.
        fields: Vec<Admitted<'a>>,
    },
    Variant {
        enum_name: String,
        index: usize,
    },
    /// A value of the engine's own making that the host hands back: a live
    /// reference, a host callable, a media value, a reflected type. It lands
    /// through the constructor of its kind, for the slot it was admitted to.
    Engine {
        value: &'a BexExternalValue,
        slot: RuntimeTy,
    },
}

/// An integer the VM can hold. A VM integer is narrower than the host's, so
/// one is made only of a host integer that was checked.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VmInt {
    int: i64,
    value: Value,
}

impl PartialEq for VmInt {
    fn eq(&self, other: &Self) -> bool {
        self.int == other.int
    }
}

impl VmInt {
    fn new(int: i64) -> Result<Self, Mismatch> {
        match Value::try_int(int) {
            Some(value) => Ok(Self { int, value }),
            None => Err(Mismatch::new(format!(
                "integer {int} is outside the BAML integer range [{}, {}]",
                Value::INT_MIN,
                Value::INT_MAX
            ))),
        }
    }
}

/// A value with the kind of type that took it.
#[derive(Debug)]
pub(crate) struct Admission<'a> {
    pub(crate) value: Admitted<'a>,
    leaf: Leaf,
}

impl<'a> Admission<'a> {
    fn of(value: Admitted<'a>) -> Self {
        Self {
            value,
            leaf: Leaf::Other,
        }
    }

    fn engine(value: &'a BexExternalValue, slot: &RuntimeTy) -> Self {
        Self::of(Admitted::Engine {
            value,
            slot: slot.clone(),
        })
    }

    fn leaf(mut self, leaf: Leaf) -> Self {
        self.leaf = leaf;
        self
    }
}

/// Judge `value` against `ty`.
pub(crate) fn admit<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
) -> Result<Admission<'a>, Mismatch> {
    admit_at(cx, value, ty, 0)
}

fn admit_at<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    if depth > MAX_DEPTH {
        return Err(Mismatch::new(format!(
            "the declared type `{ty}` is too deeply nested to check a value against"
        )));
    }
    let depth = depth + 1;

    // A live reference is judged by the object it refers to, whatever the
    // slot: the host sent a key, and the type is on the heap.
    match value {
        BexExternalValue::Handle(handle)
        | BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
            heap_handle: handle,
            ..
        }) => {
            return if cx.program.live_value_fits(handle, ty)? {
                Ok(Admission::engine(value, ty))
            } else {
                Err(Mismatch::kind(value, ty))
            };
        }
        _ => {}
    }

    // A `$rust_type` slot carries a structural host value through the VM as
    // opaque data, and hands the same value back.
    if peel_to_rust_type(ty).is_some() && is_structural_host_only(value) {
        return Ok(Admission::engine(value, ty));
    }

    match ty {
        RuntimeTy::TypeAlias(name) if is_canonical_json_alias(name) => admit_json(value, ty),
        // An alias is transparent: the value is judged against what it
        // stands for.
        RuntimeTy::TypeAlias(name) => {
            let body = cx.aliases.get(name).ok_or_else(|| {
                Mismatch::new(format!(
                    "the declared type `{ty}` is an alias this program cannot expand"
                ))
            })?;
            admit_at(cx, value, body, depth)
        }
        _ if let BexExternalValue::Union {
            value: payload,
            metadata,
        } = value =>
        {
            admit_typed_by_host(cx, value, payload, metadata, ty, depth)
        }
        RuntimeTy::Union(members) => admit_union(cx, value, ty, members, depth),
        RuntimeTy::Unknown | RuntimeTy::TypeVar(_) => {
            Ok(admit_as_itself(cx, value, ty, depth)?.leaf(Leaf::CatchAll))
        }
        RuntimeTy::Null | RuntimeTy::Void => match value {
            BexExternalValue::Null => Ok(Admission::of(Admitted::Null)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Never => Err(Mismatch::kind(value, ty)),
        RuntimeTy::Bool => match value {
            BexExternalValue::Bool(b) => Ok(Admission::of(Admitted::Bool(*b))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::String => match value {
            BexExternalValue::String(s) => Ok(Admission::of(Admitted::String(s))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Uint8Array => match value {
            BexExternalValue::Uint8Array(bytes) => Ok(Admission::of(Admitted::Bytes(bytes))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Int => match value {
            BexExternalValue::Int(i) => VmInt::new(*i).map(Admitted::Int).map(Admission::of),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Float => match value {
            BexExternalValue::Float(f) => Ok(Admission::of(Admitted::Float(*f))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Bigint => match value {
            BexExternalValue::Bigint(big) => {
                bigint(std::borrow::Cow::Borrowed(big)).map(Admission::of)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Literal(literal, _) => admit_literal(cx, value, ty, literal, depth),
        RuntimeTy::List(element) => match value {
            BexExternalValue::Array { items, .. } => {
                let items = items
                    .iter()
                    .map(|item| Ok(admit_at(cx, item, element, depth)?.value))
                    .collect::<Result<_, Mismatch>>()?;
                Ok(Admission::of(Admitted::List {
                    element: element.as_ref().clone(),
                    items,
                }))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Map {
            key,
            value: value_ty,
            ..
        } => match value {
            BexExternalValue::Map {
                key_type, entries, ..
            } => {
                let entries = entries
                    .iter()
                    .map(|(name, entry)| {
                        Ok((name.as_str(), admit_at(cx, entry, value_ty, depth)?.value))
                    })
                    .collect::<Result<_, Mismatch>>()?;
                Ok(Admission::of(Admitted::Map {
                    sent_key: key_type,
                    key: key.as_ref().clone(),
                    value: value_ty.as_ref().clone(),
                    entries,
                }))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Class(name, args) => {
            admit_class(cx, value, ty, name, args, ClassOrigin::Slot, depth)
        }
        RuntimeTy::Enum(name) => match value {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } if type_name_matches_external_name(enum_name, name) => {
                admit_variant(cx, &name.to_string(), variant_name)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::EnumVariant(name, declared) => match value {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } if type_name_matches_external_name(enum_name, name)
                && variant_name == declared.as_str() =>
            {
                admit_variant(cx, &name.to_string(), variant_name)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Media(kind) => match value {
            BexExternalValue::Adt(BexExternalAdt::Media(media))
                if *kind == MediaKind::Generic || media.kind == *kind =>
            {
                Ok(Admission::engine(value, ty))
            }
            // A bridge whose media type is a class sends its shell: the one
            // `_data` field holds the media value.
            BexExternalValue::Instance { fields, .. } => {
                let data = fields
                    .get(bex_external_types::MEDIA_WRAPPER_DATA_FIELD)
                    .ok_or_else(|| {
                        Mismatch::new(format!(
                            "host media payload for `{ty}` is missing its `_data` handle"
                        ))
                    })?;
                if fields.len() != 1 {
                    return Err(Mismatch::new(format!(
                        "host media payload for `{ty}` contains unexpected fields"
                    )));
                }
                admit_at(cx, data, ty, depth)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::Function { .. } => match value {
            BexExternalValue::HostValue(host) if host.kind == HostValueKind::Callable => {
                Ok(Admission::engine(value, ty))
            }
            BexExternalValue::FunctionRef { .. } => Ok(Admission::engine(value, ty)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::RustType => match value {
            BexExternalValue::RustData(_)
            | BexExternalValue::Adt(BexExternalAdt::Media(_) | BexExternalAdt::PromptAst(_)) => {
                Ok(Admission::engine(value, ty))
            }
            BexExternalValue::HostValue(host) if host.kind == HostValueKind::Opaque => {
                Ok(Admission::engine(value, ty))
            }
            BexExternalValue::HostValue(_) => Err(Mismatch::kind(value, ty)),
            // `$rust_type` is also what a generic call binds a type variable
            // to when no value gave it a type. Such a slot asks nothing of
            // the value, which rides through as what it is.
            _ => Ok(admit_as_itself(cx, value, ty, depth)?.leaf(Leaf::CatchAll)),
        },
        RuntimeTy::Type => match value {
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
                Ok(Admission::engine(value, ty))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        RuntimeTy::PromptAst => match value {
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => Ok(Admission::engine(value, ty)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        // No host value says which implementor it is except by being one: the
        // value is admitted as what it is, and that type is asked of the slot.
        RuntimeTy::Interface(..) | RuntimeTy::AssociatedTypeProjection { .. } => {
            let own = own_type(value).ok_or_else(|| Mismatch::kind(value, ty))?;
            if !cx.program.is_subtype(&own, ty)? {
                return Err(Mismatch::kind(value, ty));
            }
            admit_at(cx, value, &own, depth)
        }
        // Only a live value inhabits these, and that was judged above.
        RuntimeTy::Future(..) | RuntimeTy::Resource => Err(Mismatch::kind(value, ty)),
    }
}

fn bigint(big: std::borrow::Cow<'_, num_bigint::BigInt>) -> Result<Admitted<'_>, Mismatch> {
    let bits = big.bits();
    if bits > baml_type::MAX_BIGINT_BITS {
        return Err(Mismatch::new(format!(
            "bigint value requires {bits} bits (limit: {})",
            baml_type::MAX_BIGINT_BITS
        )));
    }
    Ok(Admitted::Bigint(big))
}

/// A literal type holds the one value of its base type that it names.
fn admit_literal<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
    literal: &Literal,
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    let base = match literal {
        Literal::Int(_) => RuntimeTy::Int,
        Literal::Bigint(_) => RuntimeTy::Bigint,
        Literal::Float(_) => RuntimeTy::Float,
        Literal::String(_) => RuntimeTy::String,
        Literal::Bool(_) => RuntimeTy::Bool,
    };
    let admission = admit_at(cx, value, &base, depth).map_err(|_| Mismatch::kind(value, ty))?;
    let is_the_value = match (literal, &admission.value) {
        (Literal::Int(expected), Admitted::Int(i)) => *expected == i.int,
        (Literal::Bigint(expected), Admitted::Bigint(big)) => expected == big.as_ref(),
        (Literal::Float(expected), Admitted::Float(f)) => float_literal_matches(*f, expected),
        (Literal::String(expected), Admitted::String(s)) => expected == s.as_str(),
        (Literal::Bool(expected), Admitted::Bool(b)) => expected == b,
        (
            Literal::Int(_)
            | Literal::Bigint(_)
            | Literal::Float(_)
            | Literal::String(_)
            | Literal::Bool(_),
            _,
        ) => false,
    };
    if is_the_value {
        Ok(admission)
    } else {
        Err(Mismatch::kind(value, ty))
    }
}

fn admit_variant<'a>(
    cx: &Cx<'_>,
    enum_name: &str,
    variant: &str,
) -> Result<Admission<'a>, Mismatch> {
    match cx.program.variant(enum_name, variant) {
        VariantFacts::Declared(index) => Ok(Admission::of(Admitted::Variant {
            enum_name: enum_name.to_string(),
            index,
        })),
        VariantFacts::NoVariant => Err(Mismatch::new(format!(
            "Unknown variant `{variant}` in enum `{enum_name}`"
        ))),
        VariantFacts::NoEnum => Err(Mismatch::new(format!(
            "Unknown enum `{enum_name}` in external Variant value"
        ))),
    }
}

/// Who said that a value is of the class it is judged against.
#[derive(Debug, Clone, Copy)]
enum ClassOrigin {
    /// The slot declares the class, and the value has to be one.
    Slot,
    /// The host named the class as the value's type. The wire carries a class
    /// instance as its fields with the class beside them, so this is how an
    /// instance arrives.
    NamedByHost,
}

/// The class named `name`, of an instance that names it, or of a value with
/// no class of its own.
fn admit_class<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
    name: &TypeName,
    args: &[RuntimeTy],
    origin: ClassOrigin,
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    let (fields, type_args) = match value {
        BexExternalValue::Instance {
            class_name,
            type_args,
            fields,
        } => {
            let named = if class_name.is_empty() {
                // An instance with no class of its own is one only where the
                // host named the class for it.
                matches!(origin, ClassOrigin::NamedByHost)
            } else {
                type_name_matches_external_name(class_name, name)
            };
            if !named {
                return Err(Mismatch::kind(value, ty));
            }
            // A type argument is a type the host knows; a variable there is
            // one it did not bind.
            if let Some(unbound) = type_args.iter().find_map(first_unbound_type_var) {
                return Err(Mismatch::new(format!(
                    "host class type arguments of `{ty}` carry an unbound type variable \
                     `{unbound}`; the host must send fully-bound generic instances"
                )));
            }
            if !type_args.is_empty() && !class_type_args_compatible(type_args, args) {
                return Err(Mismatch::new(format!(
                    "host class type arguments `{}` are incompatible with declared class `{ty}`",
                    type_args
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                )));
            }
            (fields, refine_class_annotation_args(type_args, args))
        }
        // The engine's own media and prompt values are instances of the
        // standard library's wrapper classes.
        BexExternalValue::Adt(BexExternalAdt::Media(media))
            if stdlib_media_wrapper_kind(ty) == Some(media.kind) =>
        {
            return Ok(Admission::engine(value, ty));
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_))
            if name.render_dotted(false) == conversion::PROMPT_CLASS =>
        {
            return Ok(Admission::engine(value, ty));
        }
        _ => return Err(Mismatch::kind(value, ty)),
    };

    let class = name.to_string();
    let declared = match cx.program.class(&class, &type_args)? {
        ClassFacts::Fields(declared) => declared,
        ClassFacts::Missing => {
            return Err(Mismatch::new(format!(
                "Unknown class `{class}` in external Instance value"
            )));
        }
    };
    if let Some(unknown) = fields
        .keys()
        .find(|key| !declared.iter().any(|(field, _)| field == *key))
    {
        return Err(Mismatch::new(format!(
            "class `{class}` has no field `{}`",
            bounded(unknown)
        )));
    }
    let fields = declared
        .iter()
        .map(|(field, field_ty)| {
            let sent = fields.get(field).ok_or_else(|| {
                Mismatch::new(format!(
                    "Missing field `{field}` in external Instance for class `{class}`"
                ))
            })?;
            Ok(admit_at(cx, sent, field_ty, depth)?.value)
        })
        .collect::<Result<_, Mismatch>>()?;
    Ok(Admission::of(Admitted::Instance {
        class,
        type_args,
        fields,
    }))
}

/// A host's own text in a message, cut to a length a message can carry.
fn bounded(text: &str) -> String {
    const LIMIT: usize = 80;
    if text.chars().count() <= LIMIT {
        text.to_string()
    } else {
        let head: String = text.chars().take(LIMIT).collect();
        format!("{head}…")
    }
}

/// The type a value has by what it is: the class it names, the element type
/// it carries. `None` for a value that has no type until a slot gives it one.
fn own_type(value: &BexExternalValue) -> Option<RuntimeTy> {
    match value {
        BexExternalValue::Null => Some(RuntimeTy::Null),
        BexExternalValue::Int(_) => Some(RuntimeTy::Int),
        BexExternalValue::Bigint(_) => Some(RuntimeTy::Bigint),
        BexExternalValue::Float(_) => Some(RuntimeTy::Float),
        BexExternalValue::Bool(_) => Some(RuntimeTy::Bool),
        BexExternalValue::String(_) => Some(RuntimeTy::String),
        BexExternalValue::Uint8Array(_) => Some(RuntimeTy::Uint8Array),
        BexExternalValue::Array { element_type, .. } => {
            Some(RuntimeTy::List(Box::new(element_type.clone())))
        }
        BexExternalValue::Map {
            key_type,
            value_type,
            ..
        } => Some(RuntimeTy::Map {
            key: Box::new(key_type.clone()),
            value: Box::new(value_type.clone()),
        }),
        BexExternalValue::Instance {
            class_name,
            type_args,
            ..
        } if !class_name.is_empty() => Some(RuntimeTy::Class(
            TypeName::from_dotted_path(class_name),
            type_args.clone().into(),
        )),
        BexExternalValue::Variant { enum_name, .. } => {
            Some(RuntimeTy::Enum(TypeName::from_dotted_path(enum_name)))
        }
        BexExternalValue::Adt(BexExternalAdt::Media(media)) => Some(RuntimeTy::Media(media.kind)),
        BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
            Some(RuntimeTy::Type)
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => Some(RuntimeTy::PromptAst),
        BexExternalValue::RustData(_) => Some(RuntimeTy::RustType),
        BexExternalValue::Union { metadata, .. } => Some(metadata.selected_option.clone()),
        // An object that names no class, a callable with no signature, and a
        // reference whose type is on the heap.
        BexExternalValue::Instance { .. }
        | BexExternalValue::FunctionRef { .. }
        | BexExternalValue::HostValue(_)
        | BexExternalValue::Handle(_)
        | BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { .. }) => None,
    }
}

/// A slot that takes every value takes this one as what it is.
fn admit_as_itself<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    match value {
        // A function reference lands as the function it indexes.
        BexExternalValue::FunctionRef { .. } => Ok(Admission::engine(value, ty)),
        _ => {
            let own = own_type(value).ok_or_else(|| {
                Mismatch::new(format!(
                    "Value of type '{}' has no type of its own, and the declared type `{ty}` \
                     gives it none",
                    conversion::described_value_type(value),
                ))
            })?;
            admit_at(cx, value, &own, depth)
        }
    }
}

/// A value of the recursive `baml.json.json` alias. Its containers take the
/// alias as their element type, as the containers of a value that BAML parsed
/// do, so a type test reads both alike.
fn admit_json<'a>(
    value: &'a BexExternalValue,
    json: &RuntimeTy,
) -> Result<Admission<'a>, Mismatch> {
    fn tree<'a>(value: &'a BexExternalValue, json: &RuntimeTy) -> Result<Admitted<'a>, Mismatch> {
        Ok(match value {
            BexExternalValue::Null => Admitted::Null,
            BexExternalValue::Int(i) => Admitted::Int(VmInt::new(*i)?),
            BexExternalValue::Bigint(big) => bigint(std::borrow::Cow::Borrowed(big))?,
            BexExternalValue::Bool(b) => Admitted::Bool(*b),
            BexExternalValue::String(s) => Admitted::String(s),
            BexExternalValue::Float(f) => Admitted::Float(*f),
            BexExternalValue::Array { items, .. } => Admitted::List {
                element: json.clone(),
                items: items
                    .iter()
                    .map(|item| tree(item, json))
                    .collect::<Result<_, _>>()?,
            },
            BexExternalValue::Map {
                key_type, entries, ..
            } => Admitted::Map {
                sent_key: key_type,
                key: RuntimeTy::string(),
                value: json.clone(),
                entries: entries
                    .iter()
                    .map(|(name, entry)| Ok((name.as_str(), tree(entry, json)?)))
                    .collect::<Result<_, Mismatch>>()?,
            },
            // A bridge may name the type of each leaf.
            BexExternalValue::Union { value, .. } => tree(value, json)?,
            BexExternalValue::Uint8Array(_)
            | BexExternalValue::Instance { .. }
            | BexExternalValue::Variant { .. }
            | BexExternalValue::RustData(_)
            | BexExternalValue::FunctionRef { .. }
            | BexExternalValue::Handle(_)
            | BexExternalValue::Adt(_)
            | BexExternalValue::HostValue(_) => return Err(Mismatch::kind(value, json)),
        })
    }

    if !value_satisfies_json(value) {
        return Err(Mismatch::kind(value, json));
    }
    Ok(Admission::of(tree(value, json)?).leaf(Leaf::CatchAll))
}

/// A value whose type the host named (`value_type`), or one the engine wrapped
/// with the member of a union it came out of.
fn admit_typed_by_host<'a>(
    cx: &Cx<'_>,
    carrier: &'a BexExternalValue,
    payload: &'a BexExternalValue,
    metadata: &UnionMetadata,
    ty: &RuntimeTy,
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    let named = &metadata.selected_option;
    // A wire value names one exact type. The decoder rejects a union there,
    // but only the program knows that an alias stands for one.
    if metadata.is_inbound_type_annotation
        && inbound_annotation_resolves_to_root_union(named, cx.aliases)
    {
        return Err(Mismatch::new(
            "inbound value_type must identify one exact selected type, not a root union or optional",
        ));
    }

    let slot = match ty {
        RuntimeTy::Union(members) => {
            let found = members
                .iter()
                .find(|member| selected_arm_equal(member, named))
                .or_else(|| {
                    members.iter().find(|member| {
                        runtime_ty_assignable_with_aliases(named, member, cx.aliases)
                    })
                })
                // The exact media kind before the generic `media` supertype,
                // whatever the order of the members.
                .or_else(|| {
                    members.iter().find(|member| {
                        stdlib_media_wrapper_matches(named, member, cx.aliases, false)
                    })
                })
                .or_else(|| {
                    members.iter().find(|member| {
                        stdlib_media_wrapper_matches(named, member, cx.aliases, true)
                    })
                });
            match found {
                Some(member) => member,
                None => {
                    let mut refinements = members
                        .iter()
                        .filter(|member| class_annotation_can_refine(named, member));
                    match (refinements.next(), refinements.next()) {
                        (Some(member), None) => member,
                        (None, _) => {
                            // The `json` alias is opaque to the subtype test
                            // above, which cannot expand it.
                            if let Some(json) = members.iter().find(|member| {
                                matches!(member, RuntimeTy::TypeAlias(name) if is_canonical_json_alias(name))
                            }) && value_satisfies_json(carrier)
                            {
                                return admit_json(carrier, json);
                            }
                            return Err(Mismatch::new(format!(
                                "host value type `{named}` is not a member of declared union `{ty}`"
                            )));
                        }
                        (Some(_), Some(_)) => {
                            return Err(Mismatch::new(format!(
                                "host class annotation `{named}` does not identify one member of declared union `{ty}`; provide concrete generic arguments"
                            )));
                        }
                    }
                }
            }
        }
        // The engine's wrapper says nothing about a slot that is no union.
        _ if !metadata.is_inbound_type_annotation => return admit_at(cx, payload, ty, depth),
        _ => ty,
    };
    // A member that is itself a union (`T | null` inside `T? | string`): the
    // type the host named is a member of that member.
    if let RuntimeTy::Union(_) = slot {
        return admit_typed_by_host(cx, carrier, payload, metadata, slot, depth);
    }

    // A media value in a class-shaped shell, named as the standard library's
    // wrapper class, in a slot of the primitive media type.
    if let Some(media) = stdlib_media_wrapper_payload_type(named, slot, cx.aliases) {
        return admit_at(cx, payload, &media, depth);
    }
    // The type the host named is the value's exact type wherever the slot
    // takes it; a class named without its type arguments takes the slot's.
    let judged_against = if selected_arm_equal(slot, named) {
        // One type in two spellings: the declaration's is the one a value
        // lands by.
        slot
    } else if runtime_ty_assignable_with_aliases(named, slot, cx.aliases) {
        named
    } else if class_annotation_can_refine(named, slot) {
        slot
    } else {
        return Err(Mismatch::new(format!(
            "host value type `{named}` is not assignable to declared type `{ty}`"
        )));
    };
    let admission = match judged_against {
        RuntimeTy::Class(name, args) => admit_class(
            cx,
            payload,
            judged_against,
            name,
            args,
            ClassOrigin::NamedByHost,
            depth,
        ),
        _ => admit_at(cx, payload, judged_against, depth),
    }
    .map_err(|mismatch| {
        Mismatch::new(format!(
            "host-typed payload `{}` does not inhabit `{named}`: {mismatch}",
            payload.type_name()
        ))
    })?;
    Ok(admission)
}

fn admit_union<'a>(
    cx: &Cx<'_>,
    value: &'a BexExternalValue,
    ty: &RuntimeTy,
    members: &[RuntimeTy],
    depth: usize,
) -> Result<Admission<'a>, Mismatch> {
    let mut taken: Vec<(&RuntimeTy, Admission<'a>)> = Vec::new();
    for member in members {
        // Two spellings of one type are one member.
        if taken
            .iter()
            .any(|(seen, _)| runtime_ty_structurally_equal(seen, member))
        {
            continue;
        }
        if let Ok(admission) = admit_at(cx, value, member, depth) {
            taken.push((member, admission));
        }
    }
    if taken.is_empty() {
        return Err(Mismatch::new(format!(
            "Value of type '{}' does not match any member of union `{ty}`",
            conversion::described_value_type(value),
        )));
    }

    // A type that takes every value does not compete with one that asks
    // something of the value.
    if taken
        .iter()
        .any(|(_, admission)| admission.leaf != Leaf::CatchAll)
    {
        taken.retain(|(_, admission)| admission.leaf != Leaf::CatchAll);
    }
    // `null` names its own type, also where an alias of an optional takes it
    // too.
    if matches!(value, BexExternalValue::Null)
        && taken
            .iter()
            .any(|(member, _)| runtime_ty_resolves_to_exact_null(member, cx.aliases))
    {
        taken.retain(|(member, _)| runtime_ty_resolves_to_exact_null(member, cx.aliases));
    }

    // What is left is the same value in more than one type. Where every
    // member lands the same value (`string | "draft"` for a `"draft"`, an
    // enum and one of its variants), which was meant makes no difference.
    // Otherwise only the host knows, and it has to name the type.
    let (first, rest) = taken
        .split_first()
        .unwrap_or_else(|| unreachable!("a value no member took was refused above"));
    if rest
        .iter()
        .any(|(_, admission)| admission.value != first.1.value)
    {
        return Err(Mismatch::new(format!(
            "value of type `{}` matches multiple union members; add an inbound `value_type` annotation to select one",
            value.type_name()
        )));
    }
    Ok(taken.swap_remove(0).1)
}

/// Whether `ty` has a position that stands for a type the boundary was not
/// told: a type variable, `unknown`, or the `$rust_type` a generic call falls
/// back to. Generics are invariant, so no live value's type equals such a
/// type, and membership in it says nothing.
fn has_erased_position(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Unknown
        | RuntimeTy::TypeVar(_)
        | RuntimeTy::AssociatedTypeProjection { .. }
        | RuntimeTy::RustType => true,
        RuntimeTy::List(element) => has_erased_position(element),
        RuntimeTy::Map { key, value } => has_erased_position(key) || has_erased_position(value),
        RuntimeTy::Union(members) => members.iter().any(has_erased_position),
        RuntimeTy::Class(_, args) => args.iter().any(has_erased_position),
        RuntimeTy::Interface(_, args, associated) => {
            args.iter().any(has_erased_position)
                || associated.iter().any(|(_, ty)| has_erased_position(ty))
        }
        RuntimeTy::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| has_erased_position(&param.ty))
                || has_erased_position(ret)
                || has_erased_position(throws)
        }
        RuntimeTy::Future(value, error) => has_erased_position(value) || has_erased_position(error),
        RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Void
        | RuntimeTy::Never
        | RuntimeTy::Uint8Array
        | RuntimeTy::Media(_)
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(_)
        | RuntimeTy::EnumVariant(..)
        | RuntimeTy::TypeAlias(_)
        | RuntimeTy::Type
        | RuntimeTy::Resource
        | RuntimeTy::PromptAst => false,
    }
}

/// The program as the engine holds it, on the heap of one call.
pub(crate) struct Heap<'h> {
    pub(crate) engine: &'h BexEngine,
    pub(crate) vm: &'h bex_vm::BexVm,
    pub(crate) permit: bex_heap::PermitProof<'h>,
    pub(crate) overlay: InboundRuntimeOverlay<'h>,
}

impl Program for Heap<'_> {
    fn class(&self, name: &str, type_args: &[RuntimeTy]) -> Result<ClassFacts, Mismatch> {
        let Some(class) = self.engine.resolve_inbound_declaration(
            self.vm,
            self.permit,
            name,
            self.overlay,
            InboundDeclarationKind::Class,
        )?
        else {
            return Ok(ClassFacts::Missing);
        };
        // SAFETY: the permit is held, and the resolver returns a live object.
        #[expect(unsafe_code, reason = "reading a class under a held heap permit")]
        let Object::Class(class) = (unsafe { class.get() }) else {
            return Err(Mismatch::new(format!(
                "Resolved name `{name}` does not point to a class"
            )));
        };
        let type_args = type_args
            .iter()
            .map(|ty| {
                self.engine
                    .anchor_wire_ty_with_runtime(self.vm, self.permit, ty, self.overlay)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let fields = class
            .fields
            .iter()
            .map(|field| {
                let ty = field.field_template.substitute_symbolic(&type_args);
                Ok((field.name.clone(), conversion::overlay_wire_ty(&ty)?))
            })
            .collect::<Result<_, EngineError>>()?;
        Ok(ClassFacts::Fields(fields))
    }

    fn variant(&self, enum_name: &str, variant: &str) -> VariantFacts {
        let Ok(Some(declared)) = self.engine.resolve_inbound_declaration(
            self.vm,
            self.permit,
            enum_name,
            self.overlay,
            InboundDeclarationKind::Enum,
        ) else {
            return VariantFacts::NoEnum;
        };
        // SAFETY: the permit is held, and the resolver returns a live object.
        #[expect(unsafe_code, reason = "reading an enum under a held heap permit")]
        let Object::Enum(declared) = (unsafe { declared.get() }) else {
            return VariantFacts::NoEnum;
        };
        match declared.variants.iter().position(|v| v.name == variant) {
            Some(index) => VariantFacts::Declared(index),
            None => VariantFacts::NoVariant,
        }
    }

    fn live_value_fits(
        &self,
        handle: &bex_external_types::Handle,
        ty: &RuntimeTy,
    ) -> Result<bool, Mismatch> {
        let object = self
            .engine
            .resolve_handle(self.permit, handle)
            .ok_or_else(|| {
                Mismatch::new("a handle is stale or belongs to a different BAML runtime")
            })?;
        // A type with an erased position names no set of values to test
        // against (a generic call binds `Box<T>` to `Box<$rust_type>` for a
        // live `Box<string>`), and a native object has no BAML type to test:
        // neither can be refused on what is known.
        if has_erased_position(ty) {
            return Ok(true);
        }
        let Ok(ty) =
            self.engine
                .realize_host_ty_with_runtime(self.vm, self.permit, ty, self.overlay)
        else {
            return Ok(true);
        };
        Ok(bex_vm::value_is_member(self.vm, Value::object(object), ty.as_ty()).unwrap_or(true))
    }

    fn is_subtype(&self, value_ty: &RuntimeTy, ty: &RuntimeTy) -> Result<bool, Mismatch> {
        let anchor = |ty: &RuntimeTy| {
            self.engine
                .anchor_wire_ty_with_runtime(self.vm, self.permit, ty, self.overlay)
        };
        let (value_ty, ty) = (anchor(value_ty)?, anchor(ty)?);
        Ok(baml_type::normalize::is_subtype(
            value_ty.as_ty(),
            ty.as_ty(),
            self.vm,
        ))
    }
}

impl From<EngineError> for Mismatch {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::TypeMismatch { message } => Self::new(message),
            other => Self::new(other.to_string()),
        }
    }
}

impl BexEngine {
    /// Judge a value from a host against the type of the slot it is bound for.
    pub(crate) fn admit<'a>(
        &self,
        holder: &impl HeapPermit<BexThread>,
        value: &'a BexExternalValue,
        ty: &RuntimeTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<Admitted<'a>, Mismatch> {
        let heap = Heap {
            engine: self,
            vm: &holder.holder().vm,
            permit: holder.proof(),
            overlay,
        };
        let cx = Cx {
            program: &heap,
            aliases: &self.inbound_alias_view,
        };
        admit(&cx, value, ty).map(|admission| admission.value)
    }

    /// Admit a value from a host to the type of its slot and put it on the
    /// heap.
    pub(crate) fn admit_to_heap(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<Value, EngineError> {
        let none = indexmap::IndexMap::new();
        let overlay = InboundRuntimeOverlay::new(&none, &none, None);
        let admitted = self.admit(holder, value, ty, overlay)?;
        self.land(holder, admitted, overlay)
    }

    /// Admit what a host callback returned to its declared return type and
    /// put it on the heap. The inner error is the callback's breach of its
    /// contract, which the caller raises as a panic of the BAML program.
    pub(crate) fn admit_host_return(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<Result<Value, Mismatch>, EngineError> {
        let none = indexmap::IndexMap::new();
        let overlay = InboundRuntimeOverlay::new(&none, &none, None);
        match self.admit(holder, value, ty, overlay) {
            Ok(admitted) => self.land(holder, admitted, overlay).map(Ok),
            Err(mismatch) => Ok(Err(Mismatch::new(format!(
                "host callable returned a value that does not match the declared return type \
                 `{ty}`: {mismatch}"
            )))),
        }
    }

    /// Put an admitted value on the heap. Every question of type was settled
    /// when the value was admitted; what can still fail here is a name or a
    /// handle that no longer resolves.
    pub(crate) fn land(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        admitted: Admitted<'_>,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<Value, EngineError> {
        Ok(match admitted {
            Admitted::Null => Value::NULL,
            Admitted::Int(int) => int.value,
            Admitted::Bigint(big) => Value::object(
                holder
                    .holder_mut()
                    .tlab_mut()
                    .alloc_bigint(big.into_owned()),
            ),
            Admitted::Float(f) => {
                Value::object(holder.holder_mut().tlab_mut().alloc(Object::Float(f)))
            }
            Admitted::Bool(b) => Value::bool(b),
            Admitted::String(s) => {
                Value::object(holder.holder_mut().tlab_mut().alloc_string(s.clone()))
            }
            Admitted::Bytes(bytes) => Value::object(
                holder
                    .holder_mut()
                    .tlab_mut()
                    .alloc_uint8array(bytes.to_vec()),
            ),
            Admitted::List { element, items } => {
                let values = items
                    .into_iter()
                    .map(|item| self.land(holder, item, overlay))
                    .collect::<Result<Vec<_>, _>>()?;
                let element = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &element,
                    overlay,
                )?;
                Value::object(holder.holder_mut().tlab_mut().alloc_array(element, values))
            }
            Admitted::Map {
                sent_key,
                key,
                value,
                entries,
            } => {
                let sent_key = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    sent_key,
                    overlay,
                )?;
                let key = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &key,
                    overlay,
                )?;
                for key_ty in [&sent_key, &key] {
                    if !baml_type::normalize::is_string_key(key_ty.as_ty(), &holder.holder().vm) {
                        return Err(EngineError::CannotConvert {
                            type_name:
                                "map with non-string keys (SDK interchange requires string keys)"
                                    .to_owned(),
                        });
                    }
                }
                for (name, _) in &entries {
                    let literal = baml_type::Ty::Literal(
                        baml_type::Literal::String((*name).to_string()),
                        baml_type::Freshness::Regular,
                    );
                    if [&sent_key, &key].into_iter().any(|key_ty| {
                        !baml_type::normalize::is_subtype(
                            &literal,
                            key_ty.as_ty(),
                            &holder.holder().vm,
                        )
                    }) {
                        return Err(EngineError::CannotConvert {
                            type_name: "map key does not match declared key type".to_owned(),
                        });
                    }
                }
                let values = entries
                    .into_iter()
                    .map(|(name, entry)| {
                        self.land(holder, entry, overlay)
                            .map(|entry| (bex_vm_types::BexStr::from(name), entry))
                    })
                    .collect::<Result<indexmap::IndexMap<_, _>, _>>()?;
                let value = self.realize_host_ty_with_runtime(
                    &holder.holder().vm,
                    holder.proof(),
                    &value,
                    overlay,
                )?;
                Value::object(holder.holder_mut().tlab_mut().alloc_map(key, value, values))
            }
            Admitted::Instance {
                class,
                type_args,
                fields,
            } => {
                let values = fields
                    .into_iter()
                    .map(|field| self.land(holder, field, overlay))
                    .collect::<Result<Vec<_>, _>>()?;
                let class_ptr = self
                    .resolve_inbound_declaration(
                        &holder.holder().vm,
                        holder.proof(),
                        &class,
                        overlay,
                        InboundDeclarationKind::Class,
                    )?
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown class `{class}` in external Instance value"),
                    })?;
                let type_args = type_args
                    .iter()
                    .map(|ty| {
                        self.realize_host_ty_with_runtime(
                            &holder.holder().vm,
                            holder.proof(),
                            ty,
                            overlay,
                        )
                    })
                    .collect::<Result<Box<[_]>, _>>()?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_instance_with_type_args(class_ptr, type_args, values),
                )
            }
            Admitted::Variant { enum_name, index } => {
                let enum_ptr = self
                    .resolve_inbound_declaration(
                        &holder.holder().vm,
                        holder.proof(),
                        &enum_name,
                        overlay,
                        InboundDeclarationKind::Enum,
                    )?
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: format!("Unknown enum `{enum_name}` in external Variant value"),
                    })?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_variant(enum_ptr, index),
                )
            }
            Admitted::Engine { value, slot } => {
                self.land_engine_value(holder, value.clone(), &slot, overlay)?
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, sync::Arc};

    use baml_type::{Freshness, FunctionParamMode, Name, RuntimeFunctionParamTy};
    use bex_external_types::HostValueArc;
    use bex_str::BexStr;
    use bex_vm_types::{MediaContent, MediaValue};
    use indexmap::IndexMap;

    use super::*;

    /// The program of a test: the classes and enums it declares, by the name
    /// a slot's type prints.
    #[derive(Default)]
    struct Facts {
        classes: IndexMap<String, Vec<(String, RuntimeTy)>>,
        enums: IndexMap<String, Vec<String>>,
    }

    impl Facts {
        fn class(mut self, name: &str, fields: &[(&str, RuntimeTy)]) -> Self {
            self.classes.insert(
                declared(name),
                fields
                    .iter()
                    .map(|(field, ty)| ((*field).to_string(), ty.clone()))
                    .collect(),
            );
            self
        }

        fn enumeration(mut self, name: &str, variants: &[&str]) -> Self {
            self.enums.insert(
                declared(name),
                variants
                    .iter()
                    .map(|variant| (*variant).to_string())
                    .collect(),
            );
            self
        }
    }

    impl Program for Facts {
        fn class(&self, name: &str, _type_args: &[RuntimeTy]) -> Result<ClassFacts, Mismatch> {
            Ok(match self.classes.get(name) {
                Some(fields) => ClassFacts::Fields(fields.clone()),
                None => ClassFacts::Missing,
            })
        }

        fn variant(&self, enum_name: &str, variant: &str) -> VariantFacts {
            match self.enums.get(enum_name) {
                None => VariantFacts::NoEnum,
                Some(variants) => match variants.iter().position(|declared| declared == variant) {
                    Some(index) => VariantFacts::Declared(index),
                    None => VariantFacts::NoVariant,
                },
            }
        }

        fn live_value_fits(
            &self,
            _handle: &bex_external_types::Handle,
            _ty: &RuntimeTy,
        ) -> Result<bool, Mismatch> {
            Err(Mismatch::new("the program of a test has no heap"))
        }

        fn is_subtype(&self, value_ty: &RuntimeTy, ty: &RuntimeTy) -> Result<bool, Mismatch> {
            Ok(runtime_ty_assignable_with_aliases(
                value_ty,
                ty,
                &IndexMap::new(),
            ))
        }
    }

    /// The name a judgment resolves a declaration by.
    fn declared(name: &str) -> String {
        TypeName::from_dotted_path(name).to_string()
    }

    fn class_ty(name: &str) -> RuntimeTy {
        RuntimeTy::Class(TypeName::from_dotted_path(name), Box::new([]))
    }

    fn enum_ty(name: &str) -> RuntimeTy {
        RuntimeTy::Enum(TypeName::from_dotted_path(name))
    }

    fn list(element: RuntimeTy) -> RuntimeTy {
        RuntimeTy::List(Box::new(element))
    }

    fn map_of(value: RuntimeTy) -> RuntimeTy {
        RuntimeTy::Map {
            key: Box::new(RuntimeTy::string()),
            value: Box::new(value),
        }
    }

    fn string_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(Literal::String(value.to_string()), Freshness::Regular)
    }

    fn int_literal(value: i64) -> RuntimeTy {
        RuntimeTy::Literal(Literal::Int(value), Freshness::Regular)
    }

    fn float_literal(value: &str) -> RuntimeTy {
        RuntimeTy::Literal(Literal::Float(value.to_string()), Freshness::Regular)
    }

    /// The standard library's recursive alias that holds any JSON value.
    fn json_ty() -> RuntimeTy {
        alias_ty("baml.json.json")
    }

    fn alias_ty(name: &str) -> RuntimeTy {
        RuntimeTy::TypeAlias(TypeName::from_dotted_path(name))
    }

    /// The alias table of a program that declares `bodies`.
    fn aliases(bodies: &[(&str, RuntimeTy)]) -> IndexMap<TypeName, RuntimeTy> {
        bodies
            .iter()
            .map(|(name, body)| (TypeName::from_dotted_path(name), body.clone()))
            .collect()
    }

    /// A function type whose one parameter is the literal `"draft"`, spelled
    /// with `freshness`.
    fn callback_ty(freshness: Freshness) -> RuntimeTy {
        RuntimeTy::Function {
            params: Box::new([RuntimeFunctionParamTy {
                name: Some(Name::new("status")),
                ty: RuntimeTy::Literal(Literal::String("draft".to_string()), freshness),
                mode: FunctionParamMode::Required,
            }]),
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never),
        }
    }

    /// The standard library's wrapper classes of two media kinds.
    const IMAGE_CLASS: &str = "baml.media.Image";
    const AUDIO_CLASS: &str = "baml.media.Audio";

    /// A program that declares a media wrapper class as the standard library
    /// does: one opaque field that holds the media value.
    fn with_media_class(facts: Facts, class: &str) -> Facts {
        facts.class(
            class,
            &[(
                bex_external_types::MEDIA_WRAPPER_DATA_FIELD,
                RuntimeTy::RustType,
            )],
        )
    }

    /// A media value. Two of them are never equal, so a test holds on to the
    /// one it sent.
    fn media_value(kind: MediaKind) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(MediaValue::new(
            kind,
            MediaContent::Url {
                url: "https://example.test/asset".to_string(),
                base64_data: None,
            },
            None,
        ))))
    }

    /// The shell a bridge whose media type is a class sends: an instance of
    /// `class` around `media`.
    fn media_shell(class: &str, media: &BexExternalValue) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: class.to_string(),
            type_args: vec![],
            fields: IndexMap::from([(
                bex_external_types::MEDIA_WRAPPER_DATA_FIELD.to_string(),
                media.clone(),
            )]),
        }
    }

    fn int(int: i64) -> Admitted<'static> {
        Admitted::Int(VmInt::new(int).expect("a test integer the VM holds"))
    }

    /// A value of the engine's own making, admitted to `slot`.
    fn engine(value: &BexExternalValue, slot: RuntimeTy) -> Admitted<'_> {
        Admitted::Engine { value, slot }
    }

    /// A map as a `json` slot holds one: its values are `json` themselves.
    fn json_map<'a>(
        sent_key: &'a RuntimeTy,
        entries: Vec<(&'a str, Admitted<'a>)>,
    ) -> Admitted<'a> {
        Admitted::Map {
            sent_key,
            key: RuntimeTy::string(),
            value: json_ty(),
            entries,
        }
    }

    /// The refusal of a value that more than one member took, which tells the
    /// host how to say which one it meant.
    #[track_caller]
    fn assert_host_has_to_name_the_member(result: Result<Admitted<'_>, Mismatch>) {
        let error = result.unwrap_err();
        assert!(
            error.message.contains("matches multiple union members")
                && error.message.contains("value_type"),
            "{error}"
        );
    }

    fn array(items: Vec<BexExternalValue>) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items,
        }
    }

    /// An instance that names no class of its own, as a host whose objects
    /// have none sends one with the class named beside it.
    fn instance(fields: &[(&str, BexExternalValue)]) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: String::new(),
            type_args: vec![],
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    fn object(entries: &[(&str, BexExternalValue)]) -> BexExternalValue {
        BexExternalValue::Map {
            key_type: RuntimeTy::string(),
            value_type: RuntimeTy::unknown(),
            entries: entries
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        }
    }

    /// Judge `value` against `ty` as a host sent it to a program of `facts`.
    fn admit_in<'a>(
        facts: &Facts,
        value: &'a BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<Admitted<'a>, Mismatch> {
        admit_with_aliases(facts, &IndexMap::new(), value, ty)
    }

    fn admit_with_aliases<'a>(
        facts: &Facts,
        aliases: &IndexMap<TypeName, RuntimeTy>,
        value: &'a BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<Admitted<'a>, Mismatch> {
        let cx = Cx {
            program: facts,
            aliases,
        };
        admit(&cx, value, ty).map(|admission| admission.value)
    }

    // ── The engine's own contract ────────────────────────────────────────

    #[test]
    fn exact_takes_a_value_of_the_declared_type_and_no_other() {
        let facts = Facts::default();
        let seven = BexExternalValue::Int(7);
        assert_eq!(admit_in(&facts, &seven, &RuntimeTy::int()), Ok(int(7)));
        for other in [
            RuntimeTy::float(),
            RuntimeTy::bigint(),
            RuntimeTy::string(),
            RuntimeTy::Bool,
            RuntimeTy::Null,
            list(RuntimeTy::int()),
        ] {
            assert!(
                admit_in(&facts, &seven, &other).is_err(),
                "an int is not a `{other}`"
            );
        }
    }

    #[test]
    fn exact_takes_no_value_for_a_function_type() {
        let facts = Facts::default();
        let callback = RuntimeTy::Function {
            params: Box::new([]),
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never),
        };
        for value in [
            BexExternalValue::String("x".into()),
            BexExternalValue::Null,
            BexExternalValue::Int(1),
            object(&[("a", BexExternalValue::Int(1))]),
        ] {
            assert!(admit_in(&facts, &value, &callback).is_err());
        }
    }

    #[test]
    fn exact_does_not_read_a_map_as_a_class_or_a_string_as_a_variant() {
        let facts = Facts::default()
            .class("user.Reading", &[("count", RuntimeTy::int())])
            .enumeration("user.Mood", &["HAPPY", "SAD"]);
        let reading = object(&[("count", BexExternalValue::Int(3))]);
        assert!(admit_in(&facts, &reading, &class_ty("user.Reading")).is_err());
        let happy = BexExternalValue::String("HAPPY".into());
        assert!(admit_in(&facts, &happy, &enum_ty("user.Mood")).is_err());
    }

    #[test]
    fn host_named_class_is_strict_about_its_fields() {
        let facts = Facts::default().class(
            "user.Reading",
            &[("label", RuntimeTy::string()), ("count", RuntimeTy::int())],
        );
        let reading = class_ty("user.Reading");
        let extra = BexExternalValue::typed(
            instance(&[
                ("label", BexExternalValue::String("a".into())),
                ("count", BexExternalValue::Int(3)),
                ("zzz", BexExternalValue::Int(1)),
            ]),
            reading.clone(),
        );
        let error = admit_in(&facts, &extra, &reading).unwrap_err();
        assert!(error.message.contains("has no field `zzz`"), "{error}");
        let missing = BexExternalValue::typed(
            instance(&[("label", BexExternalValue::String("a".into()))]),
            reading.clone(),
        );
        let error = admit_in(&facts, &missing, &reading).unwrap_err();
        assert!(error.message.contains("Missing field `count`"), "{error}");
    }

    #[test]
    fn exact_holds_at_every_depth() {
        let facts = Facts::default().class("user.Scored", &[("score", RuntimeTy::float())]);
        // An integer where a float is declared: in a list, in a class field,
        // and under an optional of a list of the class.
        let seven = BexExternalValue::Int(7);
        let sevens = array(vec![seven.clone()]);
        assert!(admit_in(&facts, &sevens, &list(RuntimeTy::float())).is_err());
        let scored =
            BexExternalValue::typed(instance(&[("score", seven)]), class_ty("user.Scored"));
        assert!(admit_in(&facts, &scored, &class_ty("user.Scored")).is_err());
        let optional = RuntimeTy::union([list(class_ty("user.Scored")), RuntimeTy::Null]);
        assert!(admit_in(&facts, &array(vec![scored]), &optional).is_err());
        // The float itself lands at every one of those depths.
        let half = BexExternalValue::Float(7.5);
        let scored = BexExternalValue::typed(instance(&[("score", half)]), class_ty("user.Scored"));
        assert_eq!(
            admit_in(&facts, &array(vec![scored]), &optional),
            Ok(Admitted::List {
                element: class_ty("user.Scored"),
                items: vec![Admitted::Instance {
                    class: declared("user.Scored"),
                    type_args: vec![],
                    fields: vec![Admitted::Float(7.5)],
                }],
            })
        );
    }

    #[test]
    fn a_host_names_the_member_of_a_value_that_lands_differently_in_each() {
        let facts = Facts::default();
        // An empty list is a value of both members, and a list of a different
        // element type in each.
        let either = RuntimeTy::union([list(RuntimeTy::int()), list(RuntimeTy::string())]);
        let empty = array(vec![]);
        assert_host_has_to_name_the_member(admit_in(&facts, &empty, &either));
        let named = BexExternalValue::typed(array(vec![]), list(RuntimeTy::string()));
        assert_eq!(
            admit_in(&facts, &named, &either),
            Ok(Admitted::List {
                element: RuntimeTy::string(),
                items: vec![],
            })
        );
    }

    #[test]
    fn literal_holds_the_one_value() {
        let facts = Facts::default();
        let draft = BexExternalValue::String("draft".into());
        assert!(admit_in(&facts, &draft, &string_literal("draft")).is_ok());
        assert!(admit_in(&facts, &draft, &string_literal("final")).is_err());
        assert!(admit_in(&facts, &draft, &map_of(RuntimeTy::string())).is_err());
    }

    // ── The `json` alias ─────────────────────────────────────────────────

    #[test]
    fn json_takes_a_json_tree_and_no_other_value() {
        let facts = Facts::default();
        let json = json_ty();
        let string_key = RuntimeTy::string();
        let tree = object(&[("value", BexExternalValue::Int(7))]);
        assert_eq!(
            admit_in(&facts, &tree, &json),
            Ok(json_map(&string_key, vec![("value", int(7))]))
        );
        let infinite = BexExternalValue::Float(f64::INFINITY);
        assert!(admit_in(&facts, &infinite, &json).is_err());
        // A value the engine wrapped as the member of some other union is no
        // JSON leaf, whatever its payload.
        let wrapped = BexExternalValue::union(
            BexExternalValue::String("json-shaped payload".into()),
            [RuntimeTy::bigint(), RuntimeTy::string()],
            RuntimeTy::bigint(),
        );
        assert!(admit_in(&facts, &wrapped, &json).is_err());

        // The engine's wrapper names `json` as the member of a union.
        let image = RuntimeTy::Media(MediaKind::Image);
        let json_or_image = RuntimeTy::union([json.clone(), image.clone()]);
        let selected = BexExternalValue::union(tree, [json.clone(), image], json);
        assert_eq!(
            admit_in(&facts, &selected, &json_or_image),
            Ok(json_map(&string_key, vec![("value", int(7))]))
        );
    }

    #[test]
    fn json_takes_a_tree_whose_leaves_the_host_typed() {
        let facts = Facts::default();
        let string_key = RuntimeTy::string();
        // A bridge may name the type of every scalar leaf.
        let tree = object(&[(
            "type",
            BexExternalValue::typed(BexExternalValue::String("ok".into()), RuntimeTy::string()),
        )]);
        let ok = BexStr::from("ok");
        assert_eq!(
            admit_in(&facts, &tree, &json_ty()),
            Ok(json_map(&string_key, vec![("type", Admitted::String(&ok))]))
        );

        let one = num_bigint::BigInt::from(1);
        let tree = object(&[(
            "huge",
            BexExternalValue::typed(BexExternalValue::Bigint(one.clone()), RuntimeTy::bigint()),
        )]);
        assert_eq!(
            admit_in(&facts, &tree, &json_ty()),
            Ok(json_map(
                &string_key,
                vec![("huge", Admitted::Bigint(Cow::Borrowed(&one)))]
            ))
        );
    }

    #[test]
    fn json_containers_take_json_as_their_element_type() {
        let facts = Facts::default();
        let string_key = RuntimeTy::string();
        // A host of untyped values sends a container with no element type. In
        // a `json` slot it is a container of `json`, as one that BAML parsed
        // is, so a type test reads both alike.
        let tree = object(&[("inner", array(vec![BexExternalValue::Int(1)]))]);
        assert_eq!(
            admit_in(&facts, &tree, &json_ty()),
            Ok(json_map(
                &string_key,
                vec![(
                    "inner",
                    Admitted::List {
                        element: json_ty(),
                        items: vec![int(1)],
                    }
                )]
            ))
        );
    }

    #[test]
    fn json_refuses_a_tree_outside_the_json_algebra() {
        let facts = Facts::default();
        let binary = object(&[("bytes", BexExternalValue::Uint8Array(vec![1, 2]))]);
        assert!(admit_in(&facts, &binary, &json_ty()).is_err());
    }

    // ── Literals and variants ────────────────────────────────────────────

    #[test]
    fn enum_variant_type_holds_the_one_variant() {
        let facts = Facts::default().enumeration("user.callbacks.Mood", &["HAPPY", "SAD"]);
        let mood = TypeName::from_dotted_path("user.callbacks.Mood");
        let happy = RuntimeTy::EnumVariant(mood.clone(), Name::new("HAPPY"));
        let sad = RuntimeTy::EnumVariant(mood.clone(), Name::new("SAD"));
        let value = BexExternalValue::Variant {
            enum_name: mood.to_string(),
            variant_name: "HAPPY".to_string(),
        };
        let admitted = || Admitted::Variant {
            enum_name: declared("user.callbacks.Mood"),
            index: 0,
        };
        assert_eq!(admit_in(&facts, &value, &happy), Ok(admitted()));
        assert!(admit_in(&facts, &value, &sad).is_err());

        // The variant's type and its enum both take the variant, and land
        // the same value, so neither has to be named.
        let enum_or_variant = RuntimeTy::union([RuntimeTy::Enum(mood), sad, happy]);
        assert_eq!(admit_in(&facts, &value, &enum_or_variant), Ok(admitted()));
    }

    #[test]
    fn float_literal_holds_its_value_by_baml_equality() {
        let facts = Facts::default();
        // `-0.0 == 0.0`, so either zero inhabits the literal of the other.
        let negative_zero = float_literal("-0.0");
        for zero in [-0.0, 0.0] {
            let value = BexExternalValue::Float(zero);
            assert_eq!(
                admit_in(&facts, &value, &negative_zero),
                Ok(Admitted::Float(zero))
            );
        }
        let one = BexExternalValue::Float(1.0);
        assert!(admit_in(&facts, &one, &negative_zero).is_err());

        // A literal keeps every digit it was written with.
        let precise = BexExternalValue::Float(1.234_567_890_123_456_7e-300);
        assert_eq!(
            admit_in(&facts, &precise, &float_literal("1.2345678901234567e-300")),
            Ok(Admitted::Float(1.234_567_890_123_456_7e-300))
        );
    }

    // ── A type the host named ────────────────────────────────────────────

    #[test]
    fn host_named_type_selects_one_of_two_members_that_take_the_value() {
        let facts = Facts::default();
        let string_or_draft = RuntimeTy::union([RuntimeTy::string(), string_literal("draft")]);
        let named = BexExternalValue::typed(
            BexExternalValue::String("draft".into()),
            RuntimeTy::string(),
        );
        let draft = BexStr::from("draft");
        assert_eq!(
            admit_in(&facts, &named, &string_or_draft),
            Ok(Admitted::String(&draft))
        );
    }

    #[test]
    fn host_named_union_is_refused() {
        let facts = Facts::default();
        // A host names the one exact type of its value, and a union is not
        // one.
        let int_or_string = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
        let named = BexExternalValue::typed(BexExternalValue::Int(7), int_or_string.clone());
        let error = admit_in(&facts, &named, &int_or_string).unwrap_err();
        assert!(
            error
                .message
                .contains("must identify one exact selected type"),
            "{error}"
        );
    }

    #[test]
    fn host_named_alias_of_a_union_is_refused() {
        let facts = Facts::default();
        let aliases = aliases(&[(
            "user.aliases.Choice",
            RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]),
        )]);
        let named =
            BexExternalValue::typed(BexExternalValue::Int(7), alias_ty("user.aliases.Choice"));
        let error = admit_with_aliases(&facts, &aliases, &named, &RuntimeTy::int()).unwrap_err();
        assert!(
            error
                .message
                .contains("must identify one exact selected type"),
            "{error}"
        );
    }

    #[test]
    fn host_named_type_outside_the_union_is_refused() {
        let facts = Facts::default();
        let int_or_string = RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()]);
        let named = BexExternalValue::typed(BexExternalValue::Bool(true), RuntimeTy::bool());
        let error = admit_in(&facts, &named, &int_or_string).unwrap_err();
        assert!(
            error.message.contains("bool") && error.message.contains("declared union"),
            "{error}"
        );
    }

    #[test]
    fn host_named_type_the_slot_does_not_take_is_refused() {
        let facts = Facts::default();
        let named = BexExternalValue::typed(BexExternalValue::Int(7), RuntimeTy::int());
        let error = admit_in(&facts, &named, &RuntimeTy::string()).unwrap_err();
        assert!(
            error.message.contains("int") && error.message.contains("string"),
            "{error}"
        );
    }

    #[test]
    fn host_named_literal_is_taken_where_its_primitive_is_declared() {
        let facts = Facts::default();
        let named = BexExternalValue::typed(
            BexExternalValue::String("draft".into()),
            string_literal("draft"),
        );
        let draft = BexStr::from("draft");
        assert_eq!(
            admit_in(&facts, &named, &RuntimeTy::string()),
            Ok(Admitted::String(&draft))
        );
        // The same where the primitive is one member of a union.
        let string_or_int = RuntimeTy::union([RuntimeTy::string(), RuntimeTy::int()]);
        assert_eq!(
            admit_in(&facts, &named, &string_or_int),
            Ok(Admitted::String(&draft))
        );
    }

    #[test]
    fn host_named_class_takes_its_type_arguments_from_the_slot() {
        let facts = Facts::default().class("user.generics.Box", &[("value", RuntimeTy::int())]);
        let name = TypeName::from_dotted_path("user.generics.Box");
        // A host whose generics are erased names the class and not its
        // arguments.
        let named = BexExternalValue::typed(
            BexExternalValue::Instance {
                class_name: String::new(),
                type_args: vec![],
                fields: IndexMap::from([("value".to_string(), BexExternalValue::Int(7))]),
            },
            class_ty("user.generics.Box"),
        );
        let box_of_int = RuntimeTy::Class(name, Box::new([RuntimeTy::int()]));
        assert_eq!(
            admit_in(&facts, &named, &box_of_int),
            Ok(Admitted::Instance {
                class: declared("user.generics.Box"),
                type_args: vec![RuntimeTy::int()],
                fields: vec![int(7)],
            })
        );
    }

    #[test]
    fn instance_type_arguments_must_be_bound() {
        let facts = Facts::default().class("user.generics.Box", &[("value", RuntimeTy::int())]);
        let name = TypeName::from_dotted_path("user.generics.Box");
        let unbound = BexExternalValue::Instance {
            class_name: "user.generics.Box".to_string(),
            type_args: vec![RuntimeTy::TypeVar(baml_type::ParamTy::new(
                0,
                Name::new("T"),
            ))],
            fields: IndexMap::from([("value".to_string(), BexExternalValue::Int(7))]),
        };
        let box_of_int = RuntimeTy::Class(name, Box::new([RuntimeTy::int()]));
        let error = admit_in(&facts, &unbound, &box_of_int).unwrap_err();
        assert!(
            error.message.contains("unbound type variable `T`"),
            "{error}"
        );
    }

    #[test]
    fn host_named_class_is_found_inside_a_nested_union_member() {
        let facts = Facts::default().class("user.unions.T", &[("v", RuntimeTy::int())]);
        // `T? | string`: the class sits in a member that is itself a union.
        let declared = RuntimeTy::union([
            RuntimeTy::union([class_ty("user.unions.T"), RuntimeTy::Null]),
            RuntimeTy::string(),
        ]);
        let named = BexExternalValue::typed(
            instance(&[("v", BexExternalValue::Int(1))]),
            class_ty("user.unions.T"),
        );
        assert_eq!(
            admit_in(&facts, &named, &declared),
            Ok(Admitted::Instance {
                class: TypeName::from_dotted_path("user.unions.T").to_string(),
                type_args: vec![],
                fields: vec![int(1)],
            })
        );
    }

    #[test]
    fn host_named_class_cannot_choose_between_two_instantiations() {
        let facts = Facts::default().class("user.generics.Box", &[]);
        let name = TypeName::from_dotted_path("user.generics.Box");
        let either_box = RuntimeTy::union([
            RuntimeTy::Class(name.clone(), Box::new([RuntimeTy::int()])),
            RuntimeTy::Class(name, Box::new([RuntimeTy::string()])),
        ]);
        let named = BexExternalValue::typed(
            BexExternalValue::Instance {
                class_name: String::new(),
                type_args: vec![],
                fields: IndexMap::new(),
            },
            class_ty("user.generics.Box"),
        );
        let error = admit_in(&facts, &named, &either_box).unwrap_err();
        assert!(
            error.message.contains("does not identify one member")
                && error.message.contains("concrete generic arguments"),
            "{error}"
        );
    }

    #[test]
    fn host_named_type_settles_the_one_child_it_is_on() {
        let facts = Facts::default();
        let either = RuntimeTy::union([list(RuntimeTy::int()), list(RuntimeTy::string())]);
        // Only the empty child needs its type named; the list around it does
        // not.
        let value = array(vec![BexExternalValue::typed(
            array(vec![]),
            list(RuntimeTy::int()),
        )]);
        assert_eq!(
            admit_in(&facts, &value, &list(either.clone())),
            Ok(Admitted::List {
                element: either,
                items: vec![Admitted::List {
                    element: RuntimeTy::int(),
                    items: vec![],
                }],
            })
        );
    }

    #[test]
    fn host_named_function_type_selects_the_member_it_spells_differently() {
        let facts = Facts::default();
        // The wire spells a literal fresh where the declaration has it
        // regular: one type in two spellings.
        let declared_member = callback_ty(Freshness::Regular);
        let wire_spelling = callback_ty(Freshness::Fresh);
        assert_ne!(declared_member, wire_spelling);
        assert!(runtime_ty_structurally_equal(
            &declared_member,
            &wire_spelling
        ));

        let callable = BexExternalValue::HostValue(HostValueArc::new(41, HostValueKind::Callable));
        let named = BexExternalValue::typed(callable.clone(), wire_spelling);
        let callback_or_string = RuntimeTy::union([declared_member.clone(), RuntimeTy::string()]);
        assert_eq!(
            admit_in(&facts, &named, &callback_or_string),
            Ok(engine(&callable, declared_member))
        );
    }

    #[test]
    fn opaque_host_value_is_not_a_function() {
        let facts = Facts::default();
        let opaque = BexExternalValue::HostValue(HostValueArc::new(42, HostValueKind::Opaque));
        assert!(admit_in(&facts, &opaque, &callback_ty(Freshness::Regular)).is_err());
    }

    #[test]
    fn engine_wrapper_names_the_member_of_a_media_value() {
        let facts = Facts::default();
        let image_ty = RuntimeTy::Media(MediaKind::Image);
        let audio_ty = RuntimeTy::Media(MediaKind::Audio);
        let image = media_value(MediaKind::Image);
        let selected = BexExternalValue::union(
            image.clone(),
            [image_ty.clone(), audio_ty.clone()],
            image_ty.clone(),
        );
        let image_or_audio = RuntimeTy::union([image_ty.clone(), audio_ty]);
        assert_eq!(
            admit_in(&facts, &selected, &image_or_audio),
            Ok(engine(&image, image_ty))
        );
    }

    #[test]
    fn engine_wrapper_names_the_member_of_a_reflected_type() {
        let facts = Facts::default();
        let reflected = BexExternalValue::Adt(BexExternalAdt::Type(baml_type::RuntimeTy::Int));
        let selected = BexExternalValue::union(
            reflected.clone(),
            [RuntimeTy::Type, RuntimeTy::string()],
            RuntimeTy::Type,
        );
        let type_or_string = RuntimeTy::union([RuntimeTy::Type, RuntimeTy::string()]);
        assert_eq!(
            admit_in(&facts, &selected, &type_or_string),
            Ok(engine(&reflected, RuntimeTy::Type))
        );
    }

    // ── A union and no type named ────────────────────────────────────────

    #[test]
    fn recursive_alias_is_judged_by_the_shape_of_the_value() {
        let facts = Facts::default();
        let rec_list = alias_ty("user.aliases.RecList");
        let aliases = aliases(&[(
            "user.aliases.RecList",
            RuntimeTy::union([RuntimeTy::int(), list(rec_list.clone())]),
        )]);
        let optional = RuntimeTy::union([rec_list.clone(), RuntimeTy::Null]);
        let value = array(vec![BexExternalValue::Int(6)]);
        assert_eq!(
            admit_with_aliases(&facts, &aliases, &value, &optional),
            Ok(Admitted::List {
                element: rec_list,
                items: vec![int(6)],
            })
        );
    }

    #[test]
    fn null_is_the_exact_null_member_beside_an_alias_of_an_optional() {
        let facts = Facts::default().enumeration("user.State", &["ON", "OFF"]);
        let aliases = aliases(&[(
            "user.aliases.OptionalState",
            RuntimeTy::optional(enum_ty("user.State")),
        )]);
        // `OptionalState?`: the alias holds `null` already, and the `?`
        // outside it adds a `null` member of its own. A `null` says which
        // without a type named for it.
        let declared =
            RuntimeTy::union([alias_ty("user.aliases.OptionalState"), RuntimeTy::null()]);
        assert_eq!(
            admit_with_aliases(&facts, &aliases, &BexExternalValue::Null, &declared),
            Ok(Admitted::Null)
        );
    }

    #[test]
    fn nested_container_is_judged_by_what_it_holds() {
        let facts = Facts::default();
        let either = RuntimeTy::union([list(RuntimeTy::int()), list(RuntimeTy::string())]);
        let value = array(vec![array(vec![BexExternalValue::String("hello".into())])]);
        let hello = BexStr::from("hello");
        assert_eq!(
            admit_in(&facts, &value, &list(either.clone())),
            Ok(Admitted::List {
                element: either,
                items: vec![Admitted::List {
                    element: RuntimeTy::string(),
                    items: vec![Admitted::String(&hello)],
                }],
            })
        );
    }

    #[test]
    fn nested_empty_container_needs_its_type_named() {
        let facts = Facts::default();
        let either = RuntimeTy::union([list(RuntimeTy::int()), list(RuntimeTy::string())]);
        let value = array(vec![array(vec![])]);
        assert_host_has_to_name_the_member(admit_in(&facts, &value, &list(either)));
    }

    #[test]
    fn literal_beside_its_primitive_lands_the_one_value() {
        let facts = Facts::default();
        // The literal and its type take the same string, so which was meant
        // makes no difference to what lands.
        let string_or_draft = RuntimeTy::union([RuntimeTy::string(), string_literal("draft")]);
        let value = BexExternalValue::String("draft".into());
        let draft = BexStr::from("draft");
        assert_eq!(
            admit_in(&facts, &value, &string_or_draft),
            Ok(Admitted::String(&draft))
        );
        let int_or_seven = RuntimeTy::union([RuntimeTy::int(), int_literal(7)]);
        assert_eq!(
            admit_in(&facts, &BexExternalValue::Int(7), &int_or_seven),
            Ok(int(7))
        );
        assert!(admit_in(&facts, &BexExternalValue::Int(8), &int_literal(7)).is_err());
    }

    #[test]
    fn a_member_that_asks_something_of_the_value_wins_over_one_that_takes_any() {
        let facts = Facts::default();
        let value = array(vec![BexExternalValue::Int(1)]);
        for takes_any in [json_ty(), RuntimeTy::unknown()] {
            let declared = RuntimeTy::union([takes_any, list(RuntimeTy::int())]);
            assert_eq!(
                admit_in(&facts, &value, &declared),
                Ok(Admitted::List {
                    element: RuntimeTy::int(),
                    items: vec![int(1)],
                }),
                "in `{declared}`"
            );
        }
    }

    #[test]
    fn member_declared_twice_is_one_member() {
        let facts = Facts::default();
        let declared = RuntimeTy::Union(Box::new([
            RuntimeTy::int(),
            RuntimeTy::int(),
            RuntimeTy::string(),
        ]));
        assert_eq!(
            admit_in(&facts, &BexExternalValue::Int(7), &declared),
            Ok(int(7))
        );
    }

    // ── Media ────────────────────────────────────────────────────────────

    #[test]
    fn media_is_taken_by_its_kind_and_by_the_generic_type() {
        let facts = Facts::default();
        let image = media_value(MediaKind::Image);
        for slot in [
            RuntimeTy::Media(MediaKind::Image),
            RuntimeTy::Media(MediaKind::Generic),
        ] {
            assert_eq!(
                admit_in(&facts, &image, &slot),
                Ok(engine(&image, slot.clone()))
            );
        }
        assert!(admit_in(&facts, &image, &RuntimeTy::Media(MediaKind::Audio)).is_err());
    }

    #[test]
    fn media_shell_named_as_its_wrapper_class_is_opened_only_in_a_media_slot() {
        let facts = with_media_class(Facts::default(), IMAGE_CLASS);
        let image = media_value(MediaKind::Image);
        let named =
            BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), class_ty(IMAGE_CLASS));
        assert_eq!(
            admit_in(&facts, &named, &RuntimeTy::Media(MediaKind::Image)),
            Ok(engine(&image, RuntimeTy::Media(MediaKind::Image)))
        );
        // Where the wrapper class is declared, the shell is an instance of
        // it.
        assert_eq!(
            admit_in(&facts, &named, &class_ty(IMAGE_CLASS)),
            Ok(Admitted::Instance {
                class: declared(IMAGE_CLASS),
                type_args: vec![],
                fields: vec![engine(&image, RuntimeTy::RustType)],
            })
        );
    }

    #[test]
    fn media_shell_is_held_to_the_kind_its_wrapper_class_names() {
        let facts = with_media_class(Facts::default(), IMAGE_CLASS);
        let any_media = RuntimeTy::Media(MediaKind::Generic);
        // Audio in the shell of an image, where any media is declared.
        let audio = media_value(MediaKind::Audio);
        let mislabelled =
            BexExternalValue::typed(media_shell(AUDIO_CLASS, &audio), class_ty(IMAGE_CLASS));
        assert!(admit_in(&facts, &mislabelled, &any_media).is_err());

        let image = media_value(MediaKind::Image);
        let labelled =
            BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), class_ty(IMAGE_CLASS));
        assert!(admit_in(&facts, &labelled, &any_media).is_ok());
    }

    #[test]
    fn local_class_cannot_pass_for_a_media_wrapper_class() {
        // A class of the user's own package, in a namespace that spells the
        // standard library's.
        let spoof = RuntimeTy::Class(
            TypeName::new(
                Name::new("user"),
                vec![Name::new("baml"), Name::new("media")],
                Name::new("Image"),
            ),
            Box::new([]),
        );
        let RuntimeTy::Class(name, ..) = &spoof else {
            unreachable!("built as a class above")
        };
        assert_eq!(name.display_name().as_str(), IMAGE_CLASS);
        assert_eq!(name.render_dotted(false), "user.baml.media.Image");
        assert_eq!(stdlib_media_wrapper_kind(&spoof), None);

        let facts = with_media_class(Facts::default(), "user.baml.media.Image");
        let image = media_value(MediaKind::Image);
        let named = BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), spoof);
        let error = admit_in(&facts, &named, &RuntimeTy::Media(MediaKind::Image)).unwrap_err();
        assert!(
            error
                .message
                .contains("is not assignable to declared type `image`"),
            "{error}"
        );
    }

    #[test]
    fn media_shell_selects_a_member_that_is_an_alias_of_its_kind() {
        let facts = with_media_class(Facts::default(), IMAGE_CLASS);
        let aliases = aliases(&[(
            "user.aliases.ImageAlias",
            RuntimeTy::Media(MediaKind::Image),
        )]);
        let alias_or_string =
            RuntimeTy::union([alias_ty("user.aliases.ImageAlias"), RuntimeTy::string()]);
        let image = media_value(MediaKind::Image);
        let named =
            BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), class_ty(IMAGE_CLASS));
        assert_eq!(
            admit_with_aliases(&facts, &aliases, &named, &alias_or_string),
            Ok(engine(&image, RuntimeTy::Media(MediaKind::Image)))
        );
    }

    #[test]
    fn media_wrapper_class_matches_no_alias_that_is_a_cycle() {
        let aliases = aliases(&[
            ("user.aliases.First", alias_ty("user.aliases.Second")),
            ("user.aliases.Second", alias_ty("user.aliases.First")),
        ]);
        assert!(!stdlib_media_wrapper_matches(
            &class_ty(IMAGE_CLASS),
            &alias_ty("user.aliases.First"),
            &aliases,
            true,
        ));
    }

    #[test]
    fn media_shell_selects_the_primitive_member_over_a_user_class_of_its_name() {
        let facts = with_media_class(Facts::default(), IMAGE_CLASS)
            .class("user.media.Image", &[("url", RuntimeTy::string())]);
        let image_or_user_class = RuntimeTy::union([
            RuntimeTy::Media(MediaKind::Image),
            class_ty("user.media.Image"),
        ]);
        let image = media_value(MediaKind::Image);
        let named =
            BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), class_ty(IMAGE_CLASS));
        assert_eq!(
            admit_in(&facts, &named, &image_or_user_class),
            Ok(engine(&image, RuntimeTy::Media(MediaKind::Image)))
        );
    }

    #[test]
    fn media_shell_named_as_its_primitive_type_selects_that_member() {
        let facts = Facts::default();
        let image_or_string =
            RuntimeTy::union([RuntimeTy::Media(MediaKind::Image), RuntimeTy::string()]);
        // The shell has the shape of a class and never that of media, so the
        // type the host named is what says it is the `image` member.
        let image = media_value(MediaKind::Image);
        let named = BexExternalValue::typed(
            media_shell(IMAGE_CLASS, &image),
            RuntimeTy::Media(MediaKind::Image),
        );
        assert_eq!(
            admit_in(&facts, &named, &image_or_string),
            Ok(engine(&image, RuntimeTy::Media(MediaKind::Image)))
        );
        let named =
            BexExternalValue::typed(BexExternalValue::String("hi".into()), RuntimeTy::string());
        let hi = BexStr::from("hi");
        assert_eq!(
            admit_in(&facts, &named, &image_or_string),
            Ok(Admitted::String(&hi))
        );
    }
}
