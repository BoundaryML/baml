use std::{collections::HashMap, sync::Mutex};

use bex_project::{BexArgs, FunctionCallContextBuilder};

static REPORTED: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

extern "C" fn capture(content: *const i8, length: usize, _cancelled: i32) {
    let bytes = if content.is_null() || length == 0 {
        Vec::new()
    } else {
        // SAFETY: bridge_cffi keeps the callback bytes valid until return.
        unsafe { std::slice::from_raw_parts(content.cast::<u8>(), length) }.to_vec()
    };
    REPORTED.lock().unwrap().push(bytes);
}

#[tokio::test]
async fn unhandled_spawn_error_reaches_registered_bridge_callback() {
    bridge_cffi::register_unhandled_spawn_error_callback(capture);
    bridge_cffi::initialize_runtime(
        ".",
        HashMap::from([(
            "main.baml".to_string(),
            r#"
                function bad() -> int throws string { throw "boom" }
                function main() -> int {
                    spawn { bad() };
                    1
                }
            "#
            .to_string(),
        )]),
    )
    .unwrap();
    let runtime = bridge_cffi::get_runtime().unwrap();
    runtime
        .call_function(
            "main",
            BexArgs(HashMap::new()),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        )
        .await
        .unwrap();
    bridge_cffi::shutdown_runtime().await.unwrap();

    assert_eq!(REPORTED.lock().unwrap().len(), 1);
}
