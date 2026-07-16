# Save and resume a harness session

Persist a serializable opaque token between HTTP requests. The harness adapter,
not application message reconstruction, owns exact continuation.

## Start

```baml
let session = CodeHarness.open(ai.HarnessOptions {
  cwd: "/workspace",
  permission_mode: "accept-edits",
})

let first = CodeHarness.run<Patch>(
  session,
  FixRepository.task(issue).with_provider(CodeHarness),
)
let token = CodeHarness.save_session(session)
db.save(conversation_id, baml.json.to_string(token))
```

## Continue in another request

```baml
let token = baml.json.from_string<ai.HarnessSessionToken>(
  db.load(conversation_id),
)

let session = CodeHarness.restore_session(token)
let next = CodeHarness.run<Patch>(
  session,
  FixRepository.task("now add a regression test").with_provider(CodeHarness),
)

db.save(conversation_id, baml.json.to_string(CodeHarness.save_session(session)))
```

## Stop versus destroy

```text
stop:    halt current work and retain resumable state
destroy: release the session and make the handle unusable
```

Store provider, adapter version, and sealed continuation in the token. Never
store credentials. The configured adapter validates ownership on restore.

## Related design and scenarios

- Scenarios 40 harness sessions, 42 HTTP-style session continuation
