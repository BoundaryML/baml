# gcp-systest — credential-safety e2e harness for `forked_google_cloud_auth`

The GCP counterpart to `forks/aws-config-systest`. Run it whenever you touch
`forks/google-cloud-auth/**` to assert that **every Google Cloud auth flow still
mints a usable access token and that real Google APIs accept it**.

Two layers:

1. **Offline** — `cargo test -p forked_google_cloud_auth` (40 tests). Deterministic,
   no creds/network. Covers each flow's logic plus the **ADC source-precedence guard**
   (`tests/adc_precedence.rs`) and the **project-id / quota-project chain**
   (`tests/project_id.rs`). Everyday / CI guard.
2. **Live** — `forks/google-cloud-auth-systest/run-e2e.py` builds the `gcp-systest` binary, mints a
   token through the fork for each flow, and validates it against real Google APIs:
   `tokeninfo` (token is valid + carries the cloud-platform scope) and Cloud Resource
   Manager (`projects.get` — the token authorizes a real API call). The cloud tier runs
   the binary on a real GCE VM to exercise the metadata-server flow, then tears it down.

## Coverage matrix (every GCP auth flow)

| Auth flow | Offline test | Live scenario (`run-e2e.py`) | Tier |
|-----------|--------------|------------------------------|------|
| Service-account JSON (RS256 JWT-bearer) | `auth_flows.rs::service_account_*` | `local:service-account-json` (requires `--sa-file`) | offline (+local w/ key) |
| ADC `authorized_user` (refresh-token grant) | `auth_flows.rs::adc_authorized_user_*` | `local:adc-well-known`, `local:adc-GOOGLE_APPLICATION_CREDENTIALS` | local |
| ADC `service_account` (via `GOOGLE_APPLICATION_CREDENTIALS`) | `auth_flows.rs::adc_service_account_via_gac` | (offline; live needs a key) | offline |
| Workload identity federation (`external_account`, file/url subject tokens, ± impersonation, workforce user-project) | `auth_flows.rs::wif_*` | (offline; live needs a WIF pool) | offline |
| Workforce refresh grant (`external_account_authorized_user`) | `auth_flows.rs::external_account_authorized_user_*` | — | offline |
| Impersonated service account (`impersonated_service_account`) | `auth_flows.rs::impersonated_service_account_*` | — | offline |
| Well-known ADC path discovery (`HOME`/`CLOUDSDK_CONFIG`/`APPDATA`) | `auth_flows.rs::adc_discovers_well_known_*`, `adc_honors_cloudsdk_config` | `local:adc-well-known` | local |
| GCE metadata server | `auth_flows.rs::adc_falls_back_to_metadata_server` | `cloud:gce-metadata` — **real VM** | cloud |
| **ADC source precedence** (GAC > well-known > metadata) + metadata failure | `adc_precedence.rs` | — | offline |
| GAC set-but-unreadable is a hard error (google-auth parity, no fallthrough) | `auth_flows.rs::gac_set_but_unreadable_*` | — | offline |
| Token caching (reuse until google-auth's 3m45s refresh threshold) | `auth_flows.rs::tokens_are_cached_*`, `lib.rs::cache_respects_refresh_threshold` | — | offline |
| Project-id / quota-project chain (env > GAC file > gcloud config > ADC file > metadata) | `project_id.rs` | — | offline |
| token-endpoint / unsupported-type (AWS-WIF, executable, GDCH) / missing-token errors | `auth_flows.rs` (error cases) | — | offline |

Token validation per live scenario: **tokeninfo** (always; asserts 200 + cloud-platform
scope) and **Cloud Resource Manager** `projects.get` (local scenarios; asserts 200 + the
expected project). The GCE default service account has no project-get role in the dev
project, so the metadata tier asserts token validity + scope (resourcemanager status is
shown but not gated). Vertex AI `generateContent` is an optional check via
`--vertex-model`/`--vertex-region` (e.g. `gemini-2.5-flash` / `us-central1`); when
supplied, every scenario also gates on `vertex=200`.

## Running it

```sh
# Everyday: offline only — no GCP creds (use this in CI).
./forks/google-cloud-auth-systest/run-e2e.py --offline-only

# Default: offline + local ADC flows (needs `gcloud auth application-default login`).
./forks/google-cloud-auth-systest/run-e2e.py

# Full: offline + local + live GCE metadata flow (provisions & tears down a VM).
./forks/google-cloud-auth-systest/run-e2e.py --all

# With the service-account flow (supply a key file) and/or a Vertex call:
./forks/google-cloud-auth-systest/run-e2e.py --sa-file ./sa.json --vertex-model gemini-2.0-flash --vertex-region us-central1
```

Flags: `--project ID` (default from `gcloud config`; must be the dev project), `--sa-file PATH`,
`--vertex-model`/`--vertex-region`, `--zone` (default `us-central1-c`), `--skip-build`.
The script **asserts** every scenario and exits non-zero on failure, so it is gate-able.

### Safety / teardown

**Dev project only.** Every live tier resolves the active project and **refuses to run
unless it is `sam-project-vertex-1`** (constant `DEV_PROJECT`), checked before any token is
minted or VM created — so pointing it at another project aborts immediately with a non-zero
exit and zero side effects. The cloud tier names resources `gcp-systest-e2e-<ts>-<pid>` and
registers a single `trap … EXIT INT TERM` that always deletes the VM and GCS bucket, even on
Ctrl-C or mid-run failure. Verify independently:

```sh
gcloud compute instances list --project sam-project-vertex-1 --filter="name~gcp-systest"
gcloud storage ls --project sam-project-vertex-1 | grep gcp-systest
```

## Adding a new auth flow

1. Add an offline integration test under `forks/google-cloud-auth/tests/` (drive
   `token_from_adc` / `token_from_service_account_json` through `tests/common/mod.rs`'s
   `MockIo`); if it changes ADC ordering, update `adc_precedence.rs` deliberately.
2. Add a live scenario in `run-e2e.py` (a `local:` block, or a `cloud:` block with a
   matching `trap` cleanup) and a row to the matrix above.

The harness binary (`src/main.rs`) is a standalone package excluded from the parent workspace so
its native-only Tokio features do not affect wasm builds. `run-e2e.py` builds it with
`--manifest-path forks/google-cloud-auth-systest/Cargo.toml --target-dir ./target`; it is
cross-compiled to a static `x86_64-unknown-linux-musl` ELF (`cargo zigbuild`, no Docker) and run on
a GCE VM for the metadata tier.
