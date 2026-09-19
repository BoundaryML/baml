//! Macros for building SAP model types in tests.
//!
//! Entry points:
//! - [`baml_ty!`] for creating [`crate::sap_model::Ty`].
//! - [`baml_tyresolved!`] for creating [`crate::sap_model::TyResolved`].
//! - [`baml_db!`] for creating [`crate::sap_model::TypeRefDb`].
//!
//! Streaming attributes go where BAML puts them: `@@stream.done` first inside a class body,
//! `@stream.done` / `@stream.must_exist` / `@alias("..")` after a field's type. Unions must
//! be wrapped in `()`.

#[macro_export]
macro_rules! baml_ty {
    (int) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Int($crate::sap_model::IntTy))
    };
    (bigint) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Bigint($crate::sap_model::BigintTy))
    };
    (float) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Float($crate::sap_model::FloatTy))
    };
    (string) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::String($crate::sap_model::StringTy))
    };
    (bool) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Bool($crate::sap_model::BoolTy))
    };
    (null) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Null($crate::sap_model::NullTy))
    };
    (true) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::LiteralBool($crate::sap_model::BoolLiteralTy(true)))
    };
    (false) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::LiteralBool($crate::sap_model::BoolLiteralTy(false)))
    };
    (($ty:tt)) => {
        $crate::baml_ty!($ty)
    };
    (-$lit:literal) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::from($crate::sap_model::LiteralTy::from(-$lit)))
    };
    // `Enum.Variant`
    ($enum_name:ident.$variant_name:ident) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::EnumVariant($crate::sap_model::EnumVariantTy {
            name: stringify!($enum_name),
            value: $crate::sap_model::AnnotatedEnumVariant {
                name: ::std::borrow::Cow::Borrowed(stringify!($variant_name)),
                aliases: ::std::vec::Vec::new(),
            },
        }))
    };
    ($ty:ident) => {
        $crate::sap_model::Ty::Unresolved(stringify!($ty))
    };
    ($($resolved:tt)*) => {
        $crate::sap_model::Ty::Resolved($crate::baml_tyresolved!($($resolved)*))
    };
}

#[macro_export]
macro_rules! baml_tyresolved {
    (int) => {
        $crate::sap_model::TyResolved::Int($crate::sap_model::IntTy)
    };
    (bigint) => {
        $crate::sap_model::TyResolved::Bigint($crate::sap_model::BigintTy)
    };
    (float) => {
        $crate::sap_model::TyResolved::Float($crate::sap_model::FloatTy)
    };
    (string) => {
        $crate::sap_model::TyResolved::String($crate::sap_model::StringTy)
    };
    (bool) => {
        $crate::sap_model::TyResolved::Bool($crate::sap_model::BoolTy)
    };
    (null) => {
        $crate::sap_model::TyResolved::Null($crate::sap_model::NullTy)
    };
    (($($ty:tt)+)) => {
        $crate::baml_tyresolved!($($ty)+)
    };
    // TODO: media
    // Using rust-like syntax for array types
    ([$($inner:tt)+]) => {
        $crate::sap_model::TyResolved::Array($crate::sap_model::ArrayTy {
            ty: Box::new($crate::baml_ty!($($inner)+)),
        })
    };
    (StreamState<$inner:tt>) => {
        $crate::sap_model::TyResolved::StreamState($crate::sap_model::StreamStateTy {
            value: Box::new($crate::baml_ty!($inner)),
        })
    };
    (map<$key_ty:tt, $value_ty:tt $(| $rest:tt)*>) => {
        $crate::sap_model::TyResolved::Map($crate::sap_model::MapTy {
            key: Box::new($crate::baml_ty!($key_ty)),
            value: Box::new($crate::baml_ty!($value_ty $(| $rest)*)),
        })
    };
    ($lit:literal) => {
        $crate::sap_model::TyResolved::from($crate::sap_model::LiteralTy::from($lit))
    };
    ($($tt:tt)+) => {
        $crate::sap_model::TyResolved::Union($crate::sap_model::UnionTy {
            variants: $crate::__baml_union_muncher!([] <= ($($tt)+)),
        })
    };
}

/// Accumulates union members into a `vec![..]` of [`crate::sap_model::Ty`].
#[macro_export]
macro_rules! __baml_union_muncher {
    // last as StreamState
    ([$($acc:expr),*] <= (StreamState<$inner:tt>)) => {
        vec![$($acc,)* $crate::baml_ty!(StreamState<$inner>)]
    };
    // last as map
    ([$($acc:expr),*] <= (map<$key_ty:tt, $value_ty:tt>)) => {
        vec![$($acc,)* $crate::baml_ty!(map<$key_ty, $value_ty>)]
    };
    // last as enum variant
    ([$($acc:expr),*] <= ($enum_name:ident.$variant_name:ident)) => {
        vec![$($acc,)* $crate::baml_ty!($enum_name.$variant_name)]
    };
    // last as tt
    ([$($acc:expr),*] <= ($last:tt)) => {
        vec![$($acc,)* $crate::baml_ty!($last)]
    };
    // item as StreamState
    ([$($acc:expr),*] <= (StreamState<$inner:tt> | $($rest:tt)+)) => {
        $crate::__baml_union_muncher!([$($acc,)* $crate::baml_ty!(StreamState<$inner>)] <= ($($rest)+))
    };
    // item as map
    ([$($acc:expr),*] <= (map<$key_ty:tt, $value_ty:tt> | $($rest:tt)+)) => {
        $crate::__baml_union_muncher!([$($acc,)* $crate::baml_ty!(map<$key_ty, $value_ty>)] <= ($($rest)+))
    };
    // item as enum variant
    ([$($acc:expr),*] <= ($enum_name:ident.$variant_name:ident | $($rest:tt)+)) => {
        $crate::__baml_union_muncher!([$($acc,)* $crate::baml_ty!($enum_name.$variant_name)] <= ($($rest)+))
    };
    // item as tt
    ([$($acc:expr),*] <= ($first:tt | $($rest:tt)+)) => {
        $crate::__baml_union_muncher!([$($acc,)* $crate::baml_ty!($first)] <= ($($rest)+))
    };
}

#[macro_export]
macro_rules! baml_db {
    {} => {
        $crate::sap_model::TypeRefDb::new()
    };
    {$($item:tt)+} => {{
        let mut types = ::indexmap::IndexMap::new();
        $crate::__baml_db_item!(types => $($item)*);
        $crate::sap_model::TypeRefDb::from_types(types)
    }};
}

#[macro_export]
macro_rules! __baml_db_add {
    ($types:ident, $name:ident, $ty:expr) => {
        assert!(
            $types.insert(stringify!($name), $ty).is_none(),
            "duplicate type `{}`",
            stringify!($name)
        );
    };
}

#[macro_export]
macro_rules! __baml_db_item {
    {$types:ident =>
        class $name:ident { @@stream.done $($fields:tt)* }
        $($rest:tt)*
    } => {
        $crate::__baml_db_add!($types, $name, $crate::sap_model::TyResolved::Class($crate::sap_model::ClassTy {
            name: stringify!($name),
            fields: $crate::__class_fields!($($fields)*),
            stream_done: true,
        }));
        $crate::__baml_db_item!($types => $($rest)*);
    };
    {$types:ident =>
        class $name:ident { $($fields:tt)* }
        $($rest:tt)*
    } => {
        $crate::__baml_db_add!($types, $name, $crate::sap_model::TyResolved::Class($crate::sap_model::ClassTy {
            name: stringify!($name),
            fields: $crate::__class_fields!($($fields)*),
            stream_done: false,
        }));
        $crate::__baml_db_item!($types => $($rest)*);
    };
    {$types:ident =>
        enum $name:ident {
            $($variant:ident $(@alias($alias:literal))*),+$(,)?
        }
        $($rest:tt)*
    } => {
        $crate::__baml_db_add!($types, $name, $crate::sap_model::TyResolved::Enum($crate::sap_model::EnumTy {
            name: stringify!($name),
            variants: vec![
                $($crate::sap_model::AnnotatedEnumVariant {
                    name: ::std::borrow::Cow::Borrowed(stringify!($variant)),
                    aliases: vec![$(::std::borrow::Cow::Borrowed($alias)),*],
                }),+
            ],
        }));
        $crate::__baml_db_item!($types => $($rest)*);
    };
    {$types:ident =>
        type $name:ident = $ty:tt;
        $($rest:tt)*
    } => {
        $crate::__baml_db_add!($types, $name, $crate::baml_tyresolved!($ty));
        $crate::__baml_db_item!($types => $($rest)*);
    };
    {$types:ident => } => {}
}

/// Builds one [`crate::sap_model::AnnotatedField`] from its name, type expression, and
/// declaration attributes.
#[macro_export]
macro_rules! __baml_field {
    ($name:ident, $ty:expr, $($attrs:tt)*) => {{
        let (aliases, stream_done, default) = $crate::__class_field_args!($($attrs)*);
        $crate::sap_model::AnnotatedField {
            name: ::std::borrow::Cow::Borrowed({
                let raw = stringify!($name);
                match raw.strip_prefix("r#") {
                    ::std::option::Option::Some(stripped) => stripped,
                    ::std::option::Option::None => raw,
                }
            }),
            ty: $ty,
            aliases,
            stream_done,
            default,
        }
    }};
}

/// We require that unions be wrapped in `()`
#[macro_export]
macro_rules! __class_fields {
    {} => {
        ::std::vec::Vec::new()
    };
    {
        $name:ident: StreamState<$ty:tt> $(@$attr:ident$(.$sub:ident)*$(($($attr_args:tt)*))?)*,
        $($rest:tt)*
    } => {
        {
            let mut fields = vec![$crate::__baml_field!(
                $name,
                $crate::baml_ty!(StreamState<$ty>),
                $(@$attr$(.$sub)*$(($($attr_args)*))?)*
            )];
            fields.extend($crate::__class_fields!($($rest)*));
            fields
        }
    };
    {
        $name:ident: map<$key_ty:tt, $value_ty:tt $(| $rest_ty:tt)*> $(@$attr:ident$(.$sub:ident)*$(($($attr_args:tt)*))?)*,
        $($rest:tt)*
    } => {
        {
            let mut fields = vec![$crate::__baml_field!(
                $name,
                $crate::baml_ty!(map<$key_ty, $value_ty $(| $rest_ty)*>),
                $(@$attr$(.$sub)*$(($($attr_args)*))?)*
            )];
            fields.extend($crate::__class_fields!($($rest)*));
            fields
        }
    };
    {
        $name:ident: $enum_name:ident.$variant_name:ident $(@$attr:ident$(.$sub:ident)*$(($($attr_args:tt)*))?)*,
        $($rest:tt)*
    } => {
        {
            let mut fields = vec![$crate::__baml_field!(
                $name,
                $crate::baml_ty!($enum_name.$variant_name),
                $(@$attr$(.$sub)*$(($($attr_args)*))?)*
            )];
            fields.extend($crate::__class_fields!($($rest)*));
            fields
        }
    };
    {
        $name:ident: $ty:tt $(@$attr:ident$(.$sub:ident)*$(($($attr_args:tt)*))?)*,
        $($rest:tt)*
    } => {
        {
            let mut fields = vec![$crate::__baml_field!(
                $name,
                $crate::baml_ty!($ty),
                $(@$attr$(.$sub)*$(($($attr_args)*))?)*
            )];
            fields.extend($crate::__class_fields!($($rest)*));
            fields
        }
    };
}

/// Parses a field's declaration attributes into `(aliases, stream_done, default)`.
#[macro_export]
macro_rules! __class_field_args {
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$stream_done:expr]
        [$default:expr]
        { @alias($alias:literal) $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases,)* ::std::borrow::Cow::<'static, str>::Borrowed($alias)]
            [$stream_done]
            [$default]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$_stream_done:expr]
        [$default:expr]
        { @stream.done $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [true]
            [$default]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$stream_done:expr]
        [$_default:expr]
        { @stream.must_exist $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [$stream_done]
            [::std::option::Option::Some($crate::sap_model::DefaultValue::Never)]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$stream_done:expr]
        [$default:expr]
        { $($guard:tt)+ }
    ) => {
        compile_error!("unknown field attribute (expected @alias(..), @stream.done, or @stream.must_exist)");
    };
    // Terminal rule: return accumulated values as a tuple
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$stream_done:expr]
        [$default:expr]
        {}
    ) => {
        (vec![$($aliases),*], $stream_done, $default)
    };
    // Entry point: initialize accumulators and dispatch to __INTERNAL__
    ($($attrs:tt)*) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            []
            [false]
            [::std::option::Option::<$crate::sap_model::DefaultValue<&'static str>>::None]
            {$($attrs)*}
        )
    };
}
