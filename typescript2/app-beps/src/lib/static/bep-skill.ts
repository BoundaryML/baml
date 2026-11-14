/**
 * BEP Skill for Claude Code - embedded as a string constant.
 */
export const BEP_SKILL = `---
name: bep
description: Work with BAML Enhancement Proposals — create, edit, review, or push BEPs.
user-invocable: true
allowed-tools: Bash Read Edit Write Grep Glob Agent
argument-hint: [number-or-title]
---

You are working in a BEP (BAML Enhancement Proposal) repository. Use the \`./bep\` CLI for all operations.

## CLI Reference

\`\`\`bash
./bep pull              # Fetch all BEPs from server, diff, prompt to apply
./bep list              # List local BEPs (--status STATUS, --json)
./bep new "Title"       # Scaffold BEP-?-slug/ locally
./bep diff <ref>        # Diff local vs server (--full for unified diff)
./bep push <ref>        # Dry-run diff + prompt to push (-y to confirm)
./bep open <number>     # Open in browser
./bep comments <number> # Fetch all comments for a BEP (current version)
./bep reply <number> <comment-id> "message"  # Reply to a comment
\`\`\`

\`<ref>\` is a BEP number (e.g. \`41\`) or slug (e.g. \`my-feature\` for \`BEP-?-my-feature\`).

## What to do

If \`$ARGUMENTS\` is a number, read that BEP's README.md and ask what the user wants to change.

If \`$ARGUMENTS\` is a title/topic, create a new BEP:
1. Run \`./bep new "$ARGUMENTS"\`
2. Read good reference BEPs (marked with \`isGoodReference\` in meta.json) for style
3. Write the README.md following the structure below
4. Run \`./bep diff <slug>\` to preview, then ask if user wants to push

If no arguments, ask what the user wants to do.

## Working with Comments

To review and respond to feedback on a BEP:

1. **Fetch comments**: \`./bep comments <number>\` to see all comments with context
2. **Review concerns**: Look for ⚠️ concerns and ❓ questions that need addressing
3. **Reply to feedback**: \`./bep reply <number> <comment-id> "Your response"\`

Comment types:
- \`discussion\` - General feedback (default)
- \`concern\` - Blocking issue that needs resolution
- \`question\` - Needs clarification

## BCS design proposals (BoundaryML team only)

BCS proposals are Boundary Cloud system designs, kept apart from BEPs. Every \`./bep bcs\` command needs \`BCS_TOKEN\`, which a team member gets at /bcs/token on the site.

\`\`\`bash
./bep bcs pull                      # Fetch every BCS proposal into BCS/
./bep bcs push <number> -n "note"   # Save local edits as a new version
./bep bcs comments <number>         # Open threads, with each commented block's Markdown source
./bep bcs reply <number> <id> "text" --agent claude   # Reply to a thread as the user's agent
\`\`\`

**When you, an agent, reply on a BCS proposal, always pass \`--agent\` with your own name**: \`claude\`, \`codex\`, \`gemini\`, \`cursor\`, \`copilot\`, or another short name. Claude Code is detected on its own. The site then shows your logo next to the user's picture with "via <agent>", and Slack says "<user>'s agent replied". Without it, the reply looks as if the user typed it. Don't add your own "on behalf of" line: the label already says it. Get the user's OK before posting.

## BEP Writing Style

Write like a [PEP](https://peps.python.org/) or [TC39 proposal](https://github.com/tc39/proposals).

Structure:

\`\`\`markdown
# Title

## Summary

Brief description + code example showing the feature.

## Prior Art

Other languages, related work.

## Proposed Design

Detailed technical design — the heart of the BEP.

## Design Tradeoffs

Alternatives considered and why this approach wins.

## Open Questions

Unresolved decisions, future work.
\`\`\`

Be concrete. Lead with code examples. Avoid hand-waving.
`;
