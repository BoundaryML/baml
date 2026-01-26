// Run from the baml-schema-wasm folder with:
// RUST_LOG=info wasm-pack test --node --test test_log_collector
// and make sure to set rust-analyzer target in vscode settings to:   "rust-analyzer.cargo.target": "wasm32-unknown-unknown",

// Install wasm bindgen cli: cargo install wasm-bindgen-cli --version 0.2.101
// Browser test command is:
// RUST_BACKTRACE=1 RUST_LOG=info wasm-pack test --chrome --headless --test test_log_collector -- --nocapture
#[cfg(target_arch = "wasm32")]
#[cfg(test)]
pub(crate) mod tests {

    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    use baml_runtime::tracingv2::publisher::publisher::flush;
    // pub static GLOBAL_TRACE_STORAGE: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));
    use baml_runtime::tracingv2::storage::storage::BAML_TRACER;
    use baml_schema_build::runtime_wasm::WasmProject;
    use serde_wasm_bindgen::to_value;
    use wasm_bindgen::{closure::Closure, JsCast, JsValue};
    use wasm_bindgen_test::*;

    // instantiate logger

    wasm_bindgen_test_configure!(run_in_browser);

    // TODO: add a flag to run_test that's debug, to attach a collector on every test. The collector can be created inside run_test. Make the collector call track_function(..)
    // Then we can check the baml_tracer events since they'll be kept.
    #[wasm_bindgen_test]
    async fn test_run_tests() {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
        let sample_baml_content = r##"
    function Func(name: string ) -> string {
            client "openai/gpt-4o"
            prompt #"
            Return the name of {{name}}
            "#
    }

    test One {
        functions [Func]
        args {
            name "john"
        }
    }

            "##;
        let mut files = HashMap::new();
        files.insert("error.baml".to_string(), sample_baml_content.to_string());
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();

        let env_vars = [("OPENAI_API_KEY", "12345")]
            .iter()
            .cloned()
            .collect::<HashMap<_, _>>();
        let env_vars_js = to_value(&env_vars).unwrap();
        let undefined_js = JsValue::UNDEFINED;

        let mut current_runtime = project.runtime(env_vars_js.clone(), undefined_js).unwrap();

        let diagnostics = project.diagnostics(&current_runtime);
        assert!(diagnostics.errors().is_empty());

        let functions = current_runtime.list_functions(None);

        for f in functions.iter() {
            f.run_test(
                &mut current_runtime,
                "One".to_string(),
                js_sys::Function::new_no_args(""),
                js_sys::Function::new_no_args(""),
                env_vars_js.clone().into(),
                None,
                js_sys::Function::new_no_args(""),
            )
            .await
            .expect("run_test should succeed");
        }

        let events = BAML_TRACER.lock().unwrap().events();
        log::info!("Events {events:#?}");
        // TODO: this makes the test hang on Node, but not in browser.
        let _ = flush().await;

        log::info!("done!!")
    }

    #[wasm_bindgen_test]
    async fn test_streaming_run_test_with_expr_events() {
        let openai_key = match std::env::var("OPENAI_API_KEY") {
            Ok(key) if !key.trim().is_empty() => key,
            _ => {
                log::warn!("Skipping streaming test due to missing OPENAI_API_KEY");
                return;
            }
        };

        BAML_TRACER.lock().unwrap().clear();

        let sample_baml_content = r##"
    function Func(name: string ) -> string {
            client "openai/gpt-4o"
            prompt #"
            Return the name of {{name}}
            "#
    }

    test One {
        functions [Func]
        args {
            name "stream"
        }
    }

            "##;

        let mut files = HashMap::new();
        files.insert(
            "streaming.baml".to_string(),
            sample_baml_content.to_string(),
        );
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();

        let env_vars = HashMap::from([("OPENAI_API_KEY".to_string(), openai_key)]);
        let env_vars_js = to_value(&env_vars).unwrap();
        let undefined_js = JsValue::UNDEFINED;

        let mut current_runtime = project.runtime(env_vars_js.clone(), undefined_js).unwrap();

        let diagnostics = project.diagnostics(&current_runtime);
        assert!(diagnostics.errors().is_empty());

        let functions = current_runtime.list_functions(None);
        let partial_results: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));
        let expr_events: Rc<RefCell<Vec<JsValue>>> = Rc::new(RefCell::new(Vec::new()));

        for f in functions.iter() {
            let partial_results_cb = {
                let responses = partial_results.clone();
                Closure::wrap(Box::new(move |value: JsValue| {
                    responses.borrow_mut().push(value);
                }) as Box<dyn FnMut(JsValue)>)
            };

            let expr_events_cb = {
                let events = expr_events.clone();
                Closure::wrap(Box::new(move |value: JsValue| {
                    events.borrow_mut().push(value);
                }) as Box<dyn FnMut(JsValue)>)
            };

            let on_partial_fn: js_sys::Function = partial_results_cb
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone();
            let on_expr_fn: js_sys::Function = expr_events_cb
                .as_ref()
                .unchecked_ref::<js_sys::Function>()
                .clone();

            f.run_test_with_expr_events(
                &mut current_runtime,
                "One".to_string(),
                on_partial_fn,
                js_sys::Function::new_no_args(""),
                on_expr_fn,
                env_vars_js.clone().into(),
                None,
                js_sys::Function::new_no_args(""),
            )
            .await
            .expect("streaming run_test_with_expr_events should succeed");

            drop(partial_results_cb);
            drop(expr_events_cb);
        }

        let tracer = BAML_TRACER.lock().unwrap();
        assert!(tracer.function_call_count() > 0);
    }
}

// use wasm_bindgen_test::*;
// wasm_bindgen_test_configure!(run_in_browser);

// #[wasm_bindgen_test]
// fn test_hello_world() {
//     wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
//     println!("Hello, world!");
// }
