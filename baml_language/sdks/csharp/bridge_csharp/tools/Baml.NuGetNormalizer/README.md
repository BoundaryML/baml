# BAML NuGet normalizer

This repository tool rewrites an unsigned `.nupkg` into the deterministic ZIP
and OPC form used as the immutable release artifact:

- stable ordinal entry order, timestamps, permissions, and compression;
- one fixed core-properties part path;
- deterministic root relationship ordering and IDs;
- an updated content-type override when a package uses one; and
- byte-for-byte preservation of all non-OPC payloads.

It rejects signed inputs, duplicate/case-colliding or unsafe paths, malformed
OPC metadata, an occupied canonical core-properties path, existing output
files, and in-place rewrites. Signing, if enabled by the release, occurs only
after normalization and exact-package consumer verification.
