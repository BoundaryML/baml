use crate::server::api::ResultExt;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeConfiguration;

impl super::NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl super::SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        _session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        tracing::info!("*** DID CHANGE CONFIGURATION");

        // Extract the BAML configuration from the params
        if let Some(settings) = params.settings.as_object() {
            if let Some(baml_settings) = settings.get("baml") {
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
            }
        }

        Ok(())
    }
}
