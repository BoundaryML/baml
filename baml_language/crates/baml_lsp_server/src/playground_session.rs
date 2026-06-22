use std::collections::HashMap;

use parking_lot::Mutex;

/// Native session-owned playground state.
///
/// Env values are logically owned by SessionStore. The native map is only the
/// process-side mirror used by `IoNamespaceEnv` before it falls back to process
/// env or the webview prompt path.
#[derive(Default)]
pub struct PlaygroundSessionStore {
    env_overrides: Mutex<HashMap<String, String>>,
}

impl PlaygroundSessionStore {
    pub fn env_override(&self, key: &str) -> Option<String> {
        self.env_overrides.lock().get(key).cloned()
    }

    pub fn set_env_override(&self, key: String, value: String) {
        self.env_overrides.lock().insert(key, value);
    }

    pub fn remove_env_override(&self, key: &str) {
        self.env_overrides.lock().remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_are_session_owned() {
        let store = PlaygroundSessionStore::default();

        store.set_env_override("API_KEY".to_string(), "secret".to_string());
        assert_eq!(store.env_override("API_KEY").as_deref(), Some("secret"));

        store.remove_env_override("API_KEY");
        assert_eq!(store.env_override("API_KEY"), None);
    }
}
