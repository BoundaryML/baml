use std::collections::HashSet;

use anyhow::Result;
use baml_types::{EvaluationContext, StringOr};

use crate::ClientSpec;

use super::helpers::{Error, PropertyHandler};

#[derive(Debug, Clone)]
pub struct UnresolvedRoundRobin<Meta> {
    pub strategy: Vec<(either::Either<StringOr, ClientSpec>, Meta)>,
    start_index: Option<i32>,
}

pub struct ResolvedRoundRobin {
    pub strategy: Vec<ClientSpec>,
    pub start_index: Option<i32>,
}

impl<Meta: Clone> UnresolvedRoundRobin<Meta> {
    pub fn without_meta(&self) -> UnresolvedRoundRobin<()> {
        UnresolvedRoundRobin {
            strategy: self.strategy.iter().map(|(s, _)| (s.clone(), ())).collect(),
            start_index: self.start_index,
        }
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
        })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let strategy = properties.ensure_strategy();
        let start_index = properties.ensure_int("start", false).map(|(_, v, _)| v);
        let errors = properties.finalize_empty();

        if !errors.is_empty() {
            return Err(errors);
        }

        let strategy = strategy.expect("strategy is required");

        Ok(Self {
            strategy,
            start_index,
        })
    }
}

impl<Meta> super::StrategyClientProperty<Meta> for UnresolvedRoundRobin<Meta> {
    fn strategy(&self) -> &Vec<(either::Either<StringOr, ClientSpec>, Meta)> {
        &self.strategy
    }
}
