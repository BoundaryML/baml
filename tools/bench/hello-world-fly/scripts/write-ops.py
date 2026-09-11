#!/usr/bin/env python3
from pathlib import Path
import json
r=Path(__file__).resolve().parents[1];m=json.loads((r/'manifest.json').read_text())
d=r/'artifacts/reports/baml-in-prod2'
d.mkdir(parents=True,exist_ok=True)
text=f'''# Seven Fly hello-world containers

Source: `{r}`. Scope follows [00-design.md](00-design.md): a deterministic hello-world stability exercise; no metrics platform or additional matrix rows. All seven requested variants are deployed. Under load, the native Alpine and Debian services have already been OOM-killed and automatically restarted, and AL2023 has produced timeouts/failed health checks; deployment success is not a stability pass. See [02-load-generation.md](02-load-generation.md). `GET /` passed exact status/body/content-type checks before load; see `{r}/pre-load.json`. Existing apps and unrelated source work were preserved.

Every measured app belongs to `boundary`, runs in `iad`, and has exactly one Linux amd64 machine, 1 shared CPU, and 1024 MiB RAM. Autostop is off, autostart is on, minimum running machines is 1. HTTP request concurrency is soft 800/hard 1000. Health checks request `/` approximately every 30 seconds. Restart policy is normal `on-failure`, maximum 10 retries; no scheduled restart, forced GC, swap, cache bypass of BAML, or profiling is configured. Shared CPU scheduling is a limitation for later performance comparisons.

| Variant/source subdirectory | URL | Machine |
| --- | --- | --- |
'''
for v in m['variants']:text+=f"| `{v['variant']}` | [{v['app']}]({v['url']}) | `{v['machine_id']}` |\n"
text+='''
## Response and execution semantics

The body is eleven UTF-8 bytes (`68656c6c6f20776f726c64`), with no newline. Every response has status 200, `Content-Type: text/plain; charset=utf-8`, `Content-Length: 11`, and `Cache-Control: no-store`. Express disables ETags and its identifying header. Uvicorn disables access logs and its server header. Fly adds its own proxy headers. No service logs each successful request.

Both baselines contain no BAML dependency. Express is 5.1.0; Starlette is 0.47.3 with Uvicorn 0.35.0 and one process. Both bridge handlers call generated `hello_world_async()` on every request. The generated SDK initializes one process-wide runtime from bytecode at import; the BAML function returns a string with no LLM/network calls or host-language constant substitution. No compiler or subprocess is invoked per request.

The three native services run `/app/hello` directly as their container entrypoint, produced at image build time by `baml-cli pack main --output /app/hello`. Their BAML handler uses `baml.http.Server.bind("0.0.0.0:8080")`, `serve`, and `Response.new`. There is no Node/Python HTTP wrapper or helper in the runtime serving path. AL2023 and Debian share the same GNU-target packed binary SHA-256 `9b91d68fcf962a094c18f4eae7b5cb7d8576b8a70ddd5df0a0a1c8a83660c741`; Alpine uses musl-target binary SHA-256 `5f281607842eb8516d880f47e1f91d8c2960ddc47910456129eb7253268f7805`.

Load uses HTTP/1.1 with keepalive enabled against the public HTTPS endpoint through Fly. Express and Uvicorn use 60-second idle keepalive timeouts. Native BAML uses Hyper's default keepalive behavior; the pinned `Server.serve` exposes header-read timeout but no idle keepalive setting. Native listeners explicitly allow HTTP/1 and disable HTTP/2. Idle timeout equality across these implementations is therefore not claimed; active 100-QPS keepalive traffic uses the same client connection policy. No compression middleware is enabled.

`BAML_PROFILE=0` is set in Docker/Fly environment before native initialization. The native profiling configuration reads this switch once and otherwise defaults on. `BAML_TELEMETRY_DISABLED=1` opts out of CLI telemetry. `BAML_LOG=off` is retained from the working deployment pattern; its effect is not relied upon for access-log suppression, because no per-request logging is implemented. Normal native/runtime defaults, including GC behavior, are retained. Actual restart and response evidence should be inspected independently of throughput.

## Versions and compatibility

BAML CLI/pack pin: `0.18.1-nightly.20260908.a`. Node bridge: `@boundaryml/baml-bridge@0.18.1-nightly.20260908.a`. Python bridge: `baml-bridge==0.18.1.dev2026090800`. The Linux release archives are downloaded inside the build containers and verified against their published SHA-256 files. Global BAML toolchain selection was not changed.

| Requested base | Observed runtime | BAML platform |
| --- | --- | --- |
| `node:20-alpine` (both rows) | Node 20.20.2, Alpine 3.23.4, musl 1.2.5 | Node addon `linux-x64-musl`; verified in serving process `/proc/.../maps` |
| `python:3.10-slim` (both rows) | Python 3.10.21, Debian 13.6/trixie, glibc 2.41 | Published Python 3.10-compatible bridge wheel |
| `amazonlinux:2023` | Amazon Linux 2023.12.20260817, glibc 2.34 | `x86_64-unknown-linux-gnu`; no libc replacement |
| `debian:bookworm-slim` | Debian 12/bookworm, glibc 2.36 | `x86_64-unknown-linux-gnu` |
| `alpine:3.22` | Alpine 3.22.5, musl 1.2.5 | `x86_64-unknown-linux-musl`; no glibc compatibility layer |

The explicit native Alpine 3.22 branch has main-repository support through May 1, 2027 ([Alpine release policy](https://alpinelinux.org/releases/)). Debian bookworm is in LTS ([Debian release information](https://www.debian.org/releases/bookworm/)). The exact user tags for Node/Python are preserved and digest-pinned, including their current underlying OS releases. Minimal native runtime packages (`libgcc`, `libstdc++`, CA certificates) are installed as applicable; build-only curl/compiler downloads are excluded from final native images. No BoundaryML/baml source changes were needed.

## Exact images

Digests below identify amd64 image manifests, not multi-architecture index digests. Dockerfiles pin the base; deployed image tags/digests identify the actual application filesystem and installed package set. Full runtime version/package output is embedded in `hello-world-manifest.json`.

'''
for v in m['variants']:
 i=v['image_ref'];text+=f"### {v['variant']}\n\nSource: `{v['source']}`. Base: `{v['base_tag']}@{v['base_digest']}`. Architecture: `linux/amd64`.\n\nDeployed image: `{i['registry']}/{i['repository']}:{i['tag']}`. Digest: `{i['digest']}`. Machine: `{v['machine_id']}`; instance at pre-load verification: `{v['instance_id']}`.\n\n"
text+='''## Deployment and logs

Fly CLI used: `v0.4.100 darwin/arm64`, authenticated as `sam@boundaryml.com`. Remote Depot builds avoid local Docker storage. Docker contexts use app-local allowlists so unrelated source, dependencies, artifacts and secrets are excluded.

```sh
cd tools/bench/hello-world-fly
python3 scripts/deploy.py                      # all seven
python3 scripts/deploy.py baml-amazonlinux     # one variant, preserving its AL2023 app name
python3 scripts/verify.py smoke
cd node-baml
fly deploy --remote-only --ha=false --yes --wait-timeout 3m
fly machine list -a baml-hw-0910-node-baml --json
fly logs -a baml-hw-0910-node-baml --no-tail
fly logs -a baml-hw-0910-node-baml
fly ssh console -a baml-hw-0910-node-baml
```

Substitute the app name from the table for any other service. `fly deploy` builds the checked-in pinned Dockerfile; the exact existing image can also be deployed with `fly deploy --image registry.fly.io/APP@sha256:DIGEST --ha=false --yes` from that variant directory. Do not redeploy measured apps during a run unless investigating a failure, and record that intervention.

## Issues and workarounds

- First Node bridge deployment compiled the generated SDK as CommonJS. The bridge package exports ESM only, causing `ERR_PACKAGE_PATH_NOT_EXPORTED` before listening. Fly performed startup retries; captured machine evidence reached `restart_count: 10` before the fix. The final source uses `type: module`, TypeScript `NodeNext`, and ESM imports, preserving Node 20/Alpine. A real async BAML call now runs during image build. The interrupted failed deploy left a lease that was cleared only for this newly created machine before updating its image. These are pre-load setup failures, not concealed load results.
- First Python bridge build lacked Pydantic, which generated reflection imports even for the trivial function. The build-time BAML-call check caught `ModuleNotFoundError: pydantic`; explicitly adding `pydantic==2.11.9` fixed it before any Python bridge machine was created.
- Fly's abuse filter rejects app names containing “amazon.” The AL2023 row is therefore named `baml-hw-0910-baml-al2023`; its source directory and exact base remain `baml-amazonlinux` and `amazonlinux:2023`.
- The supplied skill examples for generator configuration/client APIs are stale for this nightly. Fresh generation verified TOML `[generator.app]`, `output_type = "typescript/node"` or `"python/pydantic"`, and top-level sync/async functions. No older `baml_client` API is used.
- Both musl CLI/pack-host and Node musl addon are published for this pin and worked directly. The prior Node GLIBC_2.35 limitation does not apply to the musl Node row. The GNU packed binary runs on stock AL2023 glibc 2.34.
- The initial environment sandbox prevented Fly configuration writes and network resolution; the session later enabled unrestricted authorized execution. No application resource was substituted to work around environment restrictions.

`00-design.md` remains unchanged. Deployment/build logs and process maps are ignored under the source directory's `artifacts/`; small manifest/smoke/load summaries are retained as operational evidence. See [02-load-generation.md](02-load-generation.md) for the live run and controls.
'''
(d/'01-hello-world-matrix.md').write_text(text)
(d/'hello-world-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
