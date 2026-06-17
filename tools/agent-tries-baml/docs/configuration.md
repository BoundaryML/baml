# Configuration

agent-tries-baml reads all configuration from the **environment** (`os.environ` /
`os.getenv`) - no config files parsed in code. Injection is therefore an ops concern:
anything that puts the variables in the environment works (a local `.env`, Infisical,
or your host's secret store).

## Anthropic auth - API key

`claude-proxy` runs real Claude Code agent sessions (`claude -p`) and authenticates with
an **Anthropic API key**. Set `ANTHROPIC_API_KEY` and the proxy injects it for each run.
This is the intended and only supported mode: it gives genuine Claude Code session
behavior (tools, multi-turn, transcripts) billed through the Anthropic API under its
commercial terms - the correct footing for automated, server-side use.

```bash
ANTHROPIC_API_KEY=<your-anthropic-api-key>   # required for claude-proxy
```

## Environment variables

| Variable | Used by | Purpose |
|---|---|---|
| `ANTHROPIC_API_KEY` | claude-proxy | Anthropic API auth for agent runs. |
| `SERVICE_TOKEN` | all services | Bearer token for the internal `api`. |
| `CLAUDE_PROXY_TOKEN` | worker/dedup → proxy | Bearer token for `claude-proxy`. |
| `CONVEX_URL`, `CONVEX_ADMIN_KEY` | api | Convex backend URL + admin key. |
| `GITHUB_TOKEN` | api, baml-builder | GitHub API (resolve/download baml releases); avoids rate limits. |
| `SLACK_SIGNING_SECRET` | ingress | Verify inbound Slack webhook signatures. |
| `ATB_LINEAR_WEBHOOK_SECRET` | ingress | When set, verify the `Linear-Signature` (HMAC-SHA256 hex) on `/linear/webhook` (optional). |
| `SLACK_BOT_TOKEN` | worker, linear-sync, ingress | Post Slack acks / results / fix notices. |
| `ATB_LINEAR_TOKEN` | linear-sync, bug-verify, baml-redraft, ingress | Linear GraphQL API (create/update issue cards, read comments). May be a personal token — the webhook uses a status-state gate, not an actor check. |
| `LINEAR_TEAM_ID` | linear-sync, bug-verify, baml-redraft, ingress | The Linear team issues are created under (status-group label ids are baked into `linear_client`, overridable via `LINEAR_STATUS_*`). |
| `CURSOR_API_KEY` | linear-sync | Launch Cursor cloud-agent fixes; without it fix dispatch is a no-op. |

Every integration degrades gracefully when its variable is absent (Linear/Slack/Cursor
no-op), so partial configuration is fine for local work.

## Injecting config - Infisical

The monorepo standard is [Infisical](https://infisical.com). agent-tries-baml's values live in
a dedicated Infisical project/folder, per environment (`dev` / `prod`).

**Local development** - either keep a gitignored `.env` (simplest), or pull from Infisical:

```bash
infisical run --env=dev -- docker compose up        # inject at runtime
# or materialize a local .env:
infisical export --env=dev > .env
```

**Deployment** - inject from Infisical at deploy time (no runtime dependency):

```bash
infisical run --env=prod -- <your deploy command that reads the env>
```
