// run these tests with:
// RUST_LOG=info cargo test test_call_function_unions1 --no-default-features --features "internal" -- --nocapture
// need to fix the tokio runtime getting closed but at least you can log things.
// #[cfg(feature = "internal")]
mod internal_tests {
    use std::any;
    use std::collections::HashMap;

    use baml_runtime::{tracingv2::storage::storage::BAML_TRACER, BamlRuntime};
    use std::sync::Once;

    // use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
    use baml_runtime::InternalRuntimeInterface;
    use baml_runtime::{
        internal::llm_client::LLMResponse, DiagnosticsError, IRHelper, RenderedPrompt,
    };
    use baml_types::tracing::events::FunctionId;
    use baml_types::BamlValue;

    use wasm_bindgen_test::*;

    static INIT: Once = Once::new();

    #[test]
    fn test_log_collector() -> Result<(), Box<dyn std::error::Error>> {
        INIT.call_once(|| {
            env_logger::init();
            console_error_panic_hook::set_once();
        });
        log::info!("Running test_call_function");

        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"

        class Education {
          school string | null @description(#"
            111
          "#)
          degree string @description(#"
            2222222
          "#)
        }

        client<llm> GPT4Turbo {
          provider baml-openai-chat
          options {
            model gpt-4-1106-preview
            api_key env.OPENAI_API_KEY
          }
        }


        function Extract(input: string) -> Education {
          client GPT4Turbo
          prompt #"

            {{ ctx.output_format }}
          "#
        }

        test Test {
          functions [Extract]
          args {
            input "hi"
          }
        }
        "##,
        );

        let function_name = "Extract";
        let test_name = "Test";

        let runtime = BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
        )?;
        log::info!("Runtime:");

        let missing_env_vars = runtime.internal().ir().required_env_vars();

        let ctx_manager = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);
        let ctx = ctx_manager.create_ctx_with_default();

        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;

        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, Some(0));

        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        let call_function_future = runtime.call_function(
            function_name.to_string(),
            &params,
            &ctx_manager,
            None,
            None,
            // TODO: add an actual collector
            None,
        );

        let (res, function_span_id) = runtime.async_runtime.block_on(call_function_future);

        let trace_storage = BAML_TRACER.lock().unwrap();
        let trace = trace_storage
            .get(FunctionId(function_span_id.unwrap_or_default().to_string()))
            .unwrap();

        log::info!("Trace: {:#?}", trace);

        runtime.async_runtime.block_on(flush());

        Ok(())
    }
}
