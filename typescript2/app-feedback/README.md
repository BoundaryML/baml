# Feedback website

The issue and report interface for MiniATB, with views for historical ATB2 records.

- `/`: your issues or all confirmed feedback requests, with filters and a board.
- `/issues/[id]`: short description, investigation with source citations, native
  repros, PromptFiddle links, feedback side panel, comments and decision timeline.
- `/feedback`: original reports and their ticket or recorded no-issue reason.
- `/agents`: authenticated live transcripts with terminal-style tool output.
- `/runs`, `/prs`, `/proposals`: existing ATB2 run and PR records. MiniATB does not
  yet implement every producer for these historical pages.

## Local development

```sh
cd typescript2
pnpm install
pnpm --filter app-feedback dev
```

Copy `.env.example` to a local environment file and fill in server-side settings.
The Supabase **anon** key reads existing public views. Do not give this app the
Supabase service-role key. GitHub OAuth verifies BoundaryML organization membership
before issuing a signed, expiring session cookie. Register the callback at
`<ATB2_UI_URL>/api/auth/github/callback`.

Comments, Linear export and transcripts use authenticated, HMAC-signed requests to
`ATB2_RUNNER_URL`, defaulting to `https://atb2-runner.fly.dev`. The shared
`ATB2_UI_RUNNER_SECRET` must match the runner and contain at least 32 characters.
`ATB2_UI_SESSION_SECRET` is a separate key of at least 32 characters. Mutations
check the same-origin request and the runner scopes access to its configured
dataset. There is no website approval gate.

## Checks

```sh
bun test ./tests
bun run typecheck
bun run build
```

See `tools/miniatb/README.md` for the implemented pipeline and remaining parity gaps.
