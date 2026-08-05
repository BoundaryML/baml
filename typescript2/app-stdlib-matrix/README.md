# app-stdlib-matrix

Renders `tools/stdlib-matrix`'s JSON report: every compared stdlib member, with
a swatch showing which language it exists in and, once expanded, its docs and
any analysis attached to the pairing.

    pnpm dev            # then open the page
    pnpm build          # static files in dist/, deployable anywhere

The report is fetched at runtime, not bundled, so one build renders any run's
artifact. It loads `./matrix.json` by default; `?src=<url>` points at another.

    cp ../../tools/stdlib-matrix/report/matrix.json public/
