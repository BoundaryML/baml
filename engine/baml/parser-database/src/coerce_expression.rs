use crate::{ast, DatamodelError, Diagnostics};

macro_rules! impl_coercions {
    ($lt:lifetime; $($name:ident : $expected_type:expr => $out:ty;)*) => {
        /// Coerce expressions to a specific type, emitting a validation error if the coercion
        /// fails. See the `coerce_opt` module if you do not want to emit validation errors.
        pub mod coerce {
            #![allow(missing_docs)]

            use super::*;

            $(
            pub fn $name<$lt>(expr: & $lt ast::Expression, diagnostics: &mut Diagnostics) -> Option<$out> {
                coerce::<$lt>(super::coerce_opt::$name, $expected_type)(expr, diagnostics)
            }
            )*
        }
    }
}

impl_coercions! {
    'a;
    constant : "constant" => &'a str;
    string : "string" => &'a str;
    string_with_span : "string" => (&'a str, ast::Span);
    constant_with_span : "constant" => (&'a str, ast::Span);
    boolean : "boolean" => bool;
    integer : "numeric" => i64;
    float : "float" => f64;
}

/// Fallible coercions of PSL expressions to more specific types.
pub mod coerce_opt {
    #![allow(missing_docs, clippy::needless_lifetimes)] // lifetimes are used by the macro

    use super::*;

    pub fn constant<'a>(expr: &'a ast::Expression) -> Option<&'a str> {
        expr.as_constant_value().map(|(s, _)| s)
    }

    pub fn string<'a>(expr: &'a ast::Expression) -> Option<&'a str> {
        expr.as_string_value().map(|(s, _)| s)
    }

    pub fn string_with_span<'a>(expr: &'a ast::Expression) -> Option<(&'a str, ast::Span)> {
        expr.as_string_value()
    }

    pub fn constant_with_span<'a>(expr: &'a ast::Expression) -> Option<(&'a str, ast::Span)> {
        expr.as_constant_value()
    }

    pub fn boolean<'a>(expr: &'a ast::Expression) -> Option<bool> {
        expr.as_constant_value().and_then(|(constant, _)| constant.parse().ok())
    }

    pub fn integer<'a>(expr: &'a ast::Expression) -> Option<i64> {
        expr.as_numeric_value().and_then(|(num, _)| num.parse().ok())
    }

    pub fn float<'a>(expr: &'a ast::Expression) -> Option<f64> {
        expr.as_numeric_value().and_then(|(num, _)| num.parse().ok())
    }
}

const fn coerce<'a, T>(
    coercion: impl Fn(&'a ast::Expression) -> Option<T>,
    expected_type: &'static str,
) -> impl (Fn(&'a ast::Expression, &mut Diagnostics) -> Option<T>) {
    move |expr, diagnostics| match coercion(expr) {
        Some(t) => Some(t),
        None => {
            diagnostics.push_error(DatamodelError::new_type_mismatch_error(
                expected_type,
                expr.describe_value_type(),
                &expr.to_string(),
                expr.span(),
            ));
            None
        }
    }
}

/// Coerce an expression to an array. The coercion function is used to coerce the array elements.
pub fn coerce_array<'a, T>(
    expr: &'a ast::Expression,
    coercion: &dyn (Fn(&'a ast::Expression, &mut Diagnostics) -> Option<T>),
    diagnostics: &mut Diagnostics,
) -> Option<Vec<T>> {
    let mut out = Vec::new();
    let mut is_valid = true; // we keep track of validity to avoid early returns

    match expr {
        ast::Expression::Array(vals, _) => {
            for val in vals {
                match coercion(val, diagnostics) {
                    Some(val) => out.push(val),
                    None => is_valid = false,
                }
            }
        }
        _ => out.push(coercion(expr, diagnostics)?),
    }

    is_valid.then_some(out)
}
