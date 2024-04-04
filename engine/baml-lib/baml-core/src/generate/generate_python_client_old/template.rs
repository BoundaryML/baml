use handlebars::handlebars_helper;

use super::file::File;

#[derive(Debug)]
pub(super) enum HSTemplate {
    Function,
    FunctionPYI,
    Enum,
    ClassPartial,
    Class,
    Client,
    BAMLClient,
    Variant,
    DefaultVariant,
    RetryPolicy,
    SingleArgTestSnippet,
    MultiArgTestSnippet,
}

handlebars_helper!(BLOCK_OPEN: |*_args| "{");
handlebars_helper!(BLOCK_CLOSE: |*_args| "}");
fn init_hs() -> handlebars::Handlebars<'static> {
    let mut reg = handlebars::Handlebars::new();
    reg.register_helper("BLOCK_OPEN", Box::new(BLOCK_OPEN));
    reg.register_helper("BLOCK_CLOSE", Box::new(BLOCK_CLOSE));
    reg.set_strict_mode(true);
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
    register_partial!(reg, "print_code", "{{{code}}}");
    match template {
        HSTemplate::Variant => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "arg_values");
            register_partial_file!(reg, "functions", "func_def");
            register_partial_file!(reg, "functions", "func_params");
            f.add_import("baml_lib._impl.deserializer", "Deserializer");
            f.add_line("import typing");
            f.add_import("baml_core.stream", "AsyncStream");
            f.add_import("baml_core.provider_manager.llm_response", "LLMResponse");

            register_partial_file!(reg, "functions", "variant");
            String::from("variant")
        }
        HSTemplate::DefaultVariant => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "arg_values");
            register_partial_file!(reg, "functions", "func_def");
            register_partial_file!(reg, "functions", "func_params");
            f.add_import("baml_lib._impl.deserializer", "Deserializer");
            f.add_line("import typing");
            f.add_import("baml_core.stream", "AsyncStream");
            f.add_import("baml_core.provider_manager.llm_response", "LLMResponse");

            register_partial_file!(reg, "functions", "default_variant");
            String::from("default_variant")
        }
        HSTemplate::BAMLClient => {
            register_partial_file!(reg, "export", "generated_baml_client");
            f.add_import("baml_core.provider_manager", "LLMManager");
            f.add_import("baml_core.services", "LogSchema");
            f.add_import("baml_core.otel", "add_message_transformer_hook");
            f.add_import("baml_core.otel", "flush_trace_logs");
            f.add_import("baml_lib", "baml_init");
            f.add_import("baml_lib", "DeserializerException");
            f.add_import("typing", "Optional");
            f.add_import("typing", "Callable");
            f.add_import("typing", "List");
            String::from("generated_baml_client")
        }
        HSTemplate::Client => {
            register_partial_file!(reg, "types", "client");
            f.add_import("baml_core.provider_manager", "LLMManager");
            String::from("client")
        }
        HSTemplate::RetryPolicy => {
            register_partial_file!(reg, "configs", "retry_policy");
            String::from("retry_policy")
        }
        HSTemplate::Class => {
            register_partial_file!(reg, "types", "class");
            f.add_import("pydantic", "BaseModel");
            f.add_import("baml_lib._impl.deserializer", "register_deserializer");
            String::from("class")
        }
        HSTemplate::ClassPartial => {
            register_partial_file!(reg, "types", "class_partial");
            f.add_import("pydantic", "BaseModel");
            f.add_import("baml_lib._impl.deserializer", "register_deserializer");
            String::from("class_partial")
        }
        HSTemplate::Enum => {
            register_partial!(reg, "enum_value", r#"{{name}} = "{{name}}""#);
            register_partial_file!(reg, "types", "enum");
            f.add_import("enum", "Enum");
            f.add_import("baml_lib._impl.deserializer", "register_deserializer");
            String::from("enum")
        }
        HSTemplate::Function => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");
            register_partial_file!(reg, "functions", "func_params");
            register_partial_file!(reg, "functions", "arg_types_list");
            register_partial_file!(reg, "functions", "interface");
            f.add_import("typing", "runtime_checkable");
            f.add_import("typing", "Protocol");
            f.add_import("typing", "Callable");
            f.add_import("typing", "AsyncIterator");
            f.add_import("baml_core.stream", "AsyncStream");

            register_partial_file!(reg, "functions", "function_py");
            f.add_import("baml_lib._impl.functions", "BaseBAMLFunction");
            String::from("function_py")
        }
        HSTemplate::FunctionPYI => {
            register_partial_file!(reg, "functions", "arg_list");
            register_partial_file!(reg, "functions", "method_def");
            register_partial_file!(reg, "functions", "func_params");
            register_partial_file!(reg, "functions", "arg_types_list");
            register_partial_file!(reg, "functions", "interface");
            f.add_import("typing", "runtime_checkable");
            f.add_import("typing", "Protocol");
            f.add_import("typing", "Callable");
            f.add_import("baml_core.stream", "AsyncStream");

            register_partial_file!(reg, "functions", "function_pyi");
            String::from("function_pyi")
        }
        HSTemplate::SingleArgTestSnippet => {
            register_partial_file!(reg, "tests", "single_arg_snippet");
            f.add_import("..__do_not_import.generated_baml_client", "baml");
            f.add_import("baml_lib._impl.deserializer", "Deserializer");
            f.add_import("json", "dumps");
            f.add_import("typing", "Any");
            String::from("single_arg_snippet")
        }
        HSTemplate::MultiArgTestSnippet => {
            register_partial_file!(reg, "tests", "multi_arg_snippet");
            f.add_import("json", "dumps");
            f.add_import("typing", "Any");
            f.add_import("..__do_not_import.generated_baml_client", "baml");
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
