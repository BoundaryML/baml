use log::info;

macro_rules! include_template {
    ($type:expr, $file:expr) => {
        include_str!(concat!("templates/", $type, "/", $file, ".hbs"))
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

handlebars::handlebars_helper!(BLOCK_OPEN: |*_args| "{");
handlebars::handlebars_helper!(BLOCK_CLOSE: |*_args| "}");

pub(super) enum Template {
    Enum,
    EnumInternal,
    Class,
    ClassInternal,
    Function,
    Impl,
    TestCase,
    Client,
    ExportFile,
}

pub(super) fn render_with_hbs<T: serde::Serialize>(template: Template, data: &T) -> String {
    let mut reg = handlebars::Handlebars::new();
    reg.register_helper("BLOCK_OPEN", Box::new(BLOCK_OPEN));
    reg.register_helper("BLOCK_CLOSE", Box::new(BLOCK_CLOSE));
    reg.set_strict_mode(true);

    let content = serde_json::to_string(&data).unwrap();

    let template = match template {
        Template::Enum => {
            register_partial_file!(reg, "types", "enum");
            "enum"
        }
        Template::EnumInternal => {
            register_partial_file!(reg, "types", "enum_internal");
            "enum_internal"
        }
        Template::Class => {
            register_partial_file!(reg, "types", "class");
            "class"
        }
        Template::ClassInternal => {
            register_partial_file!(reg, "types", "class_internal");
            "class_internal"
        }
        Template::Function => {
            info!("Content: {}", content);
            register_partial_file!(reg, "functions", "function");
            "function"
        }
        Template::Impl => {
            register_partial_file!(reg, "functions", "impl");
            "impl"
        }
        Template::TestCase => {
            register_partial_file!(reg, "functions", "test_case");
            "test_case"
        }
        Template::Client => {
            register_partial_file!(reg, "types", "client");
            "client"
        }
        Template::ExportFile => {
            register_partial_file!(reg, "exports", "baml_client");
            "baml_client"
        }
    };

    match reg.render_template(&format!("{{{{> {}}}}}", template), &data) {
        Ok(s) => return s,
        Err(e) => panic!("Failed to render template: {}", e),
    }
}
