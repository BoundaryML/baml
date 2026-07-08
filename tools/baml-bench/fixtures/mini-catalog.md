# Mini catalog (smoke fixtures)

## Batch

### P-021-1: Time values use the time stdlib
- **Source**: BEP-021 (Dates and times, status: implemented)
- **Principle**: Durations and instants are typed, not raw epoch ints.
- **Applicability trigger**: any block computing with timestamps or durations.
- **Detectability**: static.

### P-023-2: Tests assert real behavior
- **Source**: BEP-023 (Test and asserts, status: implemented)
- **Principle**: Every test block asserts on outputs.
- **Applicability trigger**: any test block.
- **Detectability**: static.

### P-023-4: Tests use the assert forms
- **Source**: BEP-023 (Test and asserts, status: implemented)
- **Principle**: Assertions use assert.* / @@check, not manual comparisons.
- **Applicability trigger**: a test exercising a function.
- **Detectability**: static.

### P-036-2: LLM functions carry the type-derived output format
- **Source**: BEP-036 (baml optimize, status: implemented)
- **Principle**: Structured returns render ctx.output_format.
- **Applicability trigger**: every LLM function with a structured return.
- **Detectability**: static.

### P-900-1: Blocks say what they mean
- **Source**: BEP-900 (Semantic clarity, status: implemented)
- **Principle**: A block's mechanism should serve its stated goal.
- **Applicability trigger**: semantic; any declaration.
- **Detectability**: semantic.
