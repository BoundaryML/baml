//! Admission: the one judgment of a value that a host hands the engine.
//!
//! A host value arrives as a [`BexExternalValue`] and is bound for a slot with
//! a declared type: a function argument, the return value of a host callback
//! or a sys-op, a class field or a list item inside either. [`admit`] decides
//! whether the value inhabits that type. Its result is an [`Admitted`] tree in
//! which every decision is made, so landing it on the heap
//! ([`BexEngine::land`]) decides nothing.
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
//!
//! # Heads
//!
//! The judgment is made over the heads of the [`Program`] that answers it:
//! the engine's are live declaration pointers ([`TypeHead`]), so a class is
//! read where it lives and nothing is resolved by name on the way; a test's
//! are names. A host spells types by name (a `value_type`, an instance's
//! class, a variant's enum), and each spelling is resolved once, where it is
//! met.

use baml_type::{HeadDisplay, Literal, MediaKind, Name};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, Handle, HostValueKind, UnionMetadata, value_satisfies_json,
};
use bex_heap::{HeapPermit, TlabHolder};
use bex_vm_types::{HeapPtr, Object, TypeHead, Value};

use crate::{
    BexEngine, EngineError,
    conversion::{
        self, InboundDeclarationKind, InboundRuntimeOverlay, first_unbound_type_var,
        float_literal_matches, is_structural_host_only, type_name_matches_external_name,
    },
    thread::BexThread,
};

/// A declared type over the heads `N` of the program that judges against it.
pub(crate) type Ty<N> = baml_type::RuntimeTy<N>;

/// A type as a host spells it: by name.
pub(crate) type WireTy = baml_type::RuntimeTy;

/// What a program's declaration heads must afford the judgment: identity, a
/// rendering for messages, and the clone a type tree takes.
pub(crate) trait Head: Clone + PartialEq + std::fmt::Debug + HeadDisplay {}

impl<N: Clone + PartialEq + std::fmt::Debug + HeadDisplay> Head for N {}

/// How deep a judgment may nest before it is given up. A value is a finite
/// tree, so only a type can exceed this: an alias cycle that consumes no value
/// on the way round.
const MAX_DEPTH: usize = 512;

/// Why a host value does not inhabit the type it was bound for. A union
/// judges its value against every member and drops the refusals, so a
/// refusal is rendered only where it is reported.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mismatch<'a, N: Head> {
    /// `value` is of another kind than `ty` holds.
    Kind {
        value: &'a BexExternalValue,
        ty: Ty<N>,
    },
    Message(String),
}

impl<'a, N: Head> Mismatch<'a, N> {
    fn new(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }

    /// `value` is of another kind than `ty` holds.
    fn kind(value: &'a BexExternalValue, ty: &Ty<N>) -> Self {
        Self::Kind {
            value,
            ty: ty.clone(),
        }
    }
}

impl<N: Head> std::fmt::Display for Mismatch<'_, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kind { value, ty } => write!(
                f,
                "Value of type '{}' does not match the declared type `{ty}`",
                conversion::described_value_type(value),
            ),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl<N: Head> From<Mismatch<'_, N>> for EngineError {
    fn from(mismatch: Mismatch<'_, N>) -> Self {
        EngineError::TypeMismatch {
            message: mismatch.to_string(),
        }
    }
}

impl<N: Head> From<EngineError> for Mismatch<'_, N> {
    fn from(error: EngineError) -> Self {
        match error {
            EngineError::TypeMismatch { message } => Self::new(message),
            other => Self::new(other.to_string()),
        }
    }
}

/// The fields of a class, as a value is judged against them.
pub(crate) enum ClassFacts<'p, N: Head> {
    /// The head is no class the program declares.
    Missing,
    /// The fields in declaration order, each with its type for the type
    /// arguments that were asked.
    Fields(Vec<(&'p str, Ty<N>)>),
}

/// Whether an enum declares a variant.
pub(crate) enum VariantFacts {
    /// The head is no enum the program declares.
    NoEnum,
    NoVariant,
    /// The variant, at this index among the enum's variants.
    Declared(usize),
}

/// What the judgment reads of the program and of its live values. The engine
/// answers from the heap ([`Heap`]); a test answers from a table.
pub(crate) trait Program<N: Head> {
    /// The declaration the program knows by the spelling a host used.
    fn resolve(&self, name: &str, kind: InboundDeclarationKind) -> Option<N>;

    /// Whether `head` is the declaration a host spells `name`.
    fn head_is_named(&self, head: &N, name: &str) -> bool;

    /// A type the host spelled, over the program's heads.
    fn anchor(&self, ty: &WireTy) -> Result<Ty<N>, Mismatch<'static, N>>;

    /// The fields of the class `head`, for `type_args`.
    fn class<'p>(
        &'p self,
        head: &N,
        type_args: &[Ty<N>],
    ) -> Result<ClassFacts<'p, N>, Mismatch<'static, N>>;

    fn variant(&self, head: &N, variant: &str) -> VariantFacts;

    /// What the alias `head` stands for; `None` for a head that is no alias.
    fn alias(&self, head: &N) -> Option<Ty<N>>;

    /// Whether `head` is the standard library's recursive `json` alias.
    fn is_json(&self, head: &N) -> bool;

    /// The media kind whose standard-library wrapper class `head` is.
    fn media_wrapper(&self, head: &N) -> Option<MediaKind>;

    /// Whether `head` is the standard library's prompt class.
    fn is_prompt_class(&self, head: &N) -> bool;

    /// Whether the object behind `handle` is a member of `ty`. A live value
    /// is judged as `is` judges it, since its type is on the heap and not in
    /// what the host sent.
    fn live_value_fits(&self, handle: &Handle, ty: &Ty<N>) -> Result<bool, Mismatch<'static, N>>;

    /// Whether every value of `value_ty` is a member of `ty`.
    fn is_subtype(&self, value_ty: &Ty<N>, ty: &Ty<N>) -> Result<bool, Mismatch<'static, N>>;
}

/// What one judgment is made against.
pub(crate) struct Cx<'c, N: Head> {
    pub(crate) program: &'c dyn Program<N>,
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
pub(crate) enum Admitted<'a, N: Head> {
    Null,
    Int(VmInt),
    Bigint(std::borrow::Cow<'a, num_bigint::BigInt>),
    Float(f64),
    Bool(bool),
    String(&'a bex_str::BexStr),
    Bytes(&'a [u8]),
    List {
        element: Ty<N>,
        items: Vec<Admitted<'a, N>>,
    },
    Map {
        /// The key type the host sent, and the one that is declared: a key is
        /// checked against both where the map lands.
        sent_key: &'a WireTy,
        key: Ty<N>,
        value: Ty<N>,
        entries: Vec<(&'a str, Admitted<'a, N>)>,
    },
    Instance {
        class: N,
        type_args: Vec<Ty<N>>,
        /// One value for each field, in declaration order.
        fields: Vec<Admitted<'a, N>>,
    },
    Variant {
        enum_head: N,
        index: usize,
    },
    /// A value of the engine's own making that the host hands back: a live
    /// reference, a host callable, a media value, a reflected type. It lands
    /// through the constructor of its kind, for the slot it was admitted to.
    Engine {
        value: &'a BexExternalValue,
        slot: Ty<N>,
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
    fn new<N: Head>(int: i64) -> Result<Self, Mismatch<'static, N>> {
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
pub(crate) struct Admission<'a, N: Head> {
    pub(crate) value: Admitted<'a, N>,
    leaf: Leaf,
}

impl<'a, N: Head> Admission<'a, N> {
    fn of(value: Admitted<'a, N>) -> Self {
        Self {
            value,
            leaf: Leaf::Other,
        }
    }

    fn engine(value: &'a BexExternalValue, slot: &Ty<N>) -> Self {
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
pub(crate) fn admit<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    admit_at(cx, value, ty, 0)
}

fn admit_at<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
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
    if peels_to_rust_type(ty) && is_structural_host_only(value) {
        return Ok(Admission::engine(value, ty));
    }

    match ty {
        Ty::TypeAlias(head) if cx.program.is_json(head) => admit_json(value, ty),
        // An alias is transparent: the value is judged against what it
        // stands for.
        Ty::TypeAlias(head) => {
            let body = cx.program.alias(head).ok_or_else(|| {
                Mismatch::new(format!(
                    "the declared type `{ty}` is an alias this program cannot expand"
                ))
            })?;
            admit_at(cx, value, &body, depth)
        }
        _ if let BexExternalValue::Union {
            value: payload,
            metadata,
        } = value =>
        {
            admit_typed_by_host(cx, value, payload, metadata, ty, depth)
        }
        Ty::Union(members) => admit_union(cx, value, ty, members, depth),
        Ty::Unknown | Ty::TypeVar(_) => {
            Ok(admit_as_itself(cx, value, ty, depth)?.leaf(Leaf::CatchAll))
        }
        Ty::Null => match value {
            BexExternalValue::Null => Ok(Admission::of(Admitted::Null)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Never => Err(Mismatch::kind(value, ty)),
        Ty::Bool => match value {
            BexExternalValue::Bool(b) => Ok(Admission::of(Admitted::Bool(*b))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::String => match value {
            BexExternalValue::String(s) => Ok(Admission::of(Admitted::String(s))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Uint8Array => match value {
            BexExternalValue::Uint8Array(bytes) => Ok(Admission::of(Admitted::Bytes(bytes))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Int => match value {
            BexExternalValue::Int(i) => VmInt::new(*i).map(Admitted::Int).map(Admission::of),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Float => match value {
            BexExternalValue::Float(f) => Ok(Admission::of(Admitted::Float(*f))),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Bigint => match value {
            BexExternalValue::Bigint(big) => {
                bigint(std::borrow::Cow::Borrowed(big)).map(Admission::of)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Literal(literal, _) => admit_literal(cx, value, ty, literal, depth),
        Ty::List(element) => match value {
            BexExternalValue::Array { items, .. } => {
                let items = items
                    .iter()
                    .map(|item| Ok(admit_at(cx, item, element, depth)?.value))
                    .collect::<Result<_, Mismatch<'a, N>>>()?;
                Ok(Admission::of(Admitted::List {
                    element: element.as_ref().clone(),
                    items,
                }))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Map {
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
                    .collect::<Result<_, Mismatch<'a, N>>>()?;
                Ok(Admission::of(Admitted::Map {
                    sent_key: key_type,
                    key: key.as_ref().clone(),
                    value: value_ty.as_ref().clone(),
                    entries,
                }))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Class(head, args) => admit_class(cx, value, ty, head, args, ClassOrigin::Slot, depth),
        Ty::Enum(head) => match value {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } if cx.program.head_is_named(head, enum_name) => admit_variant(cx, head, variant_name),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::EnumVariant(head, declared) => match value {
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } if variant_name == declared.as_str() && cx.program.head_is_named(head, enum_name) => {
                admit_variant(cx, head, variant_name)
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::Media(kind) => match value {
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
        Ty::Function { .. } => match value {
            BexExternalValue::HostValue(host) if host.kind == HostValueKind::Callable => {
                Ok(Admission::engine(value, ty))
            }
            BexExternalValue::FunctionRef { .. } => Ok(Admission::engine(value, ty)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::RustType => match value {
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
        Ty::Type => match value {
            BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
                Ok(Admission::engine(value, ty))
            }
            _ => Err(Mismatch::kind(value, ty)),
        },
        Ty::PromptAst => match value {
            BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => Ok(Admission::engine(value, ty)),
            _ => Err(Mismatch::kind(value, ty)),
        },
        // No host value says which implementor it is except by being one: the
        // value is admitted as what it is, and that type is asked of the slot.
        Ty::Interface(..) | Ty::AssociatedTypeProjection { .. } => {
            let own = own_type(cx, value).ok_or_else(|| Mismatch::kind(value, ty))?;
            if !cx.program.is_subtype(&own, ty)? {
                return Err(Mismatch::kind(value, ty));
            }
            admit_at(cx, value, &own, depth)
        }
        // Only a live value inhabits these, and that was judged above.
        Ty::Future(..) | Ty::Resource => Err(Mismatch::kind(value, ty)),
    }
}

fn bigint<N: Head>(
    big: std::borrow::Cow<'_, num_bigint::BigInt>,
) -> Result<Admitted<'_, N>, Mismatch<'_, N>> {
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
fn admit_literal<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
    literal: &Literal,
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    let base = match literal {
        Literal::Int(_) => Ty::Int,
        Literal::Bigint(_) => Ty::Bigint,
        Literal::Float(_) => Ty::Float,
        Literal::String(_) => Ty::String,
        Literal::Bool(_) => Ty::Bool,
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

fn admit_variant<'a, N: Head>(
    cx: &Cx<'_, N>,
    head: &N,
    variant: &str,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    match cx.program.variant(head, variant) {
        VariantFacts::Declared(index) => Ok(Admission::of(Admitted::Variant {
            enum_head: head.clone(),
            index,
        })),
        VariantFacts::NoVariant => Err(Mismatch::new(format!(
            "Unknown variant `{variant}` in enum `{}`",
            head.head_display_name()
        ))),
        VariantFacts::NoEnum => Err(Mismatch::new(format!(
            "Unknown enum `{}` in external Variant value",
            head.head_display_name()
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

/// The class `head`, of an instance that names it, or of a value with no
/// class of its own.
fn admit_class<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
    head: &N,
    args: &[Ty<N>],
    origin: ClassOrigin,
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
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
                cx.program.head_is_named(head, class_name)
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
            let sent: Vec<Ty<N>> = type_args
                .iter()
                .map(|arg| cx.program.anchor(arg))
                .collect::<Result<_, _>>()?;
            if !sent.is_empty() && !type_args_compatible(&sent, args) {
                return Err(Mismatch::new(format!(
                    "host class type arguments `{}` are incompatible with declared class `{ty}`",
                    sent.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                )));
            }
            (fields, refine_type_args(&sent, args))
        }
        // The engine's own media and prompt values are instances of the
        // standard library's wrapper classes.
        BexExternalValue::Adt(BexExternalAdt::Media(media))
            if args.is_empty() && cx.program.media_wrapper(head) == Some(media.kind) =>
        {
            return Ok(Admission::engine(value, ty));
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) if cx.program.is_prompt_class(head) => {
            return Ok(Admission::engine(value, ty));
        }
        _ => return Err(Mismatch::kind(value, ty)),
    };

    let declared = match cx.program.class(head, &type_args)? {
        ClassFacts::Fields(declared) => declared,
        ClassFacts::Missing => {
            return Err(Mismatch::new(format!(
                "Unknown class `{}` in external Instance value",
                head.head_display_name()
            )));
        }
    };
    if let Some(unknown) = fields
        .keys()
        .find(|key| !declared.iter().any(|(field, _)| *field == key.as_str()))
    {
        return Err(Mismatch::new(format!(
            "class `{}` has no field `{}`",
            head.head_display_name(),
            bounded(unknown)
        )));
    }
    let fields = declared
        .iter()
        .map(|(field, field_ty)| {
            let sent = fields.get(*field).ok_or_else(|| {
                Mismatch::new(format!(
                    "Missing field `{field}` in external Instance for class `{}`",
                    head.head_display_name()
                ))
            })?;
            Ok(admit_at(cx, sent, field_ty, depth)?.value)
        })
        .collect::<Result<_, Mismatch<'a, N>>>()?;
    Ok(Admission::of(Admitted::Instance {
        class: head.clone(),
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
/// it carries. `None` for a value that has no type until a slot gives it one,
/// or that names a declaration the program does not have.
fn own_type<N: Head>(cx: &Cx<'_, N>, value: &BexExternalValue) -> Option<Ty<N>> {
    match value {
        BexExternalValue::Null => Some(Ty::Null),
        BexExternalValue::Int(_) => Some(Ty::Int),
        BexExternalValue::Bigint(_) => Some(Ty::Bigint),
        BexExternalValue::Float(_) => Some(Ty::Float),
        BexExternalValue::Bool(_) => Some(Ty::Bool),
        BexExternalValue::String(_) => Some(Ty::String),
        BexExternalValue::Uint8Array(_) => Some(Ty::Uint8Array),
        BexExternalValue::Array { element_type, .. } => {
            Some(Ty::List(Box::new(cx.program.anchor(element_type).ok()?)))
        }
        BexExternalValue::Map {
            key_type,
            value_type,
            ..
        } => Some(Ty::Map {
            key: Box::new(cx.program.anchor(key_type).ok()?),
            value: Box::new(cx.program.anchor(value_type).ok()?),
        }),
        BexExternalValue::Instance {
            class_name,
            type_args,
            ..
        } if !class_name.is_empty() => {
            let head = cx
                .program
                .resolve(class_name, InboundDeclarationKind::Class)?;
            let type_args = type_args
                .iter()
                .map(|arg| cx.program.anchor(arg).ok())
                .collect::<Option<Box<[_]>>>()?;
            Some(Ty::Class(head, type_args))
        }
        BexExternalValue::Variant { enum_name, .. } => Some(Ty::Enum(
            cx.program
                .resolve(enum_name, InboundDeclarationKind::Enum)?,
        )),
        BexExternalValue::Adt(BexExternalAdt::Media(media)) => Some(Ty::Media(media.kind)),
        BexExternalValue::Adt(BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_)) => {
            Some(Ty::Type)
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => Some(Ty::PromptAst),
        BexExternalValue::RustData(_) => Some(Ty::RustType),
        BexExternalValue::Union { metadata, .. } => {
            cx.program.anchor(&metadata.selected_option).ok()
        }
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
fn admit_as_itself<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    match value {
        // A function reference lands as the function it indexes.
        BexExternalValue::FunctionRef { .. } => Ok(Admission::engine(value, ty)),
        _ => {
            let own = own_type(cx, value).ok_or_else(|| {
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
fn admit_json<'a, N: Head>(
    value: &'a BexExternalValue,
    json: &Ty<N>,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    fn tree<'a, N: Head>(
        value: &'a BexExternalValue,
        json: &Ty<N>,
    ) -> Result<Admitted<'a, N>, Mismatch<'a, N>> {
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
                key: Ty::String,
                value: json.clone(),
                entries: entries
                    .iter()
                    .map(|(name, entry)| Ok((name.as_str(), tree(entry, json)?)))
                    .collect::<Result<_, Mismatch<'a, N>>>()?,
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
fn admit_typed_by_host<'a, N: Head>(
    cx: &Cx<'_, N>,
    carrier: &'a BexExternalValue,
    payload: &'a BexExternalValue,
    metadata: &UnionMetadata,
    ty: &Ty<N>,
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    let named = cx.program.anchor(&metadata.selected_option)?;
    // A wire value names one exact type. The decoder rejects a union there,
    // but only the program knows that an alias stands for one.
    if metadata.is_inbound_type_annotation
        && matches!(expand_alias(cx, &named).as_ref(), Some(Ty::Union(_)))
    {
        return Err(Mismatch::new(
            "inbound value_type must identify one exact selected type, not a root union or optional",
        ));
    }

    let slot = match ty {
        Ty::Union(members) => {
            let mut found = None;
            for member in members {
                if selected_arm_equal(member, &named) {
                    found = Some(member);
                    break;
                }
            }
            if found.is_none() {
                for member in members {
                    if cx.program.is_subtype(&named, member)? {
                        found = Some(member);
                        break;
                    }
                }
            }
            // The exact media kind before the generic `media` supertype,
            // whatever the order of the members.
            if found.is_none() {
                found = members
                    .iter()
                    .find(|member| media_wrapper_matches(cx, &named, member, false))
                    .or_else(|| {
                        members
                            .iter()
                            .find(|member| media_wrapper_matches(cx, &named, member, true))
                    });
            }
            match found {
                Some(member) => member,
                None => {
                    let mut refinements = members
                        .iter()
                        .filter(|member| class_annotation_can_refine(&named, member));
                    match (refinements.next(), refinements.next()) {
                        (Some(member), None) => member,
                        (None, _) => {
                            // The `json` alias is opaque to the subtype test
                            // above, which cannot expand it.
                            if let Some(json) = members.iter().find(
                                |member| matches!(member, Ty::TypeAlias(head) if cx.program.is_json(head)),
                            ) && value_satisfies_json(carrier)
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
    if let Ty::Union(_) = slot {
        return admit_typed_by_host(cx, carrier, payload, metadata, slot, depth);
    }

    // A media value in a class-shaped shell, named as the standard library's
    // wrapper class, in a slot of the primitive media type.
    if let Some(media) = media_wrapper_payload_type(cx, &named, slot) {
        return admit_at(cx, payload, &media, depth);
    }
    // The type the host named is the value's exact type wherever the slot
    // takes it; a class named without its type arguments takes the slot's.
    let judged_against = if selected_arm_equal(slot, &named) {
        // One type in two spellings: the declaration's is the one a value
        // lands by.
        slot
    } else if cx.program.is_subtype(&named, slot)? {
        &named
    } else if class_annotation_can_refine(&named, slot) {
        slot
    } else {
        return Err(Mismatch::new(format!(
            "host value type `{named}` is not assignable to declared type `{ty}`"
        )));
    };
    let admission = match judged_against {
        Ty::Class(head, args) => admit_class(
            cx,
            payload,
            judged_against,
            head,
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

fn admit_union<'a, N: Head>(
    cx: &Cx<'_, N>,
    value: &'a BexExternalValue,
    ty: &Ty<N>,
    members: &[Ty<N>],
    depth: usize,
) -> Result<Admission<'a, N>, Mismatch<'a, N>> {
    // A union rarely has more than a few members that take one value.
    let mut taken: smallvec::SmallVec<[(&Ty<N>, Admission<'a, N>); 4]> = smallvec::SmallVec::new();
    for member in members {
        // Two spellings of one type are one member.
        if taken
            .iter()
            .any(|(seen, _)| structurally_equal(seen, member))
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
            .any(|(member, _)| resolves_to_exact_null(cx, member))
    {
        taken.retain(|(member, _)| resolves_to_exact_null(cx, member));
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

// ── Type questions the judgment asks of a declared type ──────────────────

/// What `ty` stands for once every alias around it is expanded; `None` for
/// an alias the program cannot expand, or a cycle of aliases.
fn expand_alias<N: Head>(cx: &Cx<'_, N>, ty: &Ty<N>) -> Option<Ty<N>> {
    let mut current = ty.clone();
    for _ in 0..MAX_DEPTH {
        let Ty::TypeAlias(head) = &current else {
            return Some(current);
        };
        current = cx.program.alias(head)?;
    }
    None
}

/// Whether `ty` is `null` itself, also under aliases of it.
fn resolves_to_exact_null<N: Head>(cx: &Cx<'_, N>, ty: &Ty<N>) -> bool {
    match ty {
        Ty::Null => true,
        Ty::TypeAlias(head) if !cx.program.is_json(head) => {
            matches!(expand_alias(cx, ty), Some(Ty::Null))
        }
        _ => false,
    }
}

/// Whether a value named as the standard library's media wrapper class
/// `annotation` is a value of the media type `contextual` (an alias of one
/// too), of its kind or, with `allow_generic`, of the generic `media`.
fn media_wrapper_matches<N: Head>(
    cx: &Cx<'_, N>,
    annotation: &Ty<N>,
    contextual: &Ty<N>,
    allow_generic: bool,
) -> bool {
    let Some(kind) = media_wrapper_kind(cx, annotation) else {
        return false;
    };
    let Some(Ty::Media(contextual_kind)) = expand_alias(cx, contextual) else {
        return false;
    };
    kind == contextual_kind || (allow_generic && contextual_kind == MediaKind::Generic)
}

/// The media type a value named as the wrapper class `annotation` inhabits in
/// the media slot `contextual`.
fn media_wrapper_payload_type<N: Head>(
    cx: &Cx<'_, N>,
    annotation: &Ty<N>,
    contextual: &Ty<N>,
) -> Option<Ty<N>> {
    let kind = media_wrapper_kind(cx, annotation)?;
    let Ty::Media(contextual_kind) = expand_alias(cx, contextual)? else {
        return None;
    };
    (contextual_kind == kind || contextual_kind == MediaKind::Generic).then_some(Ty::Media(kind))
}

/// The media kind whose wrapper class `ty` names, with no type arguments.
fn media_wrapper_kind<N: Head>(cx: &Cx<'_, N>, ty: &Ty<N>) -> Option<MediaKind> {
    match ty {
        Ty::Class(head, args) if args.is_empty() => cx.program.media_wrapper(head),
        _ => None,
    }
}

/// Whether `ty` is `$rust_type`, or a union with exactly one such member.
fn peels_to_rust_type<N: Head>(ty: &Ty<N>) -> bool {
    match ty {
        Ty::RustType => true,
        Ty::Union(members) => members.iter().filter(|m| peels_to_rust_type(m)).count() == 1,
        _ => false,
    }
}

/// A type-argument position that fixes nothing: a type variable or `unknown`.
fn is_wildcard<N: Head>(ty: &Ty<N>) -> bool {
    matches!(ty, Ty::TypeVar(..) | Ty::Unknown)
}

/// Whether the type arguments a host sent for an instance can be those the
/// slot declares. Empty arguments say nothing; a declared wildcard takes any
/// argument.
fn type_args_compatible<N: Head>(sent: &[Ty<N>], declared: &[Ty<N>]) -> bool {
    if declared.is_empty() {
        return true;
    }
    if sent.is_empty() {
        return declared.iter().all(is_wildcard);
    }
    sent.len() == declared.len()
        && sent
            .iter()
            .zip(declared)
            .all(|(sent, declared)| compatible(sent, declared))
}

/// Structural compatibility of a type the host sent against a declared one:
/// only a positive leaf or shape mismatch says no.
fn compatible<N: Head>(sent: &Ty<N>, declared: &Ty<N>) -> bool {
    match (sent, declared) {
        (_, Ty::TypeVar(..) | Ty::Unknown) | (Ty::TypeVar(..) | Ty::Unknown, _) => true,
        (Ty::Int, Ty::Int)
        | (Ty::String, Ty::String)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Float, Ty::Float)
        | (Ty::Null, Ty::Null)
        | (Ty::Bigint, Ty::Bigint)
        | (Ty::Uint8Array, Ty::Uint8Array) => true,
        (Ty::Media(sent), Ty::Media(declared)) => {
            *declared == MediaKind::Generic || sent == declared
        }
        (
            Ty::Int | Ty::String | Ty::Bool | Ty::Float | Ty::Null | Ty::Bigint | Ty::Uint8Array,
            Ty::Int | Ty::String | Ty::Bool | Ty::Float | Ty::Null | Ty::Bigint | Ty::Uint8Array,
        ) => false,
        (Ty::List(sent), Ty::List(declared)) => compatible(sent, declared),
        (
            Ty::Map {
                key: sent_key,
                value: sent_value,
            },
            Ty::Map {
                key: declared_key,
                value: declared_value,
            },
        ) => compatible(sent_key, declared_key) && compatible(sent_value, declared_value),
        (Ty::Class(sent, sent_args), Ty::Class(declared, declared_args)) => {
            sent == declared && type_args_compatible(sent_args, declared_args)
        }
        _ => true,
    }
}

/// The type arguments an instance lands with: the host's where it sent them,
/// the slot's where it sent none or a wildcard.
fn refine_type_args<N: Head>(sent: &[Ty<N>], declared: &[Ty<N>]) -> Vec<Ty<N>> {
    if sent.is_empty() {
        return declared.to_vec();
    }
    sent.iter()
        .zip(declared)
        .map(|(sent, declared)| {
            if is_wildcard(sent) {
                declared.clone()
            } else {
                sent.clone()
            }
        })
        .collect()
}

/// Whether a class named by the host without every argument can stand for
/// the declared class `contextual`.
fn class_annotation_can_refine<N: Head>(annotation: &Ty<N>, contextual: &Ty<N>) -> bool {
    let (Ty::Class(annotation_head, annotation_args), Ty::Class(contextual_head, contextual_args)) =
        (annotation, contextual)
    else {
        return false;
    };
    annotation_head == contextual_head
        && (annotation_args.is_empty()
            || (annotation_args.len() == contextual_args.len()
                && annotation_args
                    .iter()
                    .zip(contextual_args)
                    .all(|(annotation, contextual)| compatible(annotation, contextual))))
}

/// Whether two types are one type in two spellings: structurally equal, with
/// the freshness of a literal and the order of union members immaterial.
fn structurally_equal<N: Head>(left: &Ty<N>, right: &Ty<N>) -> bool {
    match (left, right) {
        (Ty::String, Ty::String)
        | (Ty::Int, Ty::Int)
        | (Ty::Bigint, Ty::Bigint)
        | (Ty::Float, Ty::Float)
        | (Ty::Bool, Ty::Bool)
        | (Ty::Null, Ty::Null)
        | (Ty::Uint8Array, Ty::Uint8Array)
        | (Ty::Unknown, Ty::Unknown)
        | (Ty::RustType, Ty::RustType)
        | (Ty::Type, Ty::Type)
        | (Ty::Resource, Ty::Resource)
        | (Ty::PromptAst, Ty::PromptAst)
        | (Ty::Never, Ty::Never) => true,
        (Ty::Media(left), Ty::Media(right)) => left == right,
        (Ty::Literal(left, ..), Ty::Literal(right, ..)) => left == right,
        (Ty::TypeVar(left), Ty::TypeVar(right)) => left == right,
        (Ty::List(left), Ty::List(right)) => structurally_equal(left, right),
        (
            Ty::Map {
                key: left_key,
                value: left_value,
            },
            Ty::Map {
                key: right_key,
                value: right_value,
            },
        ) => structurally_equal(left_key, right_key) && structurally_equal(left_value, right_value),
        (Ty::Class(left_head, left_args), Ty::Class(right_head, right_args)) => {
            left_head == right_head && structurally_equal_slices(left_args, right_args)
        }
        (
            Ty::Interface(left_head, left_args, left_bindings),
            Ty::Interface(right_head, right_args, right_bindings),
        ) => {
            left_head == right_head
                && structurally_equal_slices(left_args, right_args)
                && structurally_equal_unordered(
                    left_bindings.iter().map(|(name, ty)| (Some(name), ty)),
                    right_bindings.iter().map(|(name, ty)| (Some(name), ty)),
                )
        }
        (Ty::Enum(left), Ty::Enum(right)) => left == right,
        (Ty::EnumVariant(left_head, left_variant), Ty::EnumVariant(right_head, right_variant)) => {
            left_head == right_head && left_variant == right_variant
        }
        (Ty::TypeAlias(left), Ty::TypeAlias(right)) => left == right,
        (
            Ty::Function {
                params: left_params,
                ret: left_ret,
                throws: left_throws,
            },
            Ty::Function {
                params: right_params,
                ret: right_ret,
                throws: right_throws,
            },
        ) => {
            left_params.len() == right_params.len()
                && left_params
                    .iter()
                    .zip(right_params.iter())
                    .all(|(left, right)| {
                        left.name == right.name
                            && left.mode == right.mode
                            && structurally_equal(&left.ty, &right.ty)
                    })
                && structurally_equal(left_ret, right_ret)
                && structurally_equal(left_throws, right_throws)
        }
        (Ty::Future(left_value, left_error), Ty::Future(right_value, right_error)) => {
            structurally_equal(left_value, right_value)
                && structurally_equal(left_error, right_error)
        }
        (
            Ty::AssociatedTypeProjection {
                base: left_base,
                interface: left_interface,
                member: left_member,
            },
            Ty::AssociatedTypeProjection {
                base: right_base,
                interface: right_interface,
                member: right_member,
            },
        ) => {
            left_member == right_member
                && structurally_equal(left_base, right_base)
                && left_interface.name == right_interface.name
                && structurally_equal_slices(&left_interface.generics, &right_interface.generics)
                && structurally_equal_unordered(
                    left_interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (Some(name), ty)),
                    right_interface
                        .associated_types
                        .iter()
                        .map(|(name, ty)| (Some(name), ty)),
                )
        }
        (Ty::Union(left), Ty::Union(right)) => structurally_equal_unordered(
            left.iter().map(|ty| (None, ty)),
            right.iter().map(|ty| (None, ty)),
        ),
        _ => false,
    }
}

fn structurally_equal_slices<N: Head>(left: &[Ty<N>], right: &[Ty<N>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| structurally_equal(left, right))
}

/// Whether two collections of (optionally named) types pair off one to one,
/// in any order.
fn structurally_equal_unordered<'t, N: Head + 't>(
    left: impl ExactSizeIterator<Item = (Option<&'t Name>, &'t Ty<N>)>,
    right: impl ExactSizeIterator<Item = (Option<&'t Name>, &'t Ty<N>)>,
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let right: Vec<_> = right.collect();
    let mut matched = vec![false; right.len()];
    left.into_iter().all(|(left_name, left_ty)| {
        let Some(index) = right
            .iter()
            .enumerate()
            .position(|(index, (right_name, right_ty))| {
                !matched[index] && left_name == *right_name && structurally_equal(left_ty, right_ty)
            })
        else {
            return false;
        };
        matched[index] = true;
        true
    })
}

/// Whether `left` and `right` name one type, also where one spells an
/// optional of the other's one non-null member (`T?` beside `T`).
fn selected_arm_equal<N: Head>(left: &Ty<N>, right: &Ty<N>) -> bool {
    if structurally_equal(left, right) {
        return true;
    }
    sole_non_null(left).is_some_and(|inner| structurally_equal(inner, right))
        || sole_non_null(right).is_some_and(|inner| structurally_equal(left, inner))
}

/// The one member of an optional (`T | null`) that is not `null`.
fn sole_non_null<N: Head>(ty: &Ty<N>) -> Option<&Ty<N>> {
    let Ty::Union(members) = ty else {
        return None;
    };
    if !members.iter().any(|member| matches!(member, Ty::Null)) {
        return None;
    }
    let mut non_null = members.iter().filter(|member| !matches!(member, Ty::Null));
    let only = non_null.next()?;
    non_null.next().is_none().then_some(only)
}

/// Whether `ty` has a position that stands for a type the boundary was not
/// told: a type variable, `unknown`, or the `$rust_type` a generic call falls
/// back to. Generics are invariant, so no live value's type equals such a
/// type, and membership in it says nothing.
fn has_erased_position<N: Head>(ty: &Ty<N>) -> bool {
    match ty {
        Ty::Unknown | Ty::TypeVar(_) | Ty::AssociatedTypeProjection { .. } | Ty::RustType => true,
        Ty::List(element) => has_erased_position(element),
        Ty::Map { key, value } => has_erased_position(key) || has_erased_position(value),
        Ty::Union(members) => members.iter().any(has_erased_position),
        Ty::Class(_, args) => args.iter().any(has_erased_position),
        Ty::Interface(_, args, associated) => {
            args.iter().any(has_erased_position)
                || associated.iter().any(|(_, ty)| has_erased_position(ty))
        }
        Ty::Function {
            params,
            ret,
            throws,
        } => {
            params.iter().any(|param| has_erased_position(&param.ty))
                || has_erased_position(ret)
                || has_erased_position(throws)
        }
        Ty::Future(value, error) => has_erased_position(value) || has_erased_position(error),
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Never
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::Literal(..)
        | Ty::Enum(_)
        | Ty::EnumVariant(..)
        | Ty::TypeAlias(_)
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst => false,
    }
}

// ── The engine's program ─────────────────────────────────────────────────

/// The program as the engine holds it, on the heap of one call. Its heads are
/// the live declaration pointers, so a class or enum is read where it lives.
pub(crate) struct Heap<'h> {
    pub(crate) engine: &'h BexEngine,
    pub(crate) vm: &'h bex_vm::BexVm,
    pub(crate) permit: bex_heap::PermitProof<'h>,
    pub(crate) overlay: InboundRuntimeOverlay<'h>,
}

impl Heap<'_> {
    /// The head of the declaration at `ptr`, or `None` for an object that is
    /// no declaration.
    fn head_at(&self, ptr: HeapPtr) -> Option<TypeHead> {
        let tag = match self.vm.get_object(ptr) {
            Object::Class(class) => class.type_tag,
            Object::Enum(enm) => enm.type_tag,
            Object::Interface(interface) => interface.type_tag,
            Object::TypeAlias(alias) => alias.type_tag,
            _ => return None,
        };
        Some(TypeHead::new(ptr, tag))
    }
}

impl Program<TypeHead> for Heap<'_> {
    fn resolve(&self, name: &str, kind: InboundDeclarationKind) -> Option<TypeHead> {
        let ptr = self
            .engine
            .resolve_inbound_declaration(self.vm, name, self.overlay, kind)?;
        self.head_at(ptr)
    }

    fn head_is_named(&self, head: &TypeHead, name: &str) -> bool {
        // The spelling resolves to the declaration itself, or renders it: a
        // host may spell a declared name by its display form.
        self.resolve(name, InboundDeclarationKind::Any)
            .is_some_and(|resolved| resolved == *head)
            || head
                .overlay_name()
                .is_some_and(|declared| type_name_matches_external_name(name, &declared))
    }

    fn anchor(&self, ty: &WireTy) -> Result<Ty<TypeHead>, Mismatch<'static, TypeHead>> {
        self.engine
            .anchor_wire_ty_with_runtime(self.vm, ty, self.overlay)
            .map_err(Into::into)
    }

    fn class<'p>(
        &'p self,
        head: &TypeHead,
        type_args: &[Ty<TypeHead>],
    ) -> Result<ClassFacts<'p, TypeHead>, Mismatch<'static, TypeHead>> {
        let vm: &'p bex_vm::BexVm = self.vm;
        let Object::Class(class) = vm.get_object(head.ptr()) else {
            return Ok(ClassFacts::Missing);
        };
        Ok(ClassFacts::Fields(
            class
                .fields
                .iter()
                .map(|field| {
                    (
                        field.name.as_str(),
                        field.field_template.substitute_symbolic(type_args),
                    )
                })
                .collect(),
        ))
    }

    fn variant(&self, head: &TypeHead, variant: &str) -> VariantFacts {
        let Object::Enum(declared) = self.vm.get_object(head.ptr()) else {
            return VariantFacts::NoEnum;
        };
        match declared.variants.iter().position(|v| v.name == variant) {
            Some(index) => VariantFacts::Declared(index),
            None => VariantFacts::NoVariant,
        }
    }

    fn alias(&self, head: &TypeHead) -> Option<Ty<TypeHead>> {
        self.vm
            .type_alias_definition(head.ptr())
            .map(|definition| definition.clone().into())
    }

    fn is_json(&self, head: &TypeHead) -> bool {
        self.vm.stdlib_heads().is_json_alias(*head)
    }

    fn media_wrapper(&self, head: &TypeHead) -> Option<MediaKind> {
        self.vm.stdlib_heads().media_kind(*head)
    }

    fn is_prompt_class(&self, head: &TypeHead) -> bool {
        head.overlay_name()
            .is_some_and(|name| name.render_dotted(false) == conversion::PROMPT_CLASS)
    }

    fn live_value_fits(
        &self,
        handle: &Handle,
        ty: &Ty<TypeHead>,
    ) -> Result<bool, Mismatch<'static, TypeHead>> {
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
        Ok(bex_vm::value_is_member(self.vm, Value::object(object), ty.as_ty()).unwrap_or(true))
    }

    fn is_subtype(
        &self,
        value_ty: &Ty<TypeHead>,
        ty: &Ty<TypeHead>,
    ) -> Result<bool, Mismatch<'static, TypeHead>> {
        Ok(baml_type::normalize::is_subtype(
            value_ty.as_ty(),
            ty.as_ty(),
            self.vm,
        ))
    }
}

/// A declaration head of a type held across a sys-op's await. The collector
/// may move a runtime declaration while the heap permit is released, so such
/// a head is rooted by a handle and found again through it; a compile-time
/// declaration never moves and is kept as it is.
#[derive(Clone, Debug)]
pub(crate) enum RootedHead {
    Static(TypeHead),
    Runtime {
        handle: Handle,
        tag: baml_type::typetag::TypeTag,
    },
}

/// A declared type held across a sys-op's await.
pub(crate) enum HeldTy {
    /// Every declaration it names was compiled with the program, and none
    /// moves: the type is held as it is.
    Pinned(bex_vm_types::RuntimeTy),
    /// It names a runtime declaration, rooted for the span of the await.
    Rooted(baml_type::RuntimeTy<RootedHead>),
}

impl BexEngine {
    /// Hold `ty` across an await, rooting every runtime declaration it names.
    pub(crate) fn hold_ty(&self, ty: bex_vm_types::RuntimeTy) -> HeldTy {
        let mut names_runtime_declaration = false;
        ty.visit_heads(&mut |head: &TypeHead| {
            names_runtime_declaration |= !self.heap.is_compile_time_ptr(head.ptr());
        });
        if !names_runtime_declaration {
            return HeldTy::Pinned(ty);
        }
        HeldTy::Rooted(ty.map_heads(&mut |head: &TypeHead| {
            if self.heap.is_compile_time_ptr(head.ptr()) {
                RootedHead::Static(*head)
            } else {
                RootedHead::Runtime {
                    handle: self.heap.create_handle(head.ptr()),
                    tag: head.tag(),
                }
            }
        }))
    }

    /// The type [`Self::hold_ty`] held, at the declarations' places after the
    /// await.
    pub(crate) fn release_ty(
        &self,
        permit: bex_heap::PermitProof<'_>,
        held: HeldTy,
    ) -> Result<bex_vm_types::RuntimeTy, EngineError> {
        match held {
            HeldTy::Pinned(ty) => Ok(ty),
            HeldTy::Rooted(ty) => ty.try_map_heads(&mut |head: &RootedHead| match head {
                RootedHead::Static(head) => Ok(*head),
                RootedHead::Runtime { handle, tag } => self
                    .resolve_handle(permit, handle)
                    .map(|ptr| TypeHead::new(ptr, *tag))
                    .ok_or_else(|| EngineError::TypeMismatch {
                        message: "a declaration of the declared type expired during the call"
                            .to_string(),
                    }),
            }),
        }
    }

    /// Judge a value from a host against the type of the slot it is bound
    /// for, as the host spelled it.
    pub(crate) fn admit<'a>(
        &self,
        holder: &impl HeapPermit<BexThread>,
        value: &'a BexExternalValue,
        ty: &WireTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<Admitted<'a, TypeHead>, Mismatch<'a, TypeHead>> {
        let heap = Heap {
            engine: self,
            vm: &holder.holder().vm,
            permit: holder.proof(),
            overlay,
        };
        let ty = heap.anchor(ty)?;
        admit(&Cx { program: &heap }, value, &ty).map(|admission| admission.value)
    }

    /// Judge a value against a type the engine already holds.
    pub(crate) fn admit_realized<'a>(
        &self,
        holder: &impl HeapPermit<BexThread>,
        value: &'a BexExternalValue,
        ty: &bex_vm_types::RuntimeTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<Admitted<'a, TypeHead>, Mismatch<'a, TypeHead>> {
        let heap = Heap {
            engine: self,
            vm: &holder.holder().vm,
            permit: holder.proof(),
            overlay,
        };
        admit(&Cx { program: &heap }, value, ty).map(|admission| admission.value)
    }

    /// Admit a value from a host to the type of its slot, as the host spelled
    /// it, and put it on the heap. `overlay` names the runtime declarations
    /// the call may spell.
    pub(crate) fn admit_to_heap(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        ty: &WireTy,
        overlay: InboundRuntimeOverlay<'_>,
    ) -> Result<Value, EngineError> {
        let admitted = self.admit(holder, value, ty, overlay)?;
        self.land(holder, admitted, overlay)
    }

    /// Admit a value to a type the engine holds and put it on the heap.
    pub(crate) fn admit_realized_to_heap(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        ty: &bex_vm_types::RuntimeTy,
    ) -> Result<Value, EngineError> {
        let overlay = InboundRuntimeOverlay::none();
        let admitted = self.admit_realized(holder, value, ty, overlay)?;
        self.land(holder, admitted, overlay)
    }

    /// Admit what a host callback returned to its declared return type and
    /// put it on the heap. The inner error is the callback's breach of its
    /// contract, which the caller raises as a panic of the BAML program.
    pub(crate) fn admit_host_return(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        ty: &bex_vm_types::RuntimeTy,
    ) -> Result<Result<Value, Mismatch<'static, TypeHead>>, EngineError> {
        let overlay = InboundRuntimeOverlay::none();
        match self.admit_realized(holder, value, ty, overlay) {
            Ok(admitted) => self.land(holder, admitted, overlay).map(Ok),
            Err(mismatch) => Ok(Err(Mismatch::new(format!(
                "host callable returned a value that does not match the declared return type \
                 `{ty}`: {mismatch}"
            )))),
        }
    }

    /// A type the judgment settled on, realized for the heap.
    fn realized(ty: &Ty<TypeHead>) -> Result<bex_vm_types::RealizedTy, EngineError> {
        bex_vm_types::RealizedTy::try_from(ty).map_err(|e| EngineError::TypeMismatch {
            message: format!("declared type is not realized: {e}"),
        })
    }

    /// Put an admitted value on the heap. Every question of type was settled
    /// when the value was admitted; what can still fail here is a handle
    /// that no longer resolves.
    pub(crate) fn land(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        admitted: Admitted<'_, TypeHead>,
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
                let element = Self::realized(&element)?;
                Value::object(holder.holder_mut().tlab_mut().alloc_array(element, values))
            }
            Admitted::Map {
                sent_key,
                key,
                value,
                entries,
            } => {
                let sent_key =
                    self.anchor_wire_ty_with_runtime(&holder.holder().vm, sent_key, overlay)?;
                let sent_key = Self::realized(&sent_key)?;
                let key = Self::realized(&key)?;
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
                let value = Self::realized(&value)?;
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
                let type_args = type_args
                    .iter()
                    .map(Self::realized)
                    .collect::<Result<Box<[_]>, _>>()?;
                Value::object(
                    holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_instance_with_type_args(class.ptr(), type_args, values),
                )
            }
            Admitted::Variant { enum_head, index } => Value::object(
                holder
                    .holder_mut()
                    .tlab_mut()
                    .alloc_variant(enum_head.ptr(), index),
            ),
            Admitted::Engine { value, slot } => {
                let slot = conversion::overlay_wire_ty(&slot)?;
                self.land_engine_value(holder, value.clone(), &slot, overlay)?
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, sync::Arc};

    use baml_type::{Freshness, FunctionParamMode, Name, RuntimeFunctionParamTy, TypeName};
    use bex_external_types::{HostValueArc, RuntimeTy, is_canonical_json_alias};
    use bex_str::BexStr;
    use bex_vm_types::{MediaContent, MediaValue};
    use indexmap::IndexMap;

    use super::*;

    /// The program of a test: the classes, enums and aliases it declares, by
    /// name. Its heads are names, so every spelling resolves to itself.
    #[derive(Default, Clone)]
    struct Facts {
        classes: IndexMap<TypeName, Vec<(String, RuntimeTy)>>,
        enums: IndexMap<TypeName, Vec<String>>,
        aliases: IndexMap<TypeName, RuntimeTy>,
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

    impl Program<TypeName> for Facts {
        fn resolve(&self, name: &str, _kind: InboundDeclarationKind) -> Option<TypeName> {
            Some(TypeName::from_dotted_path(name))
        }

        fn head_is_named(&self, head: &TypeName, name: &str) -> bool {
            type_name_matches_external_name(name, head)
        }

        fn anchor(&self, ty: &WireTy) -> Result<RuntimeTy, Mismatch<'static, TypeName>> {
            Ok(ty.clone())
        }

        fn class<'p>(
            &'p self,
            head: &TypeName,
            _type_args: &[RuntimeTy],
        ) -> Result<ClassFacts<'p, TypeName>, Mismatch<'static, TypeName>> {
            Ok(match self.classes.get(head) {
                Some(fields) => ClassFacts::Fields(
                    fields
                        .iter()
                        .map(|(name, ty)| (name.as_str(), ty.clone()))
                        .collect(),
                ),
                None => ClassFacts::Missing,
            })
        }

        fn variant(&self, head: &TypeName, variant: &str) -> VariantFacts {
            match self.enums.get(head) {
                None => VariantFacts::NoEnum,
                Some(variants) => match variants.iter().position(|declared| declared == variant) {
                    Some(index) => VariantFacts::Declared(index),
                    None => VariantFacts::NoVariant,
                },
            }
        }

        fn alias(&self, head: &TypeName) -> Option<RuntimeTy> {
            self.aliases.get(head).cloned()
        }

        fn is_json(&self, head: &TypeName) -> bool {
            is_canonical_json_alias(head)
        }

        fn media_wrapper(&self, head: &TypeName) -> Option<MediaKind> {
            MediaKind::from_wrapper_class_name(&head.render_dotted(false))
        }

        fn is_prompt_class(&self, head: &TypeName) -> bool {
            head.render_dotted(false) == conversion::PROMPT_CLASS
        }

        fn live_value_fits(
            &self,
            _handle: &Handle,
            _ty: &RuntimeTy,
        ) -> Result<bool, Mismatch<'static, TypeName>> {
            Err(Mismatch::new("the program of a test has no heap"))
        }

        fn is_subtype(
            &self,
            value_ty: &RuntimeTy,
            ty: &RuntimeTy,
        ) -> Result<bool, Mismatch<'static, TypeName>> {
            Ok(assignable_with_aliases(value_ty, ty, &self.aliases))
        }
    }

    /// Whether every value of `actual` is one of `expected`, with the aliases
    /// of a test program expanded, and no VM to ask about declarations.
    fn assignable_with_aliases(
        actual: &RuntimeTy,
        expected: &RuntimeTy,
        aliases: &IndexMap<TypeName, RuntimeTy>,
    ) -> bool {
        if structurally_equal(actual, expected) {
            return true;
        }
        if let RuntimeTy::TypeAlias(name) = actual
            && let Some(expanded) = aliases.get(name)
        {
            return assignable_with_aliases(expanded, expected, aliases);
        }
        if let RuntimeTy::TypeAlias(name) = expected
            && let Some(expanded) = aliases.get(name)
        {
            return assignable_with_aliases(actual, expanded, aliases);
        }
        #[expect(
            deprecated,
            reason = "a test program has aliases but no VM-backed type facts"
        )]
        baml_type::normalize::is_subtype(
            actual.as_ty(),
            expected.as_ty(),
            &baml_type::normalize::NoFacts,
        )
    }

    /// The head a judgment resolves a declaration by.
    fn declared(name: &str) -> TypeName {
        TypeName::from_dotted_path(name)
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

    fn int(int: i64) -> Admitted<'static, TypeName> {
        Admitted::Int(VmInt::new::<TypeName>(int).expect("a test integer the VM holds"))
    }

    /// A value of the engine's own making, admitted to `slot`.
    fn engine(value: &BexExternalValue, slot: RuntimeTy) -> Admitted<'_, TypeName> {
        Admitted::Engine { value, slot }
    }

    /// A map as a `json` slot holds one: its values are `json` themselves.
    fn json_map<'a>(
        sent_key: &'a RuntimeTy,
        entries: Vec<(&'a str, Admitted<'a, TypeName>)>,
    ) -> Admitted<'a, TypeName> {
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
    fn assert_host_has_to_name_the_member(
        result: Result<Admitted<'_, TypeName>, Mismatch<'_, TypeName>>,
    ) {
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains("matches multiple union members")
                && error.to_string().contains("value_type"),
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
    ) -> Result<Admitted<'a, TypeName>, Mismatch<'a, TypeName>> {
        admit(&Cx { program: facts }, value, ty).map(|admission| admission.value)
    }

    /// Judge `value` against `ty` in a program of `facts` that also declares
    /// `aliases`.
    fn admit_with_aliases<'a>(
        facts: &Facts,
        aliases: &IndexMap<TypeName, RuntimeTy>,
        value: &'a BexExternalValue,
        ty: &RuntimeTy,
    ) -> Result<Admitted<'a, TypeName>, Mismatch<'a, TypeName>> {
        let program = Facts {
            aliases: aliases.clone(),
            ..facts.clone()
        };
        admit(&Cx { program: &program }, value, ty).map(|admission| admission.value)
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
        assert!(error.to_string().contains("has no field `zzz`"), "{error}");
        let missing = BexExternalValue::typed(
            instance(&[("label", BexExternalValue::String("a".into()))]),
            reading.clone(),
        );
        let error = admit_in(&facts, &missing, &reading).unwrap_err();
        assert!(
            error.to_string().contains("Missing field `count`"),
            "{error}"
        );
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
            enum_head: declared("user.callbacks.Mood"),
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
                .to_string()
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
                .to_string()
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
            error.to_string().contains("bool") && error.to_string().contains("declared union"),
            "{error}"
        );
    }

    #[test]
    fn host_named_type_the_slot_does_not_take_is_refused() {
        let facts = Facts::default();
        let named = BexExternalValue::typed(BexExternalValue::Int(7), RuntimeTy::int());
        let error = admit_in(&facts, &named, &RuntimeTy::string()).unwrap_err();
        assert!(
            error.to_string().contains("int") && error.to_string().contains("string"),
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
            error.to_string().contains("unbound type variable `T`"),
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
                class: TypeName::from_dotted_path("user.unions.T"),
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
            error.to_string().contains("does not identify one member")
                && error.to_string().contains("concrete generic arguments"),
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
        assert!(structurally_equal(&declared_member, &wire_spelling));

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
        assert_eq!(Facts::default().media_wrapper(name), None);

        let facts = with_media_class(Facts::default(), "user.baml.media.Image");
        let image = media_value(MediaKind::Image);
        let named = BexExternalValue::typed(media_shell(IMAGE_CLASS, &image), spoof);
        let error = admit_in(&facts, &named, &RuntimeTy::Media(MediaKind::Image)).unwrap_err();
        assert!(
            error
                .to_string()
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
        let facts = Facts {
            aliases,
            ..Facts::default()
        };
        assert!(!media_wrapper_matches(
            &Cx { program: &facts },
            &class_ty(IMAGE_CLASS),
            &alias_ty("user.aliases.First"),
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
