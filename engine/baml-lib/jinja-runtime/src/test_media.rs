use baml_types::BamlMedia;
use minijinja::{context, value::Value, Environment};

use crate::baml_value_to_jinja_value::MinijinjaBamlMedia;

#[test]
fn test_media_value() {
    // Test that a media object evaluates to the raw string representation
    let media_val = Value::from_object(MinijinjaBamlMedia::from(BamlMedia::url(
        baml_types::BamlMediaType::Image,
        "https://example.com/image.png".to_string(),
        None,
    )));

    let template = r#"
{{ media }}
    "#;
    let mut env = Environment::new();

    env.add_template("test1", template).unwrap();
    let tmpl = env.get_template("test1").unwrap();
    let output = tmpl.render(context! { media => media_val }).unwrap();
    assert_eq!(output.trim(), "BAML_MEDIA_MAGIC_STRING_DELIMITER:baml-start-media:{\"media_type\":\"Image\",\"mime_type\":null,\"content\":{\"Url\":{\"url\":\"https://example.com/image.png\"}}}:baml-end-media:BAML_MEDIA_MAGIC_STRING_DELIMITER");
}

#[test]
fn test_media_comparison() {
    // Test that a media object evaluates to true in boolean context
    let media_val = Value::from_object(MinijinjaBamlMedia::from(BamlMedia::url(
        baml_types::BamlMediaType::Image,
        "https://example.com/image.png".to_string(),
        None,
    )));

    let template = r#"
{{ media|bool }}
    "#;
    let mut env = Environment::new();

    env.add_template("test1", template).unwrap();
    let tmpl = env.get_template("test1").unwrap();
    let output = tmpl.render(context! { media => media_val }).unwrap();
    assert_eq!(output.trim(), "true");
}
