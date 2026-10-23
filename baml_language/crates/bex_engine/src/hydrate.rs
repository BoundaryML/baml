//! Hydration: a host value is built on the heap as the value it is, then
//! checked against the slot it was bound for.
//!
//! A host value arrives as a [`BexExternalValue`] whose every node carries the
//! concrete type it has: a scalar, `null`, enum variant, media value, rendered
//! prompt, reflected type or handle implies its own; a list carries its
//! element type, a map its key and value types, an instance its class and type
//! arguments, a callable its function type. [`BexEngine::hydrate`] builds that
//! value on the heap exactly as it says, and refuses it where it says
//! something the program cannot hold: a class or variant the program does not
//! declare, a type that is not realized, a child that is not a member of the
//! component type its parent states (the same check the VM makes when a value
//! is pushed into a list or written to a field, so the heap never holds an
//! `int[]` with a string in it). The slot is not consulted.
//!
//! [`BexEngine::hydrate_into`] then asks the one question the type system asks
//! of any value (`TYPE_SYSTEM.md`, "Values and membership"): is the concrete type
//! of the finished value a subtype of the slot? The canonical algebra answers
//! over the live program, so interfaces, aliases (the recursive `json`
//! included), literals, `unknown` and bound type variables all fall out of
//! `is_subtype`, and nothing here reads a slot into a value. What a host means
//! by a value that is not a BAML value is the host's bridge to say, before the
//! value is sent.

use std::collections::HashMap;

use baml_type::{ClassCrossing, normalize};
use bex_external_types::{
    BexExternalAdt, BexExternalValue, Bounded, Handle, HostValueKind, WireHead,
};
use bex_heap::{HeapPermit, TlabHolder};
use bex_vm::BexVm;
use bex_vm_types::{HeapPtr, Object, RealizedTy, RuntimeTy, Ty, TypeHead, Value};

use crate::{
    BexEngine, EngineError,
    conversion::{InboundDeclarationKind, Peer},
    thread::BexThread,
};

/// A type as a host states it: compiled declarations by name, runtime
/// declarations by handle.
pub(crate) type WireTy = bex_external_types::WireTy;

/// How deep a value may nest before it is given up. A host value is a finite
/// tree, so this bounds the stack a hostile or runaway producer can ask for.
const MAX_DEPTH: usize = 512;

/// Why a host value was not taken: it could not be built as it was sent, or
/// the value it builds is not a member of its slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// Where in the value, from the root.
    path: Vec<Segment>,
    kind: RefusalKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Item(usize),
    Entry(Bounded),
    Field(Bounded),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RefusalKind {
    /// A value of type `found` (`None` for a value with no BAML type of its
    /// own) where `expected` is declared.
    NotAMember {
        found: Option<String>,
        expected: Bounded,
    },
    /// The value says something the program cannot hold.
    Malformed(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.path.is_empty() {
            f.write_str("at `")?;
            for segment in &self.path {
                match segment {
                    Segment::Item(index) => write!(f, "[{index}]")?,
                    Segment::Entry(key) => write!(f, "[{:?}]", key.as_str())?,
                    Segment::Field(name) => write!(f, ".{name}")?,
                }
            }
            f.write_str("`: ")?;
        }
        match &self.kind {
            RefusalKind::NotAMember {
                found: Some(found),
                expected,
            } => write!(
                f,
                "Value of type '{found}' does not match the declared type `{expected}`"
            ),
            RefusalKind::NotAMember {
                found: None,
                expected,
            } => write!(
                f,
                "A value with no BAML type does not match the declared type `{expected}`"
            ),
            RefusalKind::Malformed(message) => f.write_str(message),
        }
    }
}

impl From<Refusal> for EngineError {
    fn from(refusal: Refusal) -> Self {
        EngineError::TypeMismatch {
            message: refusal.to_string(),
        }
    }
}

/// Whether `value` is a member of `ty`. A value with no BAML type of its own
/// (a compile-time definition object, see `BexVm::value_concrete_ty`) is a
/// member only of a type that takes every value.
fn is_member(vm: &BexVm, value: Value, ty: &Ty) -> bool {
    bex_vm::value_is_member(vm, value, ty)
        .unwrap_or_else(|| normalize::is_subtype(&Ty::Unknown, ty, vm))
}

/// The concrete type of a value on the heap, rendered for a message.
fn concrete_ty_name(vm: &BexVm, value: Value) -> Option<String> {
    BexEngine::landed_ty(vm, value).map(|ty| ty.to_string())
}

/// One hydration: the heap it builds on, the program it reads, who built the
/// value, and where in the value it is. A host's value and the runtime's own
/// (a sys-op's result) are built and checked the same way; what differs is
/// what each may hand over: a host builds plain data only, and hands back
/// references to values it received.
struct Hydrator<'a, H: HeapPermit<BexThread>> {
    engine: &'a BexEngine,
    holder: &'a mut H,
    peer: Peer,
    path: Vec<Segment>,
    /// Membership answers for primitive values under slots the algebra has to
    /// decide (a union, a recursive alias): a list of a thousand ints under
    /// `json` asks the same question a thousand times. The algebra's answer
    /// for a primitive depends on nothing but the primitive and the slot
    /// unless the slot mentions a literal type, where it depends on the value
    /// (`"circle"` is a `string` but not a `"square"`); such a slot is asked
    /// per value. A primitive or `unknown` slot is answered without the
    /// algebra by [`bex_vm::value_is_member`] itself and is not recorded.
    members: HashMap<RealizedTy, SlotMembership>,
}

/// What a hydration remembers about one slot's membership of primitives.
enum SlotMembership {
    /// The slot mentions a literal type, so the answer depends on the value:
    /// per primitive, the part of the slot a value of that primitive can
    /// inhabit, worked out once (`1 | 2 | "a" | Foo` asks an int only
    /// `1 | 2`).
    PerValue(Box<[Option<Restricted>; bex_vm::Primitive::COUNT]>),
    /// The answer per primitive, filled in the first time each is asked.
    ByPrimitive([Option<bool>; bex_vm::Primitive::COUNT]),
}

/// The part of a slot's type a value of one primitive can inhabit.
#[derive(Clone)]
enum Restricted {
    /// The slot names the primitive itself: every value of it is a member.
    Always,
    /// Nothing in the slot can hold a value of the primitive.
    Never,
    /// The slot's literal types of the primitive: the value decides.
    Literals(Ty),
    /// The slot has a member the primitive may inhabit in a way only the
    /// algebra can tell (an interface, an alias): ask it of the whole slot.
    Whole(Ty),
}

impl Restricted {
    /// `ty`, normalized, restricted to what a value of `primitive` can
    /// inhabit.
    fn of(vm: &BexVm, ty: &Ty, primitive: bex_vm::Primitive) -> Self {
        let normal = normalize::normalize(ty, vm);
        let members: Vec<Ty> = match &normal {
            Ty::Union(members) => members.to_vec(),
            other => vec![other.clone()],
        };
        let mut literals = Vec::new();
        for member in &members {
            match member {
                Ty::Unknown => return Self::Always,
                Ty::Int
                | Ty::Bigint
                | Ty::Float
                | Ty::String
                | Ty::Bool
                | Ty::Null
                | Ty::Uint8Array
                    if Self::primitive_of(member) == Some(primitive) =>
                {
                    return Self::Always;
                }
                Ty::Literal(literal, _) => {
                    // A float literal is a value, never a type (TYPE_SYSTEM.md
                    // "Literal Types"); one here admits nothing.
                    let of = match literal {
                        baml_type::Literal::Int(_) => Some(bex_vm::Primitive::Int),
                        baml_type::Literal::Bigint(_) => Some(bex_vm::Primitive::Bigint),
                        baml_type::Literal::String(_) => Some(bex_vm::Primitive::String),
                        baml_type::Literal::Bool(_) => Some(bex_vm::Primitive::Bool),
                        baml_type::Literal::Float(_) => None,
                    };
                    if of == Some(primitive) {
                        literals.push(member.clone());
                    }
                }
                // A member the algebra alone can relate a primitive to.
                Ty::Interface(..) | Ty::TypeAlias(_) | Ty::TypeVar(_) => {
                    return Self::Whole(normal);
                }
                Ty::Int
                | Ty::Bigint
                | Ty::Float
                | Ty::String
                | Ty::Bool
                | Ty::Null
                | Ty::Uint8Array
                | Ty::Media(_)
                | Ty::Class(..)
                | Ty::Enum(_)
                | Ty::EnumVariant(..)
                | Ty::List(_)
                | Ty::Map { .. }
                | Ty::Union(_)
                | Ty::Function { .. }
                | Ty::Future(..)
                | Ty::RustType
                | Ty::Type
                | Ty::Resource
                | Ty::PromptAst
                | Ty::AssociatedTypeProjection { .. }
                | Ty::Never
                | Ty::Error => {}
            }
        }
        match literals.len() {
            0 => Self::Never,
            1 => Self::Literals(literals.pop().expect("one literal")),
            _ => Self::Literals(Ty::Union(literals.into())),
        }
    }

    fn primitive_of(ty: &Ty) -> Option<bex_vm::Primitive> {
        Some(match ty {
            Ty::Int => bex_vm::Primitive::Int,
            Ty::Bigint => bex_vm::Primitive::Bigint,
            Ty::Float => bex_vm::Primitive::Float,
            Ty::String => bex_vm::Primitive::String,
            Ty::Bool => bex_vm::Primitive::Bool,
            Ty::Null => bex_vm::Primitive::Null,
            Ty::Uint8Array => bex_vm::Primitive::Uint8Array,
            _ => return None,
        })
    }

    fn admits(&self, vm: &BexVm, value: Value) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Literals(ty) | Self::Whole(ty) => is_member(vm, value, ty),
        }
    }
}

/// Does `ty` mention a literal type anywhere, with recursive aliases
/// unfolded through their definitions? Membership of a primitive value in
/// such a type depends on the value itself.
fn mentions_literal(vm: &BexVm, ty: &Ty, seen: &mut Vec<HeapPtr>) -> bool {
    match ty {
        Ty::Literal(..) | Ty::EnumVariant(..) | Ty::Error => true,
        Ty::TypeAlias(head) => {
            if seen.contains(&head.ptr()) {
                return false;
            }
            seen.push(head.ptr());
            match vm.type_alias_definition(head.ptr()) {
                Some(body) => mentions_literal(vm, body.as_ty(), seen),
                // An alias with no definition is refused elsewhere; treat it
                // as value-dependent so nothing is memoized about it.
                None => true,
            }
        }
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Uint8Array
        | Ty::Media(_)
        | Ty::Class(..)
        | Ty::Interface(..)
        | Ty::Enum(_)
        | Ty::List(_)
        | Ty::Map { .. }
        | Ty::Union(_)
        | Ty::Function { .. }
        | Ty::Future(..)
        | Ty::RustType
        | Ty::Type
        | Ty::Resource
        | Ty::PromptAst
        | Ty::TypeVar(_)
        | Ty::AssociatedTypeProjection { .. }
        | Ty::Unknown
        | Ty::Never => {
            let mut found = false;
            ty.visit_children(&mut |child| found = found || mentions_literal(vm, child, seen));
            found
        }
    }
}

impl<H: HeapPermit<BexThread>> Hydrator<'_, H> {
    fn vm(&self) -> &BexVm {
        &self.holder.holder().vm
    }

    fn refuse(&self, kind: RefusalKind) -> Refusal {
        Refusal {
            path: self.path.clone(),
            kind,
        }
    }

    fn malformed(&self, message: impl Into<String>) -> Refusal {
        self.refuse(RefusalKind::Malformed(message.into()))
    }

    /// A type the host stated for a value, over the program's declarations.
    fn realized(&self, ty: &WireTy) -> Result<RealizedTy, Refusal> {
        self.engine
            .anchor_wire_ty(self.holder.proof(), self.vm(), ty)
            .map_err(|error| self.malformed(error.to_string()))
    }

    /// `value`, built, is a member of `ty`: the check a list makes on a push
    /// and a class on a field write.
    fn member(&mut self, value: Value, ty: &RealizedTy) -> Result<(), Refusal> {
        let answered_by_slot = matches!(
            ty.as_ty(),
            Ty::Unknown
                | Ty::Int
                | Ty::Bigint
                | Ty::Float
                | Ty::String
                | Ty::Bool
                | Ty::Null
                | Ty::Uint8Array
        );
        let is = match bex_vm::value_primitive(self.vm(), value) {
            Some(primitive) if !answered_by_slot => {
                let vm = &self.holder.holder().vm;
                let slot = self.members.entry(ty.clone()).or_insert_with(|| {
                    if mentions_literal(vm, ty.as_ty(), &mut Vec::new()) {
                        SlotMembership::PerValue(Box::new(
                            [const { None }; bex_vm::Primitive::COUNT],
                        ))
                    } else {
                        SlotMembership::ByPrimitive([None; bex_vm::Primitive::COUNT])
                    }
                });
                match slot {
                    SlotMembership::PerValue(restricted) => restricted[primitive.index()]
                        .get_or_insert_with(|| Restricted::of(vm, ty.as_ty(), primitive))
                        .admits(vm, value),
                    SlotMembership::ByPrimitive(answers) => match answers[primitive.index()] {
                        Some(is) => is,
                        None => {
                            let is = is_member(vm, value, ty.as_ty());
                            answers[primitive.index()] = Some(is);
                            is
                        }
                    },
                }
            }
            Some(_) | None => is_member(self.vm(), value, ty.as_ty()),
        };
        if is {
            Ok(())
        } else {
            Err(self.refuse(RefusalKind::NotAMember {
                found: concrete_ty_name(self.vm(), value),
                expected: Bounded::ty(ty.to_string()),
            }))
        }
    }

    /// Build a child at `segment` of the current node.
    fn within<T>(
        &mut self,
        segment: Segment,
        build: impl FnOnce(&mut Self) -> Result<T, Refusal>,
    ) -> Result<T, Refusal> {
        if self.path.len() >= MAX_DEPTH {
            return Err(self.malformed("the value is nested too deeply to build"));
        }
        self.path.push(segment);
        let built = build(self);
        self.path.pop();
        built
    }

    fn alloc(&mut self, object: Object) -> Value {
        Value::object(self.holder.holder_mut().tlab_mut().alloc(object))
    }

    /// Build `value` as the value it says it is.
    fn value(&mut self, value: &BexExternalValue) -> Result<Value, Refusal> {
        Ok(match value {
            BexExternalValue::Null => Value::NULL,
            BexExternalValue::Int(int) => Value::try_int(*int).ok_or_else(|| {
                self.malformed(format!(
                    "integer {int} is outside the BAML integer range [{}, {}]",
                    Value::INT_MIN,
                    Value::INT_MAX
                ))
            })?,
            BexExternalValue::Bigint(big) => {
                let bits = big.bits();
                if bits > baml_type::MAX_BIGINT_BITS {
                    return Err(self.malformed(format!(
                        "bigint value requires {bits} bits (limit: {})",
                        baml_type::MAX_BIGINT_BITS
                    )));
                }
                Value::object(
                    self.holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_bigint(big.clone()),
                )
            }
            BexExternalValue::Float(float) => self.alloc(Object::Float(*float)),
            BexExternalValue::Bool(b) => Value::bool(*b),
            BexExternalValue::String(s) => {
                Value::object(self.holder.holder_mut().tlab_mut().alloc_string(s.clone()))
            }
            BexExternalValue::Uint8Array(bytes) => Value::object(
                self.holder
                    .holder_mut()
                    .tlab_mut()
                    .alloc_uint8array(bytes.clone()),
            ),
            BexExternalValue::Array {
                element_type,
                items,
            } => {
                let element = self.realized(element_type)?;
                let mut values = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    values.push(self.within(Segment::Item(index), |this| {
                        let built = this.value(item)?;
                        this.member(built, &element)?;
                        Ok(built)
                    })?);
                }
                Value::object(
                    self.holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_array(element, values),
                )
            }
            BexExternalValue::Map {
                key_type,
                value_type,
                entries,
            } => {
                let key = self.realized(key_type)?;
                let value_ty = self.realized(value_type)?;
                // Keys cross the boundary as strings, so the stated key type
                // has to be one strings inhabit, and each key a member of it.
                if !normalize::is_string_key(key.as_ty(), self.vm()) {
                    return Err(self.malformed(format!(
                        "map keys cross the boundary as strings, which the stated key type `{key}` \
                         does not hold"
                    )));
                }
                let mut values = indexmap::IndexMap::with_capacity(entries.len());
                for (name, entry) in entries {
                    let literal = Ty::Literal(
                        baml_type::Literal::String(name.clone()),
                        baml_type::Freshness::Regular,
                    );
                    if !normalize::is_subtype(&literal, key.as_ty(), self.vm()) {
                        return Err(self.malformed(format!(
                            "map key {:?} is not a member of the stated key type `{key}`",
                            Bounded::text(name).as_str()
                        )));
                    }
                    let built = self.within(Segment::Entry(Bounded::text(name)), |this| {
                        let built = this.value(entry)?;
                        this.member(built, &value_ty)?;
                        Ok(built)
                    })?;
                    values.insert(bex_vm_types::BexStr::from(name.as_str()), built);
                }
                Value::object(
                    self.holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_map(key, value_ty, values),
                )
            }
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            } => self.instance(class_name, type_args, fields)?,
            BexExternalValue::Variant {
                enum_name,
                variant_name,
            } => {
                let ptr = self
                    .engine
                    .resolve_static_declaration(self.vm(), enum_name, InboundDeclarationKind::Enum)
                    .ok_or_else(|| {
                        self.malformed(format!("unknown enum `{}`", Bounded::text(enum_name)))
                    })?;
                let Object::Enum(declared) = self.vm().get_object(ptr) else {
                    return Err(
                        self.malformed(format!("`{}` is not an enum", Bounded::text(enum_name)))
                    );
                };
                let index = declared
                    .variants
                    .iter()
                    .position(|variant| variant.name == *variant_name)
                    .ok_or_else(|| {
                        self.malformed(format!(
                            "enum `{}` has no variant `{}`",
                            declared.name(),
                            Bounded::text(variant_name)
                        ))
                    })?;
                Value::object(
                    self.holder
                        .holder_mut()
                        .tlab_mut()
                        .alloc_variant(ptr, index),
                )
            }
            // A node states its own type; the engine's union envelope is for
            // values leaving it.
            BexExternalValue::Union { .. } => {
                return Err(self.malformed(
                    "a union envelope is not a host value; a node states the type it has",
                ));
            }
            BexExternalValue::RustData(data) => Value::object(
                self.holder
                    .holder_mut()
                    .tlab_mut()
                    .alloc_rust_data(data.clone()),
            ),
            BexExternalValue::Handle(handle)
            | BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle {
                heap_handle: handle,
                ..
            }) => self.live(handle)?,
            // The engine's own media and prompt payloads are values of the
            // standard library's wrapper classes.
            BexExternalValue::Adt(BexExternalAdt::Media(media)) => {
                let class = self
                    .vm()
                    .stdlib_heads()
                    .media_wrapper(media.kind)
                    .ok_or_else(|| {
                        self.malformed(
                            "a media value of no particular kind cannot be built; state image, \
                             audio, video or pdf",
                        )
                    })?
                    .ptr();
                self.wrapper_instance(class, media.clone())?
            }
            BexExternalValue::Adt(BexExternalAdt::PromptAst(prompt)) => {
                let class = self
                    .vm()
                    .lang_head(bex_vm::lang_items::AI_PROMPT)
                    .ok_or_else(|| self.malformed("the program declares no `ai.Prompt`"))?
                    .ptr();
                self.wrapper_instance(class, prompt.clone())?
            }
            BexExternalValue::Adt(BexExternalAdt::Type(reflected)) => {
                // A reflected type names its declarations; an anonymous one has
                // no spelling to anchor against and should have crossed as a
                // handle, which lands as the declaration itself.
                // A type value is realized; the lane's wider family is the
                // sys-op boundary's, not the value's.
                let reflected = baml_type::RealizedTy::try_from(reflected).map_err(|error| {
                    self.malformed(format!("a reflected type is not realized: {error}"))
                })?;
                let named: WireTy =
                    reflected.try_map_heads(&mut |head: &baml_type::TaggedTypeName| {
                        head.declared()
                            .cloned()
                            .map(WireHead::Static)
                            .ok_or_else(|| {
                                self.malformed(format!(
                                "host-supplied type names the anonymous declaration `{}`, which \
                                 has no resolvable spelling; pass it as a handle instead",
                                head.display_name()
                            ))
                            })
                    })?;
                let realized = self.realized(&named)?;
                Value::object(
                    self.holder
                        .holder_mut()
                        .vm
                        .alloc_type(bex_vm_types::types::TypeValue::new(realized)),
                )
            }
            BexExternalValue::Adt(BexExternalAdt::WireType(stated)) => {
                // A type the host states lands as a fresh `type` value over
                // the declarations it references: compiled ones by name,
                // runtime ones through the host's handles, which keep them
                // the same declarations they were when they left.
                let realized = self.realized(stated)?;
                Value::object(
                    self.holder
                        .holder_mut()
                        .vm
                        .alloc_type(bex_vm_types::types::TypeValue::new(realized)),
                )
            }
            BexExternalValue::HostValue(host) => match host.kind {
                HostValueKind::Opaque => {
                    let data: std::sync::Arc<dyn bex_vm_types::BexRustData> = host.clone();
                    Value::object(self.holder.holder_mut().tlab_mut().alloc_rust_data(data))
                }
                HostValueKind::Callable => {
                    return Err(self.malformed(
                        "a host callable arrives with the function type it inhabits; this one \
                         states none",
                    ));
                }
            },
            BexExternalValue::HostCallable {
                handle,
                params,
                ret,
                throws,
            } => {
                // Dispatch addresses an optional parameter by name, so a
                // function type that leaves one unnamed has no slot the host
                // could fill.
                if params.iter().any(|param| {
                    matches!(param.mode, baml_type::FunctionParamMode::Optional)
                        && param.name.is_none()
                }) {
                    return Err(self.malformed(
                        "a host callable's function type has an optional parameter without a name",
                    ));
                }
                let params = params
                    .iter()
                    .map(|param| {
                        Ok(bex_vm_types::RealizedFunctionParamTy {
                            name: param.name.clone(),
                            ty: self.realized(&param.ty)?,
                            mode: param.mode,
                        })
                    })
                    .collect::<Result<Vec<_>, Refusal>>()?;
                let closure = bex_vm_types::HostClosure {
                    handle: handle.clone(),
                    ret_ty: Box::new(self.realized(ret)?),
                    throws_ty: Box::new(self.realized(throws)?),
                    arity: params.len(),
                    params: Box::new(params),
                };
                self.alloc(Object::HostClosure(closure))
            }
        })
    }

    /// An instance of `class_name` at `type_args`, with one value per field.
    fn instance(
        &mut self,
        class_name: &str,
        type_args: &[WireTy],
        fields: &indexmap::IndexMap<String, BexExternalValue>,
    ) -> Result<Value, Refusal> {
        let class_ptr = self
            .engine
            .resolve_static_declaration(self.vm(), class_name, InboundDeclarationKind::Class)
            .ok_or_else(|| {
                self.malformed(format!("unknown class `{}`", Bounded::text(class_name)))
            })?;
        let type_args = type_args
            .iter()
            .map(|arg| self.realized(arg))
            .collect::<Result<Box<[_]>, _>>()?;
        // The declared fields at these arguments, read before any child is
        // built: building borrows the heap mutably.
        let (declared_name, declared) = {
            let vm = self.vm();
            let Object::Class(class) = vm.get_object(class_ptr) else {
                return Err(
                    self.malformed(format!("`{}` is not a class", Bounded::text(class_name)))
                );
            };
            if type_args.len() != class.generic_param_count {
                return Err(self.malformed(format!(
                    "class `{}` takes {} type argument(s), {} were stated",
                    class.name(),
                    class.generic_param_count,
                    type_args.len()
                )));
            }
            let declared = class
                .fields
                .iter()
                .map(|field| {
                    let ty = field
                        .field_template
                        .substitute(&type_args, vm)
                        .map_err(|error| {
                            self.malformed(format!(
                                "field `{}` of class `{}` has no type at these arguments: {error}",
                                field.name,
                                class.name()
                            ))
                        })?;
                    Ok((field.name.clone(), ty))
                })
                .collect::<Result<Vec<(String, RealizedTy)>, Refusal>>()?;
            // A class that crosses as its own kind of thing is built by the
            // runtime alone: a host sends that thing, or passes back the
            // instance it received.
            let crossing = class.crossing();
            if self.peer == Peer::Host && !crossing.host_builds() {
                return Err(self.malformed(match crossing {
                    ClassCrossing::Media(_) => {
                        format!("`{}` is a media value; send the media itself", class.name())
                    }
                    ClassCrossing::Prompt => format!(
                        "`{}` is a rendered prompt; send the prompt itself",
                        class.name()
                    ),
                    ClassCrossing::Opaque | ClassCrossing::Stream | ClassCrossing::FunctionSpec => {
                        format!(
                            "`{}` is the runtime's own; pass the instance you received",
                            class.name()
                        )
                    }
                    ClassCrossing::Fields => unreachable!("a host builds a `Fields` class"),
                }));
            }
            (class.name().to_string(), declared)
        };
        // A key the class does not declare is refused once the declared
        // fields are read: beside an omitted nullable field it is likely that
        // field misspelled or under its `@alias` name, and the refusal names
        // it rather than reading the field as `null`.
        let undeclared = fields
            .keys()
            .find(|key| !declared.iter().any(|(field, _)| field == *key));
        let mut values = Vec::with_capacity(declared.len());
        for (field, field_ty) in &declared {
            let built = match fields.get(field) {
                Some(sent) => self.within(Segment::Field(Bounded::text(field)), |this| {
                    let built = this.value(sent)?;
                    this.member(built, field_ty)?;
                    Ok(built)
                })?,
                // A plain host object leaves optional keys out: an omitted
                // field whose type admits `null` reads as `null`, exactly as
                // if the host had sent one.
                None if undeclared.is_none() && self.member(Value::NULL, field_ty).is_ok() => {
                    Value::NULL
                }
                None => {
                    let hint = undeclared
                        .map(|key| {
                            format!(" (the value has undeclared key `{}`)", Bounded::text(key))
                        })
                        .unwrap_or_default();
                    return Err(self.malformed(format!(
                        "Missing field `{field}` in external Instance for class `{declared_name}`{hint}"
                    )));
                }
            };
            values.push(built);
        }
        if let Some(unknown) = undeclared {
            return Err(self.malformed(format!(
                "class `{declared_name}` has no field `{}`",
                Bounded::text(unknown)
            )));
        }
        Ok(Value::object(
            self.holder
                .holder_mut()
                .tlab_mut()
                .alloc_instance_with_type_args(class_ptr, type_args, values),
        ))
    }

    /// The object `handle` refers to on this engine's heap, if it reaches one.
    fn resolve(&self, handle: &Handle) -> Option<HeapPtr> {
        self.engine.resolve_handle(self.holder.proof(), handle)
    }

    /// The object a live reference refers to. A host hands back values it
    /// received, never the runtime's internals (a declaration, a package);
    /// the runtime's own results may hold either.
    fn live(&self, handle: &Handle) -> Result<Value, Refusal> {
        let ptr = self.resolve(handle).ok_or_else(|| {
            self.malformed("a handle is stale or belongs to a different BAML runtime")
        })?;
        if self.peer == Peer::Host && !self.vm().get_object(ptr).is_first_class_value() {
            return Err(self.malformed("a handle refers to runtime internals, not a value"));
        }
        Ok(Value::object(ptr))
    }

    /// An instance of the standard library's wrapper class `class` around
    /// `payload`, which its one `_data` field holds as opaque data.
    fn wrapper_instance(
        &mut self,
        class: HeapPtr,
        payload: std::sync::Arc<dyn bex_vm_types::BexRustData>,
    ) -> Result<Value, Refusal> {
        {
            let Object::Class(declared) = self.vm().get_object(class) else {
                return Err(self.malformed("a media or prompt wrapper is not a class"));
            };
            let fields: Vec<&str> = declared.fields.iter().map(|f| f.name.as_str()).collect();
            if fields != [bex_external_types::MEDIA_WRAPPER_DATA_FIELD] {
                return Err(self.malformed(format!(
                    "wrapper class `{}` does not hold one `{}` field",
                    declared.name(),
                    bex_external_types::MEDIA_WRAPPER_DATA_FIELD
                )));
            }
        }
        let data = Value::object(self.holder.holder_mut().tlab_mut().alloc_rust_data(payload));
        Ok(Value::object(
            self.holder
                .holder_mut()
                .tlab_mut()
                .alloc_instance_with_type_args(class, Box::new([]), vec![data]),
        ))
    }
}

impl BexEngine {
    /// Build a value a host sent on the heap as the value it says it is: an
    /// argument, a callback's outcome, a cancel token. A host builds plain
    /// data only and hands back references to values it received. The slot
    /// the value is bound for plays no part; see [`Self::check_member`].
    pub(crate) fn hydrate_host_value(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
    ) -> Result<Value, Refusal> {
        self.hydrate_from(Peer::Host, holder, value)
    }

    /// Build a runtime operation's result on the heap as the value it says it
    /// is: the runtime builds instances that hold its own data by their
    /// fields.
    pub(crate) fn land_op_result(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
    ) -> Result<Value, Refusal> {
        self.hydrate_from(Peer::Runtime, holder, value)
    }

    /// The entry point for values `peer` sent: a host's, or the runtime's.
    pub(crate) fn hydrate_from(
        &self,
        peer: Peer,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
    ) -> Result<Value, Refusal> {
        let mut hydrator = Hydrator {
            engine: self,
            holder,
            peer,
            path: Vec::new(),
            members: HashMap::new(),
        };
        hydrator.value(value)
    }

    /// Whether a value on the heap is a member of `slot`; the refusal names
    /// the value's own type.
    pub(crate) fn check_member(
        holder: &impl HeapPermit<BexThread>,
        value: Value,
        slot: &RealizedTy,
    ) -> Result<(), Refusal> {
        let vm = &holder.holder().vm;
        if is_member(vm, value, slot.as_ty()) {
            Ok(())
        } else {
            Err(Refusal {
                path: Vec::new(),
                kind: RefusalKind::NotAMember {
                    found: concrete_ty_name(vm, value),
                    expected: Bounded::ty(slot.to_string()),
                },
            })
        }
    }

    /// [`Self::hydrate_host_value`], then [`Self::check_member`] against
    /// `slot`.
    pub(crate) fn hydrate_host_value_into(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        slot: &RealizedTy,
    ) -> Result<Value, Refusal> {
        let built = self.hydrate_host_value(holder, value)?;
        Self::check_member(holder, built, slot)?;
        Ok(built)
    }

    /// [`Self::land_op_result`], then [`Self::check_member`] against `slot`.
    pub(crate) fn land_op_result_into(
        &self,
        holder: &mut impl HeapPermit<BexThread>,
        value: &BexExternalValue,
        slot: &RealizedTy,
    ) -> Result<Value, Refusal> {
        let built = self.land_op_result(holder, value)?;
        Self::check_member(holder, built, slot)?;
        Ok(built)
    }

    /// The concrete type of a value on the heap, as the type system reads it
    /// for inference and membership: `int` for `1`, never the literal `1`.
    /// `None` for a value with no BAML type of its own (a compile-time
    /// definition object).
    pub(crate) fn landed_ty(vm: &BexVm, value: Value) -> Option<RuntimeTy> {
        vm.value_concrete_ty(value)
            .map(|concrete| RuntimeTy::from(&RealizedTy::from(concrete)))
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
#[derive(Clone)]
pub(crate) enum HeldTy {
    /// Every declaration it names was compiled with the program, and none
    /// moves: the type is held as it is.
    Pinned(RealizedTy),
    /// It names a runtime declaration, rooted for the span of the await.
    Rooted(baml_type::RealizedTy<RootedHead>),
}

impl BexEngine {
    /// Hold `ty` across an await, rooting every runtime declaration it names.
    pub(crate) fn hold_ty(&self, ty: RealizedTy) -> HeldTy {
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
    ) -> Result<RealizedTy, EngineError> {
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
}
