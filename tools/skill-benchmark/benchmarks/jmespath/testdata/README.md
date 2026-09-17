# JMESPath compliance corpus

`jmespath-compliance.json` contains every non-benchmark correctness case from
the official JMESPath compliance suite: 742 successful evaluations and 150
errors, for 892 cases total.

Each case has a stable ID and retains its expression, input JSON, expected JSON
or typed error kind, optional comment, source file, and source suite index. The
top-level source record contains the pinned repository commit and SHA-256 hash
of every input file.

See `../THIRD_PARTY_NOTICES.md` for attribution and licensing.
