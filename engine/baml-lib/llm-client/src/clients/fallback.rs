use std::collections::HashSet;

use anyhow::Result;
use baml_types::{EvaluationContext, StringOr};

use crate::ClientSpec;

use super::helpers::{Error, PropertyHandler};

#[derive(Debug, Clone)]
pub struct UnresolvedFallback<Meta> {
    strategy: Vec<(either::Either<StringOr, ClientSpec>, Meta)>,
}

pub struct ResolvedFallback {
    pub strategy: Vec<ClientSpec>,
}

impl<Meta: Clone> UnresolvedFallback<Meta> {
    pub fn without_meta(&self) -> UnresolvedFallback<()> {
        UnresolvedFallback {
            strategy: self.strategy.iter().map(|(s, _)| (s.clone(), ())).collect(),
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

    pub fn resolve(&self, ctx: &EvaluationContext<'_>) -> Result<ResolvedFallback> {
        let strategy = self
            .strategy
            .iter()
            .map(|(s, _)| match s {
                either::Either::Left(s) => ClientSpec::new_from_id(s.resolve(ctx)?.as_str()),
                either::Either::Right(s) => Ok(s.clone()),
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(ResolvedFallback { strategy })
    }

    pub fn create_from(mut properties: PropertyHandler<Meta>) -> Result<Self, Vec<Error<Meta>>> {
        let strategy = properties.ensure_strategy();
        let errors = properties.finalize_empty();

        if !errors.is_empty() {
            return Err(errors);
        }

        let strategy = strategy.expect("strategy is required");

        Ok(Self { strategy })
    }
}

impl<Meta> super::StrategyClientProperty<Meta> for UnresolvedFallback<Meta> {
    fn strategy(&self) -> &Vec<(either::Either<StringOr, ClientSpec>, Meta)> {
        &self.strategy
    }
}
