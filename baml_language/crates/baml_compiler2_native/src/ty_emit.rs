//! Types and interface rules written into the executable's data as the
//! runtime's copies of them hold them (`baml_native_rt/src/ty`): a
//! `TyTemplate`, a `RealizedTy` or an `ImplRule` with every child static.
//!
//! The runtime's type files are included here by path, as `layout.rs` is, so
//! the sizes, the discriminants and the struct field offsets are the ones the
//! runtime reads. The conversion from `baml_type`'s types is a `match` with
//! no catch-all arm: a variant added to the original stops this from building
//! until the copy has it.
//!
//! Every child list or box is an anonymous data object of its own, and the
//! word that points at it a relocation. A list that is empty still points at
//! a data object, since the runtime makes a slice of the address.

#![allow(
    dead_code,
    reason = "nothing writes types into a program yet: codegen calls in at a later stage"
)]

use std::mem::{align_of, offset_of, size_of};

use baml_base::Name;
use baml_compiler2_mir::{
    RealizedTy as MirRealizedTy, RuntimeInterfaceBound, TyTemplate as MirTyTemplate,
    TyTemplateInterface,
};
use baml_type::DeclName;
use cranelift_module::{DataDescription, DataId, Module};

use crate::{CompileError, layout};

/// The runtime's type files, read here for their layouts.
#[path = "../../baml_native_rt/src/ty/core_types.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    unsafe_code,
    clippy::doc_markdown,
    reason = "the runtime's file: this reads only its layouts"
)]
mod core_types;
#[path = "../../baml_native_rt/src/ty/data.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    unsafe_code,
    clippy::doc_markdown,
    reason = "the runtime's file: this reads only its layouts"
)]
mod data;
#[path = "../../baml_native_rt/src/ty/family.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    unsafe_code,
    clippy::doc_markdown,
    reason = "the runtime's file: this reads only its layouts"
)]
mod family;
#[path = "../../baml_native_rt/src/ty/freshness.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    clippy::doc_markdown,
    reason = "the runtime's file: this reads only its layouts"
)]
mod freshness;
#[path = "../../baml_native_rt/src/ty/interface.rs"]
#[allow(
    dead_code,
    unreachable_pub,
    clippy::doc_markdown,
    reason = "the runtime's file: this reads only its layouts"
)]
mod interface;

#[allow(unused_imports, reason = "the included files name these as `super::`")]
use self::{
    core_types::{Literal, MediaKind},
    data::{
        AssocBinding, AssocIndex, ClassObject, CowBox, CowSlice, EnumVariantObject,
        InterfaceObject, PackageObject, RawFunctionObject, SlotIndex, Text, TypeObject,
    },
    family::{FunctionParamTy, RealizedTy, TyTemplate},
    freshness::{Freshness, FunctionParamMode},
    interface::{ImplRule, InterfaceBound, MethodImpl},
};

/// Every `RealizedTy` is a `TyTemplate` with the same bytes: the same
/// discriminants, and children of the same size.
const _: () = assert!(size_of::<RealizedTy>() == size_of::<TyTemplate>());
const _: () = assert!(align_of::<RealizedTy>() == align_of::<TyTemplate>());

/// The discriminant of each variant of the runtime's `TyTemplate` (and of
/// `RealizedTy`, which has the same ones): the original's.
mod tag {
    pub(super) const INT: u8 = 0;
    pub(super) const BIGINT: u8 = 1;
    pub(super) const FLOAT: u8 = 2;
    pub(super) const STRING: u8 = 3;
    pub(super) const BOOL: u8 = 4;
    pub(super) const NULL: u8 = 5;
    pub(super) const UINT8ARRAY: u8 = 6;
    pub(super) const MEDIA: u8 = 7;
    pub(super) const LITERAL: u8 = 8;
    pub(super) const CLASS: u8 = 9;
    pub(super) const INTERFACE: u8 = 10;
    pub(super) const ENUM: u8 = 11;
    pub(super) const ENUM_VARIANT: u8 = 12;
    pub(super) const LIST: u8 = 13;
    pub(super) const MAP: u8 = 14;
    pub(super) const UNION: u8 = 15;
    pub(super) const FUNCTION: u8 = 16;
    pub(super) const FUTURE: u8 = 17;
    pub(super) const RUST_TYPE: u8 = 18;
    pub(super) const TYPE: u8 = 19;
    pub(super) const RESOURCE: u8 = 20;
    pub(super) const PROMPT_AST: u8 = 21;
    pub(super) const TYPE_ALIAS: u8 = 24;
    pub(super) const ASSOCIATED_TYPE_PROJECTION: u8 = 26;
    pub(super) const UNKNOWN: u8 = 27;
    pub(super) const NEVER: u8 = 28;
    pub(super) const TYPE_ARG_REF: u8 = 34;
    pub(super) const TUPLE: u8 = 38;
}

/// The discriminants of the runtime's `Literal`, in its declaration order.
mod literal_tag {
    pub(super) const INT: u8 = 0;
    pub(super) const BIGINT: u8 = 1;
    pub(super) const FLOAT: u8 = 2;
    pub(super) const STRING: u8 = 3;
    pub(super) const BOOL: u8 = 4;
}

/// Size and alignment of a field, for [`variant_fields`].
const POINTER: (usize, usize) = (size_of::<usize>(), align_of::<usize>());
const SLICE: (usize, usize) = (size_of::<CowSlice<u8>>(), align_of::<CowSlice<u8>>());
const BOX: (usize, usize) = (size_of::<CowBox<u8>>(), align_of::<CowBox<u8>>());
const INDEX: (usize, usize) = (size_of::<AssocIndex>(), align_of::<AssocIndex>());
const BYTE: (usize, usize) = (1, 1);
const LITERAL: (usize, usize) = (size_of::<Literal>(), align_of::<Literal>());

/// Where the fields of one variant of a `#[repr(C, u8)]` enum of alignment
/// `align` are: such an enum is laid out as a `#[repr(C)]` struct of the
/// `u8` tag and a union of one `#[repr(C)]` struct per variant, so the
/// fields follow each other from the union's offset, which is the enum's
/// alignment.
fn variant_fields(align: usize, fields: &[(usize, usize)]) -> Vec<usize> {
    let mut at = align;
    fields
        .iter()
        .map(|&(size, field_align)| {
            at = at.next_multiple_of(field_align);
            let offset = at;
            at += size;
            offset
        })
        .collect()
}

/// Where a declaration a type names is in the program's data. The caller
/// knows its objects; this asks it for each head.
pub(crate) trait Heads {
    /// The class object of a class or an enum.
    fn class(&mut self, head: &DeclName) -> Result<DataId, CompileError>;
    /// The interface object of an interface.
    fn interface(&mut self, head: &DeclName) -> Result<DataId, CompileError>;
    /// The object of variant `variant` of enum `head`: a data object, and
    /// the variant's offset in it.
    fn variant(&mut self, head: &DeclName, variant: &Name) -> Result<(DataId, i64), CompileError>;
    /// The type object of what the recursive alias `head` stands for.
    fn alias(&mut self, head: &DeclName) -> Result<DataId, CompileError>;
    /// The position of associated type `member` in the declaration of
    /// interface `interface`.
    fn assoc(&mut self, interface: &DeclName, member: &Name) -> Result<u32, CompileError>;
}

/// One data object being written: its bytes and its relocations.
struct Placed {
    bytes: Vec<u8>,
    description: DataDescription,
}

impl Placed {
    fn new(size: usize) -> Self {
        Self {
            // Never empty: a symbol needs a byte to be the address of.
            bytes: vec![0; size.max(layout::VALUE)],
            description: DataDescription::new(),
        }
    }

    fn put(&mut self, at: usize, value: &[u8]) {
        self.bytes[at..at + value.len()].copy_from_slice(value);
    }
}

/// The parts of one `implement ... for ...` an `ImplRule` is written from.
pub(crate) struct RuleParts<'a> {
    /// The package object of the package that declared the rule.
    pub(crate) owner: DataId,
    /// The interface the rule implements.
    pub(crate) interface: &'a DeclName,
    pub(crate) for_ty_pattern: &'a MirTyTemplate,
    pub(crate) generic_param_bounds: &'a [Vec<RuntimeInterfaceBound>],
    pub(crate) interface_args: &'a [MirTyTemplate],
    pub(crate) interface_assoc: &'a [(Name, MirTyTemplate)],
    /// Per method of the interface, in its declaration order: the function
    /// object of the one the rule provides and the frame it was compiled
    /// against, or `None` for one it adopts.
    pub(crate) methods: &'a [Option<(DataId, Vec<MirTyTemplate>)>],
    /// Per field of the interface: the implementor's slot.
    pub(crate) field_links: &'a [u32],
}

/// Writes types and rules into a module's data.
pub(crate) struct TyEmitter<'m, M: Module, H: Heads> {
    module: &'m mut M,
    heads: &'m mut H,
}

impl<'m, M: Module, H: Heads> TyEmitter<'m, M, H> {
    pub(crate) fn new(module: &'m mut M, heads: &'m mut H) -> Self {
        Self { module, heads }
    }

    /// A data object holding the template `ty`.
    pub(crate) fn template_object(&mut self, ty: &MirTyTemplate) -> Result<DataId, CompileError> {
        let mut placed = Placed::new(size_of::<TyTemplate>());
        self.template(ty, &mut placed, 0)?;
        self.define(placed)
    }

    /// A data object holding the realized type `ty`.
    pub(crate) fn realized_object(&mut self, ty: &MirRealizedTy) -> Result<DataId, CompileError> {
        // Every realized type is a template of the same bytes.
        self.template_object(&MirTyTemplate::from(ty.clone()))
    }

    /// The data object of an interface's static rules: `rules`, one after
    /// another, as the slice `InterfaceObject::static_rules` points at.
    pub(crate) fn rules_object(&mut self, rules: &[RuleParts<'_>]) -> Result<DataId, CompileError> {
        let size = size_of::<ImplRule>();
        let mut placed = Placed::new(rules.len() * size);
        for (index, rule) in rules.iter().enumerate() {
            self.rule(rule, &mut placed, index * size)?;
        }
        self.define(placed)
    }

    fn define(&mut self, mut placed: Placed) -> Result<DataId, CompileError> {
        let id = self
            .module
            .declare_anonymous_data(false, false)
            .map_err(internal)?;
        placed.description.define(placed.bytes.into_boxed_slice());
        placed.description.set_align(layout::VALUE as u64);
        self.module
            .define_data(id, &placed.description)
            .map_err(internal)?;
        Ok(id)
    }

    /// Write the address of `target`, plus `addend`, at `at`.
    fn address(
        &mut self,
        placed: &mut Placed,
        at: usize,
        target: DataId,
        addend: i64,
    ) -> Result<(), CompileError> {
        let target = self
            .module
            .declare_data_in_data(target, &mut placed.description);
        let at = u32::try_from(at).map_err(internal)?;
        placed.description.write_data_addr(at, target, addend);
        Ok(())
    }

    /// Write a static `CowSlice` at `at` of `count` items of `size` bytes,
    /// each written by `item` at its offset in a data object of its own.
    fn slice<T>(
        &mut self,
        placed: &mut Placed,
        at: usize,
        items: &[T],
        size: usize,
        mut item: impl FnMut(&mut Self, &T, &mut Placed, usize) -> Result<(), CompileError>,
    ) -> Result<(), CompileError> {
        let mut children = Placed::new(items.len() * size);
        for (index, child) in items.iter().enumerate() {
            item(self, child, &mut children, index * size)?;
        }
        let children = self.define(children)?;
        self.address(placed, at + offset_of!(CowSlice<u8>, ptr), children, 0)?;
        let len = u32::try_from(items.len()).map_err(internal)?;
        placed.put(at + offset_of!(CowSlice<u8>, len), &len.to_ne_bytes());
        // `owned` stays zero: the items are static.
        Ok(())
    }

    /// Write a static `CowBox` of one template at `at`.
    fn boxed(
        &mut self,
        placed: &mut Placed,
        at: usize,
        ty: &MirTyTemplate,
    ) -> Result<(), CompileError> {
        let mut child = Placed::new(size_of::<TyTemplate>());
        self.template(ty, &mut child, 0)?;
        let child = self.define(child)?;
        self.address(placed, at + offset_of!(CowBox<u8>, ptr), child, 0)
    }

    /// Write static `Text` at `at`.
    fn text(&mut self, placed: &mut Placed, at: usize, text: &str) -> Result<(), CompileError> {
        self.slice(placed, at, text.as_bytes(), 1, |_, byte, children, at| {
            children.put(at, &[*byte]);
            Ok(())
        })
    }

    /// Write a list of templates at `at`.
    fn templates(
        &mut self,
        placed: &mut Placed,
        at: usize,
        tys: &[MirTyTemplate],
    ) -> Result<(), CompileError> {
        self.slice(placed, at, tys, size_of::<TyTemplate>(), Self::template)
    }

    /// Write associated-type bindings of `interface` at `at`, in the order of
    /// their positions.
    fn bindings(
        &mut self,
        placed: &mut Placed,
        at: usize,
        interface: &DeclName,
        bindings: &[(Name, MirTyTemplate)],
    ) -> Result<(), CompileError> {
        let mut positioned = bindings
            .iter()
            .map(|(member, ty)| Ok((self.heads.assoc(interface, member)?, ty)))
            .collect::<Result<Vec<_>, CompileError>>()?;
        positioned.sort_by_key(|(member, _)| *member);
        self.slice(
            placed,
            at,
            &positioned,
            size_of::<AssocBinding<TyTemplate>>(),
            |this, (member, ty), children, at| {
                children.put(
                    at + offset_of!(AssocBinding<TyTemplate>, member),
                    &member.to_ne_bytes(),
                );
                this.template(ty, children, at + offset_of!(AssocBinding<TyTemplate>, ty))
            },
        )
    }

    /// Write an `InterfaceBound` at `at`.
    fn bound(
        &mut self,
        placed: &mut Placed,
        at: usize,
        interface: &DeclName,
        args: &[MirTyTemplate],
        assoc: &[(Name, MirTyTemplate)],
    ) -> Result<(), CompileError> {
        let object = self.heads.interface(interface)?;
        self.address(
            placed,
            at + offset_of!(InterfaceBound, interface),
            object,
            0,
        )?;
        self.templates(placed, at + offset_of!(InterfaceBound, args), args)?;
        self.bindings(
            placed,
            at + offset_of!(InterfaceBound, assoc),
            interface,
            assoc,
        )
    }

    /// Write the runtime's form of `literal` at `at`.
    fn literal(
        &mut self,
        placed: &mut Placed,
        at: usize,
        literal: &baml_type::Literal,
    ) -> Result<(), CompileError> {
        let payload = at + variant_fields(align_of::<Literal>(), &[POINTER])[0];
        match literal {
            baml_type::Literal::Int(value) => {
                placed.put(at, &[literal_tag::INT]);
                placed.put(payload, &value.to_ne_bytes());
            }
            baml_type::Literal::Bigint(value) => {
                placed.put(at, &[literal_tag::BIGINT]);
                // Two's complement limbs, least significant first, as a
                // `bigint` object holds them.
                let digits = value.to_signed_bytes_le();
                let limbs = digits.len().div_ceil(layout::WORD).max(1);
                let sign = if value.sign() == num_bigint::Sign::Minus {
                    0xFF
                } else {
                    0
                };
                let mut bytes = vec![sign; limbs * layout::WORD];
                bytes[..digits.len()].copy_from_slice(&digits);
                let limbs = bytes.as_chunks::<{ layout::WORD }>().0.to_vec();
                self.slice(
                    placed,
                    payload,
                    &limbs,
                    layout::WORD,
                    |_, limb, children, at| {
                        children.put(at, limb);
                        Ok(())
                    },
                )?;
            }
            baml_type::Literal::Float(text) => {
                placed.put(at, &[literal_tag::FLOAT]);
                self.text(placed, payload, text)?;
            }
            baml_type::Literal::String(text) => {
                placed.put(at, &[literal_tag::STRING]);
                self.text(placed, payload, text)?;
            }
            baml_type::Literal::Bool(value) => {
                placed.put(at, &[literal_tag::BOOL]);
                placed.put(payload, &[u8::from(*value)]);
            }
        }
        Ok(())
    }

    /// Write a `FunctionParamTy<TyTemplate>` at `at`.
    fn param(
        &mut self,
        placed: &mut Placed,
        at: usize,
        param: &baml_type::TyTemplateFunctionParamTy<DeclName>,
    ) -> Result<(), CompileError> {
        // `None` is a null `ptr`, which the bytes already are.
        if let Some(name) = &param.name {
            self.text(
                placed,
                at + offset_of!(FunctionParamTy<TyTemplate>, name),
                name,
            )?;
        }
        self.template(
            &param.ty,
            placed,
            at + offset_of!(FunctionParamTy<TyTemplate>, ty),
        )?;
        let mode = match param.mode {
            baml_type::FunctionParamMode::Required => FunctionParamMode::Required,
            baml_type::FunctionParamMode::Optional => FunctionParamMode::Optional,
        };
        placed.put(
            at + offset_of!(FunctionParamTy<TyTemplate>, mode),
            &[mode as u8],
        );
        Ok(())
    }

    /// Write the runtime's form of the template `ty` at `at` of `placed`.
    fn template(
        &mut self,
        ty: &MirTyTemplate,
        placed: &mut Placed,
        at: usize,
    ) -> Result<(), CompileError> {
        let align = align_of::<TyTemplate>();
        let field =
            |fields: &[(usize, usize)], index: usize| at + variant_fields(align, fields)[index];
        let tag = match ty {
            MirTyTemplate::Int => tag::INT,
            MirTyTemplate::Bigint => tag::BIGINT,
            MirTyTemplate::Float => tag::FLOAT,
            MirTyTemplate::String => tag::STRING,
            MirTyTemplate::Bool => tag::BOOL,
            MirTyTemplate::Null => tag::NULL,
            MirTyTemplate::Uint8Array => tag::UINT8ARRAY,
            MirTyTemplate::Media(kind) => {
                let kind = match kind {
                    baml_type::MediaKind::Image => MediaKind::Image,
                    baml_type::MediaKind::Audio => MediaKind::Audio,
                    baml_type::MediaKind::Video => MediaKind::Video,
                    baml_type::MediaKind::Pdf => MediaKind::Pdf,
                    baml_type::MediaKind::Generic => MediaKind::Generic,
                };
                placed.put(field(&[BYTE], 0), &[kind as u8]);
                tag::MEDIA
            }
            MirTyTemplate::Literal(literal, freshness) => {
                self.literal(placed, field(&[LITERAL, BYTE], 0), literal)?;
                let freshness = match freshness {
                    baml_type::Freshness::Fresh => Freshness::Fresh,
                    baml_type::Freshness::Regular => Freshness::Regular,
                };
                placed.put(field(&[LITERAL, BYTE], 1), &[freshness as u8]);
                tag::LITERAL
            }
            MirTyTemplate::Class(head, args) => {
                let object = self.heads.class(head)?;
                self.address(placed, field(&[POINTER, SLICE], 0), object, 0)?;
                self.templates(placed, field(&[POINTER, SLICE], 1), args)?;
                tag::CLASS
            }
            MirTyTemplate::Interface(head, args, bindings) => {
                let fields = [POINTER, SLICE, SLICE];
                let object = self.heads.interface(head)?;
                self.address(placed, field(&fields, 0), object, 0)?;
                self.templates(placed, field(&fields, 1), args)?;
                self.bindings(placed, field(&fields, 2), head, bindings)?;
                tag::INTERFACE
            }
            MirTyTemplate::Enum(head) => {
                let object = self.heads.class(head)?;
                self.address(placed, field(&[POINTER], 0), object, 0)?;
                tag::ENUM
            }
            MirTyTemplate::EnumVariant(head, variant) => {
                let (object, offset) = self.heads.variant(head, variant)?;
                self.address(placed, field(&[POINTER], 0), object, offset)?;
                tag::ENUM_VARIANT
            }
            MirTyTemplate::List(inner) => {
                self.boxed(placed, field(&[BOX], 0), inner)?;
                tag::LIST
            }
            MirTyTemplate::Map { key, value } => {
                self.boxed(placed, field(&[BOX, BOX], 0), key)?;
                self.boxed(placed, field(&[BOX, BOX], 1), value)?;
                tag::MAP
            }
            MirTyTemplate::Union(members) => {
                self.templates(placed, field(&[SLICE], 0), members)?;
                tag::UNION
            }
            MirTyTemplate::Function {
                params,
                ret,
                throws,
            } => {
                let fields = [SLICE, BOX, BOX];
                self.slice(
                    placed,
                    field(&fields, 0),
                    params,
                    size_of::<FunctionParamTy<TyTemplate>>(),
                    |this, param, children, at| this.param(children, at, param),
                )?;
                self.boxed(placed, field(&fields, 1), ret)?;
                self.boxed(placed, field(&fields, 2), throws)?;
                tag::FUNCTION
            }
            MirTyTemplate::Future(value, error) => {
                self.boxed(placed, field(&[BOX, BOX], 0), value)?;
                self.boxed(placed, field(&[BOX, BOX], 1), error)?;
                tag::FUTURE
            }
            MirTyTemplate::RustType => tag::RUST_TYPE,
            MirTyTemplate::Type => tag::TYPE,
            MirTyTemplate::Resource => tag::RESOURCE,
            MirTyTemplate::PromptAst => tag::PROMPT_AST,
            MirTyTemplate::TypeAlias(head) => {
                let object = self.heads.alias(head)?;
                self.address(placed, field(&[POINTER], 0), object, 0)?;
                tag::TYPE_ALIAS
            }
            MirTyTemplate::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => {
                let fields = [BOX, BOX, INDEX];
                self.boxed(placed, field(&fields, 0), base)?;
                self.projection_qualifier(placed, field(&fields, 1), interface)?;
                let member = self.heads.assoc(&interface.name, member)?;
                placed.put(field(&fields, 2), &member.to_ne_bytes());
                tag::ASSOCIATED_TYPE_PROJECTION
            }
            MirTyTemplate::Unknown => tag::UNKNOWN,
            MirTyTemplate::Never => tag::NEVER,
            MirTyTemplate::TypeArgRef(index) => {
                placed.put(field(&[INDEX], 0), &index.to_ne_bytes());
                tag::TYPE_ARG_REF
            }
            MirTyTemplate::Tuple(elements) => {
                self.templates(placed, field(&[SLICE], 0), elements)?;
                tag::TUPLE
            }
        };
        placed.put(at, &[tag]);
        Ok(())
    }

    /// Write a static `CowBox<InterfaceBound>` of a projection's qualifier at `at`.
    fn projection_qualifier(
        &mut self,
        placed: &mut Placed,
        at: usize,
        interface: &TyTemplateInterface,
    ) -> Result<(), CompileError> {
        let mut child = Placed::new(size_of::<InterfaceBound>());
        self.bound(
            &mut child,
            0,
            &interface.name,
            &interface.generics,
            &interface.associated_types,
        )?;
        let child = self.define(child)?;
        self.address(placed, at + offset_of!(CowBox<u8>, ptr), child, 0)
    }

    /// Write an `ImplRule` at `at`.
    fn rule(
        &mut self,
        rule: &RuleParts<'_>,
        placed: &mut Placed,
        at: usize,
    ) -> Result<(), CompileError> {
        self.address(placed, at + offset_of!(ImplRule, owner), rule.owner, 0)?;
        self.template(
            rule.for_ty_pattern,
            placed,
            at + offset_of!(ImplRule, for_ty_pattern),
        )?;
        self.slice(
            placed,
            at + offset_of!(ImplRule, generic_param_bounds),
            rule.generic_param_bounds,
            size_of::<CowSlice<InterfaceBound>>(),
            |this, bounds, children, at| {
                this.slice(
                    children,
                    at,
                    bounds,
                    size_of::<InterfaceBound>(),
                    |this, bound, children, at| {
                        this.bound(children, at, &bound.interface, &bound.args, &bound.assoc)
                    },
                )
            },
        )?;
        self.templates(
            placed,
            at + offset_of!(ImplRule, interface_args),
            rule.interface_args,
        )?;
        self.bindings(
            placed,
            at + offset_of!(ImplRule, interface_assoc),
            rule.interface,
            rule.interface_assoc,
        )?;
        self.slice(
            placed,
            at + offset_of!(ImplRule, methods),
            rule.methods,
            size_of::<Option<MethodImpl>>(),
            |this, method, children, at| {
                // `None` is a null `function`, which the bytes already are.
                let Some((function, frame)) = method else {
                    return Ok(());
                };
                this.address(
                    children,
                    at + offset_of!(MethodImpl, function),
                    *function,
                    0,
                )?;
                this.templates(children, at + offset_of!(MethodImpl, frame), frame)
            },
        )?;
        self.slice(
            placed,
            at + offset_of!(ImplRule, field_links),
            rule.field_links,
            size_of::<SlotIndex>(),
            |_, slot, children, at| {
                children.put(at, &slot.to_ne_bytes());
                Ok(())
            },
        )
    }
}

fn internal(error: impl std::fmt::Display) -> CompileError {
    CompileError::Codegen(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use super::*;

    /// The first byte of a `#[repr(C, u8)]` value is its discriminant.
    #[expect(unsafe_code, reason = "reads the discriminant byte of a runtime type")]
    fn tag_of<T>(value: &T) -> u8 {
        // SAFETY: every enum read here is `#[repr(C, u8)]`.
        unsafe { *std::ptr::from_ref(value).cast::<u8>() }
    }

    /// The offset of `field` in `value`.
    fn offset<T, F>(value: &T, field: &F) -> usize {
        std::ptr::from_ref(field) as usize - std::ptr::from_ref(value) as usize
    }

    fn head<T>() -> NonNull<T> {
        NonNull::dangling()
    }

    #[test]
    fn the_tags_are_the_runtime_types_discriminants() {
        assert_eq!(tag_of(&TyTemplate::Int), tag::INT);
        assert_eq!(tag_of(&TyTemplate::Uint8Array), tag::UINT8ARRAY);
        assert_eq!(tag_of(&TyTemplate::Enum(head())), tag::ENUM);
        assert_eq!(tag_of(&TyTemplate::TypeAlias(head())), tag::TYPE_ALIAS);
        assert_eq!(tag_of(&TyTemplate::Unknown), tag::UNKNOWN);
        assert_eq!(tag_of(&TyTemplate::Never), tag::NEVER);
        assert_eq!(tag_of(&TyTemplate::TypeArgRef(0)), tag::TYPE_ARG_REF);
        assert_eq!(tag_of(&RealizedTy::PromptAst), tag::PROMPT_AST);
        assert_eq!(tag_of(&Literal::Bool(true)), literal_tag::BOOL);
        assert_eq!(tag_of(&Literal::Int(0)), literal_tag::INT);
    }

    #[test]
    fn the_variant_fields_are_where_the_runtime_types_have_them() {
        let align = align_of::<TyTemplate>();
        let function = TyTemplate::Function {
            params: CowSlice::from(vec![]),
            ret: CowBox::new(TyTemplate::Int),
            throws: CowBox::new(TyTemplate::Never),
        };
        let TyTemplate::Function {
            params,
            ret,
            throws,
        } = &function
        else {
            unreachable!("made above")
        };
        assert_eq!(
            [
                offset(&function, params),
                offset(&function, ret),
                offset(&function, throws)
            ]
            .to_vec(),
            variant_fields(align, &[SLICE, BOX, BOX])
        );
        let interface =
            TyTemplate::Interface(head(), CowSlice::from(vec![]), CowSlice::from(vec![]));
        let TyTemplate::Interface(object, args, bindings) = &interface else {
            unreachable!("made above")
        };
        assert_eq!(
            [
                offset(&interface, object),
                offset(&interface, args),
                offset(&interface, bindings)
            ]
            .to_vec(),
            variant_fields(align, &[POINTER, SLICE, SLICE])
        );
        let literal = TyTemplate::Literal(Literal::Int(1), Freshness::Regular);
        let TyTemplate::Literal(value, freshness) = &literal else {
            unreachable!("made above")
        };
        assert_eq!(
            [offset(&literal, value), offset(&literal, freshness)].to_vec(),
            variant_fields(align, &[LITERAL, BYTE])
        );
        let projection = TyTemplate::AssociatedTypeProjection {
            base: CowBox::new(TyTemplate::Int),
            interface: CowBox::new(InterfaceBound {
                interface: head(),
                args: CowSlice::from(vec![]),
                assoc: CowSlice::from(vec![]),
            }),
            member: AssocIndex(0),
        };
        let TyTemplate::AssociatedTypeProjection {
            base,
            interface,
            member,
        } = &projection
        else {
            unreachable!("made above")
        };
        assert_eq!(
            [
                offset(&projection, base),
                offset(&projection, interface),
                offset(&projection, member)
            ]
            .to_vec(),
            variant_fields(align, &[BOX, BOX, INDEX])
        );
        let bigint = Literal::Bigint(CowSlice::from(vec![1]));
        let Literal::Bigint(limbs) = &bigint else {
            unreachable!("made above")
        };
        assert_eq!(
            offset(&bigint, limbs),
            variant_fields(align_of::<Literal>(), &[POINTER])[0]
        );
    }
}
