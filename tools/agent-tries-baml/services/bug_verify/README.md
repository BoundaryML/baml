# bug-verify

Re-checks reported bugs against the latest ready nightly. A singleton
poller (no queue claims, so it never disturbs the board lifecycle):

1. Every `BUG_VERIFY_POLL_SECS` it finds `open`/`confirmed` issues whose
   `verifyBamlVersion` is not the newest ready build.
2. For up to `BUG_VERIFY_BATCH` of them it runs a verification agent with
   that baml pinned on PATH (issue.json + repro as inputs, verdict.json
   out).
3. Still broken: stamps `verifiedAt` / `verifyBamlVersion` / `brokeIn`
   (derived from the first evidence run's baml sha).
4. Fixed (still_broken=false at high confidence only): also stamps
   `fixedIn`, transitions the issue to `closed`, marks it linear-dirty for
   the regular re-sync, flips the Linear card to the merged status label,
   and leaves a comment with the agent's evidence.

The /atb dashboard reads `brokeIn` / `fixedIn` / `verifiedAt` directly
("broke in X / fixed in Y" chips on the feed and issue pages).

## Deploy

The app `bench3-bug-recheck` exists with `SERVICE_TOKEN` staged ("verify"
is blocked by Fly's app-name abuse filter, hence "recheck"). It still
needs the Infisical machine identity (same one every worker uses), which
carries `ATB_SERVICE_TOKEN` / `ATB_CLAUDE_PROXY_TOKEN` / `ATB_LINEAR_TOKEN`:

```sh
fly ssh console -a bench3-baml-dedup -C "sh -c 'env'" \
  | grep -E '^(INFISICAL_CLIENT_ID|INFISICAL_CLIENT_SECRET|CLAUDE_PROXY_TOKEN)=' \
  | fly secrets import -a bench3-bug-recheck --stage
fly deploy -c fly.bug-verify.toml
```

Cost control: `BUG_VERIFY_BATCH` agent runs per cycle (default 6,
claude-sonnet-4-6, max 16 turns each); once every issue is stamped with
the current nightly the cycles are free until the next build lands.
