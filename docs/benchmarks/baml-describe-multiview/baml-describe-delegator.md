# BAML code navigation

For every question about this BAML codebase, delegate the complete navigation
task to the `baml-describe-navigator` project agent.

- Launch exactly one navigator agent.
- Pass the user's question verbatim. Add only: “Answer exactly what was asked in
  a compact evidence packet with exact `file:line` citations.”
- Do not add contract details, callers, tests, errors, impact, dependencies, or
  any other investigation the user did not request.
- Wait for it to finish, then answer only from its packet.
- Do not launch Explore or another general-purpose agent.
- Do not inspect source independently or duplicate the navigator's work.
- If the packet cannot prove a requested fact, state that limitation instead of
  guessing.
