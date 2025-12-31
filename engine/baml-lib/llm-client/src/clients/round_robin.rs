use std::collections::HashSet;

use anyhow::Result;
use baml_derive::BamlHash;
use baml_types::{EvaluationContext, StringOr};

use super::helpers::{Error, HttpConfig, PropertyHandler};
use crate::ClientSpec;

#[derive(Debug, Clone, BamlHash)]
pub struct UnresolvedRoundRobin<Meta> {
    pub strategy: Vec<(either::Either<StringOr, ClientSpec>, Meta)>,
    start_index: Option<i32>,
    http_config: HttpConfig,
}

pub struct ResolvedRoundRobin {
    pub strategy: Vec<ClientSpec>,
    pub start_index: Option<i32>,
    pub http_config: HttpConfig,
}

impl<Meta: Clone> UnresolvedRoundRobin<Meta> {
    pub fn without_meta(&self) -> UnresolvedRoundRobin<()> {
        UnresolvedRoundRobin {
            strategy: self.strategy.iter().map(|(s, _)| (s.clone(), ())).collect(),
            start_index: self.start_index,
            http_config: self.http_config.clone(),
        }
    }

    pub fn dependencies(&self) -> HashSet<String> {
        self.strategy
            .iter()
            .flat_map(|(s, _)| match s {
                either::Either::Left(s) => match s {
                    StringOr::Value(s) => HashSet::from([s.clone()]),
                    StringOr::EnvVar(_)
                    | StringOr::JinjaExpression(_)
                    | StringOr::TemplateStringCall { .. } => Default::default(),
                },
                either::Either::Right(s) => s.dependencies(),
            })
            .collect()
    }
    pub fn required_env_vars(&self) -> HashSet<String> {
        self.strategy
            .iter()
            .flat_map(|(s, _)| match s {
                either::Either::Left(s) => s.required_env_vars(),
                either::Either::Right(_) => Default::default(),
            })
            .collect()
    }

    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<ResolvedRoundRobin> {
        let strategy = self
            .strategy
            .iter()
            .map(|(s, _)| match s {
                either::Either::Left(s) => ClientSpec::new_from_id(s.resolve(ctx)?.as_str()),
                either::Either::Right(s) => Ok(s.clone()),
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ResolvedRoundRobin {
            strategy,
            start_index: self.start_index,
            http_config: self.http_config.clone(),
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let strategy = properties.ensure_strategy();
        let start_index = properties.ensure_int("start", false).map(|(_, v, _)| v);
        let http_config = properties.ensure_http_config("round-robin");
        let errors = properties.finalize_empty();

        if !errors.is_empty() {
            return Err(errors);
        }

        let strategy = strategy.expect("strategy is required");

        Ok(Self {
            strategy,
            start_index,
            http_config,
        })
    }
}

impl<Meta> super::StrategyClientProperty<Meta> for UnresolvedRoundRobin<Meta> {
    fn strategy(&self) -> &Vec<(either::Either<StringOr, ClientSpec>, Meta)> {
        &self.strategy
    }
}
