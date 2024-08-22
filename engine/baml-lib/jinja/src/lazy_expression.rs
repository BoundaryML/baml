/// A `LazyExpression` is an expression that may or may not have been fully evaluated.
use std::collections::HashMap;
use crate::BamlValue;
use crate::render_expression;
use anyhow::Result;
use std::str::FromStr;

/// In baml-core we use `LazyExpression`s to represent jinja expressions that may
/// not have been evaluated yet.
#[derive(Debug, Clone, PartialEq)]
pub enum LazyExpression<T> {
    /// The thunk has been evaluated and contains a value.
    Ready(T),
    /// The thunk has not been evaluated.
    Unevaluated(String),
}

impl <T> LazyExpression<T>
where T: FromStr + Clone,
      <T as FromStr>::Err: std::fmt::Debug + std::error::Error + Send + Sync + 'static
{
    /// Evaluate the thunk if it has not been evaluated yet.
    pub fn force(&mut self, ctx: &HashMap<String, BamlValue>) -> Result<()> {
        if let LazyExpression::Unevaluated(expr) = self {
            let value_str = render_expression(expr, ctx)?;
            let value = value_str.parse::<T>()?;
            *self = LazyExpression::Ready(value);
        }
        Ok(())
    }

    /// Get the value of the thunk if it has been evaluated.
    pub fn get(&self) -> Option<&T> {
        match self {
            LazyExpression::Ready(value) => Some(value),
            LazyExpression::Unevaluated(_) => None,
        }
    }

    /// Evaluate the thunk if needed and return its value.
    pub fn evaluate(&mut self, ctx: &HashMap<String, BamlValue>) -> Result<T> {
        match self {
            LazyExpression::Ready(value) => Ok(value.clone()),
            LazyExpression::Unevaluated(_) => {
                self.force(ctx)?;
                let value = self
                    .get()
                    .cloned()
                    .expect("LazyExpression was forced to a concrete value on the line above.");
                Ok(value)
            }
        }
    }
}