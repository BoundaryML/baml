#[cfg(test)]
mod tests {
    use minijinja::machinery::parse_expr;

    use crate::evaluate_type::{
        expr::evaluate_type,
        types::{PredefinedTypes, Type},
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
        let types = PredefinedTypes::default();
        assert_eq!(assert_evaluates_to!("1.1 + 1", &types), Type::Number);
    }

    #[test]
    fn test_evaluate_bool() {
        let types = PredefinedTypes::default();
        assert_eq!(assert_evaluates_to!("not 1.1", &types), Type::Bool);
    }

    #[test]
    fn test_evaluate_string() {
        let mut types = PredefinedTypes::default();
        assert_eq!(
            assert_fails_to!("ok ~ 1.1", &types),
            vec!["Variable 'ok' is not defined",]
        );
        types.add_variable("ok", Type::String);
        assert_eq!(assert_evaluates_to!("ok ~ 1.1", &types), Type::String);
    }

    #[test]
    fn test_evaluate_setting() {
        let mut types = PredefinedTypes::default();
        assert_eq!(
            assert_fails_to!("bar.f.g", &types),
            vec![
                "Variable 'bar' is not defined",
                "'bar' is not a class",
                "'bar.f' is not a class"
            ]
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
        let mut types = PredefinedTypes::default();
        assert_eq!(
            assert_evaluates_to!("1 if true else 2", &types),
            Type::Number
        );

        assert_eq!(
            assert_evaluates_to!("1 if true else '2'", &types),
            Type::Union(vec![Type::Number, Type::String])
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
        let types = PredefinedTypes::default();
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
        let mut types = PredefinedTypes::default();
        types.add_function("SomeFunc", Type::Float, vec![("arg".into(), Type::Bool)]);

        assert_eq!(assert_evaluates_to!("SomeFunc(true)", &types), Type::Float);
        assert_eq!(
            assert_fails_to!("SomeFunc(arg=1)", &types),
            vec!["Function 'SomeFunc' expects argument 'arg' to be of type bool, but got number"]
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
                "Function 'AnotherFunc' expects argument 'arg' to be of type bool, but got string"
            ]
        );

        assert_eq!(
            assert_fails_to!("AnotherFunc(arg=SomeFunc(true) ~ 1, arg2=1)", &types),
            vec![
            "Function 'AnotherFunc' expects argument 'arg' to be of type bool, but got string",
            "Function 'AnotherFunc' expects argument 'arg2' to be of type string, but got number"
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
                "Function 'AnotherFunc' does not have an argument 'arg4'"
            ]
        );
    }
}
