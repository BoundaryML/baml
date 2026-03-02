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
    (($ty:tt)) => {
        $crate::baml_tyresolved!($ty)
    };
    (($first:tt $(| $rest:tt)+)) => {
        $crate::sap_model::TyResolved::Union($crate::sap_model::UnionTy {
            variants: vec![
                $crate::sap_model::TyWithMeta::new($crate::baml_ty!($first), $crate::sap_model::TypeAnnotations::default()),
                $($crate::sap_model::TyWithMeta::new($crate::baml_ty!($rest), $crate::sap_model::TypeAnnotations::default()),)+
            ],
        })
    };
    // TODO: media
    // Using rust-like syntax for array types
    ([$first:tt $(| $rest:tt)+]) => {
        $crate::sap_model::TyResolved::Array($crate::sap_model::ArrayTy {
            ty: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($first $(| $rest)*), $crate::sap_model::TypeAnnotations::default())),
        })
    };
    ([$first:tt]) => {
        $crate::sap_model::TyResolved::Array($crate::sap_model::ArrayTy {
            ty: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($first), $crate::sap_model::TypeAnnotations::default())),
        })
    };
    (map<$key_ty:tt, $value_ty:tt $(| $rest:tt)+>) => {
        $crate::sap_model::TyResolved::Map($crate::sap_model::MapTy {
            key: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($key_ty), $crate::sap_model::TypeAnnotations::default())),
            value: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($value_ty $(| $rest)*), $crate::sap_model::TypeAnnotations::default())),
        })
    };
    (map<$key_ty:tt, $value_ty:tt>) => {
        $crate::sap_model::TyResolved::Map($crate::sap_model::MapTy {
            key: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($key_ty), $crate::sap_model::TypeAnnotations::default())),
            value: Box::new($crate::sap_model::TyWithMeta::new($crate::baml_ty!($value_ty), $crate::sap_model::TypeAnnotations::default())),
        })
    };
    ($lit:literal) => {
        $crate::sap_model::TyResolved::from($crate::sap_model::LiteralTy::from($lit))
    };
    ($first:tt $(| $rest:tt)+) => {
        $crate::sap_model::TyResolved::Union($crate::sap_model::UnionTy {
            variants: vec![
                $crate::sap_model::TyWithMeta::new($crate::baml_ty!($first), $crate::sap_model::TypeAnnotations::default()),
                $($crate::sap_model::TyWithMeta::new($crate::baml_ty!($rest), $crate::sap_model::TypeAnnotations::default()),)+
            ],
        })
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
    {$db:ident => class $name:ident { $($field:ident: $ty:tt),+ } $($rest:tt)*} => {
        $db.try_add(stringify!($name), $crate::sap_model::TyResolved::Class($crate::sap_model::ClassTy {
            name: stringify!($name),
            fields: vec![
                $($crate::sap_model::AnnotatedField {
                    name: std::borrow::Cow::Borrowed(stringify!($field)),
                    ty: $crate::sap_model::TyWithMeta::new($crate::baml_ty!($ty), $crate::sap_model::TypeAnnotations::default()),
                    class_in_progress_field_missing: $crate::sap_model::AttrLiteral::Null,
                    class_completed_field_missing: $crate::sap_model::AttrLiteral::Never,
                    aliases: vec![],
                }),+
            ],
        })).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {$db:ident => enum $name:ident { $($variant:ident),+ } $($rest:tt)*} => {
        $db.try_add(stringify!($name), $crate::sap_model::TyResolved::Enum($crate::sap_model::EnumTy {
            name: stringify!($name),
            variants: vec![
                $($crate::sap_model::AnnotatedEnumVariant {
                    name: std::borrow::Cow::Borrowed(stringify!($variant)),
                    aliases: vec![],
                }),+
            ],
        })).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {$db:ident => type $name:ident = $ty:tt; $($rest:tt)*} => {
        $db.try_add(stringify!($name), $crate::baml_tyresolved!($ty)).ok().unwrap();
        $crate::__baml_db_item!($db => $($rest)*);
    };
    {db => } => {}
}
