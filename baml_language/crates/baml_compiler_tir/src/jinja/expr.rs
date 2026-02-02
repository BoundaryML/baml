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

use super::{JinjaType, JinjaTypeEnv, TypeError};
use minijinja::machinery::ast;

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

/// Main expression visitor that infers types.
fn visit_expr(expr: &ast::Expr, errors: &mut Vec<TypeError>, env: &JinjaTypeEnv) -> JinjaType {
    match expr {
        ast::Expr::Var(var) => match env.resolve_variable(var.id) {
            Some(t) => t,
            None => {
                errors.push(TypeError::unresolved_variable(
                    var.id,
                    var.span(),
                    env.variable_names(),
                ));
                JinjaType::Unknown
            }
        },

        ast::Expr::Const(c) => infer_const_type(&c.value),

        ast::Expr::UnaryOp(op_expr) => {
            let expected = match op_expr.op {
                ast::UnaryOpKind::Not => JinjaType::Bool,
                ast::UnaryOpKind::Neg => JinjaType::Number,
            };

            let _inner = visit_expr(&op_expr.expr, errors, env);
            // TODO: Check for type compatibility

            expected
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
            // TODO: Check for type compatibility
            JinjaType::Bool
        }

        ast::Expr::GetAttr(attr_expr) => handle_get_attr(expr, attr_expr, errors, env),

        ast::Expr::GetItem(_item_expr) => {
            // TODO: Implement indexing type inference
            JinjaType::Unknown
        }

        ast::Expr::Slice(_slice_expr) => {
            // TODO: Implement slice type inference
            JinjaType::Unknown
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
                } else {
                    errors.push(TypeError::enum_string_comparison_deprecated(
                        expr,
                        left,
                        expr.span(),
                    ));
                    return Some(JinjaType::Bool);
                }
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
            filter_expr.expr.as_ref().unwrap(),
            &inner,
            expected,
            expr.span(),
        ));
    };

    // List of valid filters (from engine)
    let valid_filters = vec![
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
            errors.push(TypeError::invalid_filter(
                other,
                expr.span(),
                &valid_filters,
            ));
            JinjaType::Unknown
        }
    }
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

        JinjaType::Union(_) => typecheck_attr_access_on_union(&parent, attr_expr, errors, env),

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
    // Extract union items
    let union_items = match union_type {
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
            None,
            attr_expr.span(),
        ));
        return JinjaType::Unknown;
    }

    if has_type_mismatch {
        errors.push(TypeError::property_type_mismatch_in_union(
            &pretty_print(&attr_expr.expr),
            attr_expr.name,
            None,
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
        JinjaType::FunctionRef(_) | JinjaType::Unknown if func_name.is_some() => {
            let name = func_name.unwrap();

            // Look up function signature
            if let Some((return_type, expected_params)) = env.get_function(name) {
                // Validate arguments
                validate_function_call(
                    name,
                    &call_expr.args,
                    expected_params,
                    errors,
                    call_expr.span(),
                );
                return_type.clone()
            } else {
                // Function not found - error already reported by variable resolution
                JinjaType::Unknown
            }
        }
        JinjaType::FunctionRef(name) => {
            // Function reference without call site info
            if let Some((return_type, _)) = env.get_function(&name) {
                return_type.clone()
            } else {
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
) {
    use std::collections::HashSet;

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

    // Check positional arguments
    for (i, arg) in args.iter().enumerate() {
        if let ast::CallArg::Pos(_expr) = arg {
            if i < expected_params.len() {
                let (_param_name, _expected_type) = &expected_params[i];
                // Type checking would go here - simplified for now
                // let arg_type = infer_expression_type(expr, env)?;
                // errors.push(TypeError::wrong_arg_type(...));
            }
        }
    }

    // Check keyword arguments
    let valid_param_names: HashSet<&str> = expected_params
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();

    for arg in args {
        if let ast::CallArg::Kwarg(name, _expr) = arg {
            if !valid_param_names.contains(name) {
                let valid_as_strings: HashSet<&String> =
                    expected_params.iter().map(|(name, _)| name).collect();
                errors.push(TypeError::unknown_arg(
                    func_name,
                    span,
                    name,
                    valid_as_strings,
                ));
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
                if ty == prev {
                    prev
                } else if ty.is_subtype_of(&prev) {
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
