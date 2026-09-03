# app-feedback

A view over the atb2 feedback pipeline (`tools/atb2`): every issue triaged from
user feedback, and how far the pipeline has taken each one.

## Data

The pages read the atb2 store in Supabase (`tools/atb2/db/schema.sql`) through
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
