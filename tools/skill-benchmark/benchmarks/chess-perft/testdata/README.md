# Perft corpus

`chess-perft-corpus.json` contains every row from Marcel van Kervinck's
public-domain `perft-marcel.epd` suite at the pinned Chess-EPDs commit recorded in
`THIRD_PARTY_NOTICES.md`.

Each position records its exact source line, normalized six-field FEN, raw EPD
text without the source file's trailing NUL artifact, reference node counts for
depths 1 through 6, duplicate relationship, detected metadata features, and the
depths enabled by the bounded public profile.

The public profile selects 2,048 unique positions. It runs depths 1 and 2 for all
selected positions, depth 3 for a stable 128-position subset, and depth 4 for a
stable 16-position subset whose depth-4 count is at most 100,000. Positions with
castling rights, en passant targets, or zero legal root moves are selected before
the stable hash sample. This selection is reproducible from
the source metadata stored in the corpus.
