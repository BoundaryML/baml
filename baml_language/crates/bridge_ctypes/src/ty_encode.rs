//! Encode a `baml_type::RuntimeTy` into the wire `BamlTy` (`baml_type.proto`).
//!
//! The exact inverse of [`crate::ty_decode::proto_ty_to_runtime_ty`]: where the
//! decoder turns a host-supplied `BamlTy` value into a `RuntimeTy`, this turns an
//! engine `RuntimeTy` into a `BamlTy` for the outbound side. It is a faithful, total
//! mirror of `RuntimeTy` (24 variants) — strictly more capable than the older
//! `value_encode::ty_to_field_type` (`BamlTy`), which collapsed many variants to
//! `unknown`/`unreachable!`.
//!
//! Two outbound paths share this:
//!   - the type *metadata* riding alongside container values (a list's item
//!     type, a map's key/value types, a union variant's `self_type`, and a
//!     generic instance's concrete class args) — see `value_encode.rs`;
//!   - `BamlOutboundValue.ty_value` — a reflected BAML type returned as a value
//!     (`BexExternalAdt::Type`).
//!
//! Type names are rendered with `render_dotted(false)` (the canonical FQN that
//! keeps the implicit `user.` package) so they match the host typemap key the
//! reader resolves against (`get_class`/`get_enum`). This is the single
//! projection used for every outbound type position, including a tagged
//! handle's `BamlOutboundHandle.ty`.

use baml_type::{Literal, MediaKind, Name, RuntimeTy, TypeName};

use crate::baml_bridge::cffi::{
    BamlTy, BamlTyAssociatedBinding, BamlTyAssociatedTypeProjection, BamlTyClass, BamlTyEnum,
    BamlTyEnumVariant, BamlTyFunction, BamlTyFunctionParam, BamlTyFunctionParamMode, BamlTyFuture,
    BamlTyInterface, BamlTyList, BamlTyLiteral, BamlTyMap, BamlTyMediaKind, BamlTyOptional,
    BamlTyPrimitive, BamlTyPrimitiveKind, BamlTyTypeAlias, BamlTyTypeVar, BamlTyUnion,
    baml_ty::Ty as TyVariant, baml_ty_literal::Literal as TyLiteralVariant,
};

/// Encode a `RuntimeTy` into a wire `BamlTy`.
pub fn runtime_ty_to_proto_ty(ty: &RuntimeTy) -> BamlTy {
    BamlTy {
        ty: Some(runtime_ty_to_variant(ty)),
    }
}

fn primitive(kind: BamlTyPrimitiveKind) -> TyVariant {
    TyVariant::Primitive(BamlTyPrimitive { kind: kind as i32 })
}

/// Encode an interface constraint — its name, generic input arguments, and
/// associated-type bindings — into the wire `BamlTyInterface`. Shared by the
/// `RuntimeTy::Interface` existential and an associated-type projection's
/// declaring interface (`Interface` after the interface-object refactor) so the
/// two encodings never drift.
fn interface_to_proto(
    name: &TypeName,
    type_args: &[RuntimeTy],
    bindings: &[(Name, RuntimeTy)],
) -> BamlTyInterface {
    BamlTyInterface {
        name: name.render_dotted(false),
        type_args: type_args.iter().map(runtime_ty_to_proto_ty).collect(),
        bindings: bindings
            .iter()
            .map(|(n, t)| BamlTyAssociatedBinding {
                name: n.as_str().to_string(),
                ty: Some(runtime_ty_to_proto_ty(t)),
            })
            .collect(),
    }
}

fn runtime_ty_to_variant(ty: &RuntimeTy) -> TyVariant {
    match ty {
        RuntimeTy::String { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveString),
        RuntimeTy::Int { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveInt),
        RuntimeTy::Float { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveFloat),
        RuntimeTy::Bool { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBool),
        RuntimeTy::Null { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveNull),
        RuntimeTy::Uint8Array { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBytes),
        RuntimeTy::Bigint { .. } => primitive(BamlTyPrimitiveKind::BamlTyPrimitiveBigint),

        RuntimeTy::Class(name, args, _) => TyVariant::ClassTy(BamlTyClass {
            name: name.render_dotted(false),
            type_args: args.iter().map(runtime_ty_to_proto_ty).collect(),
        }),
        RuntimeTy::TypeAlias(name, _) => TyVariant::TypeAlias(BamlTyTypeAlias {
            name: name.render_dotted(false),
            type_args: vec![],
        }),
        RuntimeTy::Enum(name, _) => TyVariant::Enum(BamlTyEnum {
            name: name.render_dotted(false),
        }),
        RuntimeTy::EnumVariant(name, variant, _) => TyVariant::EnumVariant(BamlTyEnumVariant {
            name: name.render_dotted(false),
            variant: variant.as_str().to_string(),
        }),

        RuntimeTy::List(inner, _) => TyVariant::List(Box::new(BamlTyList {
            item: Some(Box::new(runtime_ty_to_proto_ty(inner))),
        })),
        RuntimeTy::Map { key, value, .. } => TyVariant::Map(Box::new(BamlTyMap {
            key: Some(Box::new(runtime_ty_to_proto_ty(key))),
            value: Some(Box::new(runtime_ty_to_proto_ty(value))),
        })),

        // A nullable union mirrors the inbound writer's choice: `T | null`
        // (exactly one non-null member) keeps the compact `Optional` shape;
        // anything else encodes as a structural `Union` carrying every member.
        // See plan 02a §5 — the type-position host readers map a structural
        // union to a wildcard, and the members are on the wire if a future
        // reader wants true `Union[...]`.
        RuntimeTy::Union(members, _) => {
            let has_null = members.iter().any(RuntimeTy::is_null);
            let non_null: Vec<&RuntimeTy> = members.iter().filter(|m| !m.is_null()).collect();
            if has_null && non_null.len() == 1 {
                TyVariant::Optional(Box::new(BamlTyOptional {
                    inner: Some(Box::new(runtime_ty_to_proto_ty(non_null[0]))),
                }))
            } else {
                TyVariant::Union(BamlTyUnion {
                    options: members.iter().map(runtime_ty_to_proto_ty).collect(),
                })
            }
        }

        RuntimeTy::Literal(lit, _, _) => TyVariant::Literal(literal_to_proto(lit)),
        RuntimeTy::Media(kind, _) => TyVariant::Media(crate::baml_bridge::cffi::BamlTyMedia {
            kind: media_kind_to_proto(*kind) as i32,
        }),
        RuntimeTy::Interface(name, args, bindings, _) => {
            TyVariant::Interface(interface_to_proto(name, args, bindings))
        }

        // A function/arrow type. In practice a function never crosses the
        // boundary as outbound *type metadata* (the old `ty_to_field_type`
        // hit `unreachable!` here), so we encode only the always-present
        // arrow shape — params, return, throws — and leave the wire's
        // `generic_params` empty rather than reading the runtime variant's
        // declared-type-parameter fields.
        //
        // Deliberate decision: generic param *bounds* are intentionally not
        // encoded — `BamlTyFunction.generic_param_bounds` was removed from
        // `baml_type.proto` (its tag is now reserved). No host reader resolves
        // a function's bounds from the wire, so carrying them was dead weight;
        // if a reader ever needs them, reintroduce the field rather than
        // leaving an always-empty one here.
        RuntimeTy::Function {
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
                    ty: Some(runtime_ty_to_proto_ty(&p.ty)),
                    mode: function_param_mode(p.mode) as i32,
                })
                .collect(),
            ret: Some(Box::new(runtime_ty_to_proto_ty(ret))),
            throws: Some(Box::new(runtime_ty_to_proto_ty(throws))),
        })),
        RuntimeTy::Future(value, error, _) => TyVariant::Future(Box::new(BamlTyFuture {
            value: Some(Box::new(runtime_ty_to_proto_ty(value))),
            error: Some(Box::new(runtime_ty_to_proto_ty(error))),
        })),
        RuntimeTy::TypeVar(param, _) => TyVariant::TypeVar(BamlTyTypeVar {
            name: param.as_str().to_string(),
            index: param.index(),
        }),
        RuntimeTy::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => TyVariant::AssociatedTypeProjection(Box::new(BamlTyAssociatedTypeProjection {
            base: Some(Box::new(runtime_ty_to_proto_ty(base))),
            // Always present on the Rust side; the wire field stays optional for
            // backward compatibility but a projection always encodes its interface.
            interface: Some(Box::new(BamlTy {
                ty: Some(TyVariant::Interface(interface_to_proto(
                    &interface.name,
                    &interface.generics,
                    &interface.associated_types,
                ))),
            })),
            member: member.as_str().to_string(),
        })),

        RuntimeTy::RustType { .. } => {
            TyVariant::RustType(crate::baml_bridge::cffi::BamlTyRustType {})
        }
        RuntimeTy::Type { .. } => TyVariant::MetaType(crate::baml_bridge::cffi::BamlTyMetaType {}),
        RuntimeTy::Resource { .. } => {
            TyVariant::Resource(crate::baml_bridge::cffi::BamlTyResource {})
        }
        RuntimeTy::PromptAst { .. } => {
            TyVariant::PromptAst(crate::baml_bridge::cffi::BamlTyPromptAst {})
        }
        RuntimeTy::Void { .. } => TyVariant::Void(crate::baml_bridge::cffi::BamlTyVoid {}),
        RuntimeTy::Never { .. } => TyVariant::Never(crate::baml_bridge::cffi::BamlTyNever {}),
        RuntimeTy::BuiltinUnknown { .. } => {
            TyVariant::Unknown(crate::baml_bridge::cffi::BamlTyUnknown {})
        }
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
    use baml_type::{
        Freshness, Literal, Name, ParamTy, RuntimeInterface, RuntimeTy, TyAttr, TypeName,
    };

    use super::runtime_ty_to_proto_ty;
    use crate::{
        baml_bridge::cffi::{
            BamlTy, BamlTyAssociatedBinding, BamlTyInterface, baml_ty::Ty as TyVariant,
        },
        ty_decode::proto_ty_to_runtime_ty,
    };

    /// Assert the encoder's documented "exact inverse" claim for `rt`:
    /// `decode(encode(rt)) == rt`.
    #[track_caller]
    fn assert_roundtrip(rt: &RuntimeTy) {
        let proto = runtime_ty_to_proto_ty(rt);
        let back = proto_ty_to_runtime_ty(&proto).expect("decode should succeed");
        assert_eq!(&back, rt, "proto round-trip changed the type");
    }

    #[test]
    fn roundtrip_primitives_and_containers() {
        assert_roundtrip(&RuntimeTy::int());
        assert_roundtrip(&RuntimeTy::string());
        assert_roundtrip(&RuntimeTy::list(RuntimeTy::int()));
        assert_roundtrip(&RuntimeTy::map(
            RuntimeTy::string(),
            RuntimeTy::list(RuntimeTy::int()),
        ));
    }

    #[test]
    fn roundtrip_preserves_type_var_index() {
        assert_roundtrip(&RuntimeTy::TypeVar(
            ParamTy::new(7, Name::new("T")),
            TyAttr::default(),
        ));
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
            assert_roundtrip(&RuntimeTy::Literal(
                literal,
                Freshness::Regular,
                TyAttr::default(),
            ));
        }
    }

    #[test]
    fn roundtrip_interface_and_projection() {
        // Interface existential with generic args + an associated binding.
        assert_roundtrip(&RuntimeTy::Interface(
            TypeName::from_dotted_path("user.Iterator"),
            vec![RuntimeTy::int()],
            vec![(Name::new("Item"), RuntimeTy::string())],
            TyAttr::default(),
        ));
        // Projection carrying a full interface constraint (encoded as a
        // `BamlTy::Interface`, decoded back into a `RuntimeInterface`).
        assert_roundtrip(&RuntimeTy::AssociatedTypeProjection {
            base: Box::new(RuntimeTy::int()),
            interface: Box::new(RuntimeInterface {
                name: TypeName::from_dotted_path("user.Iterator"),
                generics: vec![RuntimeTy::int()],
                associated_types: vec![(Name::new("Item"), RuntimeTy::string())],
            }),
            member: Name::new("Item"),
            attr: TyAttr::default(),
        });
        // A projection through an unparameterized interface.
        assert_roundtrip(&RuntimeTy::AssociatedTypeProjection {
            base: Box::new(RuntimeTy::string()),
            interface: Box::new(RuntimeInterface {
                name: TypeName::from_dotted_path("user.HasOutput"),
                generics: vec![],
                associated_types: vec![],
            }),
            member: Name::new("Output"),
            attr: TyAttr::default(),
        });
    }

    #[test]
    fn roundtrip_function() {
        assert_roundtrip(&RuntimeTy::Function {
            params: vec![],
            ret: Box::new(RuntimeTy::int()),
            throws: Box::new(RuntimeTy::Never {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
        });
    }

    /// The decode boundary sorts interface bindings by name (a non-BAML or future
    /// producer may send them in any order). Bindings arriving as `Z, A` must
    /// decode sorted to `A, Z`, so the constraint compares/hashes equal regardless
    /// of wire order.
    #[test]
    fn decode_sorts_interface_bindings() {
        let unsorted = BamlTy {
            ty: Some(TyVariant::Interface(BamlTyInterface {
                name: "user.I".to_string(),
                type_args: vec![],
                bindings: vec![
                    BamlTyAssociatedBinding {
                        name: "Z".to_string(),
                        ty: Some(runtime_ty_to_proto_ty(&RuntimeTy::int())),
                    },
                    BamlTyAssociatedBinding {
                        name: "A".to_string(),
                        ty: Some(runtime_ty_to_proto_ty(&RuntimeTy::string())),
                    },
                ],
            })),
        };
        let RuntimeTy::Interface(_, _, bindings, _) =
            proto_ty_to_runtime_ty(&unsorted).expect("decode")
        else {
            panic!("expected interface");
        };
        let names: Vec<&str> = bindings.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["A", "Z"], "decode must sort bindings by name");
    }
}
