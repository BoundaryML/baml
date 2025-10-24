use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LLMProvider {
    #[serde(rename = "openai")]
    OpenAI,
    Anthropic,
    Azure,
    #[serde(rename = "openai-generic")]
    OpenAIGeneric,
    Google,
    Vertex,
    AWS,
    Ollama,
    Groq,
}

impl LLMProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            LLMProvider::OpenAI => "openai",
            LLMProvider::Anthropic => "anthropic",
            LLMProvider::Azure => "azure",
            LLMProvider::OpenAIGeneric => "openai-generic",
            LLMProvider::Google => "google",
            LLMProvider::Vertex => "vertex",
            LLMProvider::AWS => "aws",
            LLMProvider::Ollama => "ollama",
            LLMProvider::Groq => "groq",
        }
    }

    pub fn try_from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Some(LLMProvider::OpenAI),
            "anthropic" => Some(LLMProvider::Anthropic),
            "azure" => Some(LLMProvider::Azure),
            "openai-generic" => Some(LLMProvider::OpenAIGeneric),
            "google" => Some(LLMProvider::Google),
            "vertex" => Some(LLMProvider::Vertex),
            "aws-bedrock" => Some(LLMProvider::AWS),
            "ollama" => Some(LLMProvider::Ollama),
            "groq" => Some(LLMProvider::Groq),
            _ => None,
        }
    }

    /// Returns true if this provider uses OpenAI-compatible response format
    pub fn is_openai_compatible(&self) -> bool {
        matches!(
            self,
            LLMProvider::OpenAI
                | LLMProvider::Azure
                | LLMProvider::OpenAIGeneric
                | LLMProvider::Ollama
                | LLMProvider::Groq
        )
    }

    /// Returns true if this provider uses Anthropic response format
    pub fn is_anthropic_compatible(&self) -> bool {
        matches!(self, LLMProvider::Anthropic | LLMProvider::AWS)
    }

    /// Returns true if this provider uses Google/Vertex response format
    pub fn is_google_compatible(&self) -> bool {
        matches!(self, LLMProvider::Google | LLMProvider::Vertex)
    }
}

impl std::fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_string_conversion() {
        assert_eq!(LLMProvider::OpenAI.as_str(), "openai");
        assert_eq!(LLMProvider::from_str("openai"), Some(LLMProvider::OpenAI));
        assert_eq!(LLMProvider::from_str("OPENAI"), Some(LLMProvider::OpenAI));
        assert_eq!(LLMProvider::from_str("unknown"), None);
    }

    #[test]
    fn test_provider_compatibility() {
        assert!(LLMProvider::OpenAI.is_openai_compatible());
        assert!(LLMProvider::Azure.is_openai_compatible());
        assert!(LLMProvider::Ollama.is_openai_compatible());
        assert!(!LLMProvider::Anthropic.is_openai_compatible());

        assert!(LLMProvider::Anthropic.is_anthropic_compatible());
        assert!(LLMProvider::AWS.is_anthropic_compatible());
        assert!(!LLMProvider::OpenAI.is_anthropic_compatible());

        assert!(LLMProvider::Google.is_google_compatible());
        assert!(LLMProvider::Vertex.is_google_compatible());
        assert!(!LLMProvider::OpenAI.is_google_compatible());
    }

    #[test]
    fn test_serde_serialization() {
        let provider = LLMProvider::OpenAIGeneric;
        let serialized = serde_json::to_string(&provider).unwrap();
        assert_eq!(serialized, r#""openai-generic""#);

        let deserialized: LLMProvider = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, LLMProvider::OpenAIGeneric);
    }
}
