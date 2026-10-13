# Third-party notices

## JMESPath compliance corpus

`testdata/jmespath-compliance.json` is normalized from the compliance JSON in
the official `jmespath.py` repository.

- Repository: https://github.com/jmespath/jmespath.py
- Pinned commit: `2812594e69d43098ef60f81f4efc404c071b0418`
- Source directory: `tests/compliance`
- Included source files: every JSON file except `benchmarks.json`
- Included correctness cases: 892
- Copyright: 2013 Amazon.com, Inc. or its affiliates
- License: MIT
- License copy: `third_party/JMESPATH-PY-MIT.txt`

The standalone `jmespath.test` repository is the canonical home of the suite but
does not contain an explicit license file. This benchmark therefore sources the
same tests from the official MIT-licensed Python repository. Exact source file
paths and SHA-256 hashes are embedded in the normalized JSON.

The normalizer adds stable IDs, converts error spellings to the strongly typed
BAML enum representation, and combines the source files. Expressions, input
JSON, expected JSON, and comments are otherwise preserved.

## JMESPath specification

`third_party/jmespath-specification.rst` is copied unchanged from the official
JMESPath website repository.

- Repository: https://github.com/jmespath/jmespath.site
- Pinned commit: `e7e6d36d7723cd212c58434ef56b64f97d170fd1`
- Source path: `docs/specification.rst`
- Source SHA-256:
  `f7b45c9d53998a1da53e86e8be9078ccc2b2c453c820470276d4ef5c19a1cc67`
- Copyright: 2015 James Saryerwinnie
- License: Creative Commons Attribution 4.0 International
- License copy: `third_party/CC-BY-4.0.txt`

The benchmark-specific interface, tests, and normalization code are not derived
from either upstream repository.
