# Boundary Developer Documentation

This package builds the documentation portal for `developer.boundaryml.com`.
It is a Next.js application using Fumadocs and MDX.

## Local development

From the monorepo root:

```bash
pnpm install
pnpm --filter @baml/developer-docs dev
```

Validate a change with:

```bash
pnpm --filter @baml/developer-docs test
pnpm --filter @baml/developer-docs typecheck
pnpm --filter @baml/developer-docs build
```

## Content model

- People author explanations, tutorials, examples, and the BAML book in MDX.
- Generators own implementation facts such as language signatures, standard-library declarations, and CLI flags.
- The BAML grammar comes directly from `typescript2/pkg-grammar/baml.tmLanguage.json` so highlighting changes with the language.
- Pull-request deployments must remain `noindex`; production is indexable.

The route contract is checked by `tests/routes.test.mjs`.
