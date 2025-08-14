// -pub async fn broadcast_project_update(
//   -    state: &Arc<RwLock<PlaygroundState>>,
//   -    root_path: &str,
//   -    files: HashMap<String, String>,
//   -) -> Result<()> {
//   -    let add_project_msg = FrontendMessage::add_project {
//   -        root_path: root_path.to_string(),
//   -        files,
//   -    };
//   -
//   -    let msg_str = serde_json::to_string(&add_project_msg)?;
//   -    let mut st = state.write().await;
//   -    if !st.first_client_connected {
//   -        st.buffer_event(msg_str);
//   -    } else if let Err(e) = st.broadcast_update(msg_str) {
//   -        tracing::error!("Failed to broadcast project update: {}", e);
//   -    }
//   -    Ok(())
//   -}
//   -
//   -// Helper function to broadcast function changes
//   -pub async fn broadcast_function_change(
//   -    state: &Arc<RwLock<PlaygroundState>>,
//   -    root_path: &str,
//   -    function_name: String,
//   -) -> Result<()> {
//   -    tracing::debug!("Broadcasting function change for: {}", function_name);
//   -
//   -    // broadcast to all connected clients
//   -    let select_function_msg = FrontendMessage::select_function {
//   -        root_path: root_path.to_string(),
//   -        function_name,
//   -    };
//   -
//   -    let msg_str = serde_json::to_string(&select_function_msg)?;
//   -    let mut st = state.write().await;
//   -    if !st.first_client_connected {
//   -        st.buffer_event(msg_str);
//   -    } else if let Err(e) = st.broadcast_update(msg_str) {
//   -        tracing::error!("Failed to broadcast function change: {}", e);
//   -    }
//   -    Ok(())
//   -}
//   -
//   -// Helper function to broadcast test runs
//   -pub async fn broadcast_test_run(
//   -    state: &Arc<RwLock<PlaygroundState>>,
//   -    test_name: String,
//   -) -> Result<()> {
//   -    tracing::debug!("Broadcasting test run for: {}", test_name);
//   -
//   -    // broadcast to all connected clients
//   -    let run_test_msg = FrontendMessage::run_test { test_name };
//   -
//   -    let msg_str = serde_json::to_string(&run_test_msg)?;
//   -    let mut st = state.write().await;
//   -    if !st.first_client_connected {
//   -        st.buffer_event(msg_str);
//   -    } else if let Err(e) = st.broadcast_update(msg_str) {
//   -        tracing::error!("Failed to broadcast test run: {}", e);
//   -    }
//   -    Ok(())
//   -}
