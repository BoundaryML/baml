# app-stdlib-matrix

Renders `tools/stdlib-matrix`'s JSON report: every compared stdlib member, with
a swatch showing which language it exists in and, once expanded, its docs and
any analysis attached to the pairing.

    pnpm dev            # then open the page
    pnpm build          # static files in dist/, deployable anywhere

The report is fetched at runtime, not bundled, so one build renders any run's
artifact. It loads `./matrix.json` by default; `?src=<path>` points at another
report **served from this same origin** — a second run's artifact beside the
page, say. A cross-origin URL is refused and the default is loaded instead: the
parameter exists so one build can render any of our runs, not so a link can make
this page display someone else's JSON as though it were ours.

    cp ../../tools/stdlib-matrix/report/matrix.json public/
