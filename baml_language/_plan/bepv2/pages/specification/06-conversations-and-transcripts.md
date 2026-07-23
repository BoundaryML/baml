# 6. Conversations and Transcripts

Applications and providers both need conversation history, but they own
different forms of it. The application owns portable conversation data. The
provider owns the exact state needed to continue its protocol.

## Messages have a shared view

Providers use different content blocks. BAML exposes a small interface so
drivers can read roles, text, media, and public metadata without knowing the
provider's wire format:

```baml
interface MessagePart {
  function kind(self) -> MessagePartKind throws never
  function text(self) -> string? throws never
  function media(self) -> image | audio | video | pdf | null throws never
  function annotations(self) -> map<string, json> throws never
}

interface Message {
  function role(self) -> MessageRole throws never
  function parts(self) -> MessagePart[] throws never
  function provider_metadata(self) -> json throws never
}

interface Messages {
  function items(self) -> Message[] throws never
  function append(self, message: Message) -> Messages throws never
  function to_conversation(self) -> Conversation throws never
}
```

`Conversation` is the standard serializable form. Application code may edit,
store, compact, and branch it. Provider metadata can be retained for a round
trip, but applications should not depend on its private shape.

## A transcript is exact provider state

```baml
interface Transcript {
  function provider(self) -> Provider throws never
  function messages(self) -> Messages throws never
  function conversation(self) -> Conversation throws never
}
```

A transcript can contain tool-call IDs, reasoning signatures, encrypted
blocks, citations, caches, and remote continuation IDs. Those details may be
required by the next provider request. Do not rebuild a transcript from an
edited message array and assume it is exact.

The ownership rule is:

```text
application: Conversation, UI state, logs, and business data
provider:    exact wire history and private continuation data
driver:      active Transcript and provider during one run
```

## Save and restore exact state

Providers that support durable continuation expose an opaque token:

```baml
class TranscriptToken {
  provider: string,
  version: int,
  sealed: string,
}

interface ResumableToolCallingProvider requires ToolCallingProvider {
  function save_transcript(self, transcript: Transcript) -> TranscriptToken
  function restore_transcript(self, token: TranscriptToken) -> Transcript
}
```

The application stores the token but does not edit it. Restoration happens on
a configured provider, which checks that it owns the token.

## Move history to another provider

Cross-provider transfer is an export/import operation. It reports how much
information was preserved:

```baml
enum TranscriptFidelity { Exact, MessagesOnly, Lossy }

class TranscriptImport {
  transcript: Transcript,
  fidelity: TranscriptFidelity,
  warnings: string[],
}

interface TranscriptImportProvider requires ToolCallingProvider {
  function import_conversation(
    self,
    conversation: Conversation,
  ) -> TranscriptImport throws baml.errors.TranscriptError
}
```

The import must keep normal messages and completed tool-call/result pairs when
the target provider can represent them. Provider-private reasoning, caches,
and continuation handles may be lost. The driver must report that loss instead
of silently calling the transfer exact.

An unresolved tool call cannot be transferred unless policy explains how to
cancel it or supply its result.
