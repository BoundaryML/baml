use anyhow::Result;
use anyhow::Context;
use std::collections::HashMap;
use baml_types::BamlValue;
use internal_baml_core::ir::{FieldType, TypeValue};
use internal_baml_jinja::render_expression;

use internal_baml_core::ir::repr::{ClassId, Expression};
use internal_baml_core::ir::repr::{Class, Field, Node};
use crate::BamlMap;

/// The result of running validation on a value with checks.
#[derive(Clone, Debug, PartialEq)]
pub enum UserChecksResult {
    Success,
    AssertFailure(UserFailure),
    CheckFailures(Vec<UserFailure>),
}

impl UserChecksResult {
    /// Combine two `UserChecksResult`s, following the semantics of asserts and
    /// checks. The first assert short-circuits all other results, otherwise
    /// failed checks combine, returning `Success` if neither result has any
    /// failed checks.
    ///
    pub fn combine(self, other: Self) -> Self {
        use UserChecksResult::*;
        match (&self, &other) {
            (AssertFailure(_), _) => self,
            (_, AssertFailure(_)) => other,
            (CheckFailures(fs1), CheckFailures(fs2)) => {
                let mut fs = fs1.clone();
                fs.extend_from_slice(fs2);
                CheckFailures(fs.to_vec())
            },
            (Success, _) => other,
            (_, Success) => self,
        }
    }
}

/// A single failure of a user-defined @check or @assert.
#[derive(Clone, Debug, PartialEq)]
pub struct UserFailure {
    /// The context of the field.
    pub field_context: Vec<String>,
    /// The class field that failed the check.
    pub field_name: String,
    /// The user-supplied name for the check that failed.
    pub check_name: String,
}


/// Run all checks and asserts for every field, recursing into fields
/// that contain classes with further asserts and checks.
pub fn run_user_checks(
    baml_value: &BamlValue,
    typing_env: &HashMap<ClassId, Class>
) -> Result<UserChecksResult> {

    // List all classes of this value, including the top-level class.
    let contained = contained_classes(vec![], baml_value);
    dbg!(&contained);

    let results = contained.iter().filter_map(|(field_context, value)| {
        match value {
            BamlValue::Class(class_name, class) => {
                Some((field_context, class_name, class))
            },
            _ => None,
        }})
        .map(|(field_ctx, class_name, class)| {
            typing_env
                .get(class_name)
                .context("Faild to find type in context")
                .map(|ty| (field_ctx, class_name, class, ty))

        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .map(|(field_ctx, class_name, class, type_)|
             check_class(
                 field_ctx.clone(),
                 (class_name.to_string(), class.clone()),
                 type_,
                 typing_env
             )
        )
        .collect::<Result<Vec<UserChecksResult>>>()?;

    dbg!(&results);
    let result = results
        .into_iter()
        .fold(UserChecksResult::Success, |res, acc| res.combine(acc.clone()));
    dbg!(&result);
    Ok(result)

}

/// List all classes contained in a BamlValue, recursively.
/// For a BamlValue::Class, return all fields that are also classes.
/// For a BamlValue::List, return all classes in the list, etc.
/// This is not a recursive traversal of all fields, only the top level.
pub fn contained_classes(
    field_context: Vec<String>,
    baml_value: &BamlValue
) -> Vec<(Vec<String>, &BamlValue)> {
    let mut classes: Vec<(Vec<String>, &BamlValue)> = Vec::new();
    match baml_value {
        c@BamlValue::Class(_, class_fields) => {
            classes.push((field_context.clone(), c));
            for (field_name, field_value) in class_fields.iter() {
                let mut new_context = field_context.clone();
                new_context.push(field_name.clone());
                classes.extend(contained_classes(new_context, field_value));
            }
        },
        BamlValue::List(items) => {
            for (ind, item) in items.iter().enumerate() {
                let mut new_context = field_context.clone();
                new_context.push(ind.to_string());
                classes.extend(contained_classes(new_context, item));
            }
        },
        BamlValue::Map(items) => {
            for (key, item) in items.iter() {
                let mut new_context = field_context.clone();
                new_context.push(key.clone());
                classes.extend(contained_classes(new_context, item));
            }
        },
        BamlValue::Bool(_) | BamlValue::Int(_) |
        BamlValue::String(_) | BamlValue::Float(_) |
        BamlValue::Media(_) | BamlValue::Enum(_,_) | BamlValue::Null  => {},
    }
    classes
}

/// Run
pub fn check_class(
    field_context: Vec<String>,
    (class_name, class_fields): (String, BamlMap<String, BamlValue>),
    class_type: &Class,
    typing_env: &HashMap<String, Class>,
) -> Result<UserChecksResult> {

    // Run checks in each field.
    let result = class_type
        .static_fields
        .iter()
        .map(|field| {
            let field_value = class_fields.get(&field.elem.name).unwrap();
            let res = run_user_checks_field(
                field_context.clone(),
                &field_value,
                &field.elem,
                typing_env
            );
            dbg!((&field_context, &res));
            res
        })
        .collect::<Result<Vec<UserChecksResult>>>()?
        .iter()
        .fold(UserChecksResult::Success, |res, acc| res.combine(acc.clone()));
    dbg!(&result);

    Ok(result)
}



/// Check the user-provided @check and @assert attributes for a single field.
pub fn run_user_checks_field(
    field_context: Vec<String>,
    value: &BamlValue,
    type_: &Field,
    typing_env: &HashMap<ClassId, Class>,
) -> Result<UserChecksResult> {
    let field_type = type_.r#type.elem.clone();
    let field_name = type_.name.to_string();

    for (assert_name, assert_expr) in type_.r#type.attributes.asserts.iter() {
        let res = evaluate_predicate(&value, assert_expr)?;
        if !res {
            let failure = UserFailure {
                field_context: field_context.clone(),
                field_name: type_.name.clone(),
                check_name: assert_name.to_string()
            };
            return Ok(UserChecksResult::AssertFailure(failure));
        }
    }
    let failed_checks : Vec<UserFailure> = type_
        .r#type
        .attributes
        .checks
        .iter()
        .filter_map(|(check_name, check_expr)| {
            evaluate_predicate(&value, check_expr).map(
                |r| if r {
                    None
                } else {
                    Some(UserFailure {
                        field_context: field_context.clone(),
                        field_name: field_name.clone(),
                        check_name: check_name.to_string()
                    })
                }
            ).transpose()
        }

        )
        .collect::<Result<Vec<_>>>()?;
    if failed_checks.len() > 0 {
        Ok(UserChecksResult::CheckFailures(failed_checks))
    } else {
        Ok(UserChecksResult::Success)
    }

}

// TODO: (Greg) better error handling.
// TODO: (Greg) Upstream, typecheck the expression.
pub fn evaluate_predicate(this: &BamlValue, expr: &Expression) -> Result<bool> {
    let predicate_code = match expr {
        Expression::JinjaExpression(s) => Ok(s),
        _ => Err(anyhow::anyhow!("Expected jinja expression")),
    }?;
    let ctx : HashMap<String, BamlValue> =
        [("this".to_string(), this.clone())]
        .into_iter()
        .collect();
    match render_expression(&predicate_code, &ctx)?.as_ref() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(anyhow::anyhow!("TODO")),
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ir::repr::NodeAttributes;

    use super::*;

    #[test]
    fn test_evaluate_predicate() {
        assert_eq!(evaluate_predicate(
            &BamlValue::String("foo".to_string()),
            &Expression::JinjaExpression("this|length >= 3".to_string())
        ).unwrap(), true);

        assert_eq!(evaluate_predicate(
            &BamlValue::String("foo".to_string()),
            &Expression::JinjaExpression("this|length > 3".to_string())
        ).unwrap(), false);

        assert_eq!(evaluate_predicate(
            &BamlValue::Int(10),
            &Expression::JinjaExpression("this == 10".to_string())
        ).unwrap(), true);
    }

    #[test]
    fn test_assert_single_string_field() {
        let strictly_positive = (
            "Strictly positive".to_string(),
            Expression::JinjaExpression("this > 0".to_string())
        );
        let age_like = (
            "A valid age".to_string(),
            Expression::JinjaExpression("this >= 0 and this < 150".to_string())
        );
        let field_typedef = Field {
            name: "some_number".to_string(),
            r#type: Node {
                attributes: NodeAttributes {
                    asserts: vec![strictly_positive],
                    checks: vec![age_like],
                    ..NodeAttributes::default()
                },
                elem: FieldType::int(),
            }
        };
        assert_eq!(
            run_user_checks_field(
                vec![],
                &BamlValue::Int(10),
                &field_typedef,
                &HashMap::new(),
            ).unwrap(),
            UserChecksResult::Success
        )
    }

    #[test]
    fn test_combine_results() {
        fn failure() -> UserFailure {
            UserFailure { field_context: vec![], field_name: "test".to_string(), check_name: "test".to_string() }
        }
        fn ok() -> UserChecksResult {
            UserChecksResult::Success
        }
        fn assert() -> UserChecksResult {
            UserChecksResult::AssertFailure(failure())
        }
        fn check_1() -> UserChecksResult {
            UserChecksResult::CheckFailures(vec![failure()])
        }
        fn check_2() -> UserChecksResult {
            UserChecksResult::CheckFailures(vec![failure(), failure()])
        }

        // Identity checks.
        assert_eq!( ok().combine(ok()), ok() );
        assert_eq!( assert().combine(ok()), assert() );
        assert_eq!( ok().combine(assert()), assert() );

        // Append checks.
        assert_eq!( check_1().combine(check_1()), check_2() );
        assert_eq!( ok().combine(check_1()), check_1() );
        assert_eq!( ok().combine(ok()).combine(check_1()).combine(check_1()), check_2() );

        // Fold check.
        assert_eq!(
            vec![ok(), ok(), ok(), check_1(), check_1(), ok()]
                .into_iter()
                .fold(ok(), |res, acc| res.combine(acc)),
            check_2()
        );
    }

    fn mk_example_instance(
        quxs: Vec<usize>
    ) -> (BamlValue, BamlValue, BamlValue, BamlValue, BamlValue) {
        // Foo {                                  <- Top-level class
        //    bar: Bar {                          <- Nested class
        //      baz: { "qux": Qux { i: 1 } }      <- Nested map with class inside.
        //    },
        //    quxs: [Qux { i: 1 }, Qux { i: 2 }]  <- Nested list with classes inside.
        // }

        let qux = BamlValue::Class("Qux".to_string(), vec![("i".to_string(), BamlValue::Int(10))].into_iter().collect());
        let quxs = BamlValue::List(quxs.into_iter().map(|i| {
            BamlValue::Class("Qux".to_string(), vec![("i".to_string(), BamlValue::Int(i as i64))].into_iter().collect())
        }).collect());
        // let quxs = BamlValue::List(vec![qux_1.clone(), qux_2.clone()]);
        let baz = BamlValue::Map(vec![("my_qux".to_string(), qux.clone())].into_iter().collect());
        let bar = BamlValue::Class("Bar".to_string(), vec![("baz".to_string(), baz.clone())].into_iter().collect());
        let foo = BamlValue::Class("Foo".to_string(), vec![("bar".to_string(), bar.clone()), ("quxs".to_string(), quxs.clone())].into_iter().collect());
        (foo, bar, baz, qux, quxs)
    }

    fn mk_example_typing_env() -> HashMap<String, Class> {
        let qux = Class {
            name: "Qux".to_string(),
            static_fields: vec![
                Node {
                    attributes: NodeAttributes::default(),
                    elem: Field {
                        name: "i".to_string(),
                        r#type: Node {
                            attributes: NodeAttributes {
                                checks: vec![mk_check("this >= 2")],
                                ..NodeAttributes::default()
                            },
                            elem: FieldType::Primitive(TypeValue::Int),
                        }
                    },
                }

            ],
            inputs: vec![],
        };

        let bar = Class {
            name: "Bar".to_string(),
            static_fields: vec![
                Node {
                    attributes: NodeAttributes::default(),
                    elem: Field {
                        name: "baz".to_string(),
                        r#type: Node {
                            attributes: NodeAttributes::default(),
                            elem: FieldType::Map(
                                Box::new(FieldType::Primitive(TypeValue::String)),
                                Box::new(FieldType::Class("Qux".to_string()))
                            ),
                        }
                    }
                }
            ],
            inputs: vec![],
        };

        let foo = Class {
            name: "Foo".to_string(),
            static_fields: vec![
                Node {
                    attributes: NodeAttributes::default(),
                    elem: Field {
                        name: "bar".to_string(),
                        r#type: Node {
                            attributes: NodeAttributes::default(),
                            elem: FieldType::Class("Bar".to_string()),
                        }
                    }
                },

                Node {
                    attributes: NodeAttributes::default(),
                    elem: Field {
                        name: "quxs".to_string(),
                        r#type: Node {
                            attributes: NodeAttributes {
                                asserts: vec![mk_check("this|length > 2")],
                                ..NodeAttributes::default()
                            },
                            elem: FieldType::List(Box::new( FieldType::Class("Qux".to_string()) )),
                        }
                    }
                }

            ],
            inputs: vec![],
        };

        vec![
            ("Qux".to_string(), qux),
            ("Bar".to_string(), bar),
            ("Foo".to_string(), foo),
        ].into_iter().collect::<HashMap<String, Class>>()
    }

    #[test]
    fn test_contained_classes() {
        let (foo, bar, _baz, qux, quxs) = mk_example_instance(vec![1,2,3]);
        let quxs_items = quxs.as_list_owned().unwrap();
        let contained = contained_classes(vec![], &foo);
        assert_eq!(contained.len(), 6);

        assert_eq!(contained[0].0, Vec::<String>::new());
        assert_eq!(contained[0].1, &foo);

        assert_eq!(contained[1].0, vec!["bar".to_string()]);
        assert_eq!(contained[1].1, &bar);

        assert_eq!(contained[2].0, vec!["bar".to_string(), "baz".to_string(), "my_qux".to_string()]);
        assert_eq!(contained[2].1, &qux);

        assert_eq!(contained[3].0, vec!["quxs".to_string(), "0".to_string()]);
        assert_eq!(contained[3].1, &quxs_items[0]);

        assert_eq!(contained[4].0, vec!["quxs".to_string(), "1".to_string()]);
        assert_eq!(contained[4].1, &quxs_items[1]);
    }

    #[test]
    fn test_checks_and_asserts_success() {
        let (foo, _bar, _baz, _qux, _quxs) = mk_example_instance(vec![2,3,4,5]);
        let types = mk_example_typing_env();
        let res = run_user_checks(&foo, &types).unwrap();
        assert_eq!(res, UserChecksResult::Success);
    }

    #[test]
    fn test_checks_and_asserts_warning_not_enough_quxs() {
        let (foo, _bar, _baz, _qux, _quxs) = mk_example_instance(vec![2]);
        let types = mk_example_typing_env();
        let res = run_user_checks(&foo, &types).unwrap();
        assert_eq!(res, UserChecksResult::AssertFailure(
            UserFailure {
                field_context: vec![],
                field_name: "quxs".to_string(),
                check_name: "this|length > 2".to_string(),
            }
        ));
    }

    #[test]
    fn test_checks_and_asserts_multiple_check_failures() {
        let (foo, _bar, _baz, _qux, _quxs) = mk_example_instance(vec![0, 0, 0]);
        let types = mk_example_typing_env();
        let res = run_user_checks(&foo, &types).unwrap();
        assert_eq!(res, UserChecksResult::CheckFailures(vec![
            UserFailure {
                field_context: vec!["quxs".to_string(), "0".to_string()],
                field_name: "i".to_string(),
                check_name: "this >= 2".to_string(),
            },
            UserFailure {
                field_context: vec!["quxs".to_string(), "1".to_string()],
                field_name: "i".to_string(),
                check_name: "this >= 2".to_string(),
            },
            UserFailure {
                field_context: vec!["quxs".to_string(), "2".to_string()],
                field_name: "i".to_string(),
                check_name: "this >= 2".to_string(),
            }
        ]));
    }


    fn mk_check(e: &str) -> (String, Expression) {
        (e.to_string(), Expression::JinjaExpression(e.to_string()))
    }

}

