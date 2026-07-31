# Observability v1 golden fixtures

This directory is the append-only registry for byte-exact observability
fixtures. Each format-owning phase adds its encoder output and reader oracle
to `manifest.json`; an existing fixture is never regenerated in place.

The registry now freezes BCCT and revision-dictionary bytes, canonical value
DAG bytes plus their CID, and BQF1 bytes. New formats may append fixtures and
tests, but released entries are immutable.
