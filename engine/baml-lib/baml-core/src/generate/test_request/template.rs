use crate::generate::generate_python_client::File;

use handlebars::handlebars_helper;

#[derive(Debug)]
pub(super) enum HSTemplate {
    SingleArgTestSnippet,
    MultiArgTestSnippet,
}

handlebars_helper!(BLOCK_OPEN: |*_args| "{");
handlebars_helper!(BLOCK_CLOSE: |*_args| "}");
fn init_hs() -> handlebars::Handlebars<'static> {
    let mut reg = handlebars::Handlebars::new();
    reg.register_helper("BLOCK_OPEN", Box::new(BLOCK_OPEN));
    reg.register_helper("BLOCK_CLOSE", Box::new(BLOCK_CLOSE));

    reg
}

macro_rules! include_template {
    ($type:expr, $file:expr) => {
        include_str!(concat!(
            "../generate_python_client/templates/",
            $type,
            "/",
            $file,
            ".hb"
        ))
    };
}

macro_rules! register_partial_file {
    ($reg:expr, $type:expr, $file:expr) => {
        register_partial!($reg, $file, include_template!($type, $file));
    };
}

macro_rules! register_partial {
    ($reg:expr, $name:expr, $template:expr) => {
        $reg.register_partial($name, $template)
            .unwrap_or_else(|e| panic!("Failed to register template: {}", e));
    };
}

fn use_partial(
    template: HSTemplate,
    reg: &mut handlebars::Handlebars<'static>,
    f: &mut File,
) -> String {
    register_partial!(reg, "print_code", "{{{code}}}");
    match template {
        HSTemplate::SingleArgTestSnippet => {
            register_partial_file!(reg, "tests", "single_arg_snippet");
            f.add_import("baml_client", "baml");
            f.add_import("baml_lib._impl.deserializer", "Deserializer");
            String::from("single_arg_snippet")
        }
        HSTemplate::MultiArgTestSnippet => {
            register_partial_file!(reg, "tests", "multi_arg_snippet");
            f.add_import("baml_client", "baml");
            f.add_import("baml_lib._impl.deserializer", "Deserializer");
            String::from("multi_arg_snippet")
        }
    }
}

pub(super) fn render_template(template: HSTemplate, f: &mut File, json: serde_json::Value) {
    let mut reg = init_hs();
    let template = use_partial(template, &mut reg, f);

    match reg.render_template(&format!("{{{{> {}}}}}", template), &json) {
        Ok(s) => f.add_string(&s),
        Err(e) => panic!("Failed to render template: {}", e),
    }
}
