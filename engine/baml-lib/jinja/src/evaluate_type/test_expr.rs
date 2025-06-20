use baml_types::LiteralValue;
use minijinja::machinery::parse_expr;

use crate::evaluate_type::{
    expr::evaluate_type,
    types::{PredefinedTypes, Type},
    JinjaContext,
};

macro_rules! assert_evaluates_to {
    ($expr:expr, $types:expr) => {{
        let parsed = parse_expr($expr);
        assert!(parsed.is_ok(), "Failed to parse expression: {:?}", parsed);
        let parsed = parsed.unwrap();

        let result = evaluate_type(&parsed, &$types);
        assert!(
            result.is_ok(),
            "Failed to evaluate expression: {:?}",
            result
        );
        result.unwrap()
    }};
}

macro_rules! assert_fails_to {
    ($expr:expr, $types:expr) => {{
        let parsed = parse_expr($expr);
        assert!(parsed.is_ok(), "Failed to parse expression: {:?}", parsed);
        let parsed = parsed.unwrap();

        let result = evaluate_type(&parsed, &$types);
        assert!(
            result.is_err(),
            "Expected evaluation to fail, but got: {:?}",
            result
        );
        result
            .err()
            .unwrap()
            .iter()
            .map(|x| x.message.clone())
            .collect::<Vec<_>>()
    }};
}

#[test]
fn test_evaluate_number() {
    let types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(assert_evaluates_to!("1.1 + 1", &types), Type::Number);
}

#[test]
fn test_evaluate_bool() {
    let types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(assert_evaluates_to!("not 1.1", &types), Type::Bool);
}

#[test]
fn test_evaluate_string() {
    let mut types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(
        assert_fails_to!("ok ~ 1.1", &types),
        vec!["Variable `ok` does not exist. Did you mean one of these: `_`, `ctx`?"]
    );
    types.add_variable("ok", Type::String);
    assert_eq!(assert_evaluates_to!("ok ~ 1.1", &types), Type::String);
}

#[test]
fn test_evaluate_setting() {
    let mut types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(
        assert_fails_to!("bar.f.g", &types),
        vec!["Variable `bar` does not exist. Did you mean one of these: `_`, `ctx`?"]
    );

    types.add_class(
        "Foo",
        vec![("food".into(), Type::Float)].into_iter().collect(),
    );
    types.add_variable("bar", Type::ClassRef("Foo".into()));
    assert_eq!(
        assert_fails_to!("bar.f", &types),
        vec!["class Foo (bar) does not have a property 'f'"]
    );

    types.add_class("Foo", vec![("f".into(), Type::Int)].into_iter().collect());
    assert_eq!(assert_evaluates_to!("bar.f", &types), Type::Int);
}

#[test]
fn test_ifexpr() {
    let mut types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(
        assert_evaluates_to!("1 if true else 2", &types),
        Type::Union(vec![
            Type::Literal(LiteralValue::Int(1)),
            Type::Literal(LiteralValue::Int(2))
        ])
    );

    assert_eq!(
        assert_evaluates_to!("1 if true else '2'", &types),
        Type::Union(vec![
            Type::Literal(LiteralValue::String("2".to_string())),
            Type::Literal(LiteralValue::Int(1))
        ])
    );

    assert_eq!(
        assert_evaluates_to!("'1' if true else 2", &types),
        Type::Union(vec![
            Type::Literal(LiteralValue::String("1".to_string())),
            Type::Literal(LiteralValue::Int(2))
        ])
    );

    types.add_function("AnotherFunc", Type::Float, vec![("arg".into(), Type::Bool)]);

    types.add_variable("BasicTest", Type::Int);
    assert_eq!(
        assert_evaluates_to!("BasicTest if true else AnotherFunc", &types),
        Type::Union(vec![Type::Int, Type::FunctionRef("AnotherFunc".into())])
    );
}

#[test]
fn test_eval() {
    let types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(assert_evaluates_to!("1 + 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 - 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 * 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 / 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 ** 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 // 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 % 1", &types), Type::Number);
    assert_eq!(assert_evaluates_to!("1 == 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 != 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 < 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 > 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 in 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 <= 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 >= 1", &types), Type::Bool);
    assert_eq!(assert_evaluates_to!("1 ~ 1", &types), Type::String);
}

#[test]
fn test_call_function() {
    let mut types = PredefinedTypes::default(JinjaContext::Prompt);
    types.add_function("SomeFunc", Type::Float, vec![("arg".into(), Type::Bool)]);

    assert_eq!(assert_evaluates_to!("SomeFunc(true)", &types), Type::Float);
    assert_eq!(
        assert_fails_to!("SomeFunc(arg=1)", &types),
        vec!["Function 'SomeFunc' expects argument 'arg' to be of type bool, but got literal[1]"]
    );

    types.add_function(
        "AnotherFunc",
        Type::Float,
        vec![("arg".into(), Type::Bool), ("arg2".into(), Type::String)],
    );
    assert_eq!(
        assert_fails_to!("AnotherFunc(true)", &types),
        vec!["Function 'AnotherFunc' expects 2 arguments, but got 1"]
    );

    assert_eq!(
        assert_fails_to!("AnotherFunc(arg='true', arg2='1')", &types),
        vec![
            r#"Function 'AnotherFunc' expects argument 'arg' to be of type bool, but got literal["true"]"#
        ]
    );

    assert_eq!(
        assert_fails_to!("AnotherFunc(arg=SomeFunc(true) ~ 1, arg2=1)", &types),
        vec![
            "Function 'AnotherFunc' expects argument 'arg' to be of type bool, but got string",
            "Function 'AnotherFunc' expects argument 'arg2' to be of type string, but got literal[1]"
        ]
    );

    assert_eq!(
        assert_evaluates_to!("AnotherFunc(true, arg2='1')", &types),
        Type::Float
    );

    types.add_function(
        "AnotherFunc",
        Type::Float,
        vec![
            ("arg".into(), Type::Bool),
            ("arg2".into(), Type::String),
            ("arg3".into(), Type::Number),
        ],
    );

    assert_eq!(
        assert_fails_to!("AnotherFunc(true, arg2='1')", &types),
        vec!["Function 'AnotherFunc' expects 3 arguments, but got 2"]
    );

    assert_eq!(
        assert_fails_to!("AnotherFunc(true, arg2='1')", &types),
        vec!["Function 'AnotherFunc' expects 3 arguments, but got 2",]
    );

    assert_eq!(
        assert_fails_to!("AnotherFunc(true, arg2='1', arg4=1)", &types),
        vec![
            "Function 'AnotherFunc' expects argument 'arg3'",
            "Function 'AnotherFunc' does not have an argument 'arg4'. Did you mean 'arg3'?"
        ]
    );

    types.add_function(
        "TakesLiteralFoo",
        Type::Float,
        vec![(
            "arg".to_string(),
            Type::Union(vec![
                Type::Literal(LiteralValue::String("Foo".to_string())),
                Type::Literal(LiteralValue::String("Bar".to_string())),
            ]),
        )],
    );

    assert_eq!(
        assert_evaluates_to!("TakesLiteralFoo('Foo')", &types),
        Type::Float
    );
}

#[test]
fn test_output_format() {
    let types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(
        assert_evaluates_to!("ctx.output_format(prefix='hi')", &types),
        Type::String
    );

    assert_eq!(
        assert_evaluates_to!("ctx.output_format(prefix='1', or_splitter='1')", &types),
        Type::String
    );

    assert_eq!(
        assert_evaluates_to!(
            "ctx.output_format(prefix='1', enum_value_prefix=none)",
            &types
        ),
        Type::String
    );

    assert_eq!(
        assert_fails_to!(
            "ctx.output_format(prefix='1', always_hoist_enums=1)",
            &types
        ),
        vec!["Function 'baml::OutputFormat' expects argument 'always_hoist_enums' to be of type (none | bool), but got literal[1]"]
    );

    assert_eq!(
        assert_fails_to!(
            "ctx.output_format(prefix='1', hoisted_class_prefix=1)",
            &types
        ),
        vec!["Function 'baml::OutputFormat' expects argument 'hoisted_class_prefix' to be of type (none | string), but got literal[1]"]
    );

    assert_eq!(
        assert_fails_to!(
            "ctx.output_format(hoist_classes=1)",
            &types
        ),
        vec!["Function 'baml::OutputFormat' expects argument 'hoist_classes' to be of type (none | bool | literal[\"auto\"] | list[string]), but got literal[1]"]
    );

    // TODO: There's a bug here, suggestions are not always the same, maybe some
    // ordering issue or algorithm used to determine closest strings is not
    // consistent. Code is in baml-lib/jinja/src/evaluate_type/mod.rs,
    // TypeError::new_unknown_arg
    //
    // assert_eq!(
    //     assert_fails_to!("ctx.output_format(prefix='1', unknown=1)", &types),
    //     vec!["Function 'baml::OutputFormat' does not have an argument 'unknown'. Did you mean one of these: 'enum_value_prefix', 'hoist_classes', 'or_splitter'?"]
    // );
}

#[test]
fn sum_filter() {
    let types = PredefinedTypes::default(JinjaContext::Prompt);
    assert_eq!(assert_evaluates_to!(r#"[1,2,3]|sum"#, types), Type::Int);
    assert_eq!(
        assert_evaluates_to!(r#"[1.1,2.1,3.2]|sum"#, types),
        Type::Float
    );
    // // This would be nice, but it doesn't work.
    // // Type checker says this is a subtype of `int[]`.
    // // BUG.
    // assert_eq!(
    //     assert_evaluates_to!(r#"[1.1,2,3]|sum"#, types),
    //     Type::Float
    // );
    assert_eq!(
        assert_fails_to!(r#"["hi", 1]|sum"#, types),
        vec![r#"'[hi,1]' is a list[(literal["hi"] | literal[1])], expected (int|float)[]"#]
    );
}
