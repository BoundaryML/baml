# Concepts

This page defines the pieces and how they fit together. The guides assume
this vocabulary.

## The pieces

A running agent is a **session**. A session is built from five parts:

```
Session
├── Journal   the data: an append-only log of typed events
├── Client    the codec and transport for one provider (a client<llm>)
├── Toolbox   the functions the model may call
├── Policy    pure logic that decides what happens next
└── Runner    the loop that executes decisions and performs all IO
```

One model turn works like this:

1. The **client** renders the journal into a provider-native request (the
   **transcript**) and sends it.
2. The provider's response comes back as **events**: an assistant message,
   tool call requests, token usage.
3. The runner appends the events to the **journal** and hands each one to
   the **policy**.
4. The policy returns **commands**: call the model again, run a tool, wait
   for input, finish.
5. The **runner** executes the commands. Tool results become new events.
   Repeat.

You do not write this loop. Calling an LLM function runs it.

## Who owns what

**The journal owns all data.** The conversation is the journal.

- The **client** is stateless. Every request is built fresh from the
  journal. Clients hold configuration, never history.
- The **policy** is pure. Its working state is a cache derived from the
  journal.
- The **runner** holds nothing.

This is why a session serializes to a single string, resumes on another
machine, and can move between providers: there is exactly one thing to
save.

## The two laws

**1. The function defines the turn; the policy defines the session.**
An LLM function is a static template: prompt shape, return type, initial
tools. Everything that changes during a session — mounted tools, injected
messages, budgets, modes — changes imperatively, in the policy, through
commands, and is recorded in the journal.

**2. Two lanes: data queues, control preempts.**
Messages queue; the policy decides when the model sees them. Interrupts
preempt immediately and are recorded after they take effect. Control never
waits behind data.

## Glossary

| Term | Meaning |
|---|---|
| **Event** | A fact that happened: a user message, a tool result, a usage report. |
| **Journal** | The append-only log of events. One per session. The source of truth. |
| **Transcript** | A provider-native rendering of a journal. A view, not storage. |
| **Client** | A `client<llm>`: codec plus transport for one provider wire format. |
| **Tool** | A BAML function the model may call. |
| **Toolbox** | The set of tools currently mounted on a session. |
| **Task** | A one-shot run: call the function, loop until it returns its type. |
| **Job** | A task running in the background, addressed by a handle. |
| **Session** | A long-lived run: turns over time, with state you can save. |
| **Turn** | The result of one `run()`: `Done<T>` or `Said`. |
| **Policy** | Pure logic: `(state, journal, event) -> commands`. |
| **Command** | What a policy wants done: call the model, run a tool, wait, finish. |
| **Runner** | The loop that executes commands and performs all IO. |
