# Product metrics

Renders four weeks of PLG CLI charts and BAML Early Access online join rates, a point-in-time approximate Discord community count, and an exact Sheep Council role-holder count alongside the embedded PostHog dashboard, then posts the metrics with a PNG chart attachment to `#sam-sandbox` every Monday at 9:00 AM America/Los_Angeles.

## Local development

From `typescript2/`, install dependencies and start the service with the Boundary bot token from Infisical:

```bash
pnpm install
SLACK_CHANNEL_ID=C07UTQN7N1X infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- pnpm --filter app-product-metrics dev
```

The dashboard is at `http://localhost:3000` and the health check is at `/healthz`. Dashboard results are cached in-process for 15 minutes. The process must remain running for its in-process weekly schedule, so Fly is configured with one always-on machine.

## Database

Prisma uses the PlanetScale Postgres database configured by `POSTGRES_OPSBOT_HOST`, `POSTGRES_OPSBOT_PORT`, `POSTGRES_OPSBOT_DATABASE`, `POSTGRES_OPSBOT_USERNAME`, and `POSTGRES_OPSBOT_PASSWORD`. The connection URL is assembled at runtime and always includes `sslmode=verify-full`; SSL mode is not configurable through Infisical.

Run read-only Prisma commands through Infisical so the split runtime connection settings are available. Schema migrations must temporarily override the runtime username and password with the DDL-only credentials; the DDL credentials are not synced to Fly:

```bash
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- pnpm --filter app-product-metrics prisma:validate
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-humans -- sh -c 'export POSTGRES_OPSBOT_USERNAME="$POSTGRES_OPSBOT_DDL_USERNAME" POSTGRES_OPSBOT_PASSWORD="$POSTGRES_OPSBOT_DDL_PASSWORD"; pnpm --filter app-product-metrics prisma:migrate:deploy'
```

The Prisma schema contains a canonical `plg_weekly_metrics` table and a `plg_weekly_metrics_raw` table for source payloads. The app records one mutable aggregate per week; calling the snapshot again in the same week replaces that week's aggregate. `week_start_date` is the current Monday at midnight in `America/Los_Angeles`, and `recorded_at` is the actual collection time. Total Discord and Sheep Council Discord counts are nullable because Discord provides no historical membership snapshots. Sheep Council Zoom attendance count and active count are also nullable when their source data is unavailable.

## External data sync

At 5:00 AM every day in `America/Los_Angeles`, the app runs two independent raw-data jobs for the current Monday-to-Monday window. `fetch-sheep-council-zoom.ts` discovers occurrences of the configured recurring Sheep Council meeting and captures both Zoom past-participant and report-participant pages. `fetch-luma-eap.ts` discovers Zoom-based Luma events whose name exactly matches `LUMA_EVENT_NAME`, captures their approved guest pages, resolves each event's numeric Zoom meeting ID to the closest occurrence within 24 hours, and captures both Zoom participant representations when an occurrence is available. Upcoming or otherwise unavailable occurrences are recorded with `resolution: "not-found"` and retried by the next daily run.

The daily invocation inserts one `SHEEP_COUNCIL_MEETINGS` row and one `EAP_MEETINGS` row in `plg_weekly_metrics_raw` with the same `recorded_at`. The JSON payloads contain participant personal data and must remain access-controlled. Password-bearing Luma Zoom join URLs and OAuth credentials are not persisted.

`POST /sync-external-data` triggers one job without authentication. It requires `Content-Type: application/json` and an exact request body containing `raw_metric_type` and a seven-day Monday-to-Monday `raw_metric_period`. Use `eap-meetings` instead of `sheep-council-meetings` to trigger the EAP job.

```bash
curl --fail-with-body --request POST --header 'Content-Type: application/json' --data '{"raw_metric_type":"sheep-council-meetings","raw_metric_period":{"start":"2026-08-24","end":"2026-08-31"}}' https://boundary-product-metrics.fly.dev/sync-external-data
```

Trigger today's snapshot on demand with the same bearer token used by the Slack endpoint:

```bash
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- sh -c 'curl --fail-with-body --request POST --header "Authorization: Bearer $SLACK_POST_TRIGGER_TOKEN" https://boundary-product-metrics.fly.dev/snapshot'
```

The always-on process aggregates the current week every day at 6:00 AM in `America/Los_Angeles`, after the 5:00 AM raw-data sync. Discord totals and Sheep Council membership come from the bot-authenticated guild member list, Luma signup and Zoom attendance counts come from the latest raw EAP capture, Sheep Council Zoom attendance comes from the latest raw Sheep Council capture, and distinct GitHub issue authors come from the PostHog `github_boundaryml_baml__issues` warehouse table. The aggregate is upserted by `week_start_date`, so reruns refresh the current row without erasing Zoom attendance.

## Weekly aggregation

`POST /aggregate-weekly-metric` accepts an exact Monday-to-Monday JSON body such as `{"start":"2026-08-17","end":"2026-08-24"}`. It reads the latest Sheep Council and EAP raw row for the selected week, counts external Zoom attendees whose combined duration for one occurrence is at least 600 seconds, counts approved Luma guests, queries distinct GitHub issue authors from the PostHog `github_boundaryml_baml__issues` warehouse table, and fetches the current total and Sheep Council Discord membership counts. It inserts into `plg_weekly_metrics` only after every source operation and payload validation succeeds; `sheep_council_active_count` remains null.

```bash
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- sh -c 'curl --fail-with-body --request POST --header "Authorization: Bearer $SLACK_POST_TRIGGER_TOKEN" --header "Content-Type: application/json" --data '\''{"start":"2026-08-17","end":"2026-08-24"}'\'' https://boundary-product-metrics.fly.dev/aggregate-weekly-metric'
```

The `Product metrics Slack report` GitHub Actions workflow runs on `ubuntu-latest` every Monday at 8:00 AM in `America/Los_Angeles`. It installs Playwright's matching Chromium build, screenshots the live `GET /` dashboard after the primary PostHog chart renders, and uploads the PNG to `#sam-sandbox` using `SLACK_BOUNDARY_BOT_TOKEN` from the `boundary-tools-prod` GitHub environment. Fly serves the dashboard but does not run Chromium. The workflow can also be triggered manually after it is present on the repository's default branch.

```bash
gh workflow run product-metrics-slack-report.yml
```

Trigger the configured Slack post on demand with the bearer token from Infisical:

```bash
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- sh -c 'curl --fail-with-body --request POST --header "Authorization: Bearer $SLACK_POST_TRIGGER_TOKEN" https://boundary-product-metrics.fly.dev/post'
```

## Deploy

The Docker build context must be `typescript2/` so the workspace lockfile is available:

```bash
infisical run --projectId=bdd280e2-259c-4750-9b16-a8597a67214c --env=prod-product-metrics -- sh -c 'flyctl secrets set --app boundary-product-metrics SLACK_BOUNDARY_BOT_TOKEN="$SLACK_BOUNDARY_BOT_TOKEN" SLACK_POST_TRIGGER_TOKEN="$SLACK_POST_TRIGGER_TOKEN" POSTHOG_BOUNDARY_PRODUCT_METRICS_PERSONAL_API_KEY="$POSTHOG_BOUNDARY_PRODUCT_METRICS_PERSONAL_API_KEY" POSTHOG_BOUNDARY_PRODUCT_METRICS_PROJECT_ID="$POSTHOG_BOUNDARY_PRODUCT_METRICS_PROJECT_ID" DISCORD_OPSBOT_BOT_TOKEN="$DISCORD_OPSBOT_BOT_TOKEN" DISCORD_OPSBOT_GUILD_ID="$DISCORD_OPSBOT_GUILD_ID" DISCORD_OPSBOT_SHEEP_COUNCIL_ROLE_ID="$DISCORD_OPSBOT_SHEEP_COUNCIL_ROLE_ID" LUMA_API_KEY="$LUMA_API_KEY" ZOOM_OPSBOT_ACCOUNT_ID="$ZOOM_OPSBOT_ACCOUNT_ID" ZOOM_OPSBOT_CLIENT_ID="$ZOOM_OPSBOT_CLIENT_ID" ZOOM_OPSBOT_CLIENT_SECRET="$ZOOM_OPSBOT_CLIENT_SECRET" POSTGRES_OPSBOT_HOST="$POSTGRES_OPSBOT_HOST" POSTGRES_OPSBOT_PORT="$POSTGRES_OPSBOT_PORT" POSTGRES_OPSBOT_DATABASE="$POSTGRES_OPSBOT_DATABASE" POSTGRES_OPSBOT_USERNAME="$POSTGRES_OPSBOT_USERNAME" POSTGRES_OPSBOT_PASSWORD="$POSTGRES_OPSBOT_PASSWORD"'
flyctl deploy . --config app-product-metrics/fly.toml --remote-only
flyctl scale count 1 --app boundary-product-metrics --yes
```

The Infisical `boundary-tools` project stores this app's secrets under the `prod-product-metrics` environment. Fly receives the Slack, PostHog, Discord bot, Luma, Zoom Server-to-Server OAuth, and `POSTGRES_OPSBOT_*` runtime secrets; the Discord OAuth client ID and secret stay in Infisical and are not needed at runtime. The GitHub workflow reads `SLACK_BOUNDARY_BOT_TOKEN` from the protected `boundary-tools-prod` environment. The Luma API key must belong to the Boundary calendar. The Zoom OAuth app must be able to read past meeting instances, report participants, and past-meeting participants. The public Discord invite supplies the approximate community count shown on the dashboard, while the bot paginates guild members to obtain the exact total and count holders of the configured Sheep Council role for daily snapshots; the bot must be installed in the guild with Server Members Intent enabled. The Slack channel ID, schedules, timezone, Discord invite code, expected guild name, GitHub repository, Luma event name, and Sheep Council Zoom meeting ID are non-secret values in `fly.toml` or the GitHub workflow. The Slack bot must be a member of `#sam-sandbox` and needs `chat:write` and `files:write`. Chart delivery uses `files.getUploadURLExternal` followed by the returned upload URL and `files.completeUploadExternal`; it does not use webhooks or the retired `files.upload` method. The PostHog personal API key needs Query Read permission.

## Weekly metric definitions

- The dashboard and Slack report contain the four most recently completed Monday-to-Monday weeks in `America/Los_Angeles`; each retention comparison uses the immediately preceding week, so the queries cover five weeks of cohort membership.
- CLI activity is the `cli_invocation` event with `environment=production` and `robot=0`.
- Existing users appear in both the reporting week and prior week. New users appear in the reporting week but not the prior week. Retention is existing users divided by prior-week users.
- Each reporting-week user is assigned to the major/minor release from their latest CLI invocation in the week; an `unknown` bucket preserves users with a missing or malformed version.
- Discord community size is Discord's approximate member count for the `Baml (by Boundary)` guild, fetched through the configured public invite when the report snapshot is refreshed. Sheep Council size is an exact count of current guild members holding the configured role, fetched through the bot-authenticated guild member API. Both are point-in-time metrics, not four-week series.
- BAML Early Access online join rate is approved Luma guests with a non-null `joined_at` divided by approved guests, aggregated across every matching Zoom event in the reporting week. It measures use of Luma's guest-specific online join link, not duration in the Zoom meeting; a person registered for two sessions contributes once to each session.
- Panic and segfault counts are reported as unavailable because the CLI emits invocation-start telemetry but no completion or crash event. Reporting zero would be incorrect.

For each displayed week the app runs one bounded summary query and one bounded release query. The eight independent PostHog Query API calls run concurrently, and the resulting weeks stay in chronological order for chart and Slack rendering.
