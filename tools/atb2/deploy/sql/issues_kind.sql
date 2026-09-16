-- Feature requests: an issues row is a bug or a feature request (models.baml
-- IssueKind). Apply in the Supabase dashboard before deploying a runner that
-- writes `kind`; rows written before default to 'bug'.
alter table issues add column if not exists kind text not null default 'bug'
  check (kind in ('bug', 'feature'));

-- issues_with_outcome must expose the column for the website. If the view
-- lists its columns explicitly rather than `i.*`, recreate it with `kind`
-- added; `select * from issues_with_outcome limit 1` shows whether it does.
