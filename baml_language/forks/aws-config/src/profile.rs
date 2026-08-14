//! AWS shared config / credentials file loading and profile resolution.

use std::collections::HashMap;

use crate::{CredentialIo, IniFile, Profiles};

/// Resolve the active profile name: explicit override, else `AWS_PROFILE`, else
/// `"default"`.
pub(crate) async fn active_profile_name(
    io: &dyn CredentialIo,
    profile_override: Option<&str>,
) -> String {
    if let Some(p) = profile_override.filter(|s| !s.is_empty()) {
        return p.to_string();
    }
    io.env("AWS_PROFILE")
        .await
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
}

/// The shared credentials file path (`AWS_SHARED_CREDENTIALS_FILE` or
/// `~/.aws/credentials`).
async fn credentials_path(io: &dyn CredentialIo) -> Option<String> {
    if let Some(p) = io
        .env("AWS_SHARED_CREDENTIALS_FILE")
        .await
        .filter(|s| !s.is_empty())
    {
        return Some(p);
    }
    home_dir(io).await.map(|h| format!("{h}/.aws/credentials"))
}

/// The shared config file path (`AWS_CONFIG_FILE` or `~/.aws/config`).
async fn config_path(io: &dyn CredentialIo) -> Option<String> {
    if let Some(p) = io.env("AWS_CONFIG_FILE").await.filter(|s| !s.is_empty()) {
        return Some(p);
    }
    home_dir(io).await.map(|h| format!("{h}/.aws/config"))
}

async fn home_dir(io: &dyn CredentialIo) -> Option<String> {
    if let Some(h) = io.env("HOME").await.filter(|s| !s.is_empty()) {
        return Some(h);
    }
    // Windows fallback.
    io.env("USERPROFILE").await.filter(|s| !s.is_empty())
}

/// Load and merge profiles from the config and credentials files.
///
/// In the config file, non-default profiles are written as `[profile NAME]`
/// (and `[default]` for the default); in the credentials file they are bare
/// `[NAME]`. The credentials file takes precedence on a per-key basis, matching
/// the AWS SDK.
pub(crate) async fn load_profiles(io: &dyn CredentialIo) -> Profiles {
    let mut profiles: Profiles = HashMap::new();

    // Config file first (lower precedence).
    if let Some(path) = config_path(io).await {
        if let Some(text) = io.read_file(&path).await {
            let ini = IniFile::parse(&text);
            for (section, entries) in ini.iter() {
                if let Some(name) = config_section_to_profile(section) {
                    profiles.entry(name).or_default().extend(entries.clone());
                }
            }
        }
    }

    // Credentials file (higher precedence) — bare section names are profiles.
    if let Some(path) = credentials_path(io).await {
        if let Some(text) = io.read_file(&path).await {
            let ini = IniFile::parse(&text);
            for (section, entries) in ini.iter() {
                profiles
                    .entry(section.clone())
                    .or_default()
                    .extend(entries.clone());
            }
        }
    }

    profiles
}

/// Load `[sso-session NAME]` sections from the config file.
pub(crate) async fn load_sso_sessions(
    io: &dyn CredentialIo,
) -> HashMap<String, HashMap<String, String>> {
    let mut sessions = HashMap::new();
    if let Some(path) = config_path(io).await {
        if let Some(text) = io.read_file(&path).await {
            let ini = IniFile::parse(&text);
            for (section, entries) in ini.iter() {
                if let Some(name) = section.strip_prefix("sso-session ") {
                    sessions.insert(name.trim().to_string(), entries.clone());
                }
            }
        }
    }
    sessions
}

/// Resolve `region` from a profile, following `source_profile` links the same
/// way upstream's profile region provider does.
pub(crate) fn resolve_region_from_profiles(
    profiles: &Profiles,
    selected_profile: &str,
) -> Option<String> {
    let mut selected_profile = selected_profile;
    let mut visited = Vec::new();

    loop {
        if visited.contains(&selected_profile) {
            return None;
        }
        visited.push(selected_profile);

        let profile = profiles.get(selected_profile)?;
        if let Some(region) = profile.get("region") {
            return Some(region.clone());
        }
        let source_profile = profile.get("source_profile")?;
        if source_profile == selected_profile {
            return None;
        }
        selected_profile = source_profile;
    }
}

/// Map a config-file section name to a profile name, or `None` if it is not a
/// profile section (e.g. `sso-session`, `services`).
fn config_section_to_profile(section: &str) -> Option<String> {
    if section == "default" {
        Some("default".to_string())
    } else {
        section
            .strip_prefix("profile ")
            .map(|rest| rest.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_section_mapping() {
        assert_eq!(
            config_section_to_profile("default").as_deref(),
            Some("default")
        );
        assert_eq!(
            config_section_to_profile("profile work").as_deref(),
            Some("work")
        );
        assert_eq!(config_section_to_profile("sso-session my"), None);
        // A bare non-default name in the config file is NOT a valid profile.
        assert_eq!(config_section_to_profile("work"), None);
    }
}
