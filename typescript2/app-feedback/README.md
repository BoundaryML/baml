# app-feedback

A view over the atb2 feedback pipeline (`tools/atb2`): every issue triaged from
user feedback, and how far the pipeline has taken each one.

**Mock only, for now.** The data is `src/lib/mock-data.ts`; nothing talks to
Supabase or reads `~/.atb2/runs`. The types in `src/lib/types.ts` mirror
`models.baml` (`Issue`, `IssueStatus`) and `handle_issue.baml` (`HandleOutcome`)
so wiring real data in later is a data-source change, not a UI change.

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
