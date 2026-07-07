# Vendored crates

Third-party crates vendored into this repo and consumed as `path` dependencies
(see the `minijinja` / `minijinja-contrib` entries in `engine/Cargo.toml`).

## minijinja, minijinja-contrib

- Source: https://github.com/boundaryml/minijinja.git, branch `value-cmp`
- Commit: `8cfc770a5dffeda2de5b910d2b9f870d7edeff7c`
- Base: upstream `mitsuhiko/minijinja` 2.16.0 plus one commit that adds
  `Object::value_cmp` (cross-type value comparison, needed because our enum
  values render as an alias but compare by a different underlying value).

Only the `minijinja` and `minijinja-contrib` crate sources + `Cargo.toml` are
kept here (tests and benches omitted). Re-sync by copying `src/` + `Cargo.toml`
from the commit above.

We vendor rather than depend on the git fork directly so the workspace has no
git dependencies and can build from a self-contained crate set.

## Policy: this is a verbatim snapshot; do not patch it here

The code in this directory is copied as-is from upstream. We intentionally do
**not** fix bugs or change behavior in the vendored copy, even known ones:

- Patching here would diverge from upstream and make every future re-sync a
  manual merge.
- These crates carry the same pre-existing upstream behavior as any consumer of
  minijinja 2.16.0; nothing here is specific to this repo (aside from the single
  `value_cmp` commit noted above).

If you hit a real bug in this code, fix it **upstream** in
`mitsuhiko/minijinja` and pull it in on the next re-sync, rather than editing the
vendored files.

Note for reviewers (human or automated): findings reported against files under
`engine/vendored/` are almost always pre-existing upstream characteristics, not
changes introduced by whatever PR moved these files into the repo. Treat them as
out of scope for that PR.
