//! Encode a `baml_type::RealizedTy` into the wire `BamlTy` (`baml_type.proto`).
//!
//! The exact inverse of [`crate::ty_decode::proto_ty_to_runtime_ty`]: where the
//! decoder turns a host-supplied `BamlTy` value into a `RealizedTy`, this turns an
//! engine `RealizedTy` into a `BamlTy` for the outbound side, total over every
//! variant. A wire type is realized, so a type variable or a projection has no
//! encoding.
//!
//! Two outbound paths share this:
//!   - the type *metadata* riding alongside container values (a list's item
//!     type, a map's key/value types, a union variant's `self_type`, and a
//!     generic instance's concrete class args) — see `value_encode.rs`;
//!   - `BamlOutboundValue.ty_value` — a reflected BAML type returned as a value
//!     (`BexExternalAdt::WireType`).
//!
//! Type names are rendered with `render_dotted(false)` (the canonical FQN that
//! keeps the implicit `user.` package) so they match the host typemap key the
//! reader resolves against (`get_class`/`get_enum`). This is the single
//! projection used for every outbound type position, including a tagged
//! handle's `BamlOutboundHandle.ty`.

use baml_type::{Literal, MediaKind, Name, RealizedTy, TypeName, WireHead, WireTy};

use crate::baml_bridge::cffi::{
    BamlTy, BamlTyAssociatedBinding, BamlTyClass, BamlTyEnum, BamlTyEnumVariant, BamlTyFunction,
    BamlTyFunctionParam, BamlTyFunctionParamMode, BamlTyFuture, BamlTyInterface, BamlTyList,
    BamlTyLiteral, BamlTyMap, BamlTyMediaKind, BamlTyOptional, BamlTyPrimitive,
    BamlTyPrimitiveKind, BamlTyTypeAlias, BamlTyUnion, baml_ty::Ty as TyVariant,
    baml_ty_literal::Literal as TyLiteralVariant,
};

/// How a nominal head is spelled on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtoHead {
    /// A compiled declaration, by name.
    Name(String),
    /// A runtime declaration, by the host's row to it.
    Live(u64),
    /// A declaration the host cannot name, by its label; display only.
    Label(String),
}

impl ProtoHead {
    fn of_name(name: &TypeName) -> Self {
        Self::Name(name.render_dotted(false))
    }
}

/// A name-headed type on the wire: every head a compiled declaration.
pub fn runtime_ty_to_proto_ty(ty: &RealizedTy) -> BamlTy {
    ty_to_proto(ty, &mut ProtoHead::of_name)
}

/// A wire type on the wire. `live` says how a runtime declaration's
/// reference is spelled: the engine mints a row for it, a host sends the row
/// it holds, an artifact can only label it.
pub fn wire_ty_to_proto_ty<L: Clone>(
    ty: &WireTy<L>,
    live: &mut impl FnMut(&L) -> ProtoHead,
) -> BamlTy {
    ty_to_proto(ty, &mut |head: &WireHead<L>| match head {
        WireHead::Static(name) => ProtoHead::of_name(name),
        WireHead::Live(reference) => live(reference),
        WireHead::Anonymous(label) => ProtoHead::Label(label.to_string()),
    })
}

fn ty_to_proto<N: Clone>(ty: &RealizedTy<N>, head: &mut impl FnMut(&N) -> ProtoHead) -> BamlTy {
    BamlTy {
        ty: Some(variant(ty, head)),
    }
}

fn primitive(kind: BamlTyPrimitiveKind) -> TyVariant {
    TyVariant::Primitive(BamlTyPrimitive { kind: kind as i32 })
}

/// Encode an interface constraint — its name, generic input arguments, and
/// associated-type bindings — into the wire `BamlTyInterface`.
fn interface_to_proto<N: Clone>(
    name: &N,
    type_args: &[RealizedTy<N>],
    bindings: &[(Name, RealizedTy<N>)],
    head: &mut impl FnMut(&N) -> ProtoHead,
) -> BamlTyInterface {
    use crate::baml_bridge::cffi::baml_ty_interface::Head as H;
    BamlTyInterface {
        head: Some(match head(name) {
            ProtoHead::Name(name) => H::Name(name),
            ProtoHead::Live(key) => H::Live(key),
            ProtoHead::Label(label) => H::AnonymousLabel(label),
        }),
        type_args: type_args
            .iter()
            .map(|t| ty_to_proto(t, &mut *head))
            .collect(),
        bindings: bindings
            .iter()
            .map(|(n, t)| BamlTyAssociatedBinding {
                name: n.as_str().to_string(),
                ty: Some(ty_to_proto(t, &mut *head)),
            })
            .collect(),
    }
}

fn variant<N: Clone>(ty: &RealizedTy<N>, head: &mut impl FnMut(&N) -> ProtoHead) -> TyVariant {
    match ty {
        RealizedTy::String => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveString),
        RealizedTy::Int => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveInt),
        RealizedTy::Float => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveFloat),
        RealizedTy::Bool => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBool),
        RealizedTy::Null => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveNull),
        RealizedTy::Uint8Array => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBytes),
        RealizedTy::Bigint => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBigint),

        RealizedTy::Class(name, args) => {
            use crate::baml_bridge::cffi::baml_ty_class::Head as H;
            TyVariant::ClassTy(BamlTyClass {
                head: Some(match head(name) {
                    ProtoHead::Name(name) => H::Name(name),
                    ProtoHead::Live(key) => H::Live(key),
                    ProtoHead::Label(label) => H::AnonymousLabel(label),
                }),
                type_args: args.iter().map(|t| ty_to_proto(t, &mut *head)).collect(),
            })
        }
        RealizedTy::TypeAlias(name) => {
            use crate::baml_bridge::cffi::baml_ty_type_alias::Head as H;
            TyVariant::TypeAlias(BamlTyTypeAlias {
                head: Some(match head(name) {
                    ProtoHead::Name(name) => H::Name(name),
                    ProtoHead::Live(key) => H::Live(key),
                    ProtoHead::Label(label) => H::AnonymousLabel(label),
                }),
                type_args: vec![],
            })
        }
        RealizedTy::Enum(name) => {
            use crate::baml_bridge::cffi::baml_ty_enum::Head as H;
            TyVariant::Enum(BamlTyEnum {
                head: Some(match head(name) {
                    ProtoHead::Name(name) => H::Name(name),
                    ProtoHead::Live(key) => H::Live(key),
                    ProtoHead::Label(label) => H::AnonymousLabel(label),
                }),
            })
        }
        RealizedTy::EnumVariant(name, variant) => {
            use crate::baml_bridge::cffi::baml_ty_enum_variant::Head as H;
            TyVariant::EnumVariant(BamlTyEnumVariant {
                head: Some(match head(name) {
                    ProtoHead::Name(name) => H::Name(name),
                    ProtoHead::Live(key) => H::Live(key),
                    ProtoHead::Label(label) => H::AnonymousLabel(label),
                }),
                variant: variant.as_str().to_string(),
            })
        }

        RealizedTy::List(inner) => TyVariant::List(Box::new(BamlTyList {
            item: Some(Box::new(ty_to_proto(inner, &mut *head))),
        })),
        RealizedTy::Map { key, value, .. } => TyVariant::Map(Box::new(BamlTyMap {
            key: Some(Box::new(ty_to_proto(key, &mut *head))),
            value: Some(Box::new(ty_to_proto(value, &mut *head))),
        })),

        // A nullable union mirrors the inbound writer's choice: `T | null`
        // (exactly one non-null member) keeps the compact `Optional` shape;
        // anything else encodes as a structural `Union` carrying every member.
        RealizedTy::Union(members) => {
            let has_null = members.iter().any(RealizedTy::is_null);
            let non_null: Vec<&RealizedTy<N>> = members.iter().filter(|m| !m.is_null()).collect();
            if has_null && non_null.len() == 1 {
                TyVariant::Optional(Box::new(BamlTyOptional {
                    inner: Some(Box::new(ty_to_proto(non_null[0], &mut *head))),
                }))
            } else {
                TyVariant::Union(BamlTyUnion {
                    options: members.iter().map(|t| ty_to_proto(t, &mut *head)).collect(),
                })
            }
        }

        RealizedTy::Literal(lit, _) => TyVariant::Literal(literal_to_proto(lit)),
        RealizedTy::Media(kind) => TyVariant::Media(crate::baml_bridge::cffi::BamlTyMedia {
            kind: media_kind_to_proto(*kind) as i32,
        }),
        RealizedTy::Interface(name, args, bindings) => {
            TyVariant::Interface(interface_to_proto(name, args, bindings, head))
        }

        // A function type: the arrow shape — params, return, throws — as a
        // callable handle states its specialized type and a host callable's
        // slot is spelled. The wire's `generic_params` stays empty rather
        // than reading the runtime variant's
        // declared-type-parameter fields.
        //
        // Deliberate decision: generic param *bounds* are intentionally not
        // encoded — `BamlTyFunction.generic_param_bounds` was removed from
        // `baml_type.proto` (its tag is now reserved). No host reader resolves
        // a function's bounds from the wire, so carrying them was dead weight;
        // if a reader ever needs them, reintroduce the field rather than
        // leaving an always-empty one here.
        RealizedTy::Function {
            params,
            ret,
            throws,
            ..
        } => TyVariant::Function(Box::new(BamlTyFunction {
            generic_params: vec![],
            params: params
                .iter()
                .map(|p| BamlTyFunctionParam {
                    name: p.name.as_ref().map(|n| n.as_str().to_string()),
                    ty: Some(ty_to_proto(&p.ty, &mut *head)),
                    mode: function_param_mode(p.mode) as i32,
                })
                .collect(),
            ret: Some(Box::new(ty_to_proto(ret, &mut *head))),
            throws: Some(Box::new(ty_to_proto(throws, &mut *head))),
        })),
        RealizedTy::Future(value, error) => TyVariant::Future(Box::new(BamlTyFuture {
            value: Some(Box::new(ty_to_proto(value, &mut *head))),
            error: Some(Box::new(ty_to_proto(error, &mut *head))),
        })),
        RealizedTy::RustType => TyVariant::RustType(crate::baml_bridge::cffi::BamlTyRustType {}),
        RealizedTy::Type => TyVariant::MetaType(crate::baml_bridge::cffi::BamlTyMetaType {}),
        RealizedTy::Resource => TyVariant::Resource(crate::baml_bridge::cffi::BamlTyResource {}),
        RealizedTy::PromptAst => TyVariant::PromptAst(crate::baml_bridge::cffi::BamlTyPromptAst {}),
        RealizedTy::Never => TyVariant::Never(crate::baml_bridge::cffi::BamlTyNever {}),
        RealizedTy::Unknown => TyVariant::Unknown(crate::baml_bridge::cffi::BamlTyUnknown {}),
    }
}

fn literal_to_proto(lit: &Literal) -> BamlTyLiteral {
    let literal = match lit {
        Literal::String(s) => TyLiteralVariant::StringValue(s.clone()),
        Literal::Int(i) => TyLiteralVariant::IntValue(*i),
        Literal::Bool(b) => TyLiteralVariant::BoolValue(*b),
        // Decimal string — `BamlTy`'s convention for bigint/float literals (the
        // proto has no fixed-width bigint scalar; a float keeps its source text).
        Literal::Bigint(n) => TyLiteralVariant::BigintValue(n.to_string()),
        Literal::Float(s) => TyLiteralVariant::FloatValue(s.clone()),
    };
    BamlTyLiteral {
        literal: Some(literal),
    }
}

fn media_kind_to_proto(kind: MediaKind) -> BamlTyMediaKind {
    match kind {
        MediaKind::Image => BamlTyMediaKind::Image,
        MediaKind::Audio => BamlTyMediaKind::Audio,
        MediaKind::Video => BamlTyMediaKind::Video,
        MediaKind::Pdf => BamlTyMediaKind::Pdf,
        MediaKind::Generic => BamlTyMediaKind::Generic,
    }
}

fn function_param_mode(mode: baml_type::FunctionParamMode) -> BamlTyFunctionParamMode {
    match mode {
        baml_type::FunctionParamMode::Required => BamlTyFunctionParamMode::Required,
        baml_type::FunctionParamMode::Optional => BamlTyFunctionParamMode::Optional,
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{Freshness, Literal, Name, RealizedTy, TypeName};

    use super::runtime_ty_to_proto_ty;
    use crate::{
        baml_bridge::cffi::{
            BamlTy, BamlTyAssociatedBinding, BamlTyAssociatedTypeProjection, BamlTyInterface,
            BamlTyList, BamlTyTypeVar, baml_ty::Ty as TyVariant,
        },
        ty_decode::proto_ty_to_runtime_ty,
    };

    /// Assert the encoder's documented "exact inverse" claim for `rt`:
    /// `decode(encode(rt)) == rt`.
    #[track_caller]
    fn assert_roundtrip(rt: &RealizedTy) {
        let proto = runtime_ty_to_proto_ty(rt);
        let back = proto_ty_to_runtime_ty(&proto).expect("decode should succeed");
        assert_eq!(&back, rt, "proto round-trip changed the type");
    }

    #[test]
    fn roundtrip_primitives_and_containers() {
        assert_roundtrip(&RealizedTy::int());
        assert_roundtrip(&RealizedTy::string());
        assert_roundtrip(&RealizedTy::list(RealizedTy::int()));
        assert_roundtrip(&RealizedTy::Map {
            key: Box::new(RealizedTy::string()),
            value: Box::new(RealizedTy::list(RealizedTy::int())),
        });
    }

    /// A wire type is realized: a type variable or a projection, at any
    /// depth, is refused by the parse rather than carried to the engine.
    #[test]
    fn unrealized_wire_types_are_refused() {
        let type_var = BamlTy {
            ty: Some(TyVariant::TypeVar(BamlTyTypeVar {
                name: "T".to_string(),
                index: 0,
            })),
        };
        let nested = BamlTy {
            ty: Some(TyVariant::List(Box::new(BamlTyList {
                item: Some(Box::new(type_var.clone())),
            }))),
        };
        let projection = BamlTy {
            ty: Some(TyVariant::AssociatedTypeProjection(Box::new(
                BamlTyAssociatedTypeProjection {
                    base: Some(Box::new(runtime_ty_to_proto_ty(&RealizedTy::int()))),
                    interface: Some(Box::new(runtime_ty_to_proto_ty(&RealizedTy::Interface(
                        TypeName::from_dotted_path("user.HasOutput"),
                        Box::new([]),
                        Box::new([]),
                    )))),
                    member: "Output".to_string(),
                },
            ))),
        };
        for (ty, refused) in [
            (&type_var, "type variable"),
            (&nested, "type variable"),
            (&projection, "associated-type projection"),
        ] {
            assert!(
                matches!(
                    proto_ty_to_runtime_ty(ty),
                    Err(crate::CtypesError::UnrealizedType { variant }) if variant == refused
                ),
                "{ty:?}"
            );
        }
    }

    #[test]
    fn roundtrip_preserves_every_literal_identity() {
        for literal in [
            Literal::String("draft".into()),
            Literal::Int(42),
            Literal::Bigint(42.into()),
            Literal::Float("1.25".into()),
            Literal::Bool(true),
        ] {
            assert_roundtrip(&RealizedTy::Literal(literal, Freshness::Regular));
        }
    }

    #[test]
    fn roundtrip_interface() {
        // Interface existential with generic args + an associated binding.
        assert_roundtrip(&RealizedTy::Interface(
            TypeName::from_dotted_path("user.Iterator"),
            Box::new([RealizedTy::int()]),
            Box::new([(Name::new("Item"), RealizedTy::string())]),
        ));
    }

    #[test]
    fn roundtrip_function() {
        assert_roundtrip(&RealizedTy::Function {
            params: Box::new([]),
            ret: Box::new(RealizedTy::int()),
            throws: Box::new(RealizedTy::Never),
        });
    }

    #[test]
    fn unsupported_type_fields_are_refused_not_ignored() {
        let alias = BamlTy {
            ty: Some(TyVariant::TypeAlias(
                crate::baml_bridge::cffi::BamlTyTypeAlias {
                    head: Some(crate::baml_bridge::cffi::baml_ty_type_alias::Head::Name(
                        "user.Tree".to_string(),
                    )),
                    type_args: vec![runtime_ty_to_proto_ty(&RealizedTy::int())],
                },
            )),
        };
        assert!(matches!(
            crate::ty_decode::proto_ty_to_runtime_ty(&alias),
            Err(crate::CtypesError::UnsupportedTypeField {
                field: "BamlTyTypeAlias.type_args"
            })
        ));
        let mut function = runtime_ty_to_proto_ty(&RealizedTy::Function {
            params: Box::new([]),
            ret: Box::new(RealizedTy::int()),
            throws: Box::new(RealizedTy::Never),
        });
        let Some(TyVariant::Function(f)) = &mut function.ty else {
            panic!("a function type encodes as a function");
        };
        f.generic_params.push("T".to_string());
        assert!(matches!(
            crate::ty_decode::proto_ty_to_runtime_ty(&function),
            Err(crate::CtypesError::UnsupportedTypeField {
                field: "BamlTyFunction.generic_params"
            })
        ));
    }

    /// The decode boundary sorts interface bindings by name (a non-BAML or future
    /// producer may send them in any order). Bindings arriving as `Z, A` must
    /// decode sorted to `A, Z`, so the constraint compares/hashes equal regardless
    /// of wire order.
    #[test]
    fn decode_sorts_interface_bindings() {
        let unsorted = BamlTy {
            ty: Some(TyVariant::Interface(BamlTyInterface {
                head: Some(crate::baml_bridge::cffi::baml_ty_interface::Head::Name(
                    "user.I".to_string(),
                )),
                type_args: vec![],
                bindings: vec![
                    BamlTyAssociatedBinding {
                        name: "Z".to_string(),
                        ty: Some(runtime_ty_to_proto_ty(&RealizedTy::int())),
                    },
                    BamlTyAssociatedBinding {
                        name: "A".to_string(),
                        ty: Some(runtime_ty_to_proto_ty(&RealizedTy::string())),
                    },
                ],
            })),
        };
        let RealizedTy::Interface(_, _, bindings) =
            proto_ty_to_runtime_ty(&unsorted).expect("decode")
        else {
            panic!("expected interface");
        };
        let names: Vec<&str> = bindings.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["A", "Z"], "decode must sort bindings by name");
    }
}
