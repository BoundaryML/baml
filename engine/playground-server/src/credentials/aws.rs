use aws_config::BehaviorVersion;
use aws_credential_types::provider::ProvideCredentials;

use crate::api::{LoadAwsCredsRequest, LoadAwsCredsResponse};

pub async fn load_aws_credentials(request: LoadAwsCredsRequest) -> LoadAwsCredsResponse {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(profile_name) = request.profile {
        loader = loader.profile_name(profile_name);
    }
    let config = loader.load().await;
    let Some(provider) = config.credentials_provider() else {
        return LoadAwsCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: "No credentials provider available".to_string(),
        };
    };
    let creds = provider.provide_credentials().await;
    match creds {
        Ok(creds) => LoadAwsCredsResponse::Ok {
            access_key_id: creds.access_key_id().to_string(),
            secret_access_key: creds.secret_access_key().to_string(),
            session_token: creds.session_token().map(|s| s.to_string()),
        },
        Err(e) => LoadAwsCredsResponse::Error {
            name: "CredentialLoadError".to_string(),
            message: format!("Failed to load credentials: {:?}", e),
        },
    }
}
