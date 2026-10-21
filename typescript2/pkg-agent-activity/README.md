# Agent Activity

A live view of what a coding agent does to a project: every file it changes,
animated as the change lands; the BAML workflows those changes touch; the
commands it runs and what they return; and, when its transcript is available,
what it said while working.

It is a page in the playground's web bundle (`/agent-activity`), served by
`baml agent watch`:

```bash
baml agent watch            # the current directory
baml agent watch ../my-app --port 4285 --no-open
```

The server watches the whole directory, skipping anything git ignores, and
streams to the page over `/api/agent-activity/ws`. Images (PNG, JPEG, GIF,
WebP, AVIF, BMP, ICO, and SVG) show as pictures, before and after each edit;
the server keeps each version and serves it from
`/api/agent-activity/media/<hash>`. See
`baml_language/crates/baml_agent_watch` for the server and
[`src/engine/protocol.ts`](src/engine/protocol.ts) for the messages.

## Views

| View | What it shows |
| --- | --- |
| Stage | Every touched file as a card, grouped by folder on a canvas. The camera follows the agent from file to file. |
| Tiles | The files being edited as tiled panes. The file being written takes the large pane. |
| Flow | The BAML project's workflows, as the playground draws them. Edits light up the steps they change, and runs trace the steps they executed. |
| Graph | The declarations the session touched and their references, clustered by file. |
| Contracts | The types, signatures, and fields that changed since the session started. |
| Feed | Every edit as a diff, grouped into bursts, or one diff per file since the start. |

## Developing without a server

Run the `pnpm` commands below from `typescript2/` (the repository root is a
separate workspace). Recorded sessions replay the same messages a server
sends:

```bash
pnpm --filter app-vscode-webview dev
# then open http://localhost:4000/agent-activity?mock=resume-screener
```

The picker under the menu bar switches between recordings. They are generated
by `scripts/make-fixtures.mjs`, which replays commits from this repository and
the scripted session in `scripts/scenarios/`. With a built `baml-cli`, it also
records the workflows the compiler reports after each `.baml` change:

```bash
cargo build -p baml_cli --bin baml-cli    # from baml_language/
pnpm --filter @b/pkg-agent-activity fixtures
```

To run the real server against the dev bundle:

```bash
BAML_PLAYGROUND_DEV_PORT=4000 baml agent watch --no-open
```

In development builds the page's runtime is available in the console as
`window.__agentActivity` (model, player, and source).
