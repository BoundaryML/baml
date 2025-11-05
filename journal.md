changing spec



guidelines to add to create_plan.md

- guidance on cargo test, generating new error messages for validation tests, etc
- when relevant and designing phases, always implement basic http, then streaming, then composite clients, in that order


implementation / spec notes:

- how do composite clients and RetryPolicies handle timeouts from children? Do we want to error out completely or treat them as retriable? to be decided/specified
-
