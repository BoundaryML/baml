# 8. Custom Capabilities

Standard drivers cover common lifecycles. If an application needs a genuinely
new operation, it can add a capability interface and a driver in normal BAML.
This page uses moderated generation as the full example.

## First: do you need a capability at all?

Three cheaper options usually apply. Check them in order.

**1. If the shape is still task → response, write a wrapper provider**
([Providers and capabilities](./04-providers-and-capabilities.md)). Moderation
that checks input and output around a normal call does
not change the interaction shape:

```baml
let note = ComposeNote(topic, $provider = Guarded { inner: Fast, policy: Strict })
```

Done — no new capability, and every task and compatible driver works through it
unchanged.

**2. If only one vendor has the operation, call the vendor's method
directly** with a task:

```baml
let out = Vendor.reasoning_tree(Solve.task(problem, $provider = Vendor), branches = 8)
```

Promote to a capability only when a second provider needs the same
contract.

**3. If it is not an LLM task** — listing models, uploading files, deleting
caches — it is an ordinary provider API method. Do not wrap it in a fake
task.

A new capability is warranted only when the **interaction shape or
lifecycle** is genuinely new *and* cross-provider: a different protocol
(turn-taking, polling, duplex), a different error channel, or provider
state with a lifetime. A vendor flag is not a capability. A header is not a
capability. A wrapper is not a capability.

## The worked example: moderated generation

Suppose several providers expose a native moderated-generation endpoint
that returns both the value and a moderation verdict — a genuinely
different response contract.

### Step 1 — declare the interface

```baml
class Verdict {
  flagged: bool,
  categories: string[],
}

class ModeratedResponse<T> {
  value: T,
  verdict: Verdict,
}

class ModerationRefused {
  categories: string[],
  implements baml.errors.Failure {
    function is_retryable(self) -> bool { false }
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { true }
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
  }
}

interface ModeratedGenerationProvider requires ai.Provider {
  function generate_moderated<T>(
    self,
    task: ai.Task<T>,
    policy: string,
  ) -> ModeratedResponse<T> throws ModerationRefused | baml.errors.UnknownError
}
```

Note the error class implements the shared `Failure` model truthfully
([Reliability and errors](./09-reliability-and-errors.md)): a refusal is a
non-retryable policy refusal with no committed
effect. To be
precise about what that buys: any *catch site* and any code that already
handles your capability can triage your error without knowing its concrete
class, and `may_replay` correctly refuses to re-drive it. It does **not**
mean a generic retry wrapper automatically intercepts your capability — a
wrapper participates only in capabilities it implements and forwards
([Reliability and errors](./09-reliability-and-errors.md)); your capability's
*driver* is where retry policy around moderated
generation would live, and it can reuse `may_replay` for the decision.

### Step 2 — implement it on providers

```baml
class AcmeModerated {
  model: string,
  api_key: string,

  implements ai.Provider {}
  implements ai.GenerationProvider { ... }          // it is also a normal provider

  implements ModeratedGenerationProvider {
    function generate_moderated<T>(self, task: ai.Task<T>, policy: string)
        -> ModeratedResponse<T> throws ModerationRefused | baml.errors.UnknownError {
      let messages = task.messages()
      let schema = baml.llm.render_output_format(task.output_type())
      let body = baml.http.send(self._moderated_request(messages, schema, policy))
      if (self._flagged(body)) {
        throw ModerationRefused { categories: self._categories(body) }
      }
      ModeratedResponse<T> {
        value: baml.sap.parse<T>(self._content(body)),
        verdict: self._verdict(body),
      }
    }
  }
}
```

Everything a built-in capability implementation can use — `task.messages()`,
`task.output_type()`, `baml.http.send`, `baml.sap.parse` — is equally
available to yours. There is no privileged stdlib API.

### Step 3 — write the driver

Write the safe driver first, requiring the capability statically:

```baml
function run_moderated<T>(
  provider: ModeratedGenerationProvider,
  task: ai.Task<T>,
  policy: string,
) -> ModeratedResponse<T>
    throws ModerationRefused | baml.errors.UnknownError {
  provider.generate_moderated<T>(task.with_provider(provider), policy)
}
```

If the library wants to support erased/dynamically routed providers, it may
also expose an explicitly runtime-negotiated spelling:

```baml
function unsafe_run_moderated<T>(task: ai.Task<T>, policy: string)
    -> ModeratedResponse<T> {
  match (task.$provider) {
    let p: ModeratedGenerationProvider => run_moderated<T>(p, task, policy),
    _ => throw baml.errors.Unsupported {
      message: "provider has no moderated-generation capability: " + task.provider_name(),
    },
  }
}
```

### Step 4 — use it with any task

```baml
function ComposeNote(topic: string) -> string {
  provider: AcmeSafe
  prompt: `Write a short note about ${topic}.`
}

let task = ComposeNote.task("office move", $provider = AcmeSafe)
let r = run_moderated(AcmeSafe, task, "workplace-strict")
log.info(`flagged: ${r.verdict.flagged}`)
let note = r.value
```

`ComposeNote` did not change. No task grew a member. Any task in any package
— including ones written before your library existed — works with
`run_moderated`, because `.task` is the universal bridge.

## Testing your capability

The same fixture pattern as everything else — a fake provider implementing
your interface:

```baml
class ScriptedModerated {
  reply: string,
  flag: bool,

  implements ai.Provider {}
  implements ModeratedGenerationProvider {
    function generate_moderated<T>(self, task: ai.Task<T>, policy: string)
        -> ModeratedResponse<T> throws ModerationRefused | baml.errors.UnknownError {
      if (self.flag) { throw ModerationRefused { categories: ["scripted"] } }
      ModeratedResponse<T> {
        value: baml.sap.parse<T>(self.reply),
        verdict: Verdict { flagged: false, categories: [] },
      }
    }
  }
}

test "moderated happy path" {
  let fake = ScriptedModerated { reply: "\"a note\"", flag: false }
  let r = run_moderated(fake, ComposeNote.task("x", $provider = fake), "strict")
  assert.equal(r.value, "a note")
}

test "a provider without the capability is a typed Unsupported" {
  let r = unsafe_run_moderated(ComposeNote.task("x", $provider = FakeProvider {
    reply: "\"irrelevant\"",
  }), "strict") catch (e) {
    let u: baml.errors.Unsupported => "unsupported",
    _ => "wrong error",
  }
  assert.equal(r, "unsupported")
}
```

## Stateful custom capabilities

If your capability creates provider-owned state, follow
[Resources](./07-resources.md): return a
resource with lifecycle methods and a `token()`, not an id. The provider
returns its own resource class; users see your resource interface. Nothing
about resources is reserved for the stdlib either.

## What you never wrote

Worth listing, because each item is a place this design refuses to put
complexity: no registration call, no marker comment, no naming convention
the compiler validates, no generated members on anyone's tasks, no global
uniqueness for your capability's name (it is namespaced like every
interface), no SDK-codegen involvement. Your capability is a library:
imported, versioned, and discovered like any other code.

## Alternatives considered

**A capability registry** (mark the interface and driver; the compiler
synthesizes `Foo.moderated(...)` on every task). Rejected: mode names become
program-global; installing a library mutates every task's API surface;
driver signatures become a compiler-validated convention; generated code
grows as tasks × installed drivers; and host SDKs must emit members they
cannot vouch for. The free-driver spelling costs one wrapping call
(`run_moderated(p, ComposeNote.task(x), policy)` vs `ComposeNote.moderated(x, policy)`)
and buys all of that back.

**User-extensible methods on `Task<T>`**
(`ComposeNote.task(x).run_moderated(policy)`). Requires extension
methods or UFCS in the language — a much larger decision than this BEP. If
the language grows them, drivers gain the postfix spelling automatically;
nothing here blocks it.

**Registering into the standard drivers** (make `ai.drivers.drive` consult a
mode table). Rejected: dynamic dispatch through a mutable global table is
exactly the invisible coupling typed drivers exist to avoid.
