# Monolith cutover runbook (June 2026)

agent-tries-baml absorbed **baml-changelog2**, **baml-wasm-service**, and
**growth/t-shirts**, and moved from self-hosted Convex to **Convex cloud**.
This documents what changed, what was verified, and the remaining manual
steps with their commands.

## What moved where

| was | now |
|---|---|
| baml-changelog2 (FastAPI + MySQL) | `changelogEntries` Convex table + `services/changelog_worker` (claimable queue) + `GET /entries` on ingress |
| baml-changelog2-cron | `_changelog_poll_loop` in `services/cron` (`CHANGELOG_POLL_ENABLED=1`) |
| changelog Slack bot | the `changelog_edit` route of @bammy (`services/ingress/bammy.py`) |
| growth/t-shirts (Slack Bolt + SQLite) | `promoCodes` Convex table + `promoCodes:claimNext` + the `promo_claim` route of @bammy |
| baml-wasm-service (Caddy) | `GET/PUT /wasm/bridge_wasm.tar.gz` on the api (blob volume) + `scripts/publish_wasm.sh` |
| self-hosted Convex (bench3-convex) | Convex cloud: team `boundary`, project `agent-tries-baml`, prod `https://charming-terrier-498.convex.cloud` |
| — (new) | `POST /feedback` on ingress + `baml feedback` CLI subcommand (`baml_cli/src/feedback_command.rs`). Default: issue + min repro only. Team opt-in `BAML_FEEDBACK_INCLUDE_CONTEXT=1` also attaches the Claude Code session transcript (auto-discovered for the cwd, or `BAML_FEEDBACK_TRANSCRIPT=<path>`) and the project's BAML files — the trophy then renders as a full run (transcript blob + turn log + files). |
| — (new) | worker presence (`workers` table, heartbeats from every Processor) + the agents roster at `/` on the UI |

## Convex cloud notes

- The api talks to cloud over the same HTTP function API; `gateway_from_env`
  drops the admin key for `.convex.cloud` URLs (cloud rejects self-hosted
  keys; our functions are public). **Security follow-up**: public functions
  mean anyone with the deployment URL can call them. To lock down: convert
  table functions to `internalQuery`/`internalMutation` and pass a real
  production deploy key in the gateway.
- Snapshot migrated 2026-06-10 with `_id`s preserved (verified trophy→task
  joins). Archive: `~/Desktop/baml/backups/bench3-convex-snapshot-20260610.zip`.
- Blobs (transcripts, baml binaries, wasm tarball) stay on the bench3-api
  volume; only pointers live in Convex.
- Rollback (until bench3-convex is destroyed): revert `CONVEX_URL` in
  `fly.api.toml` to `http://bench3-convex.flycast:3210` and redeploy the api.

## Data migrations (done, idempotent, re-runnable)

- `scripts/migrate_changelog_mysql.py` — 28/28 entries; MySQL dump at
  `~/Desktop/baml/backups/changelog-mysql-entries-20260610.json`.
- `scripts/migrate_promo_sqlite.py` — 400/400 codes (392 unused, parity
  checked); db copy at `~/Desktop/baml/backups/promo-20260610.db`.
- WASM artifact mirrored byte-identically (sha256-verified) to
  `https://bench3-api.fly.dev/wasm/bridge_wasm.tar.gz`.

## URL contract flips

- Website changelog feed: `next.config.mjs` rewrite →
  `https://bench3-ingress.fly.dev` (needs a Vercel redeploy to take effect;
  old `baml-changelog2.fly.dev` stays up until then).
- Dashboard `CHANGELOG_API` (fly.ui.toml) → bench3-ingress. Deployed.
- WASM: no repo consumes `baml-wasm.fly.dev` — if the Vercel project pulls
  it via project-level env/build settings, update there; keep `baml-wasm`
  alive until one Vercel build verifies.

## Remaining manual steps

1. ~~Top up Anthropic credits~~ — no longer required: the changelog worker
   and the @bammy classifier were switched to claude-proxy (2026-06-10),
   which authenticates via the Claude session on the proxy volume, same as
   bench runs. Nothing in the monolith calls the metered API directly
   anymore. (A failed entry can be requeued any time with
   `curl -X POST https://bench3-ingress.fly.dev/entries -H "Authorization: Bearer $ATB_SERVICE_TOKEN" -d '{"release":"<tag>"}'`.)
2. **Rename the existing "bamlbench" Slack app to "bammy"** and add the
   five missing scopes (see `docs/bammy-slack-manifest.yaml` for the exact
   checklist) — no new app, no token flip, no redeploy. Everything routes
   already; the scopes just light up thread-context reading + display names.
3. **Vercel**: redeploy the website (picks up the changelog rewrite), check
   project env for any `baml-wasm.fly.dev` reference.
4. **Land on canary**: this branch (`dhilan/agent-tries-baml`) carries the
   monolith + the `baml feedback` CLI command.

## Nuke status (2026-06-10)

Destroyed: baml-changelog2-cron, promobot, baml-wasm, bench3-convex (the
self-hosted rollback window is CLOSED — recovery is now the snapshot at
~/Desktop/baml/backups/bench3-convex-snapshot-20260610.zip), and the legacy
v1 apps baml-changelog-{cron,slackbot,ui}.

Still up BY CHOICE: baml-changelog2 + baml-changelog2-mysql — the live
website's /api/changelog-feed edge rewrite still points at them (and serves
stale data). Destroy both immediately after the Vercel redeploy picks up the
next.config.mjs rewrite to bench3-ingress:

```bash
fly apps destroy baml-changelog2 -y
fly apps destroy baml-changelog2-mysql -y
```

Final fleet: bench3-{api, ingress, cron, changelog-worker, claude-proxy,
baml-worker, baml-dedup, baml-redraft, cohort-compare, linear-sync,
baml-builder, ui}.
