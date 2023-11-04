use handlebars::handlebars_helper;

use super::file::File;

#[derive(Debug)]
pub(super) enum HSTemplate {
    Function,
    FunctionTestFixture,
    FunctionPYI,
    Enum,
    Class,
    Client,
    BAMLClient,
    Variant,
    RetryPolicy,
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
        include_str!(concat!("templates/", $type, "/", $file, ".hb"))
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
    match template {
        HSTemplate::Variant => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "arg_values");
            register_partial_file!(reg, "functions", "func_def");
            f.add_import("baml_core._impl.deserializer", "Deserializer");

            register_partial_file!(reg, "functions", "variant");
            String::from("variant")
        }
        HSTemplate::BAMLClient => {
            register_partial_file!(reg, "export", "generated_baml_client");
            String::from("generated_baml_client")
        }
        HSTemplate::Client => {
            register_partial_file!(reg, "types", "client");
            f.add_import("baml_core._impl.provider", "LLMManager");
            String::from("client")
        }
        HSTemplate::RetryPolicy => {
            register_partial_file!(reg, "configs", "retry_policy");
            String::from("retry_policy")
        }
        HSTemplate::Class => {
            register_partial_file!(reg, "types", "class");
            f.add_import("pydantic", "BaseModel");
            f.add_import("baml_core._impl.deserializer", "register_deserializer");
            String::from("class")
        }
        HSTemplate::Enum => {
            register_partial!(reg, "enum_value", r#"{{name}} = "{{name}}""#);
            register_partial_file!(reg, "types", "enum");
            f.add_import("enum", "Enum");
            f.add_import("baml_core._impl.deserializer", "register_deserializer");
            String::from("enum")
        }
        HSTemplate::Function => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");

            register_partial_file!(reg, "functions", "interface");
            f.add_import("typing", "runtime_checkable");
            f.add_import("typing", "Protocol");

            register_partial_file!(reg, "functions", "function_py");
            f.add_import("baml_core._impl.functions", "BaseBAMLFunction");
            String::from("function_py")
        }
        HSTemplate::FunctionTestFixture => {
            register_partial_file!(reg, "functions", "fixture");
            f.add_import("_pytest.fixtures", "FixtureRequest");
            f.add_import(".generated_baml_client", "baml");
            String::from("fixture")
        }
        HSTemplate::FunctionPYI => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");

            register_partial_file!(reg, "functions", "interface");
            f.add_import("typing", "runtime_checkable");
            f.add_import("typing", "Protocol");

            register_partial_file!(reg, "functions", "function_pyi");
            String::from("function_pyi")
        }
    }
}

pub(super) fn render_template(template: HSTemplate, f: &mut File, json: serde_json::Value) {
    let mut reg = init_hs();
    let template = use_partial(template, &mut reg, f);

    match reg.render_template(&format!("{{{{> {}}}}}", template), &json) {
        Ok(s) => {
            // info!("Rendered template:\n{}\n------\n", &s);
            f.add_string(&s)
        }
        Err(e) => panic!("Failed to render template: {}", e),
    }
}
