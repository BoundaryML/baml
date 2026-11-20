//! BAML's types as `btel-core` describes them. `btel_core::cas::encode` is the one encoder of a
//! type description, a declaration's name and a head; what is here answers what it asks of
//! BAML's own types, borrowing them, so describing a type copies nothing.
//!
//! A value's type names its classes and enums by [`TypeIdentity`] ([`ValueHead`]). A recorded
//! definition's field type names them by [`Head`] ([`FieldHead`]), and is the only place a
//! projection or a type argument reference is written.

use std::{iter::Map, slice};

use baml_type::{
    DeclarationName, Freshness, FunctionParamMode, Literal, MediaKind, Package, RealizedTy,
    TyTemplate,
};
use btel_core::cas::{
    self, DefinitionRef,
    encode::{self, Describe, FieldHead, Interface, Kind, Param, ValueHead},
};
use btel_types::Definition;

use crate::{TypeIdentity, definition::Head};

/// The segments of a declaration's namespace.
pub(crate) type Segments<'a> = Map<slice::Iter<'a, baml_type::Name>, fn(&baml_type::Name) -> &str>;

/// A declaration's name.
pub(crate) fn name(name: &DeclarationName) -> encode::Name<'_, Segments<'_>> {
    match name {
        DeclarationName::Declared(declared) => encode::Name::Declared {
            package: match declared.key() {
                Package::Local => encode::Package::Local,
                Package::Dep(dep) => encode::Package::Dep(dep.as_str()),
            },
            namespace: declared.namespace().iter().map(baml_type::Name::as_str),
            name: declared.name().as_str(),
        },
        DeclarationName::Anonymous(anonymous) => encode::Name::Anonymous(anonymous.as_str()),
    }
}

/// The definition `definition` is: its group's blob and its position there.
pub(crate) fn definition_ref(definition: &Definition) -> DefinitionRef {
    DefinitionRef {
        group: cas::CasId::from_bytes(definition.group.id()),
        member: definition.member,
    }
}

/// How a value's type names a class, enum, interface or alias.
pub(crate) fn value_head(identity: &TypeIdentity) -> ValueHead<'_, Segments<'_>> {
    match identity {
        TypeIdentity::Resolved(head) => ValueHead::Resolved(head.tag().as_i64(), name(head.name())),
        TypeIdentity::Unresolved(tag) => ValueHead::Unresolved(tag.as_i64()),
        TypeIdentity::Defined(definition) => ValueHead::Defined(*definition),
    }
}

fn media(kind: MediaKind) -> cas::MediaKind {
    match kind {
        MediaKind::Image => cas::MediaKind::Image,
        MediaKind::Audio => cas::MediaKind::Audio,
        MediaKind::Video => cas::MediaKind::Video,
        MediaKind::Pdf => cas::MediaKind::Pdf,
        MediaKind::Generic => cas::MediaKind::Generic,
    }
}

fn freshness(freshness: Freshness) -> cas::Freshness {
    match freshness {
        Freshness::Fresh => cas::Freshness::Fresh,
        Freshness::Regular => cas::Freshness::Regular,
    }
}

fn literal(literal: &Literal) -> encode::Literal<'_, Vec<u8>> {
    match literal {
        Literal::Int(value) => encode::Literal::Int(*value),
        Literal::Bigint(value) => encode::Literal::Bigint(value.to_signed_bytes_le()),
        Literal::Float(text) => encode::Literal::Float(text),
        Literal::String(text) => encode::Literal::String(text),
        Literal::Bool(value) => encode::Literal::Bool(*value),
    }
}

// A value's type.

type ValueType = RealizedTy<TypeIdentity>;
type ValueParam = baml_type::RealizedFunctionParamTy<TypeIdentity>;

/// A value's type, as `btel-core` asks to have it described.
#[derive(Clone, Copy)]
pub(crate) struct Value<'a>(pub(crate) &'a ValueType);

fn value(ty: &ValueType) -> Value<'_> {
    Value(ty)
}

fn value_binding((name, ty): &(baml_type::Name, ValueType)) -> (&str, Value<'_>) {
    (name.as_str(), Value(ty))
}

fn value_param(param: &ValueParam) -> Param<'_, Value<'_>> {
    Param {
        name: param.name.as_ref().map(baml_type::Name::as_str),
        ty: Value(&param.ty),
        optional: param.mode == FunctionParamMode::Optional,
    }
}

impl<'a> Describe<'a> for Value<'a> {
    type Head = ValueHead<'a, Segments<'a>>;
    type Bigint = Vec<u8>;
    type Types = Map<slice::Iter<'a, ValueType>, fn(&'a ValueType) -> Value<'a>>;
    type Bindings = Map<
        slice::Iter<'a, (baml_type::Name, ValueType)>,
        fn(&'a (baml_type::Name, ValueType)) -> (&'a str, Value<'a>),
    >;
    type Params = Map<slice::Iter<'a, ValueParam>, fn(&'a ValueParam) -> Param<'a, Value<'a>>>;

    fn kind(self) -> Kind<'a, Self> {
        use RealizedTy as T;
        match self.0 {
            T::Int => Kind::Int,
            T::Bigint => Kind::Bigint,
            T::Float => Kind::Float,
            T::String => Kind::String,
            T::Bool => Kind::Bool,
            T::Null => Kind::Null,
            T::Uint8Array => Kind::Uint8Array,
            T::Media(kind) => Kind::Media(media(*kind)),
            T::Literal(literal_value, fresh) => {
                Kind::Literal(literal(literal_value), freshness(*fresh))
            }
            T::Class(head, arguments) => {
                Kind::Class(value_head(head), arguments.iter().map(value as fn(_) -> _))
            }
            T::Interface(head, generics, bindings) => Kind::Interface(Interface {
                head: value_head(head),
                generics: generics.iter().map(value as fn(_) -> _),
                bindings: bindings.iter().map(value_binding as fn(_) -> _),
            }),
            T::Enum(head) => Kind::Enum(value_head(head)),
            T::EnumVariant(head, variant) => Kind::EnumVariant(value_head(head), variant.as_str()),
            T::List(item) => Kind::List(Value(item)),
            T::Map { key, value } => Kind::Map {
                key: Value(key),
                value: Value(value),
            },
            T::Union(members) => Kind::Union(members.iter().map(value as fn(_) -> _)),
            T::Tuple(elements) => Kind::Tuple(elements.iter().map(value as fn(_) -> _)),
            T::Function {
                params,
                ret,
                throws,
            } => Kind::Function {
                params: params.iter().map(value_param as fn(_) -> _),
                ret: Value(ret),
                throws: Value(throws),
            },
            T::Future(output, error) => Kind::Future(Value(output), Value(error)),
            T::Type => Kind::Type,
            T::Resource => Kind::Resource,
            T::PromptAst => Kind::PromptAst,
            T::TypeAlias(head) => Kind::TypeAlias(value_head(head)),
            T::Unknown => Kind::Unknown,
            T::Never => Kind::Never,
        }
    }
}

// A recorded definition's field type.

/// Where a field type's references to other members of its group point: their positions in the
/// blob, or, while the group's start is chosen, nowhere (`u32::MAX`), since every member is
/// then the same.
#[derive(Clone, Copy)]
pub(crate) enum Placement<'p> {
    Unplaced,
    At(&'p [u32]),
}

impl Placement<'_> {
    fn position(self, member: u32) -> u32 {
        match self {
            Self::At(positions) => positions[member as usize],
            Self::Unplaced => u32::MAX,
        }
    }
}

type FieldTemplate<'a> = TyTemplate<Head<'a>>;
type FieldParam<'a> = baml_type::TyTemplateFunctionParamTy<Head<'a>>;

/// A field type, with where its member references point.
#[derive(Clone, Copy)]
pub(crate) struct Placed<'a> {
    pub(crate) ty: &'a FieldTemplate<'a>,
    pub(crate) at: Placement<'a>,
}

/// What a field type names: another member by its position, or a declaration by name.
fn field_head<'a>(head: &'a Head<'a>, at: Placement<'_>) -> FieldHead<'a, Segments<'a>> {
    match head {
        Head::Member(member) => FieldHead::Member(at.position(*member)),
        Head::Defined(declared, definition) => {
            FieldHead::Defined(name(declared), definition_ref(definition))
        }
        Head::Named(declared) => FieldHead::Named(name(declared)),
    }
}

/// Each of a sequence of a field type's parts, with the same placement.
pub(crate) struct Each<'a, T> {
    items: slice::Iter<'a, T>,
    at: Placement<'a>,
}

impl<'a> Iterator for Each<'a, FieldTemplate<'a>> {
    type Item = Placed<'a>;
    fn next(&mut self) -> Option<Placed<'a>> {
        let at = self.at;
        self.items.next().map(|ty| Placed { ty, at })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}
impl<'a> ExactSizeIterator for Each<'a, FieldTemplate<'a>> {}

impl<'a> Iterator for Each<'a, (baml_type::Name, FieldTemplate<'a>)> {
    type Item = (&'a str, Placed<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        let at = self.at;
        self.items
            .next()
            .map(|(name, ty)| (name.as_str(), Placed { ty, at }))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}
impl<'a> ExactSizeIterator for Each<'a, (baml_type::Name, FieldTemplate<'a>)> {}

impl<'a> Iterator for Each<'a, FieldParam<'a>> {
    type Item = Param<'a, Placed<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        let at = self.at;
        self.items.next().map(|param| Param {
            name: param.name.as_ref().map(baml_type::Name::as_str),
            ty: Placed { ty: &param.ty, at },
            optional: param.mode == FunctionParamMode::Optional,
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.items.size_hint()
    }
}
impl<'a> ExactSizeIterator for Each<'a, FieldParam<'a>> {}

impl<'a> Placed<'a> {
    fn each<T>(self, items: &'a [T]) -> Each<'a, T> {
        Each {
            items: items.iter(),
            at: self.at,
        }
    }
    fn at(self, ty: &'a FieldTemplate<'a>) -> Self {
        Self { ty, at: self.at }
    }
    fn interface(
        self,
        interface: &'a baml_type::TyTemplateInterface<Head<'a>>,
    ) -> Interface<'a, Self> {
        Interface {
            head: field_head(&interface.name, self.at),
            generics: self.each(&interface.generics),
            bindings: self.each(&interface.associated_types),
        }
    }
}

impl<'a> Describe<'a> for Placed<'a> {
    type Head = FieldHead<'a, Segments<'a>>;
    type Bigint = Vec<u8>;
    type Types = Each<'a, FieldTemplate<'a>>;
    type Bindings = Each<'a, (baml_type::Name, FieldTemplate<'a>)>;
    type Params = Each<'a, FieldParam<'a>>;

    fn kind(self) -> Kind<'a, Self> {
        use TyTemplate as T;
        let head = |head: &'a Head<'a>| field_head(head, self.at);
        match self.ty {
            T::Int => Kind::Int,
            T::Bigint => Kind::Bigint,
            T::Float => Kind::Float,
            T::String => Kind::String,
            T::Bool => Kind::Bool,
            T::Null => Kind::Null,
            T::Uint8Array => Kind::Uint8Array,
            T::Media(kind) => Kind::Media(media(*kind)),
            T::Literal(value, fresh) => Kind::Literal(literal(value), freshness(*fresh)),
            T::Class(name, arguments) => Kind::Class(head(name), self.each(arguments)),
            T::Interface(name, generics, bindings) => Kind::Interface(Interface {
                head: head(name),
                generics: self.each(generics),
                bindings: self.each(bindings),
            }),
            T::Enum(name) => Kind::Enum(head(name)),
            T::EnumVariant(name, variant) => Kind::EnumVariant(head(name), variant.as_str()),
            T::List(item) => Kind::List(self.at(item)),
            T::Map { key, value } => Kind::Map {
                key: self.at(key),
                value: self.at(value),
            },
            T::Union(members) => Kind::Union(self.each(members)),
            T::Tuple(elements) => Kind::Tuple(self.each(elements)),
            T::Function {
                params,
                ret,
                throws,
            } => Kind::Function {
                params: self.each(params),
                ret: self.at(ret),
                throws: self.at(throws),
            },
            T::Future(value, error) => Kind::Future(self.at(value), self.at(error)),
            T::Type => Kind::Type,
            T::Resource => Kind::Resource,
            T::PromptAst => Kind::PromptAst,
            T::TypeAlias(name) => Kind::TypeAlias(head(name)),
            T::AssociatedTypeProjection {
                base,
                interface,
                member,
            } => Kind::AssociatedTypeProjection {
                base: self.at(base),
                interface: self.interface(interface),
                member: member.as_str(),
                field: (),
            },
            T::Unknown => Kind::Unknown,
            T::Never => Kind::Never,
            T::TypeArgRef(index) => Kind::TypeArgRef(*index, ()),
        }
    }
}

/// A type's description as `btel-core` writes it: what a decoded description's `encoded` holds.
#[cfg(test)]
pub(crate) fn described(ty: &crate::OwnedType) -> Box<[u8]> {
    let mut out = Vec::new();
    encode::infallible(encode::encode_type(Value(ty), &mut out));
    out.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use baml_type::{TaggedTypeName, TypeName, typetag::TypeTag};

    use super::*;

    fn written(head: ValueHead<'_, Segments<'_>>) -> String {
        use encode::Head;
        let mut out = Vec::new();
        encode::infallible(head.write(&mut out));
        out.iter().fold(String::new(), |mut hex, byte| {
            use std::fmt::Write as _;
            write!(hex, "{byte:02x}").expect("writing to a string");
            hex
        })
    }

    /// A type's head is written as blob format 3 first wrote it.
    #[test]
    fn nominal_identity_tags_preserve_v3_bytes() {
        let tag = TypeTag::from_i64(42);
        let name = DeclarationName::Declared(TypeName::local("Golden".into()));
        assert_eq!(
            written(value_head(&TypeIdentity::Unresolved(tag))),
            "002a00000000000000"
        );
        let resolved = TypeIdentity::Resolved(TaggedTypeName::new(tag, name));
        assert_eq!(
            written(value_head(&resolved)),
            "012a0000000000000000000000000006000000476f6c64656e"
        );
    }
}
