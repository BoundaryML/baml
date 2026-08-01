# Local observability and captured data

BAML CLI, SDK, and packaged-host calls now keep local aggregate timing history
by default. Root and LLM-function values are also eligible for local capture.
Artifacts live below the owning project's `.baml/` directory; they are not
uploaded by this implementation.

Captured prompts, responses, arguments, errors, and logs can contain sensitive
application data. Redaction attributes are applied before content hashing, but
unredacted captures are stored as exact canonical value chunks. Access to the
project directory therefore grants access to its retained observability data.

Use `BAML_HISTORY=0` to disable durable boundary/value history process-wide
while retaining the in-process aggregate profiler. A project can apply the
same durable opt-out and narrow capture in `baml.toml`:

```toml
[observability]
enabled = false
capture_values = false
capture_logs = false
latency_trigger_ms = 0
```

`enabled` defaults to `true`, `capture_values` defaults to `true`, and
`capture_logs` defaults to `false`. A positive `latency_trigger_ms` requests
bounded exact-event evidence for slow calls.

Retention is configurable in the same table and is honored by `baml clean`
unless a command-line override is supplied:

```toml
[observability]
history_max_age_days = 30
history_max_bytes = 2147483648
newest_boundary_floor = 20
gc_grace_hours = 24
```

`baml clean --dry-run` reports the deletion/GC plan. `baml clean --all`
removes retained observability artifacts and then runs value-store GC. Value
chunks are reclaimed only after their last retained root is gone and the GC
grace period has elapsed.
