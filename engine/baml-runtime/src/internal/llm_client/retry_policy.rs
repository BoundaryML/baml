use internal_baml_core::{
    internal_baml_parser_database::RetryPolicyStrategy, ir::RetryPolicyWalker,
};

#[derive(Clone)]
pub struct CallablePolicy {
    max_retries: u32,
    strategy: RetryPolicyStrategy,
    current: std::time::Duration,
    counter: u32,
}

impl From<RetryPolicyWalker<'_>> for CallablePolicy {
    fn from(policy: RetryPolicyWalker<'_>) -> Self {
        CallablePolicy {
            max_retries: policy.max_retries(),
            strategy: policy.strategy().clone(),
            current: match policy.strategy() {
                RetryPolicyStrategy::ExponentialBackoff(strategy) => {
                    std::time::Duration::from_millis(strategy.delay_ms as u64)
                }
                RetryPolicyStrategy::ConstantDelay(strategy) => {
                    std::time::Duration::from_millis(strategy.delay_ms as u64)
                }
            },
            counter: 0,
        }
    }
}

impl Iterator for CallablePolicy {
    type Item = std::time::Duration;

    fn next(&mut self) -> Option<Self::Item> {
        if self.counter > self.max_retries {
            return None;
        }

        let delay = match &self.strategy {
            RetryPolicyStrategy::ExponentialBackoff(strategy) => {
                let delay = (strategy.multiplier * self.current.as_millis() as f32) as u32;
                if delay > strategy.max_delay_ms {
                    strategy.max_delay_ms
                } else {
                    delay
                }
            }
            RetryPolicyStrategy::ConstantDelay(strategy) => strategy.delay_ms,
        };

        if self.counter == self.max_retries {
            // Last element has no delay
            self.current = std::time::Duration::from_millis(0);
        } else {
            self.current = std::time::Duration::from_millis(delay as u64);
        }
        self.counter += 1;

        Some(self.current)
    }
}
