# BAML Pricing

> BAML is free and open source. Boundary Cloud has Starter ($0 a month), Pro ($250 a month) and Enterprise plans.

Web page: https://boundaryml.com/pricing

## Local BAML

The BAML language and local toolchain are free and open source. BAML runs on your machine and does not call Boundary's servers.

You can install the toolchain, write and run BAML programs, generate SDKs, use the local playground, and integrate BAML into an existing application without purchasing a hosted service.

[Install BAML](https://boundaryml.com/quickstart.md).

## Boundary Cloud

Boundary Cloud is in early access.

| Plan | Price | Data included | Data after that |
| --- | --- | --- | --- |
| Starter | $0 a month | 1 GB a month | $4 per GB |
| Pro | $250 a month | 5 GB a month | $3 per GB |
| Enterprise | Custom | Custom | Custom |

Enterprise includes everything in Pro, plus bring your own cloud: Boundary Cloud runs in your cloud account and your data stays there.

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
- Retries: an upload that's retried is counted once.
- Going over: never blocks ingest or queries.
- Upgrading: takes effect right away, prorated to your billing date.
- Canceling: ends at the close of your billing month.

Boundary Cloud is not required to run BAML locally. [Get early access](https://calendly.com/boundary-founders/boundary-unified-stack-set-up).

## Source and license

The BAML source code and repository license are available on [GitHub](https://github.com/boundaryml/baml).
