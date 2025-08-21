use lsp_types::{self as types, notification as notif, ConfigurationItem, ConfigurationParams};

use crate::{
    server::{
        api::ResultExt,
        client::{Notifier, Requester},
        Result, Task,
    },
    session::Session,
};

pub(crate) struct DidChangeConfiguration;

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
                tracing::info!("BAML settings: {:?}", baml_settings);
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
                _session.update_baml_settings(baml_settings.clone());
            }
        }

        // Also manually schedule a request for latest settings since sometimes the above params just have Null (not sure why)
        // note that the task will run after this current task is done.
        requester
            .request::<types::request::WorkspaceConfiguration>(
                ConfigurationParams {
                    items: vec![types::ConfigurationItem {
                        scope_uri: None,
                        section: Some("baml".to_string()),
                    }],
                },
                |response| {
                    Task::local(move |session, _, _, _| {
                        tracing::info!("Workspace configuration request received: {:?}", response);
                        if let Some(first_response) = response.first() {
                            session.update_baml_settings(first_response.clone());
                        }
                    })
                },
            )
            .internal_error()?;

        Ok(())
    }
}
