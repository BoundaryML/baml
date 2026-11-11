# BAML Pricing

> BAML is free and open source. Boundary Cloud has Starter at $0 a month and Pro at $250 a month.

Web page: https://boundaryml.com/pricing

## Local BAML

The BAML language and local toolchain are free and open source. BAML runs on your machine and does not call Boundary's servers.

You can install the toolchain, write and run BAML programs, generate SDKs, use the local playground, and integrate BAML into an existing application without purchasing a hosted service.

[Install BAML](https://boundaryml.com/quickstart.md).

## Boundary Cloud

Boundary Cloud is in early access.

| Plan | Price | Data included | Data after that | Retention |
| --- | --- | --- | --- | --- |
| Starter | $0 a month | 1 GB a month | $4 per GB | 14 days |
| Pro | $250 a month | 5 GB a month | $3 per GB, with volume discounts | 30 days, or custom |

Pro can add bring your own cloud: Boundary Cloud runs in your cloud account and your data stays there.

### Included in every Cloud plan

- Unlimited spans
- Profiles
- SQL queries and `baml query`
- Dashboards
- Unlimited members
- Unlimited projects and environments
- API keys

### How billing works

- What counts: the values your programs record (inputs, outputs, logs and network payloads).
- What doesn't: spans, profiles, queries and seats.
- Repeats: a value your programs record again is billed once.
- Retries: an upload that's retried is counted once.
- Going over: never blocks ingest or queries.
- Upgrading: takes effect right away, prorated to your billing date.
- Canceling: ends at the close of your billing month.

Boundary Cloud is not required to run BAML locally. [Get early access](https://calendly.com/boundary-founders/boundary-unified-stack-set-up).

## Source and license

The BAML source code and repository license are available on [GitHub](https://github.com/boundaryml/baml).
