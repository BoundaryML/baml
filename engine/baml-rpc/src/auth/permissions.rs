use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    // Trace/Span permissions
    TracesWrite,
    TracesRead,
    TracesDelete,

    // Feedback permissions
    FeedbackWrite,
    FeedbackRead,
    FeedbackUpdate,

    // Analytics permissions
    AnalyticsRead,
    AnalyticsExport,

    // BAML permissions
    BamlRead,
    BamlDeploy,

    // Admin permissions
    ApiKeysManage,
    ProjectSettingsManage,

    // Wildcard permissions
    AllRead,
    AllWrite,
    AllAdmin,
}

impl Permission {
    pub fn to_string_pattern(&self) -> String {
        match self {
            Permission::TracesWrite => "traces:write:*".to_string(),
            Permission::TracesRead => "traces:read:*".to_string(),
            Permission::TracesDelete => "traces:delete:*".to_string(),

            Permission::FeedbackWrite => "feedback:write:*".to_string(),
            Permission::FeedbackRead => "feedback:read:*".to_string(),
            Permission::FeedbackUpdate => "feedback:update:*".to_string(),

            Permission::AnalyticsRead => "analytics:read:*".to_string(),
            Permission::AnalyticsExport => "analytics:export:*".to_string(),

            Permission::BamlRead => "baml:read:*".to_string(),
            Permission::BamlDeploy => "baml:deploy:*".to_string(),

            Permission::ApiKeysManage => "api_keys:manage:*".to_string(),
            Permission::ProjectSettingsManage => "project:settings:*".to_string(),

            Permission::AllRead => "*:read:*".to_string(),
            Permission::AllWrite => "*:write:*".to_string(),
            Permission::AllAdmin => "*:*:*".to_string(),
        }
    }

    pub fn from_string_pattern(pattern: &str) -> Option<Self> {
        match pattern {
            "traces:write:*" => Some(Permission::TracesWrite),
            "traces:read:*" => Some(Permission::TracesRead),
            "traces:delete:*" => Some(Permission::TracesDelete),

            "feedback:write:*" => Some(Permission::FeedbackWrite),
            "feedback:read:*" => Some(Permission::FeedbackRead),
            "feedback:update:*" => Some(Permission::FeedbackUpdate),

            "analytics:read:*" => Some(Permission::AnalyticsRead),
            "analytics:export:*" => Some(Permission::AnalyticsExport),

            "baml:read:*" => Some(Permission::BamlRead),
            "baml:deploy:*" => Some(Permission::BamlDeploy),

            "api_keys:manage:*" => Some(Permission::ApiKeysManage),
            "project:settings:*" => Some(Permission::ProjectSettingsManage),

            "*:read:*" => Some(Permission::AllRead),
            "*:write:*" => Some(Permission::AllWrite),
            "*:*:*" => Some(Permission::AllAdmin),

            _ => None,
        }
    }

    pub fn implies(&self, required: &Permission) -> bool {
        match self {
            Permission::AllAdmin => true,
            Permission::AllRead => matches!(
                required,
                Permission::TracesRead
                    | Permission::FeedbackRead
                    | Permission::AnalyticsRead
                    | Permission::BamlRead
            ),
            Permission::AllWrite => matches!(
                required,
                Permission::TracesWrite | Permission::FeedbackWrite
            ),
            _ => self == required,
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string_pattern())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PermissionTemplate {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<Permission>,
    pub org_id: Option<String>,
}

// Common permission sets
pub fn write_only_permissions() -> Vec<Permission> {
    vec![Permission::TracesWrite, Permission::FeedbackWrite]
}

pub fn read_only_permissions() -> Vec<Permission> {
    vec![
        Permission::TracesRead,
        Permission::FeedbackRead,
        Permission::AnalyticsRead,
        Permission::BamlRead,
    ]
}

pub fn full_access_permissions() -> Vec<Permission> {
    vec![
        Permission::TracesWrite,
        Permission::TracesRead,
        Permission::TracesDelete,
        Permission::FeedbackWrite,
        Permission::FeedbackRead,
        Permission::FeedbackUpdate,
        Permission::AnalyticsRead,
        Permission::AnalyticsExport,
        Permission::BamlRead,
        Permission::BamlDeploy,
    ]
}

pub fn admin_permissions() -> Vec<Permission> {
    vec![Permission::AllAdmin]
}
