# Writing style for this BEP

Rules for writing and editing pages in this BEP. Apply them to new pages
and to any page being revised.

## Voice

- Write complete declarative sentences. State what is true; do not
  persuade, perform, or hedge.
- One claim per sentence. Prefer a period over an em-dash chain.
- No aphoristic fragments or punch-lines. Wrong: "Three lifetimes,
  three places." Right: "The runner, the options, and the task have
  different lifetimes, so they are configured in different places."
- No colon-fragment setups. Wrong: "The test: a UI built only on the
  journal tail is correct." Right: "A UI built only on the journal tail
  renders correct state."
- No editorializing parentheticals or asides ("the whole point",
  "stated plainly", "incoherent when shared"). Put the content in a
  plain sentence or delete it.
- No metaphors, wordplay, or dramatic emphasis. Plain technical prose
  in the style of Google developer documentation.
- Second person for instructions ("Add the tool to the list"), present
  tense for behavior ("The runner appends the events").

## Recording decisions

- Guides state behavior. Arguments and alternatives live in
  `pages/05_appendix/02_alternatives_considered.md`.
- Write decisions in the register of reporting something settled, not
  winning an argument. A document distilled from a design discussion
  must not keep the discussion's voice.
- Rejections follow one flat pattern: "Rejected: X. [reason].
  [reason]." Name the cost of the chosen option honestly.
- When a claim depends on a constraint, state the constraint in place.
  Wrong: "`Job` must not expose `send`." Right: "A job runs detached,
  with no caller attached to converse, so `Job` has no `send`."

## Code samples

- Every page that calls an agent shows the LLM function it calls, on
  that page.
- Samples follow current BAML syntax: fields `name: type,`, snake_case
  methods, keyword arguments with `=`, `if/else` as the expression form.
- Use `//#` comments only for annotations a reader needs; do not
  narrate every line.
- Keep one running example domain per page where possible. The default
  is the travel agent (`PlanTrip -> Itinerary`).

## Structure

- Directories and files carry numeric prefixes for ordering
  (`01_introduction/`, `02_sessions.md`). A directory's overview file is
  `readme.md`, unnumbered.
- Headers are sentence case ("## The two lanes").
- Cross-reference other pages by relative path in backticks
  (`../02_guides/10_journal.md`). Do not restate another page's
  content; link to it.
- `outline.md` mirrors every page and its H2 headers. Update it in the
  same change that adds or renames a page or header.

## Terminology

- Use the glossary in `pages/01_introduction/03_concepts.md` and no
  synonyms: journal (not log or history), transcript (the rendered
  view), client (not provider, except as the `provider:` field inside a
  client), policy, command, runner, session, task, job, turn.
- Event and type names as defined: `UserMessage`, `AssistantMessage`,
  `Done<T>`, `Replied`. Verbs as defined: `send` and `interrupt` from
  outside, `emit` only ambient inside tools.
