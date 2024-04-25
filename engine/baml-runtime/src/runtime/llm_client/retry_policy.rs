use internal_baml_core::{
    internal_baml_parser_database::RetryPolicyStrategy,
    ir::{RetryPolicy, RetryPolicyWalker},
};

pub struct CallablePolicy<'ir> {
    policy: &'ir RetryPolicy,
    current: std::time::Duration,
    counter: u32,
}

impl CallablePolicy<'_> {
    pub fn new<'ir>(policy: &'ir RetryPolicyWalker) -> CallablePolicy<'ir> {
        CallablePolicy {
            policy: policy.item,
            current: std::time::Duration::from_secs(0),
            counter: 0,
        }
    }
}

impl Iterator for CallablePolicy<'_> {
    type Item = std::time::Duration;

    fn next(&mut self) -> Option<Self::Item> {
        if self.counter > self.policy.elem.max_retries {
            return None;
        }

        let delay = match &self.policy.elem.strategy {
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

        self.current = std::time::Duration::from_millis(delay as u64);
        self.counter += 1;

        Some(self.current)
    }
}
