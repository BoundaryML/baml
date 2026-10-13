# Third-party notices

## Marcel van Kervinck perft positions

The test data in `testdata/chess-perft-corpus.json` is derived from
`perft-marcel.epd` in Chris Whittington's Chess-EPDs collection.

- Position author and public-domain releaser: Marcel van Kervinck
- Collector: Chris Whittington
- Repository: https://github.com/ChrisWhittington/Chess-EPDs
- Pinned commit: `ba11f9145e7a249d3d202b3d7528c745972dd5eb`
- Source path: `perft-marcel.epd`
- Source SHA-256:
  `568a43b0528092dc0cffa57faa6a53737afe3ef8536e4b8adc9d0e21fd29b493`
- Public-domain evidence:
  - The repository README identifies its suites as public domain and
    specifically credits Marcel van Kervinck with placing this suite in the
    public domain.
  - Marcel's release announcement:
    https://talkchess.com/viewtopic.php?t=73812

The source file has a trailing NUL byte on each data row. Normalization removes
that byte, parses the six depth counts, and otherwise preserves each textual EPD
row in the JSON. The original repository, commit, source line, and raw EPD text
remain attached to every normalized position.

The benchmark scaffolding and normalization code are not derived from the
Chess-EPDs repository.
