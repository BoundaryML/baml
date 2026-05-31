# cache-cargo-home

R2-backed cache for the slow-to-populate cargo download caches —
`~/.cargo/git/db` and `~/.cargo/registry/cache` — for `baml_language`.

It is the replacement for the previous curl-based composite action and for
`Swatinem/rust-cache`'s registry/git caching: fetching `.crate` files from R2 is
substantially faster than hitting crates.io, and restoring a bare git `db` from
R2 avoids re-cloning our large forks (aws-sdk-rust, google-cloud-rust, minijinja)
on every runner.

## What it does

The action **takes no inputs**. It reads `baml_language/Cargo.lock` and from that
alone decides what to cache:

- **crates.io packages** → one **content-addressed** object per `.crate`, keyed
  by the sha256 checksum Cargo.lock already records
  (`…/crates/<index>/<aa>/<sha256>.crate`). Because the key is the checksum, a
  Cargo.lock change still hits the cache for every crate whose version is
  unchanged, and identical crates dedupe across branches.
- **git packages** → one tar per repo under `git/db`
  (`…/git-db/<aa>/<sha256(url)>.tar`), shared across the revisions that repo is
  pinned to. The tar preserves the real `git/db/<dir>` path so it restores into
  place even though cargo's URL-hash suffix isn't known on a cold runner.

The crates.io **index** and `registry/src` are deliberately not cached — cargo
refreshes the (sparse) index cheaply and re-extracts `src/` from the restored
`.crate` files.

It runs in two phases, GitHub-native:

- **main (restore)** — download every referenced crate/git object from R2 into
  `$CARGO_HOME`. Anything missing is recorded in the action state.
- **post (save, `always()`)** — after the build has fetched the misses, upload
  the new objects. Uploads are new-only (a `HEAD` precedes each `PUT`) so the
  many jobs that all save the same content-addressed object don't re-upload or
  stampede a key another job already wrote.

Everything is best-effort: any R2/parse error is logged as a warning and never
fails the build.

## Configuration (environment only)

Uses the same variables sccache uses (see `.envrc` /
`cargo-tests.reusable.yaml`). Resolution is tolerant of the exact spelling: it
scans for `SCCACHE_*R2_*` variables and falls back to the standard sccache and
`AWS_*` / `R2_*` names.

| Setting    | Variable(s)                                                       |
| ---------- | ---------------------------------------------------------------- |
| Endpoint   | `SCCACHE_ENDPOINT`                                                |
| Bucket     | `SCCACHE_BUCKET`                                                  |
| Region     | `SCCACHE_REGION` (R2 ignores it; defaults to `auto`)             |
| Key prefix | `SCCACHE_S3_KEY_PREFIX` (objects go under `<prefix>/cargo-home/`) |
| Access key | `BAML_SCCACHE_R2_ACCESS_KEY_ID`                                   |
| Secret key | `BAML_SCCACHE_R2_SECRET_ACCESS_KEY`                              |

If endpoint / bucket / key / secret are missing (e.g. a fork PR without secrets)
the action no-ops, mirroring sccache's secretless fallback.

## Usage

One step per job — the post-save runs automatically:

```yaml
- name: "Cache cargo home (R2)"
  uses: ./.github/actions/cache-cargo-home
```

## Outputs

`cache-hit`, `restored-crates`, `present-crates`, `missed-crates`,
`restored-git`, `missed-git`, `uploaded-crates`, `uploaded-git`.

## Development

Standalone pnpm project nested in the monorepo (it has its own
`pnpm-workspace.yaml` so a plain `pnpm install` resolves only its deps):

```bash
pnpm install
pnpm run typecheck
pnpm test
pnpm run build      # bundles src/ → bundle/restore.js + bundle/save.js (committed)
```

`bundle/` is the published entrypoint and **must be committed** — GitHub runs the
bundled JS directly. It is named `bundle/` rather than the usual `dist/` because
the monorepo root `.gitignore` excludes `dist/`. After changing anything under
`src/`, rerun `pnpm run build` and commit `bundle/`.
