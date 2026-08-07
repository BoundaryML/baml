# Writing style for this BEP

Rules for writing and editing pages in this BEP. Apply them to new pages
and to any page being revised.

## Voice

- Write complete declarative sentences. State what is true; do not
persuade, perform, or hedge.
- One claim per sentence. Prefer a period over an em-dash chain.
- No aphoristic fragments or punch-lines. Wrong: "Three lifetimes,
three places." Right: "The runner, the options, and the spec have
different lifetimes, so they are configured in different places."
- No colon-fragment setups. Wrong: "The test: a client built only on
the journal is correct." Right: "A client built only on the journal
renders a correct request."
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
Wrong: "A client must not hold conversation state." Right: "Every
request is rebuilt from the journal, so a client holds no
conversation state."

## Code samples

- Every page that runs an agent shows the LLM function it runs, on
that page.
- Annotate `let` bindings whose type is not obvious from the right-hand
side. Always annotate specs and results (`FunctionSpec<Itinerary>`,
`RunResult<Itinerary>`) and any expression whose type depends on the
runner. Write `let spec: FunctionSpec<Itinerary> = ...`, not a
trailing comment. Generic arguments at call sites may stay inferred;
the binding is where a reader learns what they are holding.
- Samples follow current BAML syntax: fields `name: type,`, snake_case
methods, keyword arguments with `=`, `if/else` as the expression form.
- Use `//` comments only for annotations a reader needs; do not
narrate every line.
- Keep one running example domain per page where possible. The default
is the travel agent (`PlanTrip -> Itinerary`).
- Note we currently dont implement desugaring of llm functions into
other things so we have to write the desugared functions manually for
now.

## Structure

- Directories and files carry numeric prefixes for ordering
(`01_introduction/`, `02_tools.md`). A directory's overview file is
`readme.md`, unnumbered.
- Headers are sentence case ("## The turn loop").
- Cross-reference other pages by relative path in backticks
(`../02_guides/04_the_journal.md`). Do not restate another page's
content; link to it.
- `outline.md` mirrors every page and its H2 headers. Update it in the
same change that adds or renames a page or header.
- Pages follow Diátaxis. `01_introduction/01_getting_started.md` is a
tutorial. `02_guides/` pages explain one topic each and show how to
use it. `03_how_to/` pages accomplish one task each; they may repeat
a guide's statements but never introduce behavior. `04_reference/`
pages enumerate the API without narrative. `02_why.md`,
`03_concepts.md`, and `05_appendix/` are explanation.

## Terminology

- Use the glossary in `pages/01_introduction/03_concepts.md` and no
synonyms: journal (not log or history), transcript (the journal as
rendered for a model), client (not provider, except in the phrase
"provider wire API" and the registry prefix), spec (not task), runner,
run, turn, tool, content block.
- Event and type names as defined: `AssistantMessage`, `ToolRequested`,
`ToolCompleted`, `RunResult<Out>`. A model turn is one `invoke` call;
a run is one `Runner.run` call.
