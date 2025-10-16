use std::str::FromStr;

use strum::{Display, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, IntoStaticStr)]
pub enum BamlFeatureFlag {
    /// Enable beta features and suppress experimental warnings
    #[strum(serialize = "beta")]
    Beta,

    /// Display all warnings in CLI output (warnings are quiet by default)
    #[strum(serialize = "display_all_warnings")]
    DisplayAllWarnings,
    // Future features can be added here
}

impl BamlFeatureFlag {
    /// Returns a user-friendly description of the feature
    pub fn description(&self) -> &'static str {
        match self {
            Self::Beta => "Enable beta features and suppress experimental warnings",
            Self::DisplayAllWarnings => {
                "Display all warnings in CLI output (warnings are quiet by default)"
            }
        }
    }
}

/// Container for active feature flags
#[derive(Debug, Clone, Default)]
pub struct FeatureFlags {
    flags: std::collections::HashSet<BamlFeatureFlag>,
}

impl FeatureFlags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(flags: Vec<String>) -> Result<Self, Vec<String>> {
        let mut feature_flags = Self::new();
        let mut errors = Vec::new();

        for flag in flags {
            match BamlFeatureFlag::from_str(&flag) {
                Ok(f) => {
                    feature_flags.insert(f);
                }
                Err(_) => {
                    errors.push(format!("Unknown feature flag: '{flag}'"));
                }
            }
        }

        if errors.is_empty() {
            Ok(feature_flags)
        } else {
            Err(errors)
        }
    }

    pub fn insert(&mut self, flag: BamlFeatureFlag) {
        self.flags.insert(flag);
    }

    pub fn contains(&self, flag: BamlFeatureFlag) -> bool {
        self.flags.contains(&flag)
    }

    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    /// Check if beta features are enabled (which suppresses experimental warnings)
    pub fn is_beta_enabled(&self) -> bool {
        self.contains(BamlFeatureFlag::Beta)
    }

    /// Check if all warnings should be displayed in CLI
    pub fn should_display_warnings(&self) -> bool {
        self.contains(BamlFeatureFlag::DisplayAllWarnings)
    }
}
