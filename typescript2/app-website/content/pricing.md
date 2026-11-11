# BAML Pricing

> BAML is free and open source. Boundary Cloud has Starter at $0 a month and Pro at $250 a month.

Web page: https://boundaryml.com/pricing

## Local BAML

The BAML language and local toolchain are free and open source. BAML runs on your machine and does not call Boundary's servers.

You can install the toolchain, write and run BAML programs, generate SDKs, use the local playground, and integrate BAML into an existing application without purchasing a hosted service.

[Install BAML](https://boundaryml.com/quickstart.md).

## Boundary Cloud

You pay for data and retention. Spans, seats, queries and profiles are free. Boundary Cloud is in early access.

| | Starter | Pro |
| --- | --- | --- |
| Price | $0 a month | $250 a month |
| **Data** | | |
| Included | 1 GB a month, kept 30 days | 5 GB a month, kept 30 days |
| Ingestion past that | $4 per GB | $3 per GB |
| Retention past 30 days | No | $0.10 per GB per 30 days |
| Usage caps | Coming soon | Coming soon |
| **Team and security** | | |
| Sign in with Google | Included | Included |
| SAML and OIDC single sign-on | No | Coming soon |
| SCIM directory sync | No | Coming soon |
| Audit logs | Coming soon | Coming soon |
| **Deployment** | | |
| Hosted by Boundary | Included | Included |
| Bring your own cloud | No | Add-on |

Free on both plans: unlimited spans, profiles, dashboards, members, projects and environments, and SQL queries with `baml query`.

Every Boundary Cloud plan comes with a 60-day money-back guarantee. [Get early access](https://calendly.com/boundary-founders/boundary-unified-stack-set-up).

### How ingestion is billed

You pay for the values your programs record, never for the metadata around them.

Billed: function inputs and outputs, logs, network requests and responses, process and thread context.

Never billed: spans and their timings, profiles, function names, source and types, a value your programs record again, retried uploads, uploads that never arrive.

- Logical size, not storage: each value is counted at its size in your program. Compression, storage layout and overhead never reach your bill.
- Each value once: values are stored by content, so a prompt your programs send a thousand times is stored once and billed once.
- Structure is free: spans, timings, profiles, function names, source and types are never billed, so you never sample to save money.
- Retries count once: a retried upload is counted once, and an upload that never arrives counts as nothing.

## Source and license

The BAML source code and repository license are available on [GitHub](https://github.com/boundaryml/baml).
