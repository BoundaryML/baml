# Save and resume exact continuation state

> **Status:** Implemented in the executable reference.

Persist opaque provider tokens when the next process must resume exact native
state. Persist `Conversation` when editable, provider-neutral history is enough.

## Save a tool transcript

```baml
let run = ai.drivers.run_agent(task)

match (run) {
  let done: ai.Done<Resolution> => {
    let resumable: ai.ResumableToolCallingProvider = task.$provider
    let token = resumable.save_transcript(done.transcript)
    db.save(ticket.id, baml.json.to_string(token))
  },
  _ => {},
}
```

## Restore it later

```baml
let token = baml.json.from_string<ai.TranscriptToken>(db.load(ticket.id))
let transcript = ToolModel.restore_transcript(token)

let continued = ResolveTicket.task(ticket, $provider = ToolModel)
  .with_transcript(transcript)
```

The owning provider validates token provider, version, and sealed payload.
Applications store the token but do not decode or edit it.

## Save a session resource

```baml
let token = session.token()
db.save(ticket.id, baml.json.to_string(token))

// In a later process, with credentials configured again:
let resumed = SessionModel.resume_session(token)
```

Resources are process-local; tokens cross process boundaries. Credentials and
live provider objects are never serialized into the token.

## Related design


- [Save and restore exact state](../specification/06-conversations-and-transcripts.md#save-and-restore-exact-state)
- [Resource tokens](../specification/07-resources.md#crossing-processes-job-tokens)
