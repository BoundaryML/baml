#[cfg(test)]
mod tests {
    use crate::evaluate_type::{
        stmt::get_variable_types,
        types::{PredefinedTypes, Type},
    };

    macro_rules! assert_evaluates_to {
        ($expr:expr, $types:expr) => {{
            let parsed = minijinja::machinery::parse(
                $expr,
                "prompt",
                minijinja::machinery::SyntaxConfig::default(),
                // TODO: this is not entirely great, but good enough for this use case.
                Default::default(),
            );
            assert!(parsed.is_ok(), "Failed to parse template: {:?}", parsed);
            let parsed = parsed.unwrap();

            let result = get_variable_types(&parsed, &mut $types);
            assert!(
                result.is_empty(),
                "Failed to evaluate expression: {:?}",
                result
            );
        }};
    }

    macro_rules! assert_fails_to {
        ($expr:expr, $types:expr, $expected:expr) => {{
            let parsed = minijinja::machinery::parse(
                $expr,
                "prompt",
                minijinja::machinery::SyntaxConfig::default(),
                // TODO: this is not entirely great, but good enough for this use case.
                Default::default(),
            );
            assert!(parsed.is_ok(), "Failed to parse template: {:?}", parsed);
            let parsed = parsed.unwrap();

            let result = get_variable_types(&parsed, &mut $types);
            assert!(
                !result.is_empty(),
                "Expected evaluation to fail, but got: {:?}",
                result
            );
            assert_eq!(
                result.iter().map(|x| x.message.clone()).collect::<Vec<_>>(),
                $expected
            );
        }};
    }

    #[test]
    fn test_evaluate_number() {
        let mut types = PredefinedTypes::default();
        assert_evaluates_to!(
            r#"
        {%- set prompt = 1.1 + 1 -%}
        {{ prompt }}
        "#,
            types
        );
    }

    #[test]
    fn test_evaluate_bool() {
        let mut types = PredefinedTypes::default();
        assert_fails_to!(
            r#"
        {{ prompt }}
        "#,
            types,
            vec!["Variable 'prompt' is not defined"]
        );
    }

    #[test]
    fn test_evaluate_string() {
        let mut types = PredefinedTypes::default();
        assert_evaluates_to!(
            r#"
        {%- set prompt = "hello" -%}
        {{ prompt }}
        "#,
            types
        );
    }

    #[test]
    fn test_evaluate_pre_vars() {
        let mut types = PredefinedTypes::default();
        types.add_variable("prompt", Type::Bool);
        assert_evaluates_to!(
            r#"
        {{ prompt }}
        "#,
            types
        );
    }

    #[test]
    fn test_function_call() {
        let mut types = PredefinedTypes::default();
        types.add_variable("prompt", Type::Bool);
        assert_fails_to!(
            r#"
        {{ prompt() }}
        "#,
            types,
            vec!["'prompt' is not a function"]
        );
    }

    #[test]
    fn test_function_call_1() {
        let mut types = PredefinedTypes::default();
        types.add_function("prompt", Type::Bool, vec![]);
        assert_evaluates_to!(
            r#"
        {{ prompt() }}
        "#,
            types
        );
    }

    #[test]
    fn test_function_call_2() {
        let mut types = PredefinedTypes::default();
        types.add_function("prompt", Type::Bool, vec![("arg".into(), Type::String)]);
        assert_fails_to!(
            r#"
        {% for x in items %}
            {{ prompt(x) }}
        {% endfor %}
        "#,
            types,
            vec!["Variable 'items' is not defined"]
        );
    }

    #[test]
    fn test_function_call_3() {
        let mut types = PredefinedTypes::default();
        types.add_function("prompt", Type::Bool, vec![("arg".into(), Type::String)]);
        types.add_variable("items", Type::List(Box::new(Type::String)));
        assert_evaluates_to!(
            r#"
        {% for x in items %}
            {{ prompt(x) }}
        {% endfor %}
        "#,
            types
        );

        assert_fails_to!(
            r#"
        {% for x in items %}
            {{ prompt(x) }}
        {% endfor %}
        {{ x }}
        "#,
            types,
            vec!["Variable 'x' is not defined"]
        );
    }

    #[test]
    fn test_function_call_4() {
        let mut types = PredefinedTypes::default();
        types.add_function("prompt", Type::Bool, vec![("arg".into(), Type::String)]);
        types.add_variable(
            "dict_item",
            Type::Map(Box::new(Type::Number), Box::new(Type::String)),
        );
        assert_evaluates_to!(
            r#"
{% for key, value in dict_item|items %}
    Key: {{key}}
    {{ prompt(value) }}
{% else %}
    No items
{% endfor %}
        "#
            .trim(),
            types
        );
    }

    #[test]
    fn test_loop() {
        let mut types = PredefinedTypes::default();
        types.add_variable("items", Type::List(Box::new(Type::String)));
        assert_fails_to!(
            r#"
{% for x in items %}
   {{ loop.a.b }}
   {{ x }}
{% endfor %}
        "#
            .trim(),
            types,
            vec![
                "class LoopVar (loop) does not have a property 'a'",
                "'loop.a' is not a class"
            ]
        );

        let mut types = PredefinedTypes::default();
        types.add_variable("items", Type::List(Box::new(Type::String)));
        assert_evaluates_to!(
            r#"
{% for x in items %}
   {{ loop.first }}
   {{ x }}
{% endfor %}
        "#
            .trim(),
            types
        );
    }

    #[test]
    fn test_if_else() {
        let mut types = PredefinedTypes::default();
        types.add_variable("prompt", Type::String);
        types.add_function("Foo", Type::Bool, vec![("arg".into(), Type::String)]);
        assert_fails_to!(
            r#"
{% if prompt == 'a' -%}
    {% set x = 1 %}
{%- elif prompt == 'abc' -%}
    {% set x = '2' %}
{%- else -%}
  {% set y = '[1]' %}
{%- endif %}
    {{ Foo(x) }}
        "#
            .trim(),
            types,
            vec!["Function 'Foo' expects argument 'arg' to be of type string, but got (undefined | number | string)"]
        );

        let mut types = PredefinedTypes::default();
        types.add_variable("prompt", Type::String);
        types.add_function("Foo", Type::Bool, vec![("arg".into(), Type::String)]);
        assert_evaluates_to!(
            r#"
{% if prompt == 'a' -%}
    {% set x = '1' %}
{%- elif prompt == 'abc' -%}
    {% set x = '2' %}
{%- else -%}
  {% set x = '[1]' %}
{%- endif %}
    {{ Foo(x) }}
        "#
            .trim(),
            types
        );
    }
}
