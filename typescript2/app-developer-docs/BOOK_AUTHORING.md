# Book authoring and validation

The book uses this app's authored MDX pipeline. There is no separate mdBook build.

## Add a chapter

Use direct, concise prose. Avoid em dashes; use a sentence, colon, or parentheses
where appropriate.

Descriptions and introductions must name a concrete task or state a language
rule. Cut narration about the document itself, such as "what carries over and
what changes," "this page explains," or "a focused example." Don't advertise
validation machinery to readers ("canonical checked source," "checked stages").
Keep useful setup and replacement instructions, but express them as reader
actions. Replace vague advice such as "harden the boundary" with the specific
test, error handler, or validation the reader should add. Apply this to cards
and metadata as well as body text.

1. Add `content/baml/book/<slug>.mdx` with a title, a description beginning with
   its intended chapter number, and breadcrumbs. Use the outline's numbers even
   while earlier chapters are unpublished.
2. Add `app/baml/book/<slug>/page.tsx`, using `AuthoredPage` and `authoredMetadata`.
3. Add the chapter to the Book branch in `lib/navigation.ts`, in reading order.
   This drives sidebar entries, authored search, and previous/next links. Those
   links traverse published pages.
4. Update the book landing page and `expectedAuthoredRoutes` in
   `tests/authored-content.test.ts`. Link only to published routes; mention
   future prerequisites by concept without creating empty pages. The landing
   page lists the agreed outline; replace a plain chapter title with its link
   when the chapter is published.
5. Put executable BAML in `content/code/standalone` or `content/code/projects`.
   Use `BamlSnippet` or `BamlProject`; do not duplicate executable BAML in MDX
   fences. Keep each incremental stage's dependency/replacement relationship
   explicit in the prose.

## Book perspectives

Every book page shows a global **Perspective** selector. The initial choices are
**Start with the basics** and **Coming from TypeScript**. Both choices are
always visible beneath the **Reading perspective** label. The selected preference persists
in a cookie scoped to `/baml/book`; book routes render it on the server to avoid
flashing the wrong chapter text. Non-book routes keep their existing rendering.

A chapter is either one shared MDX file or a directory of perspective files:

```text
content/baml/book/
├── index.mdx
├── errors.mdx
├── interfaces.mdx
└── common-programming-concepts/
    ├── default.mdx
    └── typescript.mdx
```

- `<chapter>.mdx` is shared by every reader.
- `<chapter>/default.mdx` is required for chapters with perspectives.
- `<chapter>/<perspective>.mdx` supplies an alternate version. Its filename must
  match a registered perspective ID.
- A chapter cannot have both a shared file and a perspective directory.
- Nested perspective directories and `index.mdx` inside chapter directories are
  invalid. The book landing page stays at `content/baml/book/index.mdx`.

If the selected perspective is unavailable, the chapter uses the shared or
`default.mdx` version silently. The reader's preference is preserved.
An explicit `?perspective=typescript` (or `default`) overrides the preference for
that page without overwriting it. Selecting an option explicitly saves it.

### Add a perspective to a chapter

Move `<chapter>.mdx` to `<chapter>/default.mdx`, keeping its existing frontmatter.
Add `<chapter>/typescript.mdx` alongside it:

```mdx
---
title: Common programming concepts
description: Chapter 3 · BAML for TypeScript developers.
sectionKeys:
  variables: familiar-let
---

## Familiar let

Explain which TypeScript habits transfer and which rules differ.
```

The directory determines the chapter and the filename determines the perspective.
Do not add `chapter` or `perspective` frontmatter; those fields are rejected.
The default file requires the usual title, description, and breadcrumbs. Alternate
versions require a title and can omit the description to inherit the default's.
Breadcrumbs, navigation, and canonical metadata use the default version.
All versions share `/baml/book/<chapter>`; perspective files have no separate public
routes or navigation entries. Moving a shared chapter into a directory does not
require changing its route component or navigation entry.

Each version supplies its own body and generated TOC. Authors can rename,
reorder, add, or omit headings. Only the active version is mounted in the page,
so copy-page, keyboard navigation, and assistive technology see that version.
Use the usual `BamlProject`, `CodeExample`, and `LanguageTabs` components. Reuse
canonical snippet IDs instead of copying executable BAML into the prose.
Example-language tabs do not change the reader's book perspective.

### Preserve the reader's place

Matching heading IDs work automatically. When related sections have different
headings, optionally add a `sectionKeys` mapping to **both** files. Keys identify
concepts; values are that version's generated heading IDs:

```yaml
# Default chapter
sectionKeys:
  variables: variables-and-mutability

# TypeScript version
sectionKeys:
  variables: familiar-let
```

Switching perspectives maps the section currently being read to its counterpart.
If no counterpart exists, it moves to the chapter's beginning. Changing the
preference on a chapter that still shows the same body does not move the reader.
Section links preserve the current perspective query parameter.

### Validate changes

`pnpm docs:authored:validate` includes the perspective resolver/schema tests and
checks snippet references and links in alternate MDX. `pnpm build` also rejects
invalid directory layouts, unknown perspective filenames, missing defaults, and section keys
that point at absent TOC headings. Headings are not required to match across
perspectives. Test the chapter in both modes, including its TOC, deep links,
copy-page, narrow layouts, and fallback to a chapter without an alternate body.

To introduce another perspective, add an entry in `bookPerspectiveDefinitions` in
`lib/content/book-perspectives.ts` with a label, compact `shortLabel`, and local
logo. The parser, filename validation, and selector all use that registry. Then
add `<chapter>/<new-perspective>.mdx` wherever that perspective is useful.

### Keep experienced-reader perspectives brief

Write perspectives for what the reader already knows. Use `PerspectiveNote` at the
concept or example level; don't group the whole chapter into three buckets:

- `kind="same"`: a narrowly stated behavior that transfers unchanged. The summary
  stays visible; the explanation and example are collapsed by default.
- `kind="added"`: a capability BAML adds, such as a block producing a value.
- `kind="removed"`: source-language syntax BAML doesn't support; show its replacement.
- `kind="modified"`: a familiar concept with changed syntax or behavior.

The reading-perspective selector automatically shows a shared key whenever the
reader selects a non-basics perspective. Don't add a key to chapter content.
Added is green, removed is rose, modified is amber, and same is muted. Each note
uses the matching label and a vertical bar beside its explanation and examples;
the words and symbols make the distinction without relying on color alone.

Anchor differences in an idiom the reader already uses. Show the TypeScript
IIFE next to the BAML block expression, or the ternary next to the BAML `if`.
Use `CodeComparison from="TypeScript"` with two `<div>` children: familiar code
first, then a canonical BAML excerpt. Both panes stay visible, carry language
logos, and stack on narrow screens. Use inline pairs or small mapping tables for
one-token changes. Don't make the reader reconstruct the comparison from prose.

Give each note a short `summary`. These labels describe similarity, not quality.
Put exceptions outside collapsed notes. Prefer short canonical snippet regions over
repeating a complete beginner walkthrough. Avoid restating the same difference in
an introduction, example explanation, and closing checklist.

```mdx
<PerspectiveNote kind="same" summary="Early return exits the function">

Optional explanation and example.

</PerspectiveNote>
```

## Language comparisons

Use the shared `LanguageTabs` component for language comparisons. It includes
each language's logo, accessible labels, and keyboard navigation automatically.
Give each language one `<div>` panel, in the same order as `languages`:

```mdx
<LanguageTabs languages={["TypeScript", "Effect.ts", "Rust"]}>
  <div>TypeScript examples and explanations</div>
  <div>Effect.ts examples and explanations</div>
  <div>Rust examples and explanations</div>
</LanguageTabs>
```

Keep related examples together in a panel. For comparisons introducing BAML,
show BAML separately after the language tabs. Put panel labels in
`<p className="language-tabs-heading">` so hidden panels do not add duplicate
entries to the page's table of contents.

After comparison examples, use compact “New concepts introduced” tables with
“Word” and “What it means” columns. Define error-handling vocabulary when it first
appears within each language, and only list additions in later examples. If a call
introduces no new concepts, say so in one sentence. Use the same treatment for
BAML and the comparison languages; avoid Good/Trade-off ratings. Follow Google's
[developer documentation style guide](https://developers.google.com/style/tone).
State the behavior directly; avoid slogans. Keep definitions short and detailed
mechanics in the sections that teach them.

For error-handling comparisons, show an unhandled call before adding recovery.
Include idiomatic propagation, such as Rust's `?` and Effect's `yield*`.

Use annotated SVGs for these comparisons. Wrap a host-language fence in
`CodeExample` with a stable `annotation` ID. For canonical BAML excerpts, add the
same prop to `BamlProject`. The examples on the errors page show both forms.

Add exact text targets and concise labels to `lib/snippets/annotation-specs.ts`.
The annotation kinds are `success`, `syntax`, and `recovery`; marks can be an
`underbrace`, `circle`, or `bracket`. Use a one-based `occurrence` for repeated
text. Keep labels close to the relevant code and use only the annotations needed
to explain the example. Callers should perform an operation on the successful
value; a function that only forwards a result does not need Rust's `?` or an
Effect generator.

Run `pnpm docs:annotations:generate` after editing source code, annotation targets,
labels, or the renderer. The generator reads the MDX fences and canonical BAML
regions, applies the shared syntax grammar, and writes both image themes to
`public/book/annotations`. It embeds the licensed Geist fonts so annotation
positions remain stable across browsers.

`pnpm docs:authored:validate` regenerates the images in memory and compares them
with the checked-in assets. Source changes also invalidate the image at page
render time. Stale images fail these checks until regenerated. Readers can toggle
between the annotated image and syntax-highlighted code. Both views copy the
original code, without annotation text.

The logo catalog is `lib/content/language-tabs.ts`. Add a local logo and a catalog
entry before introducing another language; unregistered labels fail rendering
rather than displaying a tab without its logo. Monochrome logos adapt to dark
mode. Keep the visible language name alongside its decorative logo.

TypeScript and Rust assets are reused from `app-website/public/logos`.
Effect's logomark comes from the official Effect website repository:
`Effect-TS/website`, commit `bf4625446a02894046b6937a317dde2cde115fe7`,
`apps/web/public/assets/effect-logo/logo-symbol/effect-logomark-black.svg`.

## LLM prompts

Use concrete client values, such as `openai.ResponsesClient.new(...)`, instead of provider/model string shorthand. Declare a named client when examples share the same configuration.

Use `snake_case` for BAML function names, including LLM functions. To override the provider for one LLM call, pass `client = model` directly to the function.

Put task instructions and `${ctx.output_format()}` in the system message. Keep user-supplied text, questions, and images in user messages.

## Canonical code excerpts

A project excerpt uses the complete project as its compilation unit:

```mdx
<BamlProject id="listing-08-01" file="baml_src/main.baml" regions={["excerpt-01"]} />
```

Mark regions with `// docs:start name` and `// docs:end name`. Regions can nest,
so a single line can be shown separately without duplicating its enclosing
example. Multiple selected regions are displayed in their requested order.
Duplicate names, crossed boundaries, missing files, and missing regions fail.
Without `file`, `BamlProject` shows the complete project. Metadata and region
markers are omitted from displayed code.

Expected failures carry `docs:meta` comments in one project source file, with
`expect.status: failure` and diagnostic codes (and message text where needed).
They are compiled, not executed. Inferred signatures and terminal diagnostics
are labeled as output, not executable source. Chapter 8's displayed signatures
are compared with the compiler's `describe --json` output.

## Commands

Run the app commands from `typescript2`:

```sh
pnpm --filter app-developer-docs docs:authored:validate
pnpm --filter app-developer-docs docs:routes:validate
pnpm --filter app-developer-docs lint
pnpm --filter app-developer-docs test
pnpm --filter app-developer-docs typecheck
pnpm --filter app-developer-docs build
```

Build the current compiler from `baml_language` with
`cargo build -p baml_cli --bin baml-cli`. Set `BAML_BINARY` to its absolute path
for these commands (the installed release may differ from this checkout):

```sh
pnpm --filter app-developer-docs docs:snippets:validate
pnpm --filter app-developer-docs docs:book:validate
```

`docs:book:validate` executes the tests in every successful example project that
contains test blocks. It requires a nonzero passing test count and rejects
compiler errors, runtime failures, timeouts, and empty test runs. It also checks
the four inferred signatures displayed in Chapter 8. The cross-package fixture
uses distinct compiler source roots and runs separately from `baml_language`:

```sh
target/debug/baml-cli test --from crates/baml_tests/baml_src -i root.compiler.mounted_interface_diagnostics
```

The Developer Docs workflow already matches authored content, example files,
and compiler changes. Its snippets job now runs the behavioral/signature checks
and the cross-package tests after compilation. The existing Vercel build
includes these ordinary static routes. No separate publishing step is needed.

## Chapter 3

Chapter 3 is the first chapter with separate `default.mdx` and `typescript.mdx`
versions, under `content/baml/book/common-programming-concepts/`. The default
introduces programming terms as they appear. The TypeScript version assumes those
terms and focuses on changed behavior. Semantic section keys connect their headings.

Both versions share the canonical `listing-03-*` projects and their behavior tests.
The `no-listing-03-*` projects check intentional compiler failures. Run the examples
on Canary; they need no model credentials. The AI example tests `@parse` offline.
The token estimator and prices are teaching examples, not exact usage or billing.

The starting material was the `ch03-*` human drafts and listings in `baml-book`.
The integrated prose removes browser-runtime claims, updates generated operations
to `@parse`/`@render_prompt`, uses snake_case and explicit prompt roles, and keeps
implementation caveats out of beginner explanations. New function and conditional
annotations use the same renderer and stale-image checks as the other chapters.

## Accepted sources and focused repairs

- Chapter 8: `baml-book/notes/pages/ch08-01-error-values-and-handling/08_final-candidate.md`.
- Chapter 11: [accepted Interfaces chapter](https://app.notion.com/p/3cabb2d262168187acc3df74adff055a).
- Chapter 12: `baml-book/notes/pages/ch12-01-concurrency/08_final-candidate.md`.
- Supporting projects: `baml-book/listings/ch08-errors`, `ch11-interfaces`, and
  `ch12-concurrency`. These were imported with their existing tests and failure
  expectations. The outline determines chapter numbering, not publication order.

Interfaces and Concurrency retain the accepted section order and explanations,
with the integration/correctness changes listed below. The Errors opening has
since been revised into language tabs with separate definition, propagation, and
recovery examples, annotated images, and vocabulary tables. Its callers format
successful values as `Tool: search`; the remainder retains the accepted prose
apart from a source-location explanation.

Initial integration repairs:

- Added metadata, breadcrumbs, prerequisite links, and explicit stage/dependency
  notes; mapped code to canonical project excerpts and Notion tables to MDX.
- Chapter 8: added deterministic error, propagation, panic-boundary, and lambda
  tests. Refreshed displayed source locations and checked inferred signatures.
- Chapter 11: the webhook excerpt declares the class locally for a runnable
  single-project example. `book_interfaces.rs` verifies real dependency ownership
  and runtime dispatch, plus the orphan rule and rejection of out-of-body field
  mappings (`E0126`). An additional stage checks that adding field mappings
  preserves the earlier notification methods.
- Chapter 11: the audited nightly exposed `baml.Comparable`; this checkout uses
  `baml.ops.Compare` and `baml.ops.Ordering`. The associated-error example now
  defines a local `PriorityComparison` contract and uses `sort_by`, preserving
  the lesson about an implementation selecting `CompareError = never`. Its
  missing-associated-type failure is checked against that same local contract.
- Chapter 12: `cancel() == false` means already settled. Successful completion
  returns the value; the cancellation handler's alternative is "already
  cancelled". Both completed success and repeated cancellation are tested.
- Chapter 12: detached errors are reported globally, without replacing the root
  call's result. This is supported by the existing `spawn_semantics` tests and
  the engine's global reporting path.

### Settled outcomes

The stdlib change is reviewed separately in PR #4816, below the book PR.
`all_settled` replaces `all_complete`: it waits for every input and returns
`Success<T>`, `Failure<E>`, or `Panicked` for each input, in input order.
`Panicked` preserves the panic and its error context, including cancellation.
Cancelling the collector stops the wait rather than becoming an input outcome.

The chapter's behavioral checks cover mixed success/failure outcomes and
preservation of every typed error. The stdlib's native tests additionally cover
panics, input cancellation, collector cancellation, empty inputs, duplicate
inputs, and successful values that happen to be error objects.

### Runtime and rendering evidence

- Native: 32 snippet checks (including expected failures), 24 project behavioral
  tests, and four inferred-signature assertions pass through the current CLI.
  The demo returns both documents in order, and its CLI telemetry query returns
  the named threads with overlapping start/end times. The
  existing `cargo test -p baml_tests --test spawn_semantics` suite also covers
  parent failure, detached root completion, and global error reporting.
- Browser scheduling: `bex_engine/src/lib.rs` dispatches logical threads through
  `spawn_local` on wasm and `tokio::spawn` natively. `sys_wasm` delegates file
  reads to a host callback; this is not unrestricted host filesystem access.
- Browser telemetry: `app-promptfiddle/src/playground/baml-lsp-worker.ts` returns
  `telemetryUnavailable` for local profile-store requests. The book's guidance
  to use the CLI/extension for telemetry matches that implementation.
- The browser-runtime claims were checked against those source paths; a rebuilt
  browser WASM runtime was not executed as part of this documentation check.
- The app suite passes 44 tests; its PostgreSQL integration test is skipped
  without database configuration. Authored-content (4), route (13), lint, type,
  and production-build checks pass. Three cross-package checks and all 11
  existing spawn lifecycle tests pass.
- Rendered all three chapters in Chromium at 1440px and 390px. Checked code
  blocks, both interface tables, scrollable text diagrams, section anchors,
  light/dark rendering, authored search, and desktop/mobile navigation.

The app's database integration test requires PostgreSQL configuration and is
skipped by its normal local suite when that configuration is absent. It tests
generated reference storage, not authored book content. Remote GitHub CI has not been run; the workflow commands were checked locally.
No push or production deployment was performed.
