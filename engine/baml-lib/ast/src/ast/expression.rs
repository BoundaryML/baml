use baml_types::{TypeValue, UnresolvedValue as UnresolvedValueBase};
use internal_baml_diagnostics::Diagnostics;

type UnresolvedValue = UnresolvedValueBase<Span>;

use std::fmt;

use baml_types::JinjaExpression;
use bstd::dedent;

use super::{app::App, ArgumentsList, Header, Identifier, Stmt, WithName, WithSpan};
use crate::ast::Span;

#[derive(Debug, Clone)]
pub struct RawString {
    raw_span: Span,
    #[allow(dead_code)]
    pub raw_value: String,
    pub inner_value: String,

    // This is useful for getting the final offset.
    pub indent: usize,
    inner_span_start: usize,
}

impl WithSpan for RawString {
    fn span(&self) -> &Span {
        &self.raw_span
    }
}

impl RawString {
    pub(crate) fn new(value: String, span: Span) -> Self {
        let dedented_value = value.trim_start_matches(['\n', '\r']);
        let start_trim_count = value.len() - dedented_value.len();
        let dedented_value = dedented_value.trim_end();
        let dedented = dedent(dedented_value);
        Self {
            raw_span: span,
            raw_value: value,
            inner_value: dedented.content,
            indent: dedented.indent_size,
            inner_span_start: start_trim_count,
        }
    }

    pub fn value(&self) -> &str {
        &self.inner_value
    }

    pub fn raw_value(&self) -> &str {
        &self.raw_value
    }

    pub fn to_raw_span(&self, span: pest::Span<'_>) -> Span {
        let start_idx = span.start();
        let end_idx = span.end();
        // Count number of \n in the raw string before the start of the span.
        let start_line_count = self.value()[..start_idx]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        let end_line_count = self.value()[..end_idx]
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();

        Span {
            file: self.raw_span.file.clone(),
            start: self.raw_span.start
                + self.inner_span_start
                + self.indent * start_line_count
                + span.start(),
            end: self.raw_span.start
                + self.inner_span_start
                + self.indent * end_line_count
                + span.end(),
        }
    }

    pub fn assert_eq_up_to_span(&self, other: &RawString) {
        assert_eq!(self.inner_value, other.inner_value);
        assert_eq!(self.raw_value, other.raw_value);
        assert_eq!(self.indent, other.indent);
    }
}

/// Represents arbitrary, even nested, expressions.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Boolean values aka true or false
    BoolValue(bool, Span),
    /// Any numeric value e.g. floats or ints.
    NumericValue(String, Span),
    /// An identifier
    Identifier(Identifier),
    /// Any string value.
    StringValue(String, Span),
    /// Any string value.
    RawStringValue(RawString),
    /// An array of other values.
    Array(Vec<Expression>, Span),
    /// A mapping function.
    Map(Vec<(Expression, Expression)>, Span),
    /// A JinjaExpression. e.g. "this|length > 5".
    JinjaExpressionValue(JinjaExpression, Span),
    /// Function abstraction.
    Lambda(ArgumentsList, Box<ExpressionBlock>, Span),
    /// Function Application
    /// TODO: Function should be an Expression, not an Identifier.
    App(App),
    /// A class constructor, e.g. `MyClass { x = 1, y = 2 }`.
    ClassConstructor(ClassConstructor, Span),
    /// An expression block, e.g. `{ let x = 1; x + 2 }`.
    ExprBlock(ExpressionBlock, Span),
    /// An if expression, e.g. `if x == 1 { "one" } else { "not one" }`.
    If(
        Box<Expression>,
        Box<Expression>,
        Option<Box<Expression>>,
        Span,
    ),
    /// Array/Map access, e.g. `arr[0]` or `map["key"]`
    ArrayAccess(Box<Expression>, Box<Expression>, Span),
    /// Field access, e.g. `obj.field`
    FieldAccess(Box<Expression>, Identifier, Span),
    MethodCall {
        receiver: Box<Expression>,
        method: Identifier,
        args: Vec<Expression>,
        type_args: Vec<super::FieldType>,
        span: Span,
    },
    /// Any form of binary operation.
    BinaryOperation {
        left: Box<Expression>,
        operator: BinaryOperator,
        right: Box<Expression>,
        span: Span,
    },
    /// Any form of unary operation.
    UnaryOperation {
        operator: UnaryOperator,
        expr: Box<Expression>,
        span: Span,
    },
    // No-op, just so we can keep track of parenthesis (Pratt Parser discards them).
    Paren(Box<Expression>, Span),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BinaryOperator {
    /// The `==` operator (equal).
    Eq,
    /// The `!=` operator (not equal).
    Neq,
    /// The `<` operator (less than).
    Lt,
    /// The `<=` operator (less than or equal).
    LtEq,
    /// The `>` operator (greater than).
    Gt,
    /// The `>=` operator (greater than or equal).
    GtEq,
    /// The `+` operator (addition).
    Add,
    /// The `-` operator (subtraction).
    Sub,
    /// The `*` operator (multiplication).
    Mul,
    /// The `/` operator (division).
    Div,
    /// The `%` operator (modulus).
    Mod,
    /// The `&` operator (bitwise and).
    BitAnd,
    /// The `|` operator (bitwise or).
    BitOr,
    /// The `^` operator (bitwise xor).
    BitXor,
    /// The `<<` operator (shift left).
    Shl,
    /// The `>>` operator (shift right).
    Shr,
    /// The `&&` operator (logical and).
    And,
    /// The `||` operator (logical or).
    Or,
    /// The `instanceof` operator (instance of).
    InstanceOf,
}

impl fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOperator::Eq => write!(f, "=="),
            BinaryOperator::Neq => write!(f, "!="),
            BinaryOperator::Lt => write!(f, "<"),
            BinaryOperator::LtEq => write!(f, "<="),
            BinaryOperator::Gt => write!(f, ">"),
            BinaryOperator::GtEq => write!(f, ">="),
            BinaryOperator::Add => write!(f, "+"),
            BinaryOperator::Sub => write!(f, "-"),
            BinaryOperator::Mul => write!(f, "*"),
            BinaryOperator::Div => write!(f, "/"),
            BinaryOperator::Mod => write!(f, "%"),
            BinaryOperator::BitAnd => write!(f, "&"),
            BinaryOperator::BitOr => write!(f, "|"),
            BinaryOperator::BitXor => write!(f, "^"),
            BinaryOperator::Shl => write!(f, "<<"),
            BinaryOperator::Shr => write!(f, ">>"),
            BinaryOperator::And => write!(f, "&&"),
            BinaryOperator::Or => write!(f, "||"),
            BinaryOperator::InstanceOf => write!(f, "instanceof"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum UnaryOperator {
    Not,
    Neg,
}

impl fmt::Display for UnaryOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOperator::Not => write!(f, "!"),
            UnaryOperator::Neg => write!(f, "-"),
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Identifier(id) => fmt::Display::fmt(id.name(), f),
            Expression::BoolValue(val, _) => fmt::Display::fmt(val, f),
            Expression::NumericValue(val, _) => fmt::Display::fmt(val, f),
            Expression::StringValue(val, _) => write!(f, "{}", crate::string_literal(val)),
            Expression::RawStringValue(val, ..) => {
                write!(f, "{}", crate::string_literal(val.value()))
            }
            Expression::JinjaExpressionValue(val, ..) => fmt::Display::fmt(val, f),
            Expression::Array(vals, _) => {
                let vals = vals
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "[{vals}]")
            }
            Expression::Map(vals, _) => {
                let vals = vals
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(",");
                write!(f, "{{{vals}}}")
            }
            Expression::ClassConstructor(cc, ..) => {
                write!(f, "{} {{", cc.class_name)?;
                for field in &cc.fields {
                    match field {
                        ClassConstructorField::Named(name, expr) => {
                            write!(f, " {name}: {expr};")?;
                        }
                        ClassConstructorField::Spread(expr) => {
                            write!(f, " ..{expr};")?;
                        }
                    }
                }
                write!(f, "}}")
            }
            Expression::Lambda(args, body, _span) => {
                write!(f, "{args} => {body}")
            }
            Expression::App(app) => {
                write!(f, "{}(", app.name)?;
                for arg in &app.args {
                    write!(f, "{arg},")?; // TODO: Drop the comma for the last argument.
                }
                write!(f, ")")?;
                Ok(())
            }
            Expression::ExprBlock(block, _span) => {
                write!(f, "{{")?;
                for stmt in &block.stmts {
                    write!(f, "{stmt};")?;
                }
                if let Some(expr) = &block.expr {
                    write!(f, "{expr}")?;
                }
                write!(f, "}}")
            }
            Expression::If(cond, then, else_, _span) => match else_ {
                Some(else_) => write!(f, "if {cond} {{ {then} }} else {{ {else_} }}"),
                None => write!(f, "if {cond} {{ {then} }}"),
            },
            Expression::ArrayAccess(base, index, _span) => write!(f, "{base}[{index}]"),
            Expression::FieldAccess(base, field, _span) => write!(f, "{base}.{}", field.name()),
            Expression::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                write!(
                    f,
                    "{receiver}.{method}({})",
                    args.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expression::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => {
                write!(f, "({left} {operator} {right})")
            }
            Expression::UnaryOperation { operator, expr, .. } => {
                write!(f, "{operator}{expr}")
            }
            Expression::Paren(expr, _span) => write!(f, "({expr})"),
        }
    }
}

impl Expression {
    pub fn from_json(value: serde_json::Value, span: Span, empty_span: Span) -> Expression {
        match value {
            serde_json::Value::Null => Expression::StringValue("Null".to_string(), empty_span),
            serde_json::Value::Bool(b) => Expression::BoolValue(b, span),
            serde_json::Value::Number(n) => Expression::NumericValue(n.to_string(), span),
            serde_json::Value::String(s) => Expression::StringValue(s, span),
            serde_json::Value::Array(arr) => {
                let arr = arr
                    .into_iter()
                    .map(|v| Expression::from_json(v, empty_span.clone(), empty_span.clone()))
                    .collect();
                Expression::Array(arr, span)
            }
            serde_json::Value::Object(obj) => {
                let obj = obj
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            Expression::StringValue(k, empty_span.clone()),
                            Expression::from_json(v, empty_span.clone(), empty_span.clone()),
                        )
                    })
                    .collect();
                Expression::Map(obj, span)
            }
        }
    }

    pub fn as_array(&self) -> Option<(&[Expression], &Span)> {
        match self {
            Expression::Array(arr, span) => Some((arr, span)),
            _ => None,
        }
    }

    pub fn as_path_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) if !(s == "true" || s == "false") => Some((s, span)),
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Ref(id, span)) => Some((&id.full_name, span)),
            _ => None,
        }
    }

    pub fn as_string_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(s, span) if !(s == "true" || s == "false") => Some((s, span)),
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
            Expression::Identifier(Identifier::String(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Invalid(id, span)) => Some((id, span)),
            Expression::Identifier(Identifier::Local(id, span)) => Some((id, span)),
            _ => None,
        }
    }

    pub fn as_raw_string_value(&self) -> Option<&RawString> {
        match self {
            Expression::RawStringValue(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_identifer(&self) -> Option<&Identifier> {
        match self {
            Expression::Identifier(id) => Some(id),
            _ => None,
        }
    }

    pub fn as_constant_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::StringValue(val, span) => Some((val, span)),
            Expression::RawStringValue(s) => Some((s.value(), s.span())),
            Expression::Identifier(idn) if idn.is_valid_value() => Some((idn.name(), idn.span())),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<(&[(Expression, Expression)], &Span)> {
        match self {
            Expression::Map(map, span) => Some((map, span)),
            _ => None,
        }
    }

    pub fn as_numeric_value(&self) -> Option<(&str, &Span)> {
        match self {
            Expression::NumericValue(s, span) => Some((s, span)),
            _ => None,
        }
    }

    pub fn span(&self) -> &Span {
        match &self {
            Self::BoolValue(_, span) => span,
            Self::NumericValue(_, span) => span,
            Self::StringValue(_, span) => span,
            Self::RawStringValue(r) => r.span(),
            Self::JinjaExpressionValue(_, span) => span,
            Self::Identifier(id) => id.span(),
            Self::Map(_, span) => span,
            Self::Array(_, span) => span,
            Self::ClassConstructor(_, span) => span,
            Self::Lambda(_, _, span) => span,
            Self::App(app) => app.span(),
            Self::ExprBlock(_, span) => span,
            Self::If(_, _, _, span) => span,
            Self::ArrayAccess(_, _, span) => span,
            Self::FieldAccess(_, _, span) => span,
            Self::MethodCall { span, .. } => span,
            Self::BinaryOperation { span, .. } => span,
            Self::UnaryOperation { span, .. } => span,
            Self::Paren(_, span) => span,
        }
    }

    pub fn is_env_expression(&self) -> bool {
        matches!(self, Self::Identifier(Identifier::ENV(..)))
    }

    /// Creates a friendly readable representation for a value's type.
    pub fn describe_value_type(&self) -> &str {
        match self {
            Expression::BoolValue(_, _) => "boolean",
            Expression::NumericValue(_, _) => "numeric",
            Expression::StringValue(_, _) => "string",
            Expression::RawStringValue(_) => "raw_string",
            Expression::JinjaExpressionValue(_, _) => "jinja_expression",
            Expression::Identifier(id) => match id {
                Identifier::String(_, _) => "string",
                Identifier::Local(_, _) => "local_type",
                Identifier::Ref(_, _) => "ref_type",
                Identifier::ENV(_, _) => "env_type",
                Identifier::Invalid(_, _) => "invalid_type",
            },
            Expression::Map(_, _) => "map",
            Expression::Array(_, _) => "array",
            Expression::ClassConstructor(cc, _) => cc.class_name.name(),
            Expression::Lambda(_, _, _) => "function",
            Expression::App(_) => "function_application",
            Expression::ExprBlock(_, _) => "expression_block",
            Expression::If(_, _, _, _) => "if_expression",
            Expression::ArrayAccess(_, _, _) => "array_access",
            Expression::FieldAccess(_, _, _) => "field_access",
            Expression::MethodCall { .. } => "method_call",
            Expression::BinaryOperation { .. } => "binary_operation",
            Expression::UnaryOperation { .. } => "unary_operation",
            Expression::Paren(_, _) => "parenthesized_expression",
        }
    }

    pub fn is_map(&self) -> bool {
        matches!(self, Expression::Map(_, _))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Expression::Array(_, _))
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Expression::StringValue(_, _)
                | Expression::RawStringValue(_)
                | Expression::Identifier(Identifier::String(_, _))
                | Expression::Identifier(Identifier::Invalid(_, _))
                | Expression::Identifier(Identifier::Local(_, _))
        )
    }

    pub fn assert_eq_up_to_span(&self, other: &Expression) {
        use Expression::*;
        match (self, other) {
            (BoolValue(v1, _), BoolValue(v2, _)) => assert_eq!(v1, v2),
            (BoolValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (NumericValue(n1, _), NumericValue(n2, _)) => assert_eq!(n1, n2),
            (NumericValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Identifier(i1), Identifier(i2)) => assert_eq!(i1, i2),
            (Identifier(_), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (StringValue(s1, _), StringValue(s2, _)) => assert_eq!(s1, s2),
            (StringValue(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (RawStringValue(s1), RawStringValue(s2)) => s1.assert_eq_up_to_span(s2),
            (RawStringValue(_), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (JinjaExpressionValue(j1, _), JinjaExpressionValue(j2, _)) => assert_eq!(j1, j2),
            (JinjaExpressionValue(_, _), _) => {
                panic!("Types do not match: {self:?} and {other:?}")
            }
            (Array(xs, _), Array(ys, _)) => {
                assert_eq!(xs.len(), ys.len());
                xs.iter().zip(ys).for_each(|(x, y)| {
                    x.assert_eq_up_to_span(y);
                })
            }
            (Array(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Map(m1, _), Map(m2, _)) => {
                assert_eq!(m1.len(), m2.len());
                m1.iter().zip(m2).for_each(|((k1, v1), (k2, v2))| {
                    k1.assert_eq_up_to_span(k2);
                    v1.assert_eq_up_to_span(v2);
                });
            }
            (Map(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (ClassConstructor(cc1, _), ClassConstructor(cc2, _)) => {
                cc1.assert_eq_up_to_span(cc2);
            }
            (ClassConstructor(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Lambda(args1, body1, _), Lambda(args2, body2, _)) => {
                assert_eq!(args1.arguments.len(), args2.arguments.len());
                for (arg1, arg2) in args1.arguments.iter().zip(args2.arguments.iter()) {
                    arg1.assert_eq_up_to_span(arg2);
                }
                body1.assert_eq_up_to_span(body2);
            }
            (Lambda(_, _, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (App(app1), App(app2)) => {
                app1.name.assert_eq_up_to_span(&app2.name);

                assert_eq!(app1.type_args.len(), app2.type_args.len());
                for (type_arg1, type_arg2) in app1.type_args.iter().zip(app2.type_args.iter()) {
                    type_arg1.assert_eq_up_to_span(type_arg2);
                }

                assert_eq!(app1.args.len(), app2.args.len());
                for (arg1, arg2) in app1.args.iter().zip(app2.args.iter()) {
                    arg1.assert_eq_up_to_span(arg2);
                }
            }
            (App(_), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (ExprBlock(block1, _), ExprBlock(block2, _)) => {
                block1.assert_eq_up_to_span(block2);
            }
            (ExprBlock(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (If(cond1, then1, else1, _), If(cond2, then2, else2, _)) => {
                cond1.assert_eq_up_to_span(cond2);
                then1.assert_eq_up_to_span(then2);
                if let (Some(else1), Some(else2)) = (else1, else2) {
                    else1.assert_eq_up_to_span(else2);
                }
            }
            (If(_, _, _, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (ArrayAccess(base1, index1, _), ArrayAccess(base2, index2, _)) => {
                base1.assert_eq_up_to_span(base2);
                index1.assert_eq_up_to_span(index2);
            }
            (ArrayAccess(_, _, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (FieldAccess(base1, field1, _), FieldAccess(base2, field2, _)) => {
                base1.assert_eq_up_to_span(base2);
                field1.assert_eq_up_to_span(field2);
            }
            (FieldAccess(_, _, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (
                MethodCall {
                    receiver,
                    method,
                    args,
                    ..
                },
                MethodCall {
                    receiver: receiver2,
                    method: method2,
                    args: args2,
                    ..
                },
            ) => {
                receiver.assert_eq_up_to_span(receiver2);
                method.assert_eq_up_to_span(method2);
                assert_eq!(args.len(), args2.len());
                for (arg1, arg2) in args.iter().zip(args2.iter()) {
                    arg1.assert_eq_up_to_span(arg2);
                }
            }
            (MethodCall { .. }, _) => panic!("Types do not match: {self:?} and {other:?}"),
            (
                BinaryOperation {
                    left,
                    right,
                    operator,
                    ..
                },
                BinaryOperation {
                    left: left2,
                    right: right2,
                    operator: operator2,
                    ..
                },
            ) => {
                left.assert_eq_up_to_span(left2);
                right.assert_eq_up_to_span(right2);
                assert_eq!(operator, operator2);
            }
            (BinaryOperation { .. }, _) => panic!("Types do not match: {self:?} and {other:?}"),
            (
                UnaryOperation { expr, operator, .. },
                UnaryOperation {
                    expr: expr2,
                    operator: operator2,
                    ..
                },
            ) => {
                expr.assert_eq_up_to_span(expr2);
                assert_eq!(operator, operator2);
            }
            (UnaryOperation { .. }, _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Paren(expr1, _), Paren(expr2, _)) => {
                expr1.assert_eq_up_to_span(expr2);
            }
            (Paren(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
        }
    }

    pub fn to_unresolved_value(
        &self,
        _diagnostics: &mut internal_baml_diagnostics::Diagnostics,
    ) -> Option<UnresolvedValue> {
        use baml_types::StringOr;

        match self {
            Expression::BoolValue(val, span) => Some(UnresolvedValue::Bool(*val, span.clone())),
            Expression::NumericValue(val, span) => {
                Some(UnresolvedValue::Numeric(val.clone(), span.clone()))
            }
            Expression::Identifier(identifier) => match identifier {
                Identifier::ENV(val, span) => Some(UnresolvedValue::String(
                    StringOr::EnvVar(val.to_string()),
                    span.clone(),
                )),
                Identifier::Ref(ref_identifier, span) => Some(UnresolvedValue::String(
                    StringOr::Value(ref_identifier.full_name.as_str().to_string()),
                    span.clone(),
                )),
                Identifier::Invalid(val, span)
                | Identifier::String(val, span)
                | Identifier::Local(val, span) => match val.as_str() {
                    "null" => Some(UnresolvedValue::Null(span.clone())),
                    "true" => Some(UnresolvedValue::Bool(true, span.clone())),
                    "false" => Some(UnresolvedValue::Bool(false, span.clone())),
                    _ => Some(UnresolvedValue::String(
                        StringOr::Value(val.to_string()),
                        span.clone(),
                    )),
                },
            },
            Expression::StringValue(val, span) => Some(UnresolvedValue::String(
                StringOr::Value(val.to_string()),
                span.clone(),
            )),
            Expression::RawStringValue(raw_string) => {
                // Do standard dedenting / trimming.
                let val = raw_string.value();
                Some(UnresolvedValue::String(
                    StringOr::Value(val.to_string()),
                    raw_string.span().clone(),
                ))
            }
            Expression::Array(vec, span) => {
                let values = vec
                    .iter()
                    .filter_map(|e| e.to_unresolved_value(_diagnostics))
                    .collect::<Vec<_>>();
                Some(UnresolvedValue::Array(values, span.clone()))
            }
            Expression::Map(map, span) => {
                let values = map
                    .iter()
                    .filter_map(|(k, v)| {
                        let key = k.to_unresolved_value(_diagnostics);
                        if let Some(UnresolvedValue::String(StringOr::Value(key), key_span)) = key {
                            if let Some(value) = v.to_unresolved_value(_diagnostics) {
                                return Some((key, (key_span, value)));
                            }
                        }
                        None
                    })
                    .collect::<_>();
                Some(UnresolvedValue::Map(values, span.clone()))
            }
            Expression::JinjaExpressionValue(jinja_expression, span) => {
                Some(UnresolvedValue::String(
                    StringOr::JinjaExpression(jinja_expression.clone()),
                    span.clone(),
                ))
            }
            Expression::ClassConstructor(cc, span) => {
                let fields = cc
                    .fields
                    .iter()
                    .filter_map(|f| f.to_unresolved_value(_diagnostics))
                    .collect::<Vec<_>>();
                Some(UnresolvedValue::ClassConstructor(
                    cc.class_name.name().to_string(),
                    fields,
                    span.clone(),
                ))
            }
            Expression::Lambda(_arg_names, _body, _span) => todo!(),
            Expression::App(app) => {
                // Convert function application to TemplateStringCall
                // At this stage we don't know if it's a template_string - validation happens later
                let args: Vec<_> = app
                    .args
                    .iter()
                    .filter_map(|arg| {
                        arg.to_unresolved_value(_diagnostics)
                            .map(|v| v.without_meta())
                    })
                    .collect();

                // Only convert if all args converted successfully
                if args.len() == app.args.len() {
                    Some(UnresolvedValue::String(
                        StringOr::TemplateStringCall {
                            name: app.name.name().to_string(),
                            args,
                        },
                        app.span.clone(),
                    ))
                } else {
                    None
                }
            }
            Expression::ExprBlock(_, _) => None, // Is this right?
            Expression::If(_, _, _, _) => None,
            Expression::ArrayAccess(_, _, _) => None,
            Expression::FieldAccess(_, _, _) => None,
            Expression::MethodCall { .. } => None,
            Expression::BinaryOperation { .. } => None,
            Expression::UnaryOperation { .. } => None,
            Expression::Paren(_, _) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassConstructor {
    pub class_name: Identifier,
    pub fields: Vec<ClassConstructorField>,
}

#[derive(Debug, Clone)]
pub enum ClassConstructorField {
    Named(Identifier, Expression),
    Spread(Expression),
}

impl ClassConstructor {
    pub fn assert_eq_up_to_span(&self, other: &ClassConstructor) {
        assert_eq!(self.class_name, other.class_name);
        assert_eq!(self.fields.len(), other.fields.len());
        self.fields
            .iter()
            .zip(other.fields.iter())
            .for_each(|(a, b)| a.assert_eq_up_to_span(b));
    }
}

impl ClassConstructorField {
    pub fn assert_eq_up_to_span(&self, other: &ClassConstructorField) {
        use ClassConstructorField::*;
        match (self, other) {
            (Named(name1, expr1), Named(name2, expr2)) => {
                name1.assert_eq_up_to_span(name2);
                expr1.assert_eq_up_to_span(expr2);
            }
            (Spread(expr1), Spread(expr2)) => {
                expr1.assert_eq_up_to_span(expr2);
            }
            (Named(_, _), _) => panic!("Types do not match: {self:?} and {other:?}"),
            (Spread(_expr), _) => panic!("Types do not match: {self:?} and {other:?}"),
        }
    }

    // TODO: This is weird. Figure out what should happen with UnresolvedValue on spreads.
    pub fn to_unresolved_value(
        &self,
        _diagnostics: &mut internal_baml_diagnostics::Diagnostics,
    ) -> Option<(String, UnresolvedValue)> {
        match self {
            ClassConstructorField::Named(name, expr) => Some((
                name.name().to_string(),
                expr.to_unresolved_value(_diagnostics)?,
            )),
            ClassConstructorField::Spread(_expr) => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpressionBlock {
    pub stmts: Vec<Stmt>,
    pub expr: Option<Box<Expression>>,
    /// Headers that apply to the final expression
    pub expr_headers: Vec<std::sync::Arc<Header>>,
}

// TODO: How do we indent the inner statements?
impl fmt::Display for ExpressionBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{")?;
        for stmt in &self.stmts {
            write!(f, "{stmt}")?;
        }
        if let Some(expr) = &self.expr {
            write!(f, "{expr}")?;
        }
        write!(f, "}}")
    }
}

impl ExpressionBlock {
    pub fn assert_eq_up_to_span(&self, other: &ExpressionBlock) {
        self.stmts
            .iter()
            .zip(other.stmts.iter())
            .for_each(|(a, b)| {
                a.assert_eq_up_to_span(b);
            });

        match (&self.expr, &other.expr) {
            (Some(expr1), Some(expr2)) => expr1.assert_eq_up_to_span(expr2),
            (None, None) => {}
            _ => panic!("Types do not match: {self:?} and {other:?}"),
        }
    }
}
