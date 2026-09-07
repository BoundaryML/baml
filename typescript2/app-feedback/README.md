# app-feedback

A view over the atb2 feedback pipeline (`tools/atb2`): every issue triaged from
user feedback, and how far the pipeline has taken each one.

## Data

The pages read the atb2 store in Supabase (schema SQL is in the stacked PR descriptions) through
PostgREST with the anon key, which sees issues, runs and events but never a
reporter's identity. `src/lib/db.ts` is the whole data layer; the view
`issues_with_outcome` gives each issue its latest `handle_issue` run.

```sh
FEEDBACK_SUPABASE_URL=https://igraichzcidsylvzkjlc.supabase.co   # as in Infisical (boundary-tools)
FEEDBACK_SUPABASE_ANON_KEY=...                                     # the anon key, never the service key
```

Without those two variables the pages render `src/lib/mock-data.ts` and say
so under the title. Results are cached for 30 seconds.

The types in `src/lib/types.ts` mirror `models.baml` (`Issue`, `IssueStatus`)
and `handle_issue.baml` (`HandleOutcome`); the BAML side owns the shape.

## Pages

- `/` all issues. Stat tiles, status / subsystem / difficulty filters, search,
  list and board views. Every row carries a pipeline strip: one segment per
  stage (triaged, organized, gauged, design pass, fix pass, gate, PR), colored
  done / running / failed / pending.
- `/issues/[id]` one issue: description, repros, resolution plan, design doc,
  comments, the pipeline timeline, and the last `handle_issue` run (outcome,
  turns, time, gate steps, PR).

## Run

```sh
cd typescript2
pnpm install
pnpm --filter app-feedback dev
```

Built on the same stack and theme tokens as `app-beps` (Next 15, Tailwind v4,
shadcn primitives) so the two read as one family of tools.

## Slack approvals

This website is read-only. Pending issues and proposal pages say **Approve on
Slack**. Set `FEEDBACK_SLACK_URL` to the channel URL, for example
`https://your-workspace.slack.com/archives/C0123456789`. The assigned shepherd
approves an issue by reacting to its announcement; authorized shepherds approve
babysitter fixes by reacting to the exact proposal message.

No GitHub OAuth, session key, or Supabase service-role key is needed by this app.
Private proposals and run transcripts stay in the store; `/proposals/[id]`,
`/runs`, and `/runs/[id]` provide Slack instructions without reading private data.
The existing SQL can remain in place; no schema changes are needed for this UI.
