-- Intuitions: cross-issue findings written by run_intuition (intuition.baml).
-- Apply in the Supabase dashboard (project igraichzcidsylvzkjlc), like the
-- other live tables. The anon role reads them; only the runner writes.

create table if not exists intuitions (
  id                text primary key,
  dataset           text not null default 'live' check (dataset in ('live', 'eval')),
  title             text not null,
  kind              text not null check (kind in ('Pattern', 'SharedCause', 'Hotspot', 'Process')),
  insight           text not null,
  evidence          text not null,
  issue_ids         jsonb not null default '[]'::jsonb,
  subsystem         text not null,
  confidence        text not null check (confidence in ('low', 'medium', 'high')),
  suggested_action  text not null,
  -- the current set; a run retires the previous one instead of deleting it
  active            boolean not null default true,
  generated_at      timestamptz not null default now(),
  created_at        timestamptz not null default now()
);

create index if not exists intuitions_active_idx on intuitions (dataset, active, generated_at desc);
create index if not exists intuitions_issue_ids_idx on intuitions using gin (issue_ids);

alter table intuitions enable row level security;
drop policy if exists "anon read" on intuitions;
create policy "anon read" on intuitions for select to anon using (true);
