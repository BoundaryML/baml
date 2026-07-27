# Create a resource

Return a resource when an operation remains alive after the initial call.

## Utilities used

| Utility | Purpose |
| --- | --- |
| Resource class | Stores live provider coordinates |
| `close()` | Explicit, idempotent release |
| `cleanup()` | Garbage-collection fallback |
| Runner associated output | Returns the exact resource type |

## Example

```baml
class Report {
  summary: string,
}

function BuildReport(topic: string) -> Report {
  provider: AcmeModel
  prompt: `
    Build a detailed report.

    ${topic}

    ${ctx.output_format}
  `
}

class AcmeJob<T> {
  id: string,
  provider: AcmeProvider,
  closed: bool,

  function poll(self) -> ai.Response<T>? {
    acme_poll<T>(self.provider, self.id)
  }

  function cancel(self) -> null {
    acme_cancel(self.provider, self.id)
  }

  function close(self) -> null {
    if (!self.closed) {
      self.cancel();
      self.closed = true;
    }
    null
  }

  function cleanup(self) -> null {
    self.close()
  }
}

let job = BuildReport.task("battery recycling").run(
  runner = AcmeBackground<Report>.new(),
);

defer { job.close() }
```

The resource owns polling, cancellation, and cleanup after submission. The
runner owns only the transition from task to resource.

[Back to build your own](../build-your-own.md)
