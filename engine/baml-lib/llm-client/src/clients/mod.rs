use std::collections::HashSet;

use baml_types::{EvaluationContext, StringOr};
pub use helpers::{HttpConfig, PropertyHandler};

use crate::ClientSpec;

mod helpers;

pub mod anthropic;
pub mod aws_bedrock;
pub mod fallback;
pub mod google_ai;
pub mod openai;
pub mod round_robin;
pub mod vertex;

#[derive(Debug)]
/// `Meta` is a generic carrying span information, so that if it comes from a .baml file,
/// we can trace it back to the original location in said .baml file. In dynamic clients,
/// though, we can't do that, so we just pass in `()`.
#[derive(Clone, Hash)]
pub enum UnresolvedClientProperty<Meta> {
    OpenAI(openai::UnresolvedOpenAI<Meta>),
    Anthropic(anthropic::UnresolvedAnthropic<Meta>),
    AWSBedrock(aws_bedrock::UnresolvedAwsBedrock<Meta>),
    Vertex(vertex::UnresolvedVertex<Meta>),
    GoogleAI(google_ai::UnresolvedGoogleAI<Meta>),
    RoundRobin(round_robin::UnresolvedRoundRobin<Meta>),
    Fallback(fallback::UnresolvedFallback<Meta>),
}

pub enum ResolvedClientProperty {
    OpenAI(openai::ResolvedOpenAI),
    Anthropic(anthropic::ResolvedAnthropic),
    AWSBedrock(aws_bedrock::ResolvedAwsBedrock),
    Vertex(vertex::ResolvedVertex),
    GoogleAI(google_ai::ResolvedGoogleAI),
    RoundRobin(round_robin::ResolvedRoundRobin),
    Fallback(fallback::ResolvedFallback),
}

impl ResolvedClientProperty {
    pub fn name(&self) -> &str {
        match self {
            ResolvedClientProperty::RoundRobin(_) => "round-robin",
            ResolvedClientProperty::Fallback(_) => "fallback",
            ResolvedClientProperty::OpenAI(_) => "openai",
            ResolvedClientProperty::Anthropic(_) => "anthropic",
            ResolvedClientProperty::AWSBedrock(_) => "aws-bedrock",
            ResolvedClientProperty::Vertex(_) => "vertex",
            ResolvedClientProperty::GoogleAI(_) => "google-ai",
        }
    }
}

impl<Meta: Clone> UnresolvedClientProperty<Meta> {
    pub fn dependencies(&self) -> HashSet<String> {
        match self {
            UnresolvedClientProperty::OpenAI(_)
            | UnresolvedClientProperty::Anthropic(_)
            | UnresolvedClientProperty::AWSBedrock(_)
            | UnresolvedClientProperty::Vertex(_)
            | UnresolvedClientProperty::GoogleAI(_) => Default::default(),
            UnresolvedClientProperty::RoundRobin(r) => r.dependencies(),
            UnresolvedClientProperty::Fallback(f) => f.dependencies(),
        }
    }

    pub fn required_env_vars(&self) -> HashSet<String> {
        match self {
            UnresolvedClientProperty::OpenAI(o) => o.required_env_vars(),
            UnresolvedClientProperty::Anthropic(a) => a.required_env_vars(),
            UnresolvedClientProperty::AWSBedrock(a) => a.required_env_vars(),
            UnresolvedClientProperty::Vertex(v) => v.required_env_vars(),
            UnresolvedClientProperty::GoogleAI(g) => g.required_env_vars(),
            UnresolvedClientProperty::RoundRobin(r) => r.required_env_vars(),
            UnresolvedClientProperty::Fallback(f) => f.required_env_vars(),
        }
    }

    pub fn resolve(
        &self,
        provider: &crate::ClientProvider,
        ctx: &EvaluationContext<'_>,
    ) -> anyhow::Result<ResolvedClientProperty> {
        match self {
            UnresolvedClientProperty::OpenAI(o) => {
                o.resolve(provider, ctx).map(ResolvedClientProperty::OpenAI)
            }
            UnresolvedClientProperty::Anthropic(a) => {
                a.resolve(ctx).map(ResolvedClientProperty::Anthropic)
            }
            UnresolvedClientProperty::AWSBedrock(a) => {
                a.resolve(ctx).map(ResolvedClientProperty::AWSBedrock)
            }
            UnresolvedClientProperty::Vertex(v) => {
                v.resolve(ctx).map(ResolvedClientProperty::Vertex)
            }
            UnresolvedClientProperty::GoogleAI(g) => {
                g.resolve(ctx).map(ResolvedClientProperty::GoogleAI)
            }
            UnresolvedClientProperty::RoundRobin(r) => {
                r.resolve(ctx).map(ResolvedClientProperty::RoundRobin)
            }
            UnresolvedClientProperty::Fallback(f) => {
                f.resolve(ctx).map(ResolvedClientProperty::Fallback)
            }
        }
    }

    pub fn without_meta(&self) -> UnresolvedClientProperty<()> {
        match self {
            UnresolvedClientProperty::OpenAI(o) => {
                UnresolvedClientProperty::OpenAI(o.without_meta())
            }
            UnresolvedClientProperty::Anthropic(a) => {
                UnresolvedClientProperty::Anthropic(a.without_meta())
            }
            UnresolvedClientProperty::AWSBedrock(a) => {
                UnresolvedClientProperty::AWSBedrock(a.without_meta())
            }
            UnresolvedClientProperty::Vertex(v) => {
                UnresolvedClientProperty::Vertex(v.without_meta())
            }
            UnresolvedClientProperty::GoogleAI(g) => {
                UnresolvedClientProperty::GoogleAI(g.without_meta())
            }
            UnresolvedClientProperty::RoundRobin(r) => {
                UnresolvedClientProperty::RoundRobin(r.without_meta())
            }
            UnresolvedClientProperty::Fallback(f) => {
                UnresolvedClientProperty::Fallback(f.without_meta())
            }
        }
    }
}

impl crate::ClientProvider {
    /// This codepath is common to both static clients (defined in a .baml file) and dynamic clients
    /// (defined in user's python/ts code).
    pub fn parse_client_property<Meta: Clone>(
        &self,
        properties: PropertyHandler<Meta>,
    ) -> Result<UnresolvedClientProperty<Meta>, Vec<helpers::Error<Meta>>> {
        Ok(match self {
            crate::ClientProvider::OpenAI(open_aiclient_provider_variant) => {
                UnresolvedClientProperty::OpenAI(
                    open_aiclient_provider_variant.create_from(properties)?,
                )
            }
            crate::ClientProvider::Anthropic => UnresolvedClientProperty::Anthropic(
                anthropic::UnresolvedAnthropic::create_from(properties)?,
            ),
            crate::ClientProvider::AwsBedrock => UnresolvedClientProperty::AWSBedrock(
                aws_bedrock::UnresolvedAwsBedrock::create_from(properties)?,
            ),
            crate::ClientProvider::GoogleAi => UnresolvedClientProperty::GoogleAI(
                google_ai::UnresolvedGoogleAI::create_from(properties)?,
            ),
            crate::ClientProvider::Vertex => {
                UnresolvedClientProperty::Vertex(vertex::UnresolvedVertex::create_from(properties)?)
            }
            crate::ClientProvider::Strategy(s) => s.create_from(properties)?,
        })
    }
}

impl crate::OpenAIClientProviderVariant {
    fn create_from<Meta: Clone>(
        &self,
        properties: PropertyHandler<Meta>,
    ) -> Result<openai::UnresolvedOpenAI<Meta>, Vec<helpers::Error<Meta>>> {
        match self {
            crate::OpenAIClientProviderVariant::Base => {
                openai::UnresolvedOpenAI::create_standard(properties)
            }
            crate::OpenAIClientProviderVariant::Ollama => {
                openai::UnresolvedOpenAI::create_ollama(properties)
            }
            crate::OpenAIClientProviderVariant::Azure => {
                openai::UnresolvedOpenAI::create_azure(properties)
            }
            crate::OpenAIClientProviderVariant::Generic => {
                openai::UnresolvedOpenAI::create_generic(properties)
            }
            crate::OpenAIClientProviderVariant::Responses => {
                openai::UnresolvedOpenAI::create_responses(properties)
            }
            crate::OpenAIClientProviderVariant::OpenRouter => {
                openai::UnresolvedOpenAI::create_openrouter(properties)
            }
        }
    }
}

impl crate::StrategyClientProvider {
    fn create_from<Meta: Clone>(
        &self,
        properties: PropertyHandler<Meta>,
    ) -> Result<UnresolvedClientProperty<Meta>, Vec<helpers::Error<Meta>>> {
        match self {
            crate::StrategyClientProvider::Fallback => Ok(UnresolvedClientProperty::Fallback(
                fallback::UnresolvedFallback::create_from(properties)?,
            )),
            crate::StrategyClientProvider::RoundRobin => Ok(UnresolvedClientProperty::RoundRobin(
                round_robin::UnresolvedRoundRobin::create_from(properties)?,
            )),
        }
    }
}

pub trait StrategyClientProperty<Meta> {
    fn strategy(&self) -> &Vec<(either::Either<StringOr, ClientSpec>, Meta)>;
}
