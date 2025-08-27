use minijinja::{context, Environment, Value};

use crate::baml_value_to_jinja_value::MinijinjaBamlEnumValue;

#[test]
fn test_enum_comparison_in_template() {
    let mut env = Environment::new();

    // Create an enum value with alias
    let status = Value::from_object(MinijinjaBamlEnumValue {
        value: "InProgress".to_string(),
        alias: Some("in_progress".to_string()),
        enum_name: "Status".to_string(),
    });

    // Test equality comparison with value name
    let template = r#"
        {%- if status == "InProgress" -%}
            Status matches value name
        {%- else -%}
            Status does not match
        {%- endif -%}
        "#;

    env.add_template("test1", template).unwrap();
    let tmpl = env.get_template("test1").unwrap();
    let output = tmpl.render(context! { status }).unwrap();
    assert_eq!(output.trim(), "Status matches value name");

    // Test that alias is NOT used for comparison
    let template2 = r#"
        {%- if status == "in_progress" -%}
            Status matches alias
        {%- else -%}
            Status does not match alias
        {%- endif -%}
        "#;

    env.add_template("test2", template2).unwrap();
    let tmpl2 = env.get_template("test2").unwrap();
    let output2 = tmpl2.render(context! { status => status.clone() }).unwrap();
    assert_eq!(output2.trim(), "Status does not match alias");

    // Test display uses alias
    let template3 = r#"{{ status }}"#;
    env.add_template("test3", template3).unwrap();
    let tmpl3 = env.get_template("test3").unwrap();
    let output3 = tmpl3.render(context! { status => status.clone() }).unwrap();
    assert_eq!(output3.trim(), "in_progress");

    // Test ordering in template
    let template5 = r#"
        {%- if status < "ZZZ" -%}
            Status is less than ZZZ
        {%- else -%}
            Status is not less than ZZZ
        {%- endif -%}
        "#;

    env.add_template("test5", template5).unwrap();
    let tmpl5 = env.get_template("test5").unwrap();
    let output5 = tmpl5.render(context! { status => status.clone() }).unwrap();
    assert_eq!(output5.trim(), "Status is less than ZZZ");

    // Test commutativity
    let template6 = r#"
        {%- if "InProgress" == status -%}
            Reverse comparison works
        {%- else -%}
            Reverse comparison failed
        {%- endif -%}
        "#;

    env.add_template("test6", template6).unwrap();
    let tmpl6 = env.get_template("test6").unwrap();
    let output6 = tmpl6.render(context! { status }).unwrap();
    assert_eq!(output6.trim(), "Reverse comparison works");
}
