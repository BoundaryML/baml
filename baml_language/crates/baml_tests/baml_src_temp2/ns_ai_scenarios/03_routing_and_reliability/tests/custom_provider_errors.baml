// A user-defined provider throwing a user-defined error, caught by the USER'S
// code in a typesafe way. The `ai` namespace never learns about
// VendorQuotaExceeded: it rides the `root.ai.Failure | baml.errors.UnknownError`
// channel because it implements the interface, retry/fallback classify it via
// is_transient()/effects() without knowing the concrete type, and the app
// recovers the concrete type at the catch site.

class VendorQuotaExceeded {
  quota_name: string,
  transient: bool,
}

implements root.ai.Failure for VendorQuotaExceeded {
  function is_transient(self) -> bool throws never { self.transient }
  function effects(self) -> root.ai.Effects throws never { root.ai.Effects.None }
}

// A user's provider wrapper: throws the vendor error for the first
// `failures_remaining` calls, then delegates to the inner fake.
class QuotaLimitedProvider {
  inner: root.ai.testing.FakeProvider,
  failures_remaining: int,
  transient: bool,
  calls: int,

  implements root.ai.Provider {
    function name(self) -> string throws never { "quota-limited" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements root.ai.CompletionProvider {
    function complete<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T>
        throws root.ai.Failure | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      if (self.failures_remaining > 0) {
        self.failures_remaining = self.failures_remaining - 1;
        throw VendorQuotaExceeded { quota_name: "daily", transient: self.transient };
      }
      self.inner.complete<T>(task.with_provider(self.inner))
    }
    function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
      root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Safe, idempotency_key: null }
    }
  }
}

test "guide 03.07: a terminal custom error surfaces through retry with its type intact" {
  let limited = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 99, transient: false, calls: 0,
  };
  //# The app catches the CONCRETE user type, not a stringly wrapper
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(limited, 3)) catch (e) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    let failure: root.ai.Failure => "other-failure",
    let unknown: baml.errors.UnknownError => "unknown",
  };
  assert.equal(label, "quota:daily")
  //# Terminal per its own classification: retry must not replay it
  assert.equal(limited.calls, 1)
}

test "guide 03.07: a transient custom error is replayed like any classified failure" {
  let limited = QuotaLimitedProvider {
    inner: fake_resolution(), failures_remaining: 1, transient: true, calls: 0,
  };
  let value = ResolveTicket_manual(sample_ticket(), root.ai.retry(limited, 2));
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
  let label = ResolveTicket_manual(
    sample_ticket(),
    root.ai.fallback([first, second]),
  ) catch (e) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    _ => "erased",
  };
  assert.equal(label, "quota:daily")
  assert.equal(first.calls, 1)
  assert.equal(second.calls, 1)
}

test "guide 03.07: unknown failures accumulate breadcrumbs through wrapping layers" {
  let failed = root.ai.testing.FakeProvider {
    ...fake_resolution(),
    failures_remaining: 1,
    failure_mode: root.ai.testing.FakeFailureMode.Unknown,
  };
  let label = ResolveTicket_manual(sample_ticket(), root.ai.retry(failed, 3)) catch (e) {
    //# One breadcrumb from the fake, one from the retry layer that wrapped it
    let unknown: baml.errors.UnknownError =>
      unknown.message.length().to_string() + ":" + (unknown.message.at(1) ?? ""),
    _ => "unexpected",
  };
  assert.equal(label, "2:retry(fake)")
}

test "guide 03.07: UnknownError.from recovers a wrapped domain error typesafely" {
  //# A blind layer wrapped the vendor error; the handler reasserts its channel
  let wrapped = baml.errors.UnknownError.with_message<never>(
    VendorQuotaExceeded { quota_name: "burst", transient: false },
    "transport layer",
  );
  let label = match (baml.errors.UnknownError.from<VendorQuotaExceeded>(wrapped)) {
    let quota: VendorQuotaExceeded => "quota:" + quota.quota_name,
    let unknown: baml.errors.UnknownError => "still-unknown",
  };
  assert.equal(label, "quota:burst")
}
