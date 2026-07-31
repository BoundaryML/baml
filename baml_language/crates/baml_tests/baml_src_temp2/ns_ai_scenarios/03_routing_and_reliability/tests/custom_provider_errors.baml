// A user-defined provider throwing a user-defined error, caught by the USER'S
// code in a typesafe way. The `ai` namespace never learns about
// VendorQuotaExceeded: it rides the `ai.Failure | baml.errors.UnknownError`
// channel because it implements the interface, retry/fallback classify it via
// effects() without knowing the concrete type, and the app
// recovers the concrete type at the catch site.

class VendorQuotaExceeded {
  quota_name: string,
  transient: bool,
}

implements ai.Failure for VendorQuotaExceeded {
  function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

// A user's provider wrapper: throws the vendor error for the first
// `failures_remaining` calls, then delegates to the inner fake.
class QuotaLimitedProvider {
  inner: ai.testing.FakeProvider,
  failures_remaining: int,
  transient: bool,
  calls: int,

  implements ai.Provider {
    function name(self) -> string throws never { "quota-limited" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements ai.AgentProvider {
    function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws ai.Failure {
      self.inner.begin<T>(task.with_provider(self.inner))
    }

    function step<T>(
      self,
      conversation: ai.Conversation,
      tools: ai.tools.Tool[],
    ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      if (self.failures_remaining > 0) {
        self.failures_remaining = self.failures_remaining - 1;
        throw VendorQuotaExceeded { quota_name: "daily", transient: self.transient };
      }
      self.inner.step<T>(conversation, tools)
    }

    function submit(
      self,
      conversation: ai.Conversation,
      results: ai.tools.ToolResult[],
    ) -> ai.Conversation throws ai.Failure {
      self.inner.submit(conversation, results)
    }
  }
}

test "guide 03.07: the caller's retry_if judgment surfaces a custom error un-replayed" {
  let limited = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 99, transient: false, calls: 0,
  };
  //# Retryability is the app's judgment — here it reads its own error data
  let judgment = (failure: ai.Failure) -> bool {
    match (failure) {
      let quota: VendorQuotaExceeded => quota.transient,
      _ => true,
    }
  };
  //# The app catches the CONCRETE user type, not a stringly wrapper
  let label = ResolveTicket@task(sample_ticket()).with_provider(ai.retry(limited, 3, retry_if = judgment)).complete(runner = ai.run.Agent<Resolution>.new()) catch (e) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    let failure: ai.Failure => "other-failure",
    let unknown: baml.errors.UnknownError => "unknown",
  };
  assert.equal(label, "quota:daily")
  //# The judgment said no: retry must not replay it
  assert.equal(limited.calls, 1)
}

test "guide 03.07: a transient custom error is replayed like any classified failure" {
  let limited = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 1, transient: true, calls: 0,
  };
  let value = ResolveTicket@task(sample_ticket()).with_provider(ai.retry(limited, 2)).complete(runner = ai.run.Agent<Resolution>.new());
  assert.equal(value.category, "billing")
  assert.equal(limited.calls, 2)
}

test "guide 03.07: fallback exhaustion rethrows the last REAL failure, typed" {
  //# Both members fail with the vendor error; exhaustion must not erase it
  let first = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 99, transient: true, calls: 0,
  };
  let second = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 99, transient: true, calls: 0,
  };
  let label = ResolveTicket@task(sample_ticket()).with_provider(ai.fallback([first, second])).complete(runner = ai.run.Agent<Resolution>.new()) catch (e) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    _ => "erased",
  };
  assert.equal(label, "quota:daily")
  assert.equal(first.calls, 1)
  assert.equal(second.calls, 1)
}

test "guide 03.07: unknown failures accumulate breadcrumbs through wrapping layers" {
  let failed = ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: ai.testing.FakeFailureMode.Unknown,
  };
  let label = ResolveTicket@task(sample_ticket()).with_provider(ai.retry(failed, 3)).complete(runner = ai.run.Agent<Resolution>.new()) catch (e) {
    // One breadcrumb from the fake, one from the retry step that wrapped it.
    let unknown: baml.errors.UnknownError =>
      unknown.message.length().to_string() + ":" + (unknown.message.at(1) ?? ""),
    _ => "unexpected",
  };
  assert.equal(label, "2:retry(fake) step")
}

test "guide 03.07: UnknownError preserves a wrapped domain error as structured data" {
  //# A blind layer records the vendor error without flattening it to text
  let wrapped = baml.errors.UnknownError {
    data: VendorQuotaExceeded { quota_name: "burst", transient: false },
    message: ["transport layer"],
  };
  let label = match (wrapped.data) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    _ => "still-unknown",
  };
  assert.equal(label, "quota:burst")
}
