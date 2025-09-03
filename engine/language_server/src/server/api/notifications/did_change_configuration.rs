use lsp_types::{self as types, notification as notif, ConfigurationItem, ConfigurationParams};

use crate::{
    server::{
        api::{diagnostics::publish_diagnostics, ResultExt},
        client::{Notifier, Requester},
        Result, Task,
    },
    session::Session,
};

pub(crate) struct DidChangeConfiguration;

/// Republishes diagnostics for all projects when feature flags change
fn republish_all_diagnostics(notifier: &Notifier, session: &mut Session) {
    let projects = session.baml_src_projects.clone();
    let projects_guard = projects.lock();

    let default_flags = vec!["beta".to_string()];
    let effective_flags = session
        .baml_settings
        .feature_flags
        .as_ref()
        .unwrap_or(&default_flags);
    tracing::info!(
        "Republishing diagnostics for {} projects with feature_flags: {:?}",
        projects_guard.len(),
        &effective_flags
    );

    for (root_path, project) in projects_guard.iter() {
        tracing::info!("Republishing diagnostics for project at: {:?}", root_path);
        if let Err(e) =
            publish_diagnostics(notifier, project.clone(), None, effective_flags, session)
        {
            tracing::error!(
                "Failed to republish diagnostics for project {:?}: {}",
                root_path,
                e
            );
        }
    }
}

impl super::NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl super::SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        _session: &mut Session,
        notifier: Notifier,
        requester: &mut Requester,
        params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        tracing::info!("*** DID CHANGE CONFIGURATION: {:?}", params);

        // Extract the BAML configuration from the params
        if let Some(settings) = params.settings.as_object() {
            if let Some(baml_settings) = settings.get("baml") {
                tracing::info!(
                    "BAML settings received in did_change_configuration: {:?}",
                    baml_settings
                );

                // Extract and log feature flags specifically
                if let Some(feature_flags) = baml_settings.get("featureFlags") {
                    tracing::info!("Feature flags in configuration change: {:?}", feature_flags);
                }

                // Send the BAML settings as a notification
                notifier
                    .0
                    .send(lsp_server::Message::Notification(
                        lsp_server::Notification::new(
                            "baml_settings_updated".to_string(),
                            baml_settings.clone(),
                        ),
                    ))
                    .internal_error()?;
                tracing::info!("Sent baml_settings_updated notification");
                let feature_flags_changed = _session.update_baml_settings(baml_settings.clone());

                // Republish diagnostics if feature flags changed
                if feature_flags_changed {
                    tracing::info!(
                        "Feature flags changed, republishing diagnostics for all projects"
                    );
                    republish_all_diagnostics(&notifier, _session);
                }
            }
        }

        // Also manually schedule a request for latest settings since sometimes the above params just have Null (not sure why)
        // note that the task will run after this current task is done.
        let notifier_clone = notifier.clone();
        requester
            .request::<types::request::WorkspaceConfiguration>(
                ConfigurationParams {
                    items: vec![types::ConfigurationItem {
                        scope_uri: None,
                        section: Some("baml".to_string()),
                    }],
                },
                move |response| {
                    let notifier = notifier_clone.clone();
                    Task::local(move |session, _, _, _| {
                        tracing::info!("Workspace configuration request received: {:?}", response);
                        if let Some(first_response) = response.first() {
                            tracing::info!("Workspace configuration first_response: {:?}", first_response);
                            if let Some(feature_flags) = first_response.get("featureFlags") {
                                tracing::info!("Feature flags in workspace configuration: {:?}", feature_flags);
                            }
                            let feature_flags_changed = session.update_baml_settings(first_response.clone());

                            // Republish diagnostics if feature flags changed
                            if feature_flags_changed {
                                tracing::info!("Feature flags changed from workspace config, republishing diagnostics for all projects");
                                republish_all_diagnostics(&notifier, session);
                            }
                        }
                    })
                },
            )
            .internal_error()?;

        Ok(())
    }
}
