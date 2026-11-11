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
| Members | Unlimited | Unlimited |
| Projects and environments | Unlimited | Unlimited |
| SSO with Google | Coming soon | Coming soon |
| SAML and OIDC single sign-on | No | Coming soon |
| SCIM directory sync | No | Coming soon |
| Audit logs | No | Add-on, coming soon |
| **Deployment** | | |
| Hosted by Boundary | Included | Included |
| Bring your own cloud | No | Add-on |

Free on both plans: unlimited spans, profiles, dashboards, members, projects and environments, and unlimited queries (SQL or `baml query`), up to 90 seconds each.

Every Boundary Cloud plan comes with a 60-day money-back guarantee. [Get early access](https://calendly.com/boundary-founders/boundary-unified-stack-set-up).

### How ingestion is billed

Billed: function inputs and outputs, logs, network requests and responses, process and thread context.

Never billed: spans and their timings, profiles, function names, source and types and a value your programs record again.

A lot of our engineering goes into shrinking what you send over the wire. A few examples of many:

- What you pay for: the content of your values, like `4812`, not the field name `ticket_id`. A class's schema is stored once and never billed, and so is each call's timing and identity.
- Images: images are fingerprinted by their content, so the same logo on every call is stored and billed once.
- Repeated values: every value is fingerprinted by its content, so a prompt sent again matches what's stored and isn't billed again.

## Source and license

The BAML source code and repository license are available on [GitHub](https://github.com/boundaryml/baml).
