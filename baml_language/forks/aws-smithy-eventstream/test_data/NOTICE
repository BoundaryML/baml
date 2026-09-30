# Upstream test vectors

These binary event-stream frames are copied unchanged from `aws-smithy-eventstream` 0.60.11 (`test_data/`), part of [smithy-rs](https://github.com/smithy-lang/smithy-rs), Copyright Amazon.com, Inc. or its affiliates, licensed under the Apache License 2.0.

`tests/upstream_vectors.rs` asserts the fork decodes the valid frames and rejects each invalid one with the same error kind upstream's `frame.rs` tests expect, except `invalid_header_name_length_too_long`, where the fork bounds header reads to the declared frame (see `bounds_header_reads_to_the_frame`).
