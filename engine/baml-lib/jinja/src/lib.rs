mod evaluate_type;

use evaluate_type::get_variable_types;
pub use evaluate_type::{
    EnumDefinition, EnumValueDefinition, JinjaContext, PredefinedTypes, Type, TypeError,
};

#[derive(Debug)]
pub struct ValidationError {
    pub errors: Vec<TypeError>,
    pub parsing_errors: Option<minijinja::Error>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for err in &self.errors {
            writeln!(f, "{err}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

pub fn validate_expression(
    expression: &str,
    types: &mut PredefinedTypes,
) -> Result<(), ValidationError> {
    let parsed = match minijinja::machinery::parse_expr(expression) {
        Ok(parsed) => parsed,
        Err(err) => {
            return Err(ValidationError {
                errors: vec![],
                parsing_errors: Some(err),
            });
        }
    };

    let expr_type = evaluate_type::evaluate_type(&parsed, types);
    match expr_type {
        Ok(_) => Ok(()),
        Err(err) => Err(ValidationError {
            errors: err,
            parsing_errors: None,
        }),
    }
}

pub fn validate_template(
    name: &str,
    template: &str,
    types: &mut PredefinedTypes,
) -> Result<(), ValidationError> {
    let parsed =
        match minijinja::machinery::parse(template, name, Default::default(), Default::default()) {
            Ok(parsed) => parsed,
            Err(err) => {
                return Err(ValidationError {
                    errors: vec![],
                    parsing_errors: Some(err),
                });
            }
        };

    let errs = get_variable_types(&parsed, types);

    if errs.is_empty() {
        Ok(())
    } else {
        Err(ValidationError {
            errors: errs,
            parsing_errors: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use baml_types::LiteralValue;
    use indexmap::IndexMap;

    use super::*;

    fn mk_params() -> PredefinedTypes {
        let mut types = PredefinedTypes::default(JinjaContext::Prompt);
        types.add_class("Foo", IndexMap::from([("name".to_string(), Type::String)]));
        types.add_variable(
            "foo",
            Type::Union(vec![Type::None, Type::ClassRef("Foo".to_string())]),
        );
        types.add_variable(
            "foo2",
            Type::Union(vec![Type::None, Type::ClassRef("Foo".to_string())]),
        );
        types
    }

    #[test]
    fn test_type_narrowing_not_narrowed() {
        let mut types = mk_params();
        let err_unnarrowed = validate_template(
            "test",
            r#"
            {{ foo.name }}
            "#,
            &mut types,
        )
        .expect_err("Should fail")
        .errors
        .into_iter()
        .next()
        .unwrap();
        assert_eq!(
            err_unnarrowed.message(),
            "cannot access property 'name' on 'foo': union contains non-class type none"
        );
    }

    #[test]
    fn test_type_narrowing_truthy() {
        let mut types = mk_params();
        validate_template(
            "test",
            r#"
            {% if foo %}
              {{ foo.name }}
            {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }

    #[test]
    fn test_type_narrowing_truthy_and() {
        let mut types = mk_params();
        validate_template(
            "test",
            r#"
            {% if (foo and foo2) %}
              {{ foo.name }}
              {{ foo2.name }}
            {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }

    #[test]
    fn test_type_narrowing_ne_none() {
        let mut types = mk_params();
        validate_template(
            "test",
            r#"
            {% if foo!=none %}
              {{ foo.name }}
            {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }

    #[test]
    fn test_should_narrow_union_type() {
        let mut types = PredefinedTypes::default(JinjaContext::Prompt);

        types.add_class(
            "UserMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("user_message".to_string())),
                "user_message".to_string() => Type::String,
            }
        );

        types.add_class(
            "AssistantMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("assistant_message".to_string())),
                "assistant_message".to_string() => Type::String,
            },
        );

        types.add_variable(
            "message",
            Type::Union(vec![
                Type::ClassRef("UserMessage".to_string()),
                Type::ClassRef("AssistantMessage".to_string()),
            ]),
        );

        validate_template(
            "test",
            r#"
                {% if message.kind == "user_message" %}
                    User Message
                {% elif message.kind == "assistant_message" %}
                    Assistant Message
                {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }

    #[test]
    fn test_should_infer_narrowed_type_on_if_branch() {
        let mut types = PredefinedTypes::default(JinjaContext::Prompt);

        types.add_class(
            "UserMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("user_message".to_string())),
                "user_message".to_string() => Type::String,
            }
        );

        types.add_class(
            "AssistantMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("assistant_message".to_string())),
                "assistant_message".to_string() => Type::String,
            },
        );

        types.add_variable(
            "message",
            Type::Union(vec![
                Type::ClassRef("UserMessage".to_string()),
                Type::ClassRef("AssistantMessage".to_string()),
            ]),
        );

        validate_template(
            "test",
            r#"
                {% if message.kind == "user_message" %}
                    {{ message.user_message }}
                {% elif message.kind == "assistant_message" %}
                    {{ message.assistant_message }}
                {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }

    #[test]
    fn test_should_infer_narrowed_type_on_if_branch_with_alias() {
        let mut types = PredefinedTypes::default(JinjaContext::Prompt);

        types.add_class(
            "UserMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("user_message".to_string())),
                "user_message".to_string() => Type::String,
            }
        );

        types.add_class(
            "AssistantMessage",
            indexmap::indexmap! {
                "kind".to_string() => Type::Literal(LiteralValue::String("assistant_message".to_string())),
                "assistant_message".to_string() => Type::String,
            },
        );

        types.add_alias(
            "Message",
            Type::Union(vec![
                Type::ClassRef("UserMessage".to_string()),
                Type::ClassRef("AssistantMessage".to_string()),
            ]),
        );

        types.add_variable(
            "message",
            Type::Alias {
                name: "Message".to_string(),
                target: Box::new(Type::Union(vec![
                    Type::ClassRef("UserMessage".to_string()),
                    Type::ClassRef("AssistantMessage".to_string()),
                ])),
                resolved: Box::new(Type::Union(vec![
                    Type::ClassRef("UserMessage".to_string()),
                    Type::ClassRef("AssistantMessage".to_string()),
                ])),
            },
        );

        validate_template(
            "test",
            r#"
                {% if message.kind == "user_message" %}
                    {{ message.user_message }}
                {% elif message.kind == "assistant_message" %}
                    {{ message.assistant_message }}
                {% endif %}
            "#,
            &mut types,
        )
        .expect("Should succeed");
    }
}
