use lsp_types::ClientCapabilities;
// use ruff_linter::display_settings;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ResolvedClientCapabilities {
    pub code_action_deferred_edit_resolution: bool,
    pub apply_edit: bool,
    pub document_changes: bool,
    pub workspace_refresh: bool,
    pub pull_diagnostics: bool,
}

impl ResolvedClientCapabilities {
    pub fn new(client_capabilities: &ClientCapabilities) -> Self {
        let code_action_settings = client_capabilities
            .text_document
            .as_ref()
            .and_then(|doc_settings| doc_settings.code_action.as_ref());
        let code_action_data_support = code_action_settings
            .and_then(|code_action_settings| code_action_settings.data_support)
            .unwrap_or_default();
        let code_action_edit_resolution = code_action_settings
            .and_then(|code_action_settings| code_action_settings.resolve_support.as_ref())
            .is_some_and(|resolve_support| resolve_support.properties.contains(&"edit".into()));

        let apply_edit = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.apply_edit)
            .unwrap_or_default();

        let document_changes = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.workspace_edit.as_ref())
            .and_then(|workspace_edit| workspace_edit.document_changes)
            .unwrap_or_default();

        let workspace_refresh = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.diagnostic.as_ref())
            .and_then(|diagnostic| diagnostic.refresh_support)
            .unwrap_or_default();

        let pull_diagnostics = client_capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.diagnostic.as_ref())
            .is_some();

        Self {
            code_action_deferred_edit_resolution: code_action_data_support
                && code_action_edit_resolution,
            apply_edit,
            document_changes,
            workspace_refresh,
            pull_diagnostics,
        }
    }
}

impl std::fmt::Display for ResolvedClientCapabilities {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // display_settings! {
        //     formatter = f,
        //     namespace = "capabilities",
        //     fields = [
        //         self.code_action_deferred_edit_resolution,
        //         self.apply_edit,
        //         self.document_changes,
        //         self.workspace_refresh,
        //         self.pull_diagnostics,
        //     ]
        // };
        Ok(())
    }
}
