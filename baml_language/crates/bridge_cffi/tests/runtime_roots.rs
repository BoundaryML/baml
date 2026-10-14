use std::collections::HashMap;

fn sources() -> HashMap<String, String> {
    HashMap::from([(
        "main.baml".to_string(),
        "function Add(a: int, b: int) -> int { a + b }".to_string(),
    )])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runtime_initialization_accepts_trailing_slashes() {
    for path in [".", "baml_src/", "baml_src///"] {
        if let Err(error) = bridge_cffi::initialize_runtime(path, sources()) {
            panic!("native initialization of {path:?} failed: {error}");
        }
        bridge_cffi::shutdown_runtime(Some(std::time::Duration::from_secs(1)))
            .await
            .unwrap();
        if let Err(error) = bridge_cffi::initialize_runtime_from_files_with_sys_ops(
            path,
            sources(),
            sys_ops::SysOps::all_host_unavailable(),
        ) {
            panic!("platform-neutral initialization of {path:?} failed: {error}");
        }
        bridge_cffi::shutdown_runtime(Some(std::time::Duration::from_secs(1)))
            .await
            .unwrap();
    }
}
