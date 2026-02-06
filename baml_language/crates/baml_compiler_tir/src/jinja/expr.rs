//! Expression type inference for Jinja templates.
//!
//! This module performs type inference and validation on Jinja expressions,
//! including:
//! - Variable references and property access
//! - Binary and unary operations
//! - Filter applications
//! - Function calls
//! - Literal values
//!
//! Ported from `engine/baml-lib/jinja/src/evaluate_type/expr.rs`.

use minijinja::machinery::ast;

use super::{JinjaType, JinjaTypeEnv, TypeError};

/// Entry point for expression type inference.
///
/// Returns the inferred type or a list of type errors.
pub fn infer_expression_type(
    expr: &ast::Expr,
    env: &JinjaTypeEnv,
) -> Result<JinjaType, Vec<TypeError>> {
    let mut errors = Vec::new();

    // Lint: Check for bare function reference without call
    if let ast::Expr::Var(var) = expr {
        if env.is_function(var.id) {
            errors.push(TypeError::function_reference_without_call(
                var.id,
                var.span(),
            ));
        }
    }

    let result = visit_expr(expr, &mut errors, env);

    if errors.is_empty() {
        Ok(result)
    } else {
        Err(errors)
    }
}

// List of valid filters (from engine)
const VALID_FILTERS: &[&str] = &[
    "abs",
    "attrs",
    "batch",
    "bool",
    "capitalize",
    "escape",
    "first",
    "last",
    "default",
    "float",
    "indent",
    "int",
    "dictsort",
    "items",
    "join",
    "length",
    "list",
    "lower",
    "upper",
    "map",
    "max",
    "min",
    "pprint",
    "regex_match",
    "reject",
    "rejectattr",
    "replace",
    "reverse",
    "round",
    "safe",
    "select",
    "selectattr",
    "slice",
    "sort",
    "split",
    "sum",
    "title",
    "tojson",
    "json",
    "format",
    "trim",
    "unique",
    "urlencode",
];

/// Main expression visitor that infers types.
fn visit_expr(expr: &ast::Expr, errors: &mut Vec<TypeError>, env: &JinjaTypeEnv) -> JinjaType {
    match expr {
        ast::Expr::Var(var) => match env.resolve_variable(var.id) {
            Some(t) => t,
            None => {
                errors.push(TypeError::unresolved_variable(
                    var.id,
                    var.span(),
                    &env.variable_names(),
                ));
                JinjaType::Unknown
            }
        },

        ast::Expr::Const(c) => infer_const_type(&c.value),

        ast::Expr::UnaryOp(op_expr) => {
            let inner = visit_expr(&op_expr.expr, errors, env);

            match op_expr.op {
                ast::UnaryOpKind::Not => {
                    // `not` coerces to bool in Jinja, so most types are fine.
                    // Only flag clearly non-boolean types that are likely mistakes.
                    JinjaType::Bool
                }
                ast::UnaryOpKind::Neg => {
                    if !inner.is_subtype_of(&JinjaType::Number)
                        && !matches!(inner, JinjaType::Unknown)
                    {
                        errors.push(TypeError::invalid_type(
                            &op_expr.expr,
                            &inner,
                            "number",
                            expr.span(),
                        ));
                    }
                    JinjaType::Number
                }
            }
        }

        ast::Expr::BinOp(bin_expr) => handle_binary_op(expr, bin_expr, errors, env),

        ast::Expr::IfExpr(if_expr) => {
            let _test = visit_expr(&if_expr.test_expr, errors, env);

            let true_type = visit_expr(&if_expr.true_expr, errors, env);
            let false_type = if_expr
                .false_expr
                .as_ref()
                .map(|e| visit_expr(e, errors, env))
                .unwrap_or(JinjaType::Unknown);

            merge_types(vec![true_type, false_type])
        }

        ast::Expr::Filter(filter_expr) => handle_filter(expr, filter_expr, errors, env),

        ast::Expr::Test(test_expr) => {
            let _inner = visit_expr(&test_expr.expr, errors, env);
            handle_test(test_expr, errors)
        }

        ast::Expr::GetAttr(attr_expr) => handle_get_attr(expr, attr_expr, errors, env),

        ast::Expr::GetItem(item_expr) => {
            let base = visit_expr(&item_expr.expr, errors, env);
            let _subscript = visit_expr(&item_expr.subscript_expr, errors, env);

            match base {
                JinjaType::List(elem) => *elem,
                JinjaType::Map(_, val) => *val,
                JinjaType::String => JinjaType::String,
                JinjaType::Unknown => JinjaType::Unknown,
                _ => JinjaType::Unknown,
            }
        }

        ast::Expr::Slice(slice_expr) => {
            let base = visit_expr(&slice_expr.expr, errors, env);
            if let Some(start) = &slice_expr.start {
                let _ = visit_expr(start, errors, env);
            }
            if let Some(stop) = &slice_expr.stop {
                let _ = visit_expr(stop, errors, env);
            }
            if let Some(step) = &slice_expr.step {
                let _ = visit_expr(step, errors, env);
            }

            match base {
                JinjaType::List(_) => base,
                JinjaType::String => JinjaType::String,
                JinjaType::Unknown => JinjaType::Unknown,
                _ => JinjaType::Unknown,
            }
        }

        ast::Expr::Call(call_expr) => handle_call(call_expr, errors, env),

        ast::Expr::List(list_expr) => {
            let elem_type = merge_types(
                list_expr
                    .items
                    .iter()
                    .map(|item| visit_expr(item, errors, env)),
            );
            JinjaType::List(Box::new(elem_type))
        }

        ast::Expr::Map(map_expr) => {
            let key_type =
                merge_types(map_expr.keys.iter().map(|key| visit_expr(key, errors, env)));
            let value_type = merge_types(
                map_expr
                    .values
                    .iter()
                    .map(|val| visit_expr(val, errors, env)),
            );
            JinjaType::Map(Box::new(key_type), Box::new(value_type))
        }
    }
}

/// Handle binary operations with proper type checking.
fn handle_binary_op(
    expr: &ast::Expr,
    bin_expr: &ast::Spanned<ast::BinOp>,
    errors: &mut Vec<TypeError>,
    env: &JinjaTypeEnv,
) -> JinjaType {
    let lhs = visit_expr(&bin_expr.left, errors, env);
    let rhs = visit_expr(&bin_expr.right, errors, env);

    // Handle enum operations specially
    if let Some(result) = handle_enum_binary_op(expr, bin_expr, &lhs, &rhs, errors, env) {
        return result;
    }

    // Normal operator handling
    match bin_expr.op {
        ast::BinOpKind::Add => {
            if lhs.is_subtype_of(&JinjaType::String) || rhs.is_subtype_of(&JinjaType::String) {
                JinjaType::String
            } else {
                JinjaType::Number
            }
        }
        ast::BinOpKind::Sub
        | ast::BinOpKind::Mul
        | ast::BinOpKind::Div
        | ast::BinOpKind::Pow
        | ast::BinOpKind::FloorDiv
        | ast::BinOpKind::Rem => JinjaType::Number,

        ast::BinOpKind::Eq
        | ast::BinOpKind::Ne
        | ast::BinOpKind::Lt
        | ast::BinOpKind::Gt
        | ast::BinOpKind::Lte
        | ast::BinOpKind::Gte
        | ast::BinOpKind::In => JinjaType::Bool,

        ast::BinOpKind::Concat => JinjaType::String,

        ast::BinOpKind::ScAnd | ast::BinOpKind::ScOr => JinjaType::Bool,
    }
}

/// Check if an operator is a comparison operator.
fn is_comparison_op(op: &ast::BinOpKind) -> bool {
    matches!(
        op,
        ast::BinOpKind::Eq
            | ast::BinOpKind::Ne
            | ast::BinOpKind::Lt
            | ast::BinOpKind::Gt
            | ast::BinOpKind::Lte
            | ast::BinOpKind::Gte
    )
}

/// Extract enum name from a nullable union (enum + null/undefined only).
fn extract_enum_from_nullable_union(types: &[JinjaType]) -> Option<&str> {
    let mut enum_name: Option<&str> = None;

    for t in types {
        match t {
            JinjaType::EnumValueRef(name) => {
                if enum_name.is_some() {
                    // Multiple different enums - not a simple nullable enum
                    return None;
                }
                enum_name = Some(name);
            }
            JinjaType::None | JinjaType::Undefined => {
                // Nullish types are allowed in nullable enums
                continue;
            }
            _ => {
                // Any other type means this isn't a nullable enum
                return None;
            }
        }
    }

    enum_name
}

/// Handle enum-specific binary operations with proper error messages.
fn handle_enum_binary_op(
    expr: &ast::Expr,
    bin_expr: &ast::Spanned<ast::BinOp>,
    lhs: &JinjaType,
    rhs: &JinjaType,
    errors: &mut Vec<TypeError>,
    _env: &JinjaTypeEnv,
) -> Option<JinjaType> {
    // Handle nullable enum to string literal comparisons
    if let (JinjaType::Union(union_types), JinjaType::String) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                return Some(JinjaType::Bool);
            }
        }
    }
    if let (JinjaType::String, JinjaType::Union(union_types)) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                return Some(JinjaType::Bool);
            }
        }
    }

    // Handle nullable-to-nullable enum comparisons
    if let (JinjaType::Union(left_types), JinjaType::Union(right_types)) = (lhs, rhs) {
        let left_enum = extract_enum_from_nullable_union(left_types);
        let right_enum = extract_enum_from_nullable_union(right_types);

        if let (Some(left), Some(right)) = (left_enum, right_enum) {
            if is_comparison_op(&bin_expr.op) {
                if left == right {
                    return Some(JinjaType::Bool);
                }
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    left,
                    expr.span(),
                ));
                return Some(JinjaType::Bool);
            }
        }
    }

    // Handle direct EnumValueRef operations
    match (lhs, rhs) {
        // Both are EnumValueRef - only allow comparison between same enum
        (JinjaType::EnumValueRef(e1), JinjaType::EnumValueRef(e2)) => {
            if is_comparison_op(&bin_expr.op) {
                if e1 == e2 {
                    Some(JinjaType::Bool)
                } else {
                    errors.push(TypeError::enum_string_comparison_deprecated(
                        expr,
                        e1,
                        expr.span(),
                    ));
                    Some(JinjaType::Unknown)
                }
            } else {
                // Disallow arithmetic/string ops on enums
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    e1,
                    expr.span(),
                ));
                Some(JinjaType::Unknown)
            }
        }

        // EnumValueRef with generic string
        (JinjaType::EnumValueRef(enum_name), JinjaType::String)
        | (JinjaType::String, JinjaType::EnumValueRef(enum_name)) => {
            if is_comparison_op(&bin_expr.op) {
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                Some(JinjaType::Bool)
            } else {
                errors.push(TypeError::enum_string_comparison_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                Some(JinjaType::Unknown)
            }
        }

        // Any other combination with EnumValueRef is invalid
        (JinjaType::EnumValueRef(enum_name), _) | (_, JinjaType::EnumValueRef(enum_name)) => {
            errors.push(TypeError::enum_string_comparison_deprecated(
                expr,
                enum_name,
                expr.span(),
            ));
            Some(JinjaType::Unknown)
        }

        // No enums involved
        _ => None,
    }
}

/// Handle filter expressions with type checking.
fn handle_filter(
    expr: &ast::Expr,
    filter_expr: &ast::Spanned<ast::Filter>,
    errors: &mut Vec<TypeError>,
    env: &JinjaTypeEnv,
) -> JinjaType {
    let inner = filter_expr
        .expr
        .as_ref()
        .map(|e| visit_expr(e, errors, env))
        .unwrap_or(JinjaType::Unknown);

    let mut ensure_type = |expected: &str| {
        errors.push(TypeError::invalid_type(
            // Use the parent expr as a backup expr in the error message if
            // the expr inside of the filter can't be found for some reason.
            filter_expr.expr.as_ref().unwrap_or(expr),
            &inner,
            expected,
            expr.span(),
        ));
    };

    match filter_expr.name {
        "abs" => {
            if !inner.is_subtype_of(&JinjaType::Number) {
                ensure_type("number");
            }
            JinjaType::Number
        }
        "attrs" | "batch" => JinjaType::Unknown,
        "bool" => JinjaType::Bool,
        "capitalize" | "escape" => {
            if !inner.is_subtype_of(&JinjaType::String) {
                ensure_type("string");
            }
            JinjaType::String
        }
        "first" | "last" => match inner {
            JinjaType::List(t) => merge_types(vec![*t, JinjaType::None]),
            JinjaType::Unknown => JinjaType::Unknown,
            _ => {
                ensure_type("list");
                JinjaType::Unknown
            }
        },
        "default" => JinjaType::Unknown,
        "float" => JinjaType::Float,
        "indent" => JinjaType::String,
        "int" => JinjaType::Int,
        "dictsort" | "items" => match inner {
            JinjaType::Map(k, v) => JinjaType::List(Box::new(JinjaType::Tuple(vec![*k, *v]))),
            JinjaType::ClassRef(_) => JinjaType::List(Box::new(JinjaType::Tuple(vec![
                JinjaType::String,
                JinjaType::Unknown,
            ]))),
            _ => {
                ensure_type("map or class");
                JinjaType::Unknown
            }
        },
        "join" => JinjaType::String,
        "length" => match inner {
            JinjaType::List(_)
            | JinjaType::String
            | JinjaType::ClassRef(_)
            | JinjaType::Map(_, _) => JinjaType::Int,
            JinjaType::Unknown => JinjaType::Unknown,
            _ => {
                ensure_type("list, string, class or map");
                JinjaType::Unknown
            }
        },
        "list" => JinjaType::List(Box::new(JinjaType::Unknown)),
        "lower" | "upper" => {
            if !inner.is_subtype_of(&JinjaType::String) {
                ensure_type("string");
            }
            JinjaType::String
        }
        "map" | "max" | "min" | "pprint" => JinjaType::Unknown,
        "regex_match" => JinjaType::Bool,
        "reject" | "rejectattr" | "reverse" | "slice" | "sort" | "unique" => JinjaType::Unknown,
        "replace" => JinjaType::String,
        "round" => JinjaType::Float,
        "safe" => JinjaType::String,
        "select" | "selectattr" => JinjaType::Unknown,
        "split" => JinjaType::List(Box::new(JinjaType::String)),
        "sum" => match &inner {
            JinjaType::List(elem_type) => {
                if elem_type.is_subtype_of(&JinjaType::Float) {
                    JinjaType::Float
                } else if elem_type.is_subtype_of(&JinjaType::Int) {
                    JinjaType::Int
                } else {
                    ensure_type("(int|float)[]");
                    JinjaType::Number
                }
            }
            _ => {
                ensure_type("(int|float)[]");
                JinjaType::Number
            }
        },
        "title" | "format" | "trim" => JinjaType::String,
        "tojson" | "json" => JinjaType::String,
        "urlencode" => JinjaType::String,
        other => {
            errors.push(TypeError::invalid_filter(other, expr.span(), VALID_FILTERS));
            JinjaType::Unknown
        }
    }
}

/// Valid Jinja test names (used with `is` operator).
const VALID_TESTS: &[&str] = &[
    "boolean",
    "callable",
    "defined",
    "divisibleby",
    "eq",
    "equalto",
    "even",
    "false",
    "filter",
    "float",
    "ge",
    "gt",
    "greaterthan",
    "in",
    "integer",
    "iterable",
    "le",
    "lower",
    "lt",
    "lessthan",
    "mapping",
    "ne",
    "none",
    "number",
    "odd",
    "sameas",
    "sequence",
    "string",
    "test",
    "true",
    "undefined",
    "upper",
];

/// Handle `is` test expressions (e.g., `x is defined`, `x is odd`).
fn handle_test(test_expr: &ast::Spanned<ast::Test>, errors: &mut Vec<TypeError>) -> JinjaType {
    if !VALID_TESTS.contains(&test_expr.name) {
        errors.push(TypeError::invalid_test(
            test_expr.name,
            test_expr.span(),
            VALID_TESTS,
        ));
    }
    JinjaType::Bool
}

/// Handle property access (dot notation).
fn handle_get_attr(
    _expr: &ast::Expr,
    attr_expr: &ast::Spanned<ast::GetAttr>,
    errors: &mut Vec<TypeError>,
    env: &JinjaTypeEnv,
) -> JinjaType {
    let parent = visit_expr(&attr_expr.expr, errors, env);

    match &parent {
        JinjaType::ClassRef(class_name) => {
            match env.get_class_property(class_name, attr_expr.name) {
                Some(prop_type) => prop_type,
                None => {
                    errors.push(TypeError::property_not_defined(
                        &pretty_print(&attr_expr.expr),
                        class_name,
                        attr_expr.name,
                        attr_expr.span(),
                    ));
                    JinjaType::Unknown
                }
            }
        }

        JinjaType::EnumRef(enum_name) => match env.get_enum_value(enum_name, attr_expr.name) {
            Some(_) => JinjaType::EnumValueRef(enum_name.clone()),
            None => {
                errors.push(TypeError::property_not_defined(
                    &pretty_print(&attr_expr.expr),
                    enum_name,
                    attr_expr.name,
                    attr_expr.span(),
                ));
                JinjaType::Unknown
            }
        },

        JinjaType::EnumValueRef(enum_value) => match attr_expr.name {
            "value" => JinjaType::String,
            _ => {
                errors.push(TypeError::enum_value_property_error(
                    &pretty_print(&attr_expr.expr),
                    enum_value,
                    attr_expr.name,
                    attr_expr.span(),
                ));
                JinjaType::Unknown
            }
        },

        JinjaType::Union(_) | JinjaType::Alias { .. } => {
            typecheck_attr_access_on_union(&parent, attr_expr, errors, env)
        }

        JinjaType::Unknown => JinjaType::Unknown,

        other => {
            errors.push(TypeError::invalid_type(
                &attr_expr.expr,
                other,
                "class",
                attr_expr.span(),
            ));
            JinjaType::Unknown
        }
    }
}

/// Typecheck attribute access on union types.
///
/// Verifies that an attribute is present in all items of a union.
fn typecheck_attr_access_on_union(
    union_type: &JinjaType,
    attr_expr: &ast::Spanned<ast::GetAttr>,
    errors: &mut Vec<TypeError>,
    env: &JinjaTypeEnv,
) -> JinjaType {
    // Extract union name if this is a type alias
    let union_name = match union_type {
        JinjaType::Alias { name, .. } => Some(name.as_str()),
        _ => None,
    };

    // Resolve items — handle both bare Union and Alias wrapping a Union
    let union_items = match union_type {
        JinjaType::Union(items) => items,
        JinjaType::Alias { resolved, .. } => match resolved.as_ref() {
            JinjaType::Union(items) => items,
            _ => {
                errors.push(TypeError::invalid_type(
                    &attr_expr.expr,
                    union_type,
                    "class",
                    attr_expr.span(),
                ));
                return JinjaType::Unknown;
            }
        },
        _ => {
            errors.push(TypeError::invalid_type(
                &attr_expr.expr,
                union_type,
                "class",
                attr_expr.span(),
            ));
            return JinjaType::Unknown;
        }
    };

    // Attribute must be present on all items with the same type
    let mut attr_type = None;
    let mut classes_missing_property: Vec<&str> = Vec::new();
    let mut has_type_mismatch = false;

    // Check all union items recursively
    let mut stack: Vec<&JinjaType> = union_items.iter().collect();

    while let Some(union_item) = stack.pop() {
        match union_item {
            JinjaType::ClassRef(class_name) => {
                // Check if this class has the property
                match env.get_class_property(class_name, attr_expr.name) {
                    Some(prop_type) => {
                        // Check if type matches previous types
                        match &attr_type {
                            None => attr_type = Some(prop_type),
                            Some(prev_type) => {
                                if !prop_type.equals_ignoring_literals(prev_type) {
                                    has_type_mismatch = true;
                                }
                            }
                        }
                    }
                    None => {
                        classes_missing_property.push(class_name.as_str());
                    }
                }
            }

            // Resolve aliases
            JinjaType::Alias { resolved, .. } => stack.push(resolved),

            // Recurse into nested unions
            JinjaType::Union(nested) => stack.extend(nested.iter()),

            // Non-class type in union
            _ => {
                errors.push(TypeError::non_class_in_union(
                    &pretty_print(&attr_expr.expr),
                    attr_expr.name,
                    &union_item.name(),
                    attr_expr.span(),
                ));
                return JinjaType::Unknown;
            }
        }
    }

    // Report specific errors
    if !classes_missing_property.is_empty() {
        errors.push(TypeError::property_not_found_in_union(
            &pretty_print(&attr_expr.expr),
            attr_expr.name,
            &classes_missing_property,
            union_name,
            attr_expr.span(),
        ));
        return JinjaType::Unknown;
    }

    if has_type_mismatch {
        errors.push(TypeError::property_type_mismatch_in_union(
            &pretty_print(&attr_expr.expr),
            attr_expr.name,
            union_name,
            attr_expr.span(),
        ));
        return JinjaType::Unknown;
    }

    attr_type.unwrap_or(JinjaType::Unknown)
}

/// Handle function calls (including template string functions).
fn handle_call(
    call_expr: &ast::Spanned<ast::Call>,
    errors: &mut Vec<TypeError>,
    env: &JinjaTypeEnv,
) -> JinjaType {
    let func_type = visit_expr(&call_expr.expr, errors, env);

    // Get function name for better error messages
    let func_name = match &call_expr.expr {
        ast::Expr::Var(v) => Some(v.id),
        _ => None,
    };

    match func_type {
        JinjaType::FunctionRef(_) | JinjaType::Unknown => {
            // Look up name and function signature
            let fn_data =
                func_name.and_then(|name| env.get_function(name).map(|ty_info| (name, ty_info)));
            if let Some((name, (return_type, expected_params))) = fn_data {
                // Validate arguments
                validate_function_call(
                    name,
                    &call_expr.args,
                    expected_params,
                    errors,
                    call_expr.span(),
                    env,
                );
                return_type.clone()
            } else {
                // Function not found - error already reported by variable resolution
                JinjaType::Unknown
            }
        }
        _ => {
            // Not a function type
            JinjaType::Unknown
        }
    }
}

/// Validate function call arguments.
fn validate_function_call(
    func_name: &str,
    args: &[ast::CallArg],
    expected_params: &[(String, JinjaType)],
    errors: &mut Vec<TypeError>,
    span: minijinja::machinery::Span,
    env: &JinjaTypeEnv,
) {
    use std::collections::{HashMap, HashSet};

    // Separate positional and keyword arguments
    let mut positional_count = 0;
    let mut provided_kwargs = HashSet::new();

    for arg in args {
        match arg {
            ast::CallArg::Pos(_) => positional_count += 1,
            ast::CallArg::Kwarg(name, _) => {
                provided_kwargs.insert(*name);
            }
            ast::CallArg::PosSplat(_) | ast::CallArg::KwargSplat(_) => {
                // Can't validate splat args statically
                return;
            }
        }
    }

    // Check argument count for positional args
    if positional_count > expected_params.len() {
        errors.push(TypeError::wrong_arg_count(
            func_name,
            span,
            expected_params.len(),
            args.len(),
        ));
        return;
    }

    let expected_map: HashMap<&str, &JinjaType> = expected_params
        .iter()
        .map(|(name, ty)| (name.as_str(), ty))
        .collect();

    // Check positional arguments (count and types)
    for (i, arg) in args.iter().enumerate() {
        if let ast::CallArg::Pos(expr) = arg {
            if i < expected_params.len() {
                let (param_name, expected_type) = &expected_params[i];
                let got = visit_expr(expr, errors, env);
                if !got.is_subtype_of(expected_type)
                    && !matches!(got, JinjaType::Unknown)
                    && !matches!(expected_type, JinjaType::Unknown)
                {
                    errors.push(TypeError::wrong_arg_type(
                        func_name,
                        span,
                        param_name,
                        expected_type,
                        &got,
                    ));
                }
            }
        }
    }

    // Check keyword arguments (names and types)
    let valid_param_names: HashSet<&str> = expected_params
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();

    for arg in args {
        if let ast::CallArg::Kwarg(name, expr) = arg {
            if !valid_param_names.contains(name) {
                let valid_as_strings: HashSet<&String> =
                    expected_params.iter().map(|(name, _)| name).collect();
                errors.push(TypeError::unknown_arg(
                    func_name,
                    span,
                    name,
                    valid_as_strings,
                ));
            } else if let Some(expected_type) = expected_map.get(name) {
                let got = visit_expr(expr, errors, env);
                if !got.is_subtype_of(expected_type)
                    && !matches!(got, JinjaType::Unknown)
                    && !matches!(expected_type, JinjaType::Unknown)
                {
                    errors.push(TypeError::wrong_arg_type(
                        func_name,
                        span,
                        name,
                        expected_type,
                        &got,
                    ));
                }
            }
        }
    }

    // Check for missing required arguments
    let provided_positional: HashSet<_> = (0..positional_count).collect();

    for (i, (param_name, _param_type)) in expected_params.iter().enumerate() {
        let provided =
            provided_positional.contains(&i) || provided_kwargs.contains(param_name.as_str());
        if !provided {
            errors.push(TypeError::missing_arg(func_name, span, param_name));
        }
    }
}

/// Infer the type of a constant value.
fn infer_const_type(value: &minijinja::value::Value) -> JinjaType {
    use minijinja::value::ValueKind;

    match value.kind() {
        ValueKind::Undefined => JinjaType::Undefined,
        ValueKind::None => JinjaType::None,
        ValueKind::Bool => JinjaType::Bool,
        ValueKind::String => JinjaType::String,
        ValueKind::Number => {
            // Try to determine if it's int or float
            if value.to_string().contains('.') {
                JinjaType::Float
            } else {
                JinjaType::Int
            }
        }
        ValueKind::Seq => {
            // Infer element type from sequence
            match value.len() {
                Some(0) => JinjaType::List(Box::new(JinjaType::Unknown)),
                Some(_) => {
                    if let Ok(iter) = value.try_iter() {
                        let elem_type = merge_types(iter.map(|v| infer_const_type(&v)));
                        JinjaType::List(Box::new(elem_type))
                    } else {
                        JinjaType::List(Box::new(JinjaType::Unknown))
                    }
                }
                None => JinjaType::List(Box::new(JinjaType::Unknown)),
            }
        }
        ValueKind::Map => {
            JinjaType::Map(Box::new(JinjaType::Unknown), Box::new(JinjaType::Unknown))
        }
        _ => JinjaType::Unknown,
    }
}

/// Merge multiple types into a single type (creating unions if needed).
fn merge_types<I>(types: I) -> JinjaType
where
    I: IntoIterator<Item = JinjaType>,
{
    let mut result: Option<JinjaType> = None;

    for ty in types {
        result = Some(match result {
            None => ty,
            Some(prev) => {
                if ty == prev || ty.is_subtype_of(&prev) {
                    prev
                } else if prev.is_subtype_of(&ty) {
                    ty
                } else {
                    // Create or extend union
                    match prev {
                        JinjaType::Union(mut items) => {
                            items.push(ty);
                            JinjaType::Union(items)
                        }
                        _ => JinjaType::Union(vec![prev, ty]),
                    }
                }
            }
        });
    }

    result.unwrap_or(JinjaType::Unknown)
}

/// Pretty-print an expression for error messages.
fn pretty_print(expr: &ast::Expr) -> String {
    match expr {
        ast::Expr::Var(v) => v.id.to_string(),
        ast::Expr::Const(c) => c.value.to_string(),
        ast::Expr::GetAttr(attr) => format!("{}.{}", pretty_print(&attr.expr), attr.name),
        ast::Expr::GetItem(item) => {
            format!(
                "{}[{}]",
                pretty_print(&item.expr),
                pretty_print(&item.subscript_expr)
            )
        }
        ast::Expr::Call(call) => {
            let args: Vec<_> = call.args.iter().map(|_| "...".to_string()).collect();
            format!("{}({})", pretty_print(&call.expr), args.join(", "))
        }
        _ => "...".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse a Jinja expression and run type inference on it.
    fn infer(expr: &str, env: &JinjaTypeEnv) -> Result<JinjaType, Vec<TypeError>> {
        let ast = minijinja::machinery::parse_expr(expr).expect("failed to parse expression");
        infer_expression_type(&ast, env)
    }

    // ── Literals ──────────────────────────────────────────────────────

    #[test]
    fn test_string_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(infer(r#""hello""#, &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_int_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(infer("42", &env).unwrap(), JinjaType::Int);
    }

    #[test]
    fn test_float_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(infer("3.14", &env).unwrap(), JinjaType::Float);
    }

    #[test]
    fn test_bool_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(infer("true", &env).unwrap(), JinjaType::Bool);
        assert_eq!(infer("false", &env).unwrap(), JinjaType::Bool);
    }

    #[test]
    fn test_none_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(infer("none", &env).unwrap(), JinjaType::None);
    }

    // ── Variables ─────────────────────────────────────────────────────

    #[test]
    fn test_variable_resolution() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);
        assert_eq!(infer("x", &env).unwrap(), JinjaType::Int);
    }

    #[test]
    fn test_unresolved_variable() {
        let env = JinjaTypeEnv::new();
        let errs = infer("missing_var", &env).unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(
            matches!(&errs[0], TypeError::UnresolvedVariable { name, .. } if name == "missing_var")
        );
    }

    // ── Attribute access ──────────────────────────────────────────────

    #[test]
    fn test_class_property_access() {
        let mut env = JinjaTypeEnv::new();
        let mut fields = indexmap::IndexMap::new();
        fields.insert("name".to_string(), JinjaType::String);
        fields.insert("age".to_string(), JinjaType::Int);
        env.add_class("Person", fields);
        env.add_variable("p", JinjaType::ClassRef("Person".to_string()));

        assert_eq!(infer("p.name", &env).unwrap(), JinjaType::String);
        assert_eq!(infer("p.age", &env).unwrap(), JinjaType::Int);
    }

    #[test]
    fn test_class_missing_property() {
        let mut env = JinjaTypeEnv::new();
        env.add_class("Person", indexmap::IndexMap::new());
        env.add_variable("p", JinjaType::ClassRef("Person".to_string()));

        let errs = infer("p.missing", &env).unwrap_err();
        assert!(
            matches!(&errs[0], TypeError::PropertyNotDefined { property, .. } if property == "missing")
        );
    }

    // ── Enum access ───────────────────────────────────────────────────

    #[test]
    fn test_enum_value_access() {
        let mut env = JinjaTypeEnv::new();
        env.add_enum("Color", vec!["Red".to_string(), "Blue".to_string()]);
        env.add_variable("Color", JinjaType::EnumRef("Color".to_string()));

        assert_eq!(
            infer("Color.Red", &env).unwrap(),
            JinjaType::EnumValueRef("Color".to_string())
        );
    }

    #[test]
    fn test_enum_invalid_value() {
        let mut env = JinjaTypeEnv::new();
        env.add_enum("Color", vec!["Red".to_string(), "Blue".to_string()]);
        env.add_variable("Color", JinjaType::EnumRef("Color".to_string()));

        let errs = infer("Color.Green", &env).unwrap_err();
        assert!(
            matches!(&errs[0], TypeError::PropertyNotDefined { property, .. } if property == "Green")
        );
    }

    // ── Filters ───────────────────────────────────────────────────────

    #[test]
    fn test_length_filter() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("items", JinjaType::List(Box::new(JinjaType::Int)));

        assert_eq!(infer("items|length", &env).unwrap(), JinjaType::Int);
    }

    #[test]
    fn test_lower_filter() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("s", JinjaType::String);

        assert_eq!(infer("s|lower", &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_unknown_filter() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::String);

        let errs = infer("x|nonexistent_filter", &env).unwrap_err();
        assert!(
            matches!(&errs[0], TypeError::InvalidFilter { filter_name, .. } if filter_name == "nonexistent_filter")
        );
    }

    // ── Binary operations ─────────────────────────────────────────────

    #[test]
    fn test_arithmetic() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("a", JinjaType::Int);
        env.add_variable("b", JinjaType::Int);

        assert_eq!(infer("a + b", &env).unwrap(), JinjaType::Number);
    }

    #[test]
    fn test_string_concat() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("s", JinjaType::String);

        assert_eq!(infer("s ~ \"!\"", &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_comparison() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("a", JinjaType::Int);

        assert_eq!(infer("a > 0", &env).unwrap(), JinjaType::Bool);
    }

    // ── Function calls ────────────────────────────────────────────────

    #[test]
    fn test_function_call() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "greet",
            JinjaType::String,
            vec![("name".to_string(), JinjaType::String)],
        );
        env.add_variable("greet", JinjaType::FunctionRef("greet".to_string()));

        assert_eq!(
            infer("greet(name=\"world\")", &env).unwrap(),
            JinjaType::String
        );
    }

    #[test]
    fn test_function_missing_arg() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "greet",
            JinjaType::String,
            vec![("name".to_string(), JinjaType::String)],
        );
        env.add_variable("greet", JinjaType::FunctionRef("greet".to_string()));

        let errs = infer("greet()", &env).unwrap_err();
        assert!(matches!(&errs[0], TypeError::MissingArg { arg_name, .. } if arg_name == "name"));
    }

    #[test]
    fn test_function_reference_without_call() {
        let mut env = JinjaTypeEnv::new();
        env.add_function("greet", JinjaType::String, vec![]);
        env.add_variable("greet", JinjaType::FunctionRef("greet".to_string()));

        let errs = infer("greet", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::FunctionReferenceWithoutCall { .. }
        ));
    }

    // ── Conditional expressions ───────────────────────────────────────

    #[test]
    fn test_ternary_expression() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("flag", JinjaType::Bool);

        // Both branches are strings → result is String
        assert_eq!(
            infer(r#""yes" if flag else "no""#, &env).unwrap(),
            JinjaType::String
        );
    }

    // ── List / Map construction ───────────────────────────────────────

    #[test]
    fn test_list_literal() {
        let env = JinjaTypeEnv::new();
        assert_eq!(
            infer("[1, 2, 3]", &env).unwrap(),
            JinjaType::List(Box::new(JinjaType::Int))
        );
    }

    // ── Union attribute access ────────────────────────────────────────

    #[test]
    fn test_union_property_access() {
        let mut env = JinjaTypeEnv::new();
        let mut dog_fields = indexmap::IndexMap::new();
        dog_fields.insert("name".to_string(), JinjaType::String);
        let mut cat_fields = indexmap::IndexMap::new();
        cat_fields.insert("name".to_string(), JinjaType::String);
        env.add_class("Dog", dog_fields);
        env.add_class("Cat", cat_fields);
        env.add_variable(
            "pet",
            JinjaType::Union(vec![
                JinjaType::ClassRef("Dog".to_string()),
                JinjaType::ClassRef("Cat".to_string()),
            ]),
        );

        assert_eq!(infer("pet.name", &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_union_missing_property() {
        let mut env = JinjaTypeEnv::new();
        let mut dog_fields = indexmap::IndexMap::new();
        dog_fields.insert("name".to_string(), JinjaType::String);
        env.add_class("Dog", dog_fields);
        env.add_class("Cat", indexmap::IndexMap::new());
        env.add_variable(
            "pet",
            JinjaType::Union(vec![
                JinjaType::ClassRef("Dog".to_string()),
                JinjaType::ClassRef("Cat".to_string()),
            ]),
        );

        let errs = infer("pet.name", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::PropertyNotFoundInUnion { .. }
        ));
    }

    #[test]
    fn test_alias_union_property_access() {
        let mut env = JinjaTypeEnv::new();
        let mut dog_fields = indexmap::IndexMap::new();
        dog_fields.insert("name".to_string(), JinjaType::String);
        let mut cat_fields = indexmap::IndexMap::new();
        cat_fields.insert("name".to_string(), JinjaType::String);
        env.add_class("Dog", dog_fields);
        env.add_class("Cat", cat_fields);
        // pet is Alias("Pet" -> Union(Dog, Cat)) — like `type Pet = Cat | Dog`
        env.add_variable(
            "pet",
            JinjaType::Alias {
                name: "Pet".to_string(),
                resolved: Box::new(JinjaType::Union(vec![
                    JinjaType::ClassRef("Dog".to_string()),
                    JinjaType::ClassRef("Cat".to_string()),
                ])),
            },
        );

        assert_eq!(infer("pet.name", &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_alias_union_missing_property() {
        let mut env = JinjaTypeEnv::new();
        let mut dog_fields = indexmap::IndexMap::new();
        dog_fields.insert("name".to_string(), JinjaType::String);
        env.add_class("Dog", dog_fields);
        env.add_class("Cat", indexmap::IndexMap::new());
        env.add_variable(
            "pet",
            JinjaType::Alias {
                name: "Pet".to_string(),
                resolved: Box::new(JinjaType::Union(vec![
                    JinjaType::ClassRef("Dog".to_string()),
                    JinjaType::ClassRef("Cat".to_string()),
                ])),
            },
        );

        let errs = infer("pet.name", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::PropertyNotFoundInUnion { .. }
        ));
    }

    // ── Unary operations (type checking) ──────────────────────────────

    #[test]
    fn test_negation_of_number() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);

        // No error — negating a number is fine
        assert_eq!(infer("-x", &env).unwrap(), JinjaType::Number);
    }

    #[test]
    fn test_negation_of_string_errors() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("s", JinjaType::String);

        let errs = infer("-s", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::InvalidType { expected, .. } if expected == "number"
        ));
    }

    #[test]
    fn test_not_returns_bool() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);

        // `not` always returns bool, no error for any type
        assert_eq!(infer("not x", &env).unwrap(), JinjaType::Bool);
    }

    // ── Test expressions (`is` operator) ──────────────────────────────

    #[test]
    fn test_is_defined() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);

        assert_eq!(infer("x is defined", &env).unwrap(), JinjaType::Bool);
    }

    #[test]
    fn test_is_odd() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);

        assert_eq!(infer("x is odd", &env).unwrap(), JinjaType::Bool);
    }

    #[test]
    fn test_is_none() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::None);

        assert_eq!(infer("x is none", &env).unwrap(), JinjaType::Bool);
    }

    #[test]
    fn test_is_unknown_test() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("x", JinjaType::Int);

        let errs = infer("x is foobar", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::InvalidTest { test_name, .. } if test_name == "foobar"
        ));
    }

    // ── GetItem (indexing) ────────────────────────────────────────────

    #[test]
    fn test_list_indexing() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("items", JinjaType::List(Box::new(JinjaType::String)));

        assert_eq!(infer("items[0]", &env).unwrap(), JinjaType::String);
    }

    #[test]
    fn test_map_indexing() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable(
            "m",
            JinjaType::Map(Box::new(JinjaType::String), Box::new(JinjaType::Int)),
        );

        assert_eq!(infer("m[\"key\"]", &env).unwrap(), JinjaType::Int);
    }

    #[test]
    fn test_string_indexing() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("s", JinjaType::String);

        assert_eq!(infer("s[0]", &env).unwrap(), JinjaType::String);
    }

    // ── Slice ─────────────────────────────────────────────────────────

    #[test]
    fn test_list_slice() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("items", JinjaType::List(Box::new(JinjaType::Int)));

        assert_eq!(
            infer("items[1:3]", &env).unwrap(),
            JinjaType::List(Box::new(JinjaType::Int))
        );
    }

    #[test]
    fn test_string_slice() {
        let mut env = JinjaTypeEnv::new();
        env.add_variable("s", JinjaType::String);

        assert_eq!(infer("s[1:3]", &env).unwrap(), JinjaType::String);
    }

    // ── Function call arg type checking ───────────────────────────────

    #[test]
    fn test_function_positional_arg_type_mismatch() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "greet",
            JinjaType::String,
            vec![("name".to_string(), JinjaType::String)],
        );
        env.add_variable("greet", JinjaType::FunctionRef("greet".to_string()));

        let errs = infer("greet(42)", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::WrongArgType { arg_name, expected, found, .. }
                if arg_name == "name" && expected == "string" && found == "int"
        ));
    }

    #[test]
    fn test_function_kwarg_type_mismatch() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "greet",
            JinjaType::String,
            vec![("name".to_string(), JinjaType::String)],
        );
        env.add_variable("greet", JinjaType::FunctionRef("greet".to_string()));

        let errs = infer("greet(name=42)", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::WrongArgType { arg_name, expected, found, .. }
                if arg_name == "name" && expected == "string" && found == "int"
        ));
    }

    #[test]
    fn test_function_arg_subtype_ok() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "compute",
            JinjaType::Number,
            vec![("val".to_string(), JinjaType::Number)],
        );
        env.add_variable("compute", JinjaType::FunctionRef("compute".to_string()));

        // Int is a subtype of Number, so no error
        assert_eq!(infer("compute(42)", &env).unwrap(), JinjaType::Number);
    }

    #[test]
    fn test_function_multiple_args_type_check() {
        let mut env = JinjaTypeEnv::new();
        env.add_function(
            "add",
            JinjaType::String,
            vec![
                ("a".to_string(), JinjaType::String),
                ("b".to_string(), JinjaType::Int),
            ],
        );
        env.add_variable("add", JinjaType::FunctionRef("add".to_string()));

        // First arg is wrong type, second is correct
        let errs = infer("add(123, 456)", &env).unwrap_err();
        assert!(matches!(
            &errs[0],
            TypeError::WrongArgType { arg_name, .. } if arg_name == "a"
        ));
        // Second arg (int) matches Int, so only one error
        assert_eq!(errs.len(), 1);
    }
}
