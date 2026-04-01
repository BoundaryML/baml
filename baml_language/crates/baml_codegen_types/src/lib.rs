//! Types that can now be used in multiple languages
//! Attributes "don't exist", dual-types (types vs `stream_types`) are now modules
//! Union / Optional / Null invariance guaranteed

mod errors;
mod objects;
mod ty;

pub use errors::*;
pub use objects::*;
pub use ty::*;

/// Macro to define an askama template function with less boilerplate.
///
/// Supports arbitrary parameters. Reference parameters (`&Type`) automatically
/// get a lifetime in the template struct, while copy/owned types are passed as-is.
///
/// Usage:
/// ```ignore
/// render_fn! {
///     /// ```askama
///     /// Hello, {{name}} from {{namespace}}!
///     /// ```
///     pub fn greet(name: &String, namespace: &Namespace) -> String;
/// }
/// ```
#[macro_export]
macro_rules! render_fn {
    // Entry point
    (
        $(#[$doc:meta])*
        $vis:vis fn $fn_name:ident($($params:tt)*) -> String;
    ) => {
        $crate::render_fn!(@parse
            doc_attrs = [$(#[$doc])*],
            vis = [$vis],
            fn_name = [$fn_name],
            fn_params = [],
            struct_fields = [],
            init_fields = [],
            rest = [$($params)*],
        );
    };

    // Reference parameter (handles optional trailing comma/more params)
    (@parse
        doc_attrs = [$(#[$doc:meta])*],
        vis = [$vis:vis],
        fn_name = [$fn_name:ident],
        fn_params = [$($fn_params:tt)*],
        struct_fields = [$($struct_fields:tt)*],
        init_fields = [$($init_fields:tt)*],
        rest = [$param:ident: &$ty:ty $(, $($rest:tt)*)?],
    ) => {
        $crate::render_fn!(@parse
            doc_attrs = [$(#[$doc])*],
            vis = [$vis],
            fn_name = [$fn_name],
            fn_params = [$($fn_params)* $param: &$ty,],
            struct_fields = [$($struct_fields)* $param: &'a $ty,],
            init_fields = [$($init_fields)* $param,],
            rest = [$($($rest)*)?],
        );
    };

    // Non-reference parameter (handles optional trailing comma/more params)
    (@parse
        doc_attrs = [$(#[$doc:meta])*],
        vis = [$vis:vis],
        fn_name = [$fn_name:ident],
        fn_params = [$($fn_params:tt)*],
        struct_fields = [$($struct_fields:tt)*],
        init_fields = [$($init_fields:tt)*],
        rest = [$param:ident: $ty:ty $(, $($rest:tt)*)?],
    ) => {
        $crate::render_fn!(@parse
            doc_attrs = [$(#[$doc])*],
            vis = [$vis],
            fn_name = [$fn_name],
            fn_params = [$($fn_params)* $param: $ty,],
            struct_fields = [$($struct_fields)* $param: $ty,],
            init_fields = [$($init_fields)* $param,],
            rest = [$($($rest)*)?],
        );
    };

    // Done: generate the function
    (@parse
        doc_attrs = [$(#[$doc:meta])*],
        vis = [$vis:vis],
        fn_name = [$fn_name:ident],
        fn_params = [$($fn_params:tt)*],
        struct_fields = [$($struct_fields:tt)*],
        init_fields = [$($init_fields:tt)*],
        rest = [],
    ) => {
        $vis fn $fn_name($($fn_params)*) -> String {
            $(#[$doc])*
            #[derive(askama::Template)]
            #[template(in_doc = true, ext = "txt")]
            struct Tpl<'a> {
                $($struct_fields)*
                #[allow(dead_code)]
                _phantom: std::marker::PhantomData<&'a ()>,
            }

            Tpl { $($init_fields)* _phantom: std::marker::PhantomData }.to_string().trim().to_string()
        }
    };
}
