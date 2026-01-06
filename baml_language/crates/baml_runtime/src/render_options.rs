//! Render options for controlling output formatting.

/// Controls rendering behavior for curl and request building.
#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// Whether to show actual API keys or mask them.
    pub expose_secrets: bool,
    /// Whether to expand media URLs to base64 inline data.
    pub expand_media: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            expose_secrets: false,
            expand_media: false,
        }
    }
}

impl RenderOptions {
    /// Create options that expose secrets (for actual API calls).
    pub fn for_execution() -> Self {
        Self {
            expose_secrets: true,
            expand_media: true,
        }
    }

    /// Create options for display (masks secrets, shows URLs).
    pub fn for_display() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_masks_secrets() {
        let opts = RenderOptions::default();
        assert!(!opts.expose_secrets);
        assert!(!opts.expand_media);
    }

    #[test]
    fn test_for_execution_exposes_all() {
        let opts = RenderOptions::for_execution();
        assert!(opts.expose_secrets);
        assert!(opts.expand_media);
    }
}
