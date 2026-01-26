use std::str::FromStr;

use baml_types::LiteralValue;
use indexmap::IndexMap;
use minijinja::machinery::ast;

use super::{
    pretty_print::pretty_print,
    types::{PredefinedTypes, Type},
    ScopeTracker, TypeError,
};

fn parse_as_function_call(
    expr: &ast::Spanned<ast::Call>,
    state: &mut ScopeTracker,
    types: &PredefinedTypes,
    t: &Type,
) -> (Type, Vec<TypeError>) {
    match t {
        Type::FunctionRef(name) => {
            let mut positional_args = Vec::new();
            let mut kwargs = IndexMap::new();
            for arg in &expr.args {
                match arg {
                    ast::CallArg::Pos(expr) => {
                        let t = tracker_visit_expr(expr, state, types);
                        positional_args.push(t);
                    }
                    ast::CallArg::Kwarg(key, expr) => {
                        let t = tracker_visit_expr(expr, state, types);
                        kwargs.insert(*key, t);
                    }
                    ast::CallArg::PosSplat(_) | ast::CallArg::KwargSplat(_) => {
                        // For now, we'll handle splats as unknown
                        positional_args.push(Type::Unknown);
                    }
                }
            }

            types.check_function_args((name, expr), &positional_args, &kwargs)
        }
        Type::Both(x, y) => {
            match (x.as_ref(), y.as_ref()) {
                (Type::FunctionRef(_), Type::FunctionRef(_)) => {}
                (Type::FunctionRef(_), _) => return parse_as_function_call(expr, state, types, x),
                (_, Type::FunctionRef(_)) => return parse_as_function_call(expr, state, types, y),
                _ => {}
            }

            let (t1, e1) = parse_as_function_call(expr, state, types, x);
            let (t2, e2) = parse_as_function_call(expr, state, types, y);
            match (e1.is_empty(), e2.is_empty()) {
                (true, true) => (Type::merge([t1, t2]), vec![]),
                (true, false) => (t1, e1),
                (false, true) => (t2, e2),
                (false, false) => (Type::merge([t1, t2]), e1.into_iter().chain(e2).collect()),
            }
        }
        Type::Union(items) => {
            let items = items
                .iter()
                .map(|x| parse_as_function_call(expr, state, types, x))
                .reduce(|acc, x| {
                    let (t1, e1) = acc;
                    let (t2, e2) = x;
                    (Type::merge([t1, t2]), e1.into_iter().chain(e2).collect())
                });
            match items {
                Some(x) => x,
                None => (
                    Type::Unknown,
                    vec![TypeError::new_invalid_type(
                        &expr.expr,
                        t,
                        "function",
                        expr.span(),
                    )],
                ),
            }
        }
        _ => (
            Type::Unknown,
            vec![TypeError::new_invalid_type(
                &expr.expr,
                t,
                "function",
                expr.span(),
            )],
        ),
    }
}

// Helper function to check if binary operator is a comparison operator
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

// Helper function to check if union is nullable enum (enum + nullish only)
fn extract_enum_from_nullable_union(types: &[Type]) -> Option<&str> {
    let mut enum_name: Option<&str> = None;

    for t in types {
        match t {
            Type::EnumValueRef(name) => {
                if enum_name.is_some() {
                    // Multiple different enums in union - not a simple nullable enum
                    return None;
                }
                enum_name = Some(name);
            }
            Type::None | Type::Undefined => {
                // Nullish types are allowed in nullable enums
                continue;
            }
            _ => {
                // Any other type (String, Int, etc.) means this isn't a nullable enum
                return None;
            }
        }
    }

    enum_name
}

// Helper function to handle enum binary operations
fn handle_enum_binary_operation(
    expr: &ast::Expr,
    bin_expr: &ast::Spanned<ast::BinOp>,
    lhs: &Type,
    rhs: &Type,
    state: &mut ScopeTracker,
    types: &PredefinedTypes,
) -> Option<Type> {
    // First check for nullable enum patterns before strict enum handling
    // Handle nullable enum to string literal comparisons
    if let (Type::Union(union_types), Type::Literal(LiteralValue::String(str_val))) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                state.errors.push(TypeError::new_enum_literal_suggestion(
                    expr,
                    enum_name,
                    str_val,
                    types,
                    expr.span(),
                ));
                return Some(Type::Bool);
            }
        }
    }
    if let (Type::Literal(LiteralValue::String(str_val)), Type::Union(union_types)) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                state.errors.push(TypeError::new_enum_literal_suggestion(
                    expr,
                    enum_name,
                    str_val,
                    types,
                    expr.span(),
                ));
                return Some(Type::Bool);
            }
        }
    }

    // Handle nullable enum vs generic string
    if let (Type::Union(union_types), Type::String) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                state.errors.push(TypeError::new_enum_string_cmp_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                return Some(Type::Bool);
            }
        }
    }
    if let (Type::String, Type::Union(union_types)) = (lhs, rhs) {
        if let Some(enum_name) = extract_enum_from_nullable_union(union_types) {
            if is_comparison_op(&bin_expr.op) {
                state.errors.push(TypeError::new_enum_string_cmp_deprecated(
                    expr,
                    enum_name,
                    expr.span(),
                ));
                return Some(Type::Bool);
            }
        }
    }

    // Handle nullable-to-nullable enum comparisons
    if let (Type::Union(left_types), Type::Union(right_types)) = (lhs, rhs) {
        let left_enum = extract_enum_from_nullable_union(left_types);
        let right_enum = extract_enum_from_nullable_union(right_types);

        if let (Some(left), Some(right)) = (left_enum, right_enum) {
            if is_comparison_op(&bin_expr.op) {
                if left == right {
                    return Some(Type::Bool);
                } else {
                    state.errors.push(TypeError::new_enum_literal_suggestion(
                        expr,
                        left,
                        "different_enum",
                        types,
                        expr.span(),
                    ));
                    return Some(Type::Bool);
                }
            }
        }
    }

    // Now check if either operand is an EnumValueRef for strict handling
    match (lhs, rhs) {
        // Both are EnumValueRef - only allow comparison ops between same enum
        (Type::EnumValueRef(e1), Type::EnumValueRef(e2)) => {
            match &bin_expr.op {
                op if is_comparison_op(op) => {
                    if e1 == e2 {
                        Some(Type::Bool)
                    } else {
                        state.errors.push(TypeError::new_enum_literal_suggestion(
                            expr,
                            e1,
                            "different_enum",
                            types,
                            expr.span(),
                        ));
                        Some(Type::Unknown)
                    }
                }
                _ => {
                    // Disallow arithmetic/string ops on enums
                    state.errors.push(TypeError::new_enum_literal_suggestion(
                        expr,
                        e1,
                        "arithmetic_operation",
                        types,
                        expr.span(),
                    ));
                    Some(Type::Unknown)
                }
            }
        }
        // EnumValueRef with string literal - suggest proper enum syntax
        (Type::EnumValueRef(enum_name), Type::Literal(LiteralValue::String(str_val)))
        | (Type::Literal(LiteralValue::String(str_val)), Type::EnumValueRef(enum_name)) => {
            match &bin_expr.op {
                op if is_comparison_op(op) => {
                    state.errors.push(TypeError::new_enum_literal_suggestion(
                        expr,
                        enum_name,
                        str_val,
                        types,
                        expr.span(),
                    ));
                    Some(Type::Bool)
                }
                _ => {
                    // Disallow arithmetic/string ops on enums
                    state.errors.push(TypeError::new_enum_literal_suggestion(
                        expr,
                        enum_name,
                        str_val,
                        types,
                        expr.span(),
                    ));
                    Some(Type::Unknown)
                }
            }
        }
        // EnumValueRef with generic string - placeholder message
        (Type::EnumValueRef(enum_name), Type::String)
        | (Type::String, Type::EnumValueRef(enum_name)) => {
            match &bin_expr.op {
                op if is_comparison_op(op) => {
                    state.errors.push(TypeError::new_enum_string_cmp_deprecated(
                        expr,
                        enum_name,
                        expr.span(),
                    ));
                    Some(Type::Bool)
                }
                _ => {
                    // Disallow arithmetic/string ops on enums
                    state.errors.push(TypeError::new_enum_string_cmp_deprecated(
                        expr,
                        enum_name,
                        expr.span(),
                    ));
                    Some(Type::Unknown)
                }
            }
        }
        // Any other combination with EnumValueRef is invalid
        (Type::EnumValueRef(enum_name), _) | (_, Type::EnumValueRef(enum_name)) => {
            state.errors.push(TypeError::new_enum_string_cmp_deprecated(
                expr,
                enum_name,
                expr.span(),
            ));
            Some(Type::Unknown)
        }
        // No enums involved - return None to fall through to normal operator handling
        _ => None,
    }
}

fn tracker_visit_expr(
    expr: &ast::Expr<'_>,
    state: &mut ScopeTracker,
    types: &PredefinedTypes,
) -> Type {
    match expr {
        ast::Expr::Var(var) => match types.resolve(var.id) {
            Some(t) => t,
            None => {
                state.errors.push(TypeError::new_unresolved_variable(
                    var.id,
                    var.span(),
                    types.variable_names(),
                ));
                Type::Unknown
            }
        },
        ast::Expr::Const(c) => infer_const_type(&c.value),
        ast::Expr::UnaryOp(expr) => {
            let expected = match expr.op {
                ast::UnaryOpKind::Not => Type::Bool,
                ast::UnaryOpKind::Neg => Type::Number,
            };

            let _inner = tracker_visit_expr(&expr.expr, state, types);
            // TODO: Check for type compatibility

            expected
        }
        ast::Expr::BinOp(bin_expr) => {
            let lhs = tracker_visit_expr(&bin_expr.left, state, types);
            let rhs = tracker_visit_expr(&bin_expr.right, state, types);

            // Handle enum operations with the helper function
            if let Some(result) =
                handle_enum_binary_operation(expr, bin_expr, &lhs, &rhs, state, types)
            {
                return result;
            }

            // No enums involved - fall through to normal operator handling
            match bin_expr.op {
                ast::BinOpKind::Add => {
                    if lhs.is_subtype_of(&Type::String) || rhs.is_subtype_of(&Type::String) {
                        Type::String
                    } else {
                        Type::Number
                    }
                }
                ast::BinOpKind::Sub => Type::Number,
                ast::BinOpKind::Mul => Type::Number,
                ast::BinOpKind::Div => Type::Number,
                ast::BinOpKind::Pow => Type::Number,
                ast::BinOpKind::FloorDiv => Type::Number,
                ast::BinOpKind::Rem => Type::Number,
                ast::BinOpKind::Eq => Type::Bool,
                ast::BinOpKind::Ne => Type::Bool,
                ast::BinOpKind::Lt => Type::Bool,
                ast::BinOpKind::Gt => Type::Bool,
                ast::BinOpKind::Lte => Type::Bool,
                ast::BinOpKind::Gte => Type::Bool,
                ast::BinOpKind::In => Type::Bool,
                ast::BinOpKind::Concat => Type::String,
                ast::BinOpKind::ScAnd => Type::Bool,
                ast::BinOpKind::ScOr => Type::Bool,
            }
        }
        ast::Expr::IfExpr(expr) => {
            let _test = tracker_visit_expr(&expr.test_expr, state, types);

            let true_expr = tracker_visit_expr(&expr.true_expr, state, types);
            let false_expr = expr
                .false_expr
                .as_ref()
                .map(|x| tracker_visit_expr(x, state, types))
                .unwrap_or(Type::Unknown);
            Type::merge([true_expr, false_expr])
        }
        ast::Expr::Filter(expr) => {
            // Filters have a name
            let inner = tracker_visit_expr(expr.expr.as_ref().unwrap(), state, types);

            let mut ensure_type = |error_string: &str| {
                state.errors.push(TypeError::new_invalid_type(
                    expr.expr.as_ref().unwrap(),
                    &inner,
                    error_string,
                    expr.span(),
                ));
            };

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
            match expr.name {
                "abs" => {
                    if !inner.is_subtype_of(&Type::Number) {
                        ensure_type("number");
                    }
                    Type::Number
                }
                "attrs" => Type::Unknown,
                "batch" => Type::Unknown,
                "bool" => Type::Bool,
                "capitalize" | "escape" => {
                    if !inner.is_subtype_of(&Type::String) {
                        ensure_type("string");
                    }
                    Type::String
                }
                "first" | "last" => match inner {
                    Type::List(t) => Type::merge([*t, Type::None]),
                    Type::Unknown => Type::Unknown,
                    _ => {
                        ensure_type("list");
                        Type::Unknown
                    }
                },
                "default" => Type::Unknown,
                "float" => Type::Float,
                "indent" => Type::String,
                "int" => Type::Int,
                "dictsort" | "items" => match inner {
                    Type::Map(k, v) => Type::List(Box::new(Type::Tuple(vec![*k, *v]))),
                    Type::ClassRef(_) => {
                        Type::List(Box::new(Type::Tuple(vec![Type::String, Type::Unknown])))
                    }
                    _ => {
                        ensure_type("map or class");
                        Type::Unknown
                    }
                },
                "join" => Type::String,
                "length" => match inner {
                    Type::List(_) | Type::String | Type::ClassRef(_) | Type::Map(_, _) => Type::Int,
                    Type::Unknown => Type::Unknown,
                    _ => {
                        ensure_type("list, string, class or map");
                        Type::Unknown
                    }
                },
                "list" => Type::List(Box::new(Type::Unknown)),
                "lower" | "upper" => {
                    if !inner.is_subtype_of(&Type::String) {
                        ensure_type("string");
                    }
                    Type::String
                }
                "map" => Type::Unknown,
                "max" => Type::Unknown,
                "min" => Type::Unknown,
                "pprint" => Type::Unknown,
                "regex_match" => Type::Bool,
                "reject" => Type::Unknown,
                "rejectattr" => Type::Unknown,
                "replace" => Type::String,
                "reverse" => Type::Unknown,
                "round" => Type::Float,
                "safe" => Type::String,
                "select" => Type::Unknown,
                "selectattr" => Type::Unknown,
                "slice" => Type::Unknown,
                "sort" => Type::Unknown,
                "split" => Type::List(Box::new(Type::String)),
                "sum" => match inner.clone() {
                    Type::List(elem_type) => {
                        if elem_type.is_subtype_of(&Type::Float) {
                            Type::Float
                        } else if elem_type.is_subtype_of(&Type::Int) {
                            Type::Int
                        } else {
                            ensure_type("(int|float)[]");
                            Type::String
                        }
                    }
                    _ => {
                        ensure_type("(int|float)[]");
                        Type::Bool
                    }
                },
                "title" => Type::String,
                "tojson" | "json" => Type::String,
                "format" => Type::String,
                "trim" => Type::String,
                "unique" => Type::Unknown,
                "urlencode" => Type::String,
                other => {
                    state.errors.push(TypeError::new_invalid_filter(
                        other,
                        expr.span(),
                        &valid_filters,
                    ));
                    Type::Unknown
                }
            }
        }
        ast::Expr::Test(expr) => {
            let _test = tracker_visit_expr(&expr.expr, state, types);
            // TODO: Check for type compatibility
            Type::Bool
        }
        ast::Expr::GetAttr(expr) => {
            let parent = tracker_visit_expr(&expr.expr, state, types);

            match &parent {
                Type::ClassRef(c) => {
                    let (t, err) = types.check_class_property(
                        &pretty_print(&expr.expr),
                        c,
                        expr.name,
                        expr.span(),
                    );
                    if let Some(e) = err {
                        state.errors.push(e);
                    }
                    t
                }
                Type::EnumTypeRef(e) => {
                    let (t, err) = types.check_enum_property(
                        &pretty_print(&expr.expr),
                        e,
                        expr.name,
                        expr.span(),
                    );
                    if let Some(e) = err {
                        state.errors.push(e);
                    }
                    t
                }
                Type::EnumValueRef(enum_value) => match expr.name {
                    "value" => Type::String,
                    _ => {
                        state.errors.push(TypeError::new_enum_value_property_error(
                            &pretty_print(&expr.expr),
                            enum_value,
                            expr.name,
                            expr.span(),
                        ));
                        Type::Unknown
                    }
                },
                Type::Union(_) | Type::Alias { .. } => {
                    typecheck_attr_access_on_union(&parent, expr, types, state)
                }
                Type::Unknown => Type::Unknown,
                other => expected_class_got(other, expr, state),
            }
        }
        ast::Expr::GetItem(_expr) => Type::Unknown,
        ast::Expr::Slice(_slice) => Type::Unknown,
        ast::Expr::Call(expr) => {
            let func = tracker_visit_expr(&expr.expr, state, types);
            let (t, errs) = parse_as_function_call(expr, state, types, &func);
            state.errors.extend(errs);
            t
        }
        ast::Expr::List(expr) => {
            let inner = Type::merge(
                expr.items
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            Type::List(Box::new(inner))
        }
        ast::Expr::Map(expr) => {
            let keys = Type::merge(
                expr.keys
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            let values = Type::merge(
                expr.values
                    .iter()
                    .map(|x| tracker_visit_expr(x, state, types)),
            );
            Type::Map(Box::new(keys), Box::new(values))
        }
    }
}

fn infer_const_type(v: &minijinja::value::Value) -> Type {
    match v.kind() {
        minijinja::value::ValueKind::Undefined => Type::Undefined,
        minijinja::value::ValueKind::None => Type::None,
        minijinja::value::ValueKind::Bool => match bool::from_str(&v.to_string()) {
            Ok(b) => Type::Literal(LiteralValue::Bool(b)),
            Err(_) => Type::Bool,
        },
        minijinja::value::ValueKind::String => Type::Literal(LiteralValue::String(v.to_string())),
        minijinja::value::ValueKind::Seq => match v.len() {
            Some(0) => Type::List(Box::new(Type::Unknown)),
            Some(_) => {
                if let Ok(iter) = v.try_iter() {
                    let inner = iter
                        .map(|x| infer_const_type(&x))
                        .fold(None, |acc, x| match acc {
                            None => Some(x),
                            Some(Type::Union(acc)) => {
                                let t = Type::Union(acc);
                                if x.is_subtype_of(&t) {
                                    Some(t)
                                } else if let Type::Union(mut acc) = t {
                                    acc.push(x);
                                    Some(Type::Union(acc))
                                } else {
                                    unreachable!("minijinja")
                                }
                            }
                            Some(acc) => {
                                if x.is_subtype_of(&acc) {
                                    Some(acc)
                                } else {
                                    Some(Type::Union(vec![acc, x]))
                                }
                            }
                        })
                        .unwrap_or(Type::Unknown);
                    Type::List(Box::new(inner))
                } else {
                    Type::List(Box::new(Type::Unknown))
                }
            }
            None => Type::List(Box::new(Type::Unknown)),
        },
        minijinja::value::ValueKind::Map => Type::Unknown,
        // We don't handle these types
        minijinja::value::ValueKind::Number => match i64::from_str(&v.to_string()) {
            Ok(i) => Type::Literal(LiteralValue::Int(i)),
            Err(_) => Type::Number,
        },
        minijinja::value::ValueKind::Bytes => Type::Undefined,
        minijinja::value::ValueKind::Iterable => Type::Unknown,
        minijinja::value::ValueKind::Plain => Type::Unknown,
        minijinja::value::ValueKind::Invalid => Type::Unknown,
        _ => Type::Unknown,
    }
}

pub fn evaluate_type(expr: &ast::Expr, types: &PredefinedTypes) -> Result<Type, Vec<TypeError>> {
    let mut state = ScopeTracker::new();
    // Lint: bare function reference without call, e.g. `{{ MyTemplateString }}` vs `{{ MyTemplateString() }}`
    if let ast::Expr::Var(var) = expr {
        if let Some((_, _)) = types.as_function(var.id) {
            state
                .errors
                .push(TypeError::new_function_reference_without_call(
                    var.id,
                    var.span(),
                ));
        }
    }
    let result = tracker_visit_expr(expr, &mut state, types);

    if state.errors.is_empty() {
        Ok(result)
    } else {
        Err(state.errors)
    }
}

/// Verifies that an attribute is present in all items of a union.
///
/// This is used especially for if statements like `if v.kind == "X"` where v
/// is a union of types and we need to check that `kind` is present in all of
/// the types, thus making the attr access valid in every case, therefore not
/// a type error.
///
/// This functions returns the type of the attr if present in all items.
/// Otherwise, it returns [`Type::Unknown`] and pushes a type error to the
/// `state` param.
///
/// TODO: This function is very similar to `narrow_attr_access_on_union_var` in
/// `stmt.rs`. Reusing the code is not straightforward though (at least if we
/// want it to be readable), but we should try something because this is kind of
/// error prone if we add more types that need to be covered.
fn typecheck_attr_access_on_union(
    union_type: &Type,
    get_attr: &ast::Spanned<ast::GetAttr<'_>>,
    types: &PredefinedTypes,
    state: &mut ScopeTracker,
) -> Type {
    // Extract union name if this is a type alias
    let union_name = match union_type {
        Type::Alias { name, .. } => Some(name.as_str()),
        _ => None,
    };

    // Resolve items.
    let union_items = match union_type {
        Type::Union(items) => items,
        Type::Alias { resolved, .. } => match resolved.as_ref() {
            Type::Union(items) => items,
            _ => return expected_class_got(union_type, get_attr, state),
        },
        _ => {
            return expected_class_got(union_type, get_attr, state);
        }
    };

    // Attribute must be present on all items of the union and also have the
    // same type.
    let mut attr_type = None;
    let mut classes_missing_property: Vec<&str> = Vec::new();
    let mut has_type_mismatch = false;

    // Search recursively for all types in the union to check
    // if they all contain the property.
    let mut stack = Vec::from_iter(union_items.iter());

    while let Some(union_item_type) = stack.pop() {
        match union_item_type {
            Type::ClassRef(class_name) => {
                // Get type of prop
                let (class_prop_type, err) = types.check_class_property(
                    &pretty_print(&get_attr.expr),
                    class_name,
                    get_attr.name,
                    get_attr.span(),
                );

                // Prop not found in one of the types - track it
                if err.is_some() {
                    classes_missing_property.push(class_name);
                    continue;
                }

                // Check if previous type matches the current one
                match &attr_type {
                    None => attr_type = Some(class_prop_type),

                    Some(prev_type) => {
                        // Found two distinct types for the same prop.
                        if !class_prop_type.equals_ignoring_literal_values(prev_type) {
                            has_type_mismatch = true;
                        }
                    }
                }
            }

            // Resolve aliases.
            Type::Alias { resolved, .. } => stack.push(resolved),

            // Recurse into nested unions
            Type::Union(nested) => stack.extend(nested.iter()),

            // Found a type that's not a class, stop here.
            _ => {
                let variable_name = pretty_print(&get_attr.expr);
                state.errors.push(TypeError::new_non_class_in_union(
                    &variable_name,
                    get_attr.name,
                    &union_item_type.name(),
                    get_attr.span(),
                ));
                return Type::Unknown;
            }
        }
    }

    // Report specific errors based on what went wrong
    if !classes_missing_property.is_empty() {
        let variable_name = pretty_print(&get_attr.expr);
        state
            .errors
            .push(TypeError::new_property_not_found_in_union(
                &variable_name,
                get_attr.name,
                &classes_missing_property,
                union_name,
                get_attr.span(),
            ));
        return Type::Unknown;
    }

    if has_type_mismatch {
        let variable_name = pretty_print(&get_attr.expr);
        state
            .errors
            .push(TypeError::new_property_type_mismatch_in_union(
                &variable_name,
                get_attr.name,
                union_name,
                get_attr.span(),
            ));
        return Type::Unknown;
    }

    match attr_type {
        Some(attr_type) => attr_type,
        None => expected_class_got(union_type, get_attr, state),
    }
}

/// Helper for [`typecheck_attr_access_on_union`].
/// Used when the type is not a union at all (e.g., primitive type alias).
fn expected_class_got(
    got: &Type,
    get_attr: &ast::Spanned<ast::GetAttr<'_>>,
    state: &mut ScopeTracker,
) -> Type {
    state.errors.push(TypeError::new_invalid_type(
        &get_attr.expr,
        got,
        "class",
        get_attr.span(),
    ));

    Type::Unknown
}
