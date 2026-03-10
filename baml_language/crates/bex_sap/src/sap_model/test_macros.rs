//! This module contains macros for building types and annotations.
//!
//! Entry points:
//! - [`baml_tyannotated!`] for creating [`crate::sap_model::AnnotatedTy`].
//! - [`baml_ty!`] for creating [`crate::sap_model::Ty`] (unannotated).
//! - [`baml_tyresolved!`] for creating [`crate::sap_model::TyResolved`] (unannotated).
//! - [`baml_db!`] for creating [`crate::sap_model::TypeRefDb`].

/// For standalone types. Not used in db: it handles annotations itself.
#[macro_export]
macro_rules! baml_tyannotated {
    // Flatten attributes at different levels of parentheses
    (($($inner:tt)+) $(@$first_part:ident$(.$part:ident)*$(($($attr_args:tt)*))?)*) => {
        $crate::baml_tyannotated!($($inner)+ $(@$first_part$(.$part)*$(($($attr_args)*))?)*)
    };
    // Negative literal
    (-$lit:literal $(@$attr_name:ident($($attr_args:tt)*))*) => {
        $crate::sap_model::AnnotatedTy::new(
            $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::from($crate::sap_model::LiteralTy::from(-$lit))),
            $crate::__parse_ty_attrs!{$(@$attr_name($($attr_args)*))*}
        )
    };
    ($ty:tt $(@$first_part:ident$(.$part:ident)*$(($($attr_args:tt)*))?)*) => {{
        let annotations = $crate::__parse_ty_attrs!{$(@$first_part$(.$part)*$(($($attr_args)*))?)*};
        $crate::sap_model::TyWithMeta::new($crate::baml_ty!($ty), annotations)
    }};
    // StreamState cannot have attributes as it is not a user-provided type.
    (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*>) => {
        $crate::sap_model::TyWithMeta::new(
            $crate::baml_ty!(StreamState<$inner $(@$attr_name($($attr_args)*))*>),
            $crate::sap_model::TypeAnnotations::default(),
        )
    };
    (map<
        $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
        $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
    > $(@$attr_name:ident($($attr_args:tt)*))*) => {
        $crate::sap_model::TyWithMeta::new(
            $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Map($crate::sap_model::MapTy {
                key: Box::new($crate::baml_tyannotated!($key_ty $(@$key_attr_name($($key_attr_args)*))*)),
                value: Box::new($crate::baml_tyannotated!($value_ty $(@$value_attr_name($($value_attr_args)*))*)),
            })),
            $crate::__parse_ty_attrs!{$(@$attr_name($($attr_args)*))*}
        )
    };
    ($($tt:tt)+) => {{
        let mut vec = Vec::new();
        let attrs = $crate::__baml_tyannotated_union_muncher!(vec <= ($($tt)+));
        $crate::sap_model::AnnotatedTy::new(
            $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Union($crate::sap_model::UnionTy { variants: vec })),
            attrs,
        )
    }};
}

#[macro_export]
macro_rules! __baml_tyannotated_union_muncher {
    // last as StreamState (attributes allowed here since they apply to the union not the member)
    ($vec:ident <= (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*> $(@$last_attr:ident($($last_attr_args:tt)*))*)) => {{
        $vec.push($crate::baml_tyannotated!(StreamState<$inner $(@$attr_name($($attr_args)*))*>));
        $crate::__parse_ty_attrs!{$(@$last_attr($($last_attr_args)*))*}
    }};
    // last as map
    ($vec:ident <= (
        map<
            $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
            $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
        > $(@$attr:ident($($attr_args:tt)*))*
    )) => {{
        $vec.push($crate::baml_tyannotated!(
            map<
                $key_ty $(@$key_attr_name($($key_attr_args)*))*,
                $value_ty $(@$value_attr_name($($value_attr_args)*))*
            >
        ));
        $crate::__parse_ty_attrs!{$(@$attr($($attr_args)*))*}
    }};
    // last as tt
    ($vec:ident <= ($last:tt $(@$last_attr:ident($($last_attr_args:tt)*))*)) => {{
        $vec.push($crate::baml_tyannotated!($last));
        $crate::__parse_ty_attrs!{$(@$last_attr($($last_attr_args)*))*}
    }};
    // item as StreamState (stream state cannot have attributes)
    ($vec:ident <= (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*> | $($rest:tt)+)) => {{
        $vec.push($crate::baml_tyannotated!(StreamState<$inner $(@$attr_name($($attr_args)*))*>));
        $crate::__baml_tyannotated_union_muncher!($vec <= ($($rest)+));
    }};
    // item as map
    ($vec:ident <= (
        map<
            $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
            $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
        > $(@$attr:ident($($attr_args:tt)*))* | $($rest:tt)+
    )) => {{
        $vec.push($crate::baml_tyannotated!(
            map<
                $key_ty $(@$key_attr_name($($key_attr_args)*))*,
                $value_ty $(@$value_attr_name($($value_attr_args)*))*
            > $(@$attr($($attr_args)*))*
        ));
        $crate::__baml_tyannotated_union_muncher!($vec <= ($($rest)+))
    }};
    // item as tt
    ($vec:ident <= (
        $tt:tt $(@$attr:ident($($attr_args:tt)*))* | $($rest:tt)+
    )) => {{
        $vec.push($crate::baml_tyannotated!($tt $(@$attr($($attr_args)*))*));
        $crate::__baml_tyannotated_union_muncher!($vec <= ($($rest)+))
    }};
}

#[macro_export]
macro_rules! baml_ty {
    (int) => {
        $crate::sap_model::Ty::Resolved($crate::sap_model::TyResolved::Int($crate::sap_model::IntTy))
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
    ($ty:ident) => {
        $crate::sap_model::Ty::Unresolved(stringify!($ty))
    };
    ($($resolved:tt)*) => {
        $crate::sap_model::Ty::Resolved($crate::baml_tyresolved!($($resolved)*))
    };
}

#[macro_export]
macro_rules! __parse_ty_attrs {
    {} => {
        $crate::sap_model::TypeAnnotations::default()
    };
    // TODO: asserts
    {@assert($($assert:tt)*) $($rest:tt)*} => {{
        let mut attrs = $crate::__parse_ty_attrs!{$($rest)*};
        // attrs.asserts.push($crate::__parse_assert!{$($assert)*});
        attrs
    }};
    {@in_progress($lit:tt) $($rest:tt)*} => {{
        let mut attrs = $crate::__parse_ty_attrs!{$($rest)*};
        attrs.in_progress = Some($crate::__parse_attr_literal!{$lit});
        attrs
    }};
}

#[macro_export]
macro_rules! __parse_attr_literal {
    {never} => {
        $crate::sap_model::AttrLiteral::Never
    };
    {null} => {
        $crate::sap_model::AttrLiteral::Null
    };
    // Array recursion start
    {[$($arr:tt)*]} => {
        $crate::__parse_attr_literal!{><><ARRAY []><>< [$($arr)*]}
    };
    {$enum_name:ident.$variant_name:ident} => {
        $crate::sap_model::AttrLiteral::EnumVariant {
            enum_name: stringify!($enum_name),
            variant_name: ::std::borrow::Cow::Borrowed(stringify!($variant_name)),
        }
    };
    // Array recursion: tt
    {$lit:literal} => {
        $crate::sap_model::AttrLiteral::from($lit)
    };
    // Array recursion end
    {><><ARRAY [$($prev:tt)*]><>< []} => {
        $crate::sap_model::AttrLiteral::Array(vec![$($prev)*])
    };
    // Array recursion: class
    {><><ARRAY [$($prev:tt)*]><>< [$cls_name:ident {$($cls_inner:tt)*}$(, $($rest:tt)*)?]} => {
        $crate::__parse_attr_literal!{><><ARRAY [
            $($prev)*,
            $crate::__parse_attr_literal!{$cls_name {$($cls_inner)*}}
        ]><>< [$($rest)*]}
    };
    // Array recursion: enum variant
    {><><ARRAY [$($prev:tt)*]><>< [$enum_name:ident.$variant_name:ident $(, $($rest:tt)*)?]} => {
        $crate::__parse_attr_literal!{><><ARRAY [
            $($prev)*,
            $crate::sap_model::AttrLiteral::EnumVariant {
                enum_name: stringify!($enum_name),
                variant_name: ::std::borrow::Cow::Borrowed(stringify!($variant_name)),
            }
        ]><>< [$($rest)*]}
    };
    // Map/object recursion end
    {><><KV ($map:ident)><>< {}} => {};
    // Map/object recursion: class
    {><><KV ($map:ident)><>< {$key:literal: $cls_key:ident {$($cls_inner:tt)*}$(, $($rest:tt)*)?}} => {
        $map.insert(::std::borrow::Cow::Borrowed($key), $crate::__parse_attr_literal!{$cls_key:ident {$($cls_inner:tt)*}});
        $crate::__parse_attr_literal!{><><KV ($map)><>< {$($rest)*}}
    };
    // Map/object recursion: enum variant
    {><><KV ($map:ident)><>< {$key:literal: $enum_name:ident.$variant_name:ident $(, $($rest:tt)*)?}} => {
        $map.insert(::std::borrow::Cow::Borrowed($key), $crate::sap_model::AttrLiteral::EnumVariant {
            enum_name: stringify!($enum_name),
            variant_name: ::std::borrow::Cow::Borrowed(stringify!($variant_name)),
        });
        $crate::__parse_attr_literal!{><><KV ($map)><>< {$($rest)*}}
    };
    // Map/object recursion: tt
    {><><KV ($map:ident)><>< {$key:literal: $value:tt$(, $($rest:tt)*)?}} => {
        $map.insert(::std::borrow::Cow::Borrowed($key), $crate::__parse_attr_literal!{$value});
        $crate::__parse_attr_literal!{><><KV ($map)><>< {$($rest)*}}
    };
    // Object recursion start
    {$cls_name:ident {$($cls_inner:tt)*}} => {
        $crate::sap_model::AttrLiteral::Object {
            name: stringify!($cls_name),
            data: {
                let mut map = ::indexmap::IndexMap::new();
                $crate::__parse_attr_literal!{><><KV (map)><>< {$($cls_inner)*}};
                map
            },
        }
    };
    // Map recursion start
    {{$($map_inner:tt)*}} => {
        $crate::sap_model::AttrLiteral::Map({
            #[allow(unused_mut)]
            let mut map = ::indexmap::IndexMap::new();
            $crate::__parse_attr_literal!{><><KV (map)><>< {$($map_inner)*}};
            map
        })
    };
}

#[macro_export]
macro_rules! __parse_attr_literal_or_default {
    {try () else ($($default:tt)*)} => {
        $crate::__parse_attr_literal!{$($default)*}
    };
    {try ($($attr_args:tt)+) else ($($default:tt)*)} => {
        $crate::__parse_attr_literal!{$($attr_args)+}
    };
}

#[macro_export]
macro_rules! baml_tyresolved {
    (int) => {
        $crate::sap_model::TyResolved::Int($crate::sap_model::IntTy)
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
    (($(ty:tt)+)) => {
        $crate::baml_tyresolved!($(ty)+)
    };
    // TODO: media
    // Using rust-like syntax for array types
    ([$($inner:tt)+]) => {
        $crate::sap_model::TyResolved::Array($crate::sap_model::ArrayTy {
            ty: Box::new($crate::baml_tyannotated!($($inner)+)),
        })
    };
    (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*>) => {
        $crate::sap_model::TyResolved::StreamState($crate::sap_model::StreamStateTy {
            value: Box::new($crate::baml_tyannotated!($inner $(@$attr_name($($attr_args)*))*)),
        })
    };
    (map<
        $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
        $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))* $(| $rest:tt $(@$rest_attr:ident($($rest_attr_args:tt)*))*)*
    >) => {
        $crate::sap_model::TyResolved::Map($crate::sap_model::MapTy {
            key: Box::new($crate::baml_tyannotated!($key_ty $(@$key_attr_name($($key_attr_args)*))*)),
            value: Box::new($crate::baml_tyannotated!($value_ty $(@$value_attr_name($($value_attr_args)*))* $(| $rest $(@$rest_attr($($rest_attr_args)*))*)*)),
        })
    };
    ($lit:literal) => {
        $crate::sap_model::TyResolved::from($crate::sap_model::LiteralTy::from($lit))
    };
    ($($tt:tt)+) => {
        $crate::sap_model::TyResolved::Union($crate::sap_model::UnionTy {
            variants: {
                let mut vec = Vec::new();
                $crate::__baml_resolved_union_muncher!(vec <= ($($tt)+));
                vec
            },
        })
    };
}

#[macro_export]
macro_rules! __baml_resolved_union_muncher {
    // last as StreamState
    ($vec:ident <= (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*>)) => {{
        $vec.push($crate::baml_tyannotated!(StreamState<$inner $(@$attr_name($($attr_args)*))*>));
        $crate::__parse_ty_attrs!{$(@$last_attr($($last_attr_args)*))*}
    }};
    // last as map
    ($vec:ident <= (
        map<
            $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
            $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
        >
    )) => {
        $vec.push($crate::baml_tyannotated!(
            map<
                $key_ty $(@$key_attr_name($($key_attr_args)*))*,
                $value_ty $(@$value_attr_name($($value_attr_args)*))*
            >
        ));
    };
    // last as tt
    ($vec:ident <= ($last:tt)) => {
        $vec.push($crate::baml_tyannotated!($last));
    };
    // last as StreamState (stream state cannot have attributes)
    ($vec:ident <= (StreamState<$inner:tt $(@$attr_name:ident($($attr_args:tt)*))*> | $($rest:tt)+)) => {{
        $vec.push($crate::baml_tyannotated!(StreamState<$inner $(@$attr_name($($attr_args)*))*>));
        $crate::__baml_resolved_union_muncher!($vec <= ($($rest)+));
    }};
    // item as map
    ($vec:ident <= (
        map<
            $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
            $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
        > $(@$attr:ident($($attr_args:tt)*))* | $($rest:tt)+
    )) => {
        $vec.push($crate::baml_tyannotated!(
            map<
                $key_ty $(@$key_attr_name($($key_attr_args)*))*,
                $value_ty $(@$value_attr_name($($value_attr_args)*))*
            > $(@$attr($($attr_args)*))*
        ));
        $crate::__baml_resolved_union_muncher!($vec <= ($($rest)+));
    };
    // item as tt
    ($vec:ident <= (
        $first:tt $(@$attr:ident($($attr_args:tt)*))* | $($rest:tt)+
    )) => {
        $vec.push($crate::baml_tyannotated!($first $(@$attr($($attr_args)*))*));
        $crate::__baml_resolved_union_muncher!($vec <= ($($rest)+));
    };
}

#[macro_export]
macro_rules! baml_db {
    {} => {
        $crate::sap_model::TypeRefDb::new()
    };
    {$($item:tt)+} => {{
        let mut db = $crate::sap_model::TypeRefDb::new();
        $crate::__baml_db_item!(db => $($item)*);
        db
    }};
}

#[macro_export]
macro_rules! __baml_db_item {
    {$db:ident =>
        class $name:ident {$($fields:tt)*}
        $($rest:tt)*
    } => {
        $db.try_add(stringify!($name), $crate::sap_model::TyResolved::Class($crate::sap_model::ClassTy {
            name: stringify!($name),
            fields: $crate::__class_fields!($($fields)*),
        })).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {$db:ident =>
        enum $name:ident {
            $($variant:ident $(@alias($alias:literal))*),+
        }
        $($rest:tt)*
    } => {
        $db.try_add(stringify!($name), $crate::sap_model::TyResolved::Enum($crate::sap_model::EnumTy {
            name: stringify!($name),
            variants: vec![
                $($crate::sap_model::AnnotatedEnumVariant {
                    name: std::borrow::Cow::Borrowed(stringify!($variant)),
                    aliases: vec![$(std::borrow::Cow::Borrowed($alias)),*],
                }),+
            ],
        })).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {$db:ident =>
        type $name:ident = $ty:tt;
        $($rest:tt)*
    } => {
        $db.try_add(stringify!($name), $crate::baml_tyresolved!($ty)).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {db => } => {}
}

/// We require that unions be wrapped in `()`
#[macro_export]
macro_rules! __class_fields {
    {} => {
        Vec::new()
    };
    {
        $name:ident: StreamState<
            $ty:tt $(@$attr_name:ident($($attr_args:tt)*))*
        > $(@$field_attr_name:ident($($field_attr_args:tt)*))*,
        $($rest:tt)*
    } => {
        {
            let mut fields = $crate::__class_fields!($($rest)*);
            let (aliases, _, class_in_progress_field_missing, class_completed_field_missing) = $crate::__class_field_args!($(@$field_attr_name($($field_attr_args)*))*);
            let field = $crate::sap_model::AnnotatedField {
                name: Cow::Borrowed({
                    let raw = stringify!($name);
                    match raw.strip_prefix("r#") {
                        Some(stripped) => stripped,
                        None => raw,
                    }
                }),
                ty: $crate::baml_tyannotated!(StreamState<$ty $(@$attr_name($($attr_args)*))*>),
                class_in_progress_field_missing,
                class_completed_field_missing,
                aliases,
            };
            fields.push(field);
            fields
        }
    };
    {
        $name:ident: map<
            $key_ty:tt $(@$key_attr_name:ident($($key_attr_args:tt)*))*,
            $value_ty:tt $(@$value_attr_name:ident($($value_attr_args:tt)*))*
        > $(@$attr_name:ident($($attr_args:tt)*))*,
        $($rest:tt)*
    } => {
        {
            let mut fields = $crate::__class_fields!($($rest)*);
            let (aliases, type_annotations, class_in_progress_field_missing, class_completed_field_missing) = $crate::__class_field_args!($(@$attr_name($($attr_args)*))*);
            let field = $crate::sap_model::AnnotatedField {
                name: ::std::borrow::Cow::Borrowed({
                    let raw = stringify!($name);
                    match raw.strip_prefix("r#") {
                        Some(stripped) => stripped,
                        None => raw,
                    }
                }),
                ty: $crate::sap_model::TyWithMeta::new(
                    $crate::baml_ty!(map<
                        $key_ty $(@$key_attr_name($($key_attr_args)*))*,
                        $value_ty $(@$value_attr_name($($value_attr_args)*))*
                    >),
                    type_annotations
                ),
                class_in_progress_field_missing,
                class_completed_field_missing,
                aliases,
            };
            fields.push(field);
            fields
        }
    };
    {
        $name:ident: $ty:tt $(@$attr_name:ident($($attr_args:tt)*))*,
        $($rest:tt)*
    } => {
        {
            let mut fields = $crate::__class_fields!($($rest)*);
            let (aliases, type_annotations, class_in_progress_field_missing, class_completed_field_missing) = $crate::__class_field_args!($(@$attr_name($($attr_args)*))*);
            let field = $crate::sap_model::AnnotatedField {
                name: ::std::borrow::Cow::Borrowed({
                    let raw = stringify!($name);
                    match raw.strip_prefix("r#") {
                        Some(stripped) => stripped,
                        None => raw,
                    }
                }),
                ty: $crate::sap_model::TyWithMeta::new($crate::baml_ty!($ty), type_annotations),
                class_in_progress_field_missing,
                class_completed_field_missing,
                aliases,
            };
            fields.push(field);
            fields
        }
    };
}

#[macro_export]
macro_rules! __class_field_args {
    // --- Accumulator-based internal rules ---
    // Each rule carries: aliases, type_annotations, in_progress_opt, completed_opt, remaining attrs
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$completed_opt:expr]
        { @in_progress($($attr_lit:tt)+) $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [{
                let mut ta = $type_annotations;
                ta.in_progress = Some($crate::__parse_attr_literal!{$($attr_lit)+});
                ta
            }]
            [$in_progress_opt]
            [$completed_opt]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$completed_opt:expr]
        { @assert($($assertion:tt)+) $($rest:tt)* }
    ) => {
        // TODO: make assertions work
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [$type_annotations]
            [$in_progress_opt]
            [$completed_opt]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$completed_opt:expr]
        { @alias($alias:literal) $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases,)* ::std::borrow::Cow::<'static, str>::Borrowed($alias)]
            [$type_annotations]
            [$in_progress_opt]
            [$completed_opt]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$_in_progress_opt:expr]
        [$completed_opt:expr]
        { @class_in_progress_field_missing($($attr_lit:tt)+) $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [$type_annotations]
            [::std::option::Option::Some($crate::__parse_attr_literal!{$($attr_lit)+})]
            [$completed_opt]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$_completed_opt:expr]
        { @class_completed_field_missing($($attr_lit:tt)+) $($rest:tt)* }
    ) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            [$($aliases),*]
            [$type_annotations]
            [$in_progress_opt]
            [::std::option::Option::Some($crate::__parse_attr_literal!{$($attr_lit)+})]
            { $($rest)* }
        )
    };
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$completed_opt:expr]
        { $($guard:tt)+ }
    ) => {
        compile_error!("Invalid attribute");
    };
    // Terminal rule: return accumulated values as a tuple
    (<><><> __INTERNAL__
        [$($aliases:expr),*]
        [$type_annotations:expr]
        [$in_progress_opt:expr]
        [$completed_opt:expr]
        {}
    ) => {
        (
            vec![$($aliases),*],
            $type_annotations,
            $in_progress_opt.unwrap_or($crate::sap_model::AttrLiteral::Never),
            $completed_opt.unwrap_or($crate::sap_model::AttrLiteral::Never),
        )
    };
    // Entry point: initialize accumulators and dispatch to __INTERNAL__
    ($($attrs:tt)*) => {
        $crate::__class_field_args!(<><><> __INTERNAL__
            []
            [$crate::sap_model::TypeAnnotations::<&'static str>::default()]
            [::std::option::Option::<$crate::sap_model::AttrLiteral<&str>>::None]
            [::std::option::Option::<$crate::sap_model::AttrLiteral<&str>>::None]
            {$($attrs)*}
        )
    };
}
