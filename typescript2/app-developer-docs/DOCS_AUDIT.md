# Authored documentation audit — September 15, 2026

The priority is the pages readers follow: getting started, examples, tutorials,
bridges, and the book. This audit covers all 17 authored MDX pages plus the home
page. Historical API reference review is outside the final scope.

Baseline: canary commit `29fcfe5e6c4513417c037f2388dc3efb5acecb77`, released Canary
CLI 0.19.0, and published Node runtime `@boundaryml/baml-bridge@0.19.0`.
Versions here record the test environment; reader-facing guides target Canary.

## Page decisions

“Verified” means the page passed the checks listed below. It does not mean a live
model response is guaranteed. “Corrected” means a problem was found and the
changed example or instructions were checked again.

| Page | Decision | What was checked or corrected |
| --- | --- | --- |
| `/` | Verified | Navigation destinations and production HTML. |
| `/baml` | Corrected | Explains both ordinary functions and model calls; CLI and SDK entry points. |
| `/baml/get-started` | Corrected | Self-contained macOS/Linux, Windows, and Arch installation, PATH setup, editor/agent setup, updates, project creation, and first run. The example returns `42` without a provider. |
| `/baml/book` | Verified | Available chapters link correctly; unpublished titles are explicitly planned. |
| `/baml/book/errors` | Corrected | Effect examples explicitly target Effect 3. BAML behavior and inferred signatures pass; TypeScript, Effect, and Rust success/propagation/recovery examples execute. |
| `/baml/book/interfaces` | Verified | Canonical projects compile; behavior and compiler-diagnostic checks pass. |
| `/baml/book/concurrency` | Corrected | Explains input-order error observation and typed-error cancellation in `all`; distinguishes panics. TypeScript observes both promises with `Promise.all`. Replaces unverified browser-playground instructions with a tested CLI simulation and profile query. |
| `/baml/language` | Corrected | Removes placeholder claims and links readers to available syntax and chapters. |
| `/baml/language/functions` | Corrected | Uses snake_case; explains named/default parameters and host SDK conventions. Snippets compile. |
| `/baml/bridges` | Corrected | Describes the available Node.js guide and actual setup prerequisites. |
| `/baml/bridges/typescript` | Corrected | Replaces `baml_client`/`b.stream` examples with generated `baml_sdk` exports, ESM setup, `_async`, and `$stream_async`. Starts with a synchronous call, then shows the optional async variant. Tests actual SDK output and documents the `Done` typing limitation. |
| `/cli` | Corrected | Shows channel selection and update separately. Command links resolve. |
| `/bcs` | Corrected | Removes unsupported product-roadmap claims; says documentation is unavailable and provides the support link. |
| `/examples` | Corrected | Describes the available examples and their credential requirements. |
| `/examples/classify-support-tickets` | Corrected | Current client construction, prompt roles, snake_case, CLI invocation, and Node async export. Typed classification passes against a local provider mock. |
| `/examples/vision` | Corrected | Installation link now reaches actual instructions. Nine fixture tests verify provider requests, labeled images, loops, output schema, custom-client restrictions, and response parsing. Download and image links resolve. |
| `/tutorials` | Corrected | Describes the available tutorial and prerequisites. |
| `/tutorials/structured-extraction` | Corrected | Current syntax and SDK calls, exact CLI example, API-key setup, and safe shell quoting for dollar amounts. Typed receipt extraction passes against a local provider mock. |

## Verification

- All 33 canonical snippet validation targets pass on Canary.
- Project validation: 34 behavior checks (including the nine vision tests) and four inferred-signature checks pass.
- Vision: all nine tests pass without calling a model.
- Generated Node SDK: synchronous and asynchronous calls execute; the exact
  TypeScript guide blocks typecheck. Classification, receipt extraction, streamed
  partials, the `Done` sentinel, and final values pass against a local HTTP mock.
- Production-mode authored-page crawl: 18 pages and 43 linked destinations/assets;
  no failed requests, duplicate IDs, missing anchors, or broken internal links.
- App tests, lint, typecheck, production build, and compiled-style validation pass.
- Mobile quickstart and navigation were inspected in a 390px viewport.
- External links were checked for reachability. Provider details were checked
  against their official documentation; reachability alone is not API validation.

The external provider sources include [Cloudflare's vision model schema](https://developers.cloudflare.com/workers-ai/models/llama-3.2-11b-vision-instruct/),
[Cloudflare's vision tutorial](https://developers.cloudflare.com/workers-ai/guides/tutorials/llama-vision-tutorial/),
[Claude's model catalog](https://platform.claude.com/docs/en/models/overview), and
[Gemini 2.5 Flash](https://ai.google.dev/gemini-api/docs/models/gemini-2.5-flash).

### Limits

No live OpenAI, Anthropic, Google, or Cloudflare inference was performed in this
audit: provider credentials were unavailable. Request construction, local HTTP
transport, fixture response parsing, and host SDK behavior were tested. Sample
model answers remain explicitly illustrative.

The Node generator currently omits `ai.stream.Done` from `nextAsync()`'s declared
return type. The guide uses a tested `unknown` narrowing workaround. This audit
does not change the SDK implementation.

## Retained site fixes

Before the scope was narrowed, the crawl found 444 current-reference `$stream`
URLs returning 404, member anchors colliding with section anchors, and docstrings
creating extra top-level headings. Those fixes and regression checks are retained
because authored pages link into the reference. `throws never` is also preserved
in displayed interface signatures.

Production logs showed PostgreSQL connection exhaustion during requests. The
serverless pool now has a lower per-instance connection limit and releases idle
connections. Local integration and crawl checks pass; production behavior must
be observed after deployment.

## CI corrections

The required check is `Developer Docs`; the aggregate job now publishes that
exact name on pull requests and merge groups. Change detection uses the merge
group's base/head diff, so unrelated queue entries skip the expensive jobs.
Failure to read the diff fails the check. Tests exercise failure and cancellation
propagation, trusted/untrusted PR behavior, and unrelated/runtime merge-group
diffs.

The HTTP job previously assumed a release existed in CI's database because it
was on the public site. It now verifies the latest release stored in the configured
database (or the explicitly configured version), then uses the resolved version
for every HTTP assertion. Missing releases still fail verification. The CSV
heading check selects `Record` or its older name, `CsvRecord`, from that release's
manifest. The full HTTP checks pass locally against both the September 1 and
September 11 snapshots. HTTP failures now print the requested URL and status.

## Repeat the authored-page audit

From this directory, with the production-mode site running:

```sh
python3 scripts/audit-site.py \
  --base-url http://localhost:3099 \
  --output /tmp/docs-audit.json
BAML_VERSION=canary pnpm docs:snippets:validate
BAML_VERSION=canary pnpm docs:book:validate
BAML_VERSION=canary baml test --project content/code/projects/vision
pnpm test
pnpm lint
pnpm typecheck
pnpm build
```

Set `DEVELOPER_DOCS_TEST_DATABASE_URL` to an isolated PostgreSQL test database to
include database integration tests; otherwise those tests are skipped. The
production-mode site needs `GENERATED_CONTENT_DATABASE_URL` populated with a
reference release to check links into generated pages. `--scope all` opts into
historical reference crawling; it is not the default.
