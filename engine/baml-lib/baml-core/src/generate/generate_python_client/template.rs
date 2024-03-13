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
    // EnumInternal,
    Class,
    ClassPartial,

    // ClassInternal,
    Function,
    FunctionPYI,

    Impl,
    Client,
    ExportFile,

    RetryPolicy,

    SingleArgTestSnippet,
    MultiArgTestSnippet,
}

pub(super) fn render_with_hbs<T: serde::Serialize>(template: Template, data: &T) -> String {
    let mut reg = handlebars::Handlebars::new();
    reg.register_helper("BLOCK_OPEN", Box::new(BLOCK_OPEN));
    reg.register_helper("BLOCK_CLOSE", Box::new(BLOCK_CLOSE));
    reg.set_strict_mode(true);

    let content = serde_json::to_string(&data).unwrap();
    register_partial!(reg, "print_code", "{{{code}}}");

    let template = match template {
        Template::Enum => {
            register_partial!(reg, "enum_value", r#"{{name}} = "{{name}}""#);

            register_partial_file!(reg, "types", "enum");
            "enum"
        }
        // Template::EnumInternal => {
        //     register_partial_file!(reg, "types", "enum_internal");
        //     "enum_internal"
        // }
        Template::Class => {
            register_partial_file!(reg, "types", "class");
            "class"
        }
        Template::ClassPartial => {
            register_partial_file!(reg, "types", "class_partial");
            // f.add_import("pydantic", "BaseModel");
            // f.add_import("baml_lib._impl.deserializer", "register_deserializer");
            "class_partial"
        }
        // Template::ClassInternal => {
        //     register_partial_file!(reg, "types", "class_internal");
        //     "class_internal"
        // }
        Template::Function => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");
            register_partial_file!(reg, "functions", "func_params");
            register_partial_file!(reg, "functions", "arg_types_list");
            register_partial_file!(reg, "functions", "interface");
            register_partial_file!(reg, "functions", "function_py");

            // register_partial_file!(reg, "functions", "function");
            "function"
        }
        Template::FunctionPYI => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");
            register_partial_file!(reg, "functions", "func_params");
            register_partial_file!(reg, "functions", "arg_types_list");
            register_partial_file!(reg, "functions", "interface");

            register_partial_file!(reg, "functions", "function_pyi");
            "function_pyi"
        }
        Template::Impl => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "arg_values");
            register_partial_file!(reg, "functions", "func_def");
            register_partial_file!(reg, "functions", "func_params");

            register_partial_file!(reg, "functions", "impl");

            "impl"
        }
        Template::Client => {
            register_partial_file!(reg, "types", "client");
            "client"
        }
        Template::RetryPolicy => {
            register_partial_file!(reg, "configs", "retry_policy");
            "retry_policy"
        }
        Template::ExportFile => {
            register_partial_file!(reg, "export", "generated_baml_client");
            "generated_baml_client"
        }
        Template::SingleArgTestSnippet => {
            register_partial_file!(reg, "tests", "single_arg_snippet");

            "single_arg_snippet"
        }
        Template::MultiArgTestSnippet => {
            register_partial_file!(reg, "tests", "multi_arg_snippet");

            "multi_arg_snippet"
        }
    };

    match reg.render_template(&format!("{{{{> {}}}}}", template), &data) {
        Ok(s) => return s,
        Err(e) => panic!("Failed to render template: {}", e),
    }
}
