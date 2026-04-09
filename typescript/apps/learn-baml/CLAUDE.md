# Documentation Writing Guide for Learn BAML

This guide teaches how to write effective documentation for the Learn BAML Docusaurus site. Follow these principles to create consistent, high-quality documentation that serves both humans and AI agents.

---

## The Core Thesis

**Learn BAML is a product surface, not a collection of pages.**

Documentation succeeds when:
- New users get to first success fast (5-minute wow)
- Intermediate users can get work done without friction
- Advanced users can trust the reference
- The team can keep everything current as features ship
- Humans *and* agents can treat it as canonical

---

## Diátaxis: The Organizing Principle

Diátaxis defines four distinct documentation types. **Mixing them is why docs rot.**

| Type | Purpose | User Need | Writing Style |
|------|---------|-----------|---------------|
| **Tutorial** | Learn by doing | "Teach me" | Guided, hand-holding |
| **How-to** | Task recipes | "Help me do X" | Goal-oriented, minimal |
| **Reference** | Complete facts | "What does X do?" | Precise, scannable |
| **Explanation** | The "why" | "Help me understand" | Conceptual, tradeoffs |

### The Golden Rule

**When a page starts drifting, don't add paragraphs—add links.**

Each page should do ONE thing well. If you find yourself explaining background in a reference page, link to a concepts page instead.

### BAML's Diátaxis Mapping

| Section | Type | URL Pattern | Examples |
|---------|------|-------------|----------|
| Tour | Tutorial | `/tour/*` | Interactive intro modules |
| Tutorials | Tutorial | `/tutorials/*` | "Build a data extractor" |
| How-to | How-to | `/how-to/*` | "Switch LLM providers" |
| Cookbook | How-to | `/cookbook/*` | Code-first recipes |
| Concepts | Explanation | `/concepts/*` | "Schema-Aligned Parsing", "What belongs in BAML" |
| Reference | Reference | `/reference/*` | Types, Functions, CLI, SDK |

---

## Tone and Voice

### The BAML Voice

Sound like **Rust docs meets TypeScript UX**:
- Crisp, direct, technically grounded
- Respectful of the reader's time and intelligence
- Friendly and ergonomic—"this just fits into your world"
- Show real code and real outputs early

### Confidence Without Hype

| Do | Don't |
|----|-------|
| "designed to" / "built for" | "guarantees" / "always" |
| Show screenshots/code | Use vague adjectives |
| Ground metaphors in examples | Make bold claims without proof |
| "robust to imperfect outputs" | "perfect structured output" |

### Safe Phrasing Examples

```markdown
<!-- GOOD -->
"BAML is designed for incremental adoption—add .baml files to an existing repo."
"Schema-Aligned Parsing heals common failure modes in model output."
"BAML brings language-level rigor to AI features: typed interfaces, tests, and reliable parsing."

<!-- BAD -->
"BAML is the only language that can do X"
"Guaranteed structured output from any model"
"BAML is the Rust of AI"
```

---

## What BAML Is (Positioning)

When writing docs, keep this positioning in mind:

> BAML is a language for the AI boundary: write typed LLM functions (with tests) and call them from your existing codebase in any language. BAML's runtime (Schema-Aligned Parsing, retries/strategies, streaming) turns probabilistic model text into reliable, typed values.

### The Three Pillars

1. **Incremental adoption** - Add BAML without rewriting your app
2. **The loop** - Write → run → see → iterate (the product experience)
3. **Reliability runtime** - SAP, streaming, retries built-in

### What Belongs Where

This is critical for users to understand:

| Put in BAML | Keep in Host Language |
|-------------|----------------------|
| AI contracts (types + prompts) | Business logic |
| Tests and assertions | Side effects (DB, API calls) |
| Retry/fallback strategies | Application state |
| Type definitions for LLM output | Orchestration beyond LLM calls |

---

## Page Templates by Diátaxis Type

### Tutorial Template

```markdown
# Build [X] with BAML

What you'll build: [one sentence]

## Prerequisites
- [list]

## Step 1: [Action]
[Instructions with expected output]

## Step 2: [Action]
[Instructions with expected output]

...

## What You Built
[Summary of accomplishment]

## Next Steps
- [Link to related tutorial]
- [Link to reference]
```

### How-to Template

```markdown
# How to [Do X]

[One sentence problem statement]

## Prerequisites
- [minimal list]

## Steps

1. [Step with code]
2. [Step with code]

## Common Pitfalls
- [Issue and fix]

## See Also
- [Reference link]
- [Concepts link]
```

### Reference Template

```markdown
# [Thing Name]

[One sentence: what it is]

```[language]
[Minimal working example]
```

## Syntax / Signature

[Code block or table]

## Parameters / Options

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| ... | ... | ... | ... |

## Examples

[Additional examples for edge cases]

## See Also
- [Related reference]
```

### Concepts Template

```markdown
# [Concept Name]

## What It Is
[Brief explanation]

## Why It Matters
[Problem it solves]

## How It Works
[Mental model, with diagrams if helpful]

## Tradeoffs
[What you gain, what you give up]

## See Also
- [Tutorial that uses this]
- [How-to that applies this]
- [Reference for details]
```

---

## Writing Philosophy

### 1. Concise Over Comprehensive

Write the minimum needed to be useful. Users want to solve problems, not read essays.

**Do:**
- Lead with the most common use case
- Use tables for API references
- Put complex examples in collapsible sections

**Don't:**
- Explain every edge case upfront
- Write long intros before showing code
- Repeat information (link instead)

### 2. Code-First Documentation

Show, don't tell. Code examples are worth more than explanations.

**Good:**
```markdown
## Usage

```python
from baml_client import b

result = await b.ExtractName("John Smith is a developer")
print(result)  # "John Smith"
```
```

**Bad:**
```markdown
## Usage

To use the ExtractName function, you first need to import the baml_client
module. Then you can call the function with a string parameter...
[50 more words before any code]
```

### 3. The 5-Second Rule

Can a reader find what they need in 5 seconds?

- Lead with code examples
- Use tables instead of paragraphs
- Link to details instead of inlining

---

## Messaging Guardrails

From the v1 release plan—what NOT to over-claim:

| Avoid | Prefer |
|-------|--------|
| "Only language that can X" | "Designed for X" |
| Leading with agents/agentic | "Build agent loops in host language; BAML makes each step reliable" |
| "Guaranteed structured output" | "Robust to imperfect outputs" |
| "Full backend language" | "Language for the AI boundary" |

### The "New Language" Objection

Handle this upfront. Users will bounce if they think BAML requires rewriting their app.

**Answer:** We're not asking you to rewrite your app; we're giving you a better unit of composition for AI calls.

**Frame it as:**
- LLM calls aren't APIs; they're probabilistic programs
- The pain is in the *boundary*: schema drift, prompt drift, tests, parsing
- BAML makes that boundary a first-class language surface

---

## File Structure

### Frontmatter (Required)

```yaml
---
sidebar_position: 1
sidebar_label: Short Label
title: Full Page Title
description: One-line description for SEO
---
```

### Standard Page Structure

```markdown
---
sidebar_position: N
sidebar_label: Label
title: Title
description: Description
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Title

One-sentence description.

## Quick Example

[Minimal working code]

## API Reference

[Tables]

## Examples

[Additional examples]

## See Also

- [Related Page](./path) - Brief description
```

---

## MDX Syntax Rules

### Multi-Language Examples

```markdown
<Tabs>
<TabItem value="python" label="Python">

```python
from baml_client import b
result = await b.MyFunction(input)
```

</TabItem>
<TabItem value="typescript" label="TypeScript">

```typescript
import { b } from './baml_client'
const result = await b.MyFunction(input)
```

</TabItem>
</Tabs>
```

### Admonitions (Use Sparingly)

```markdown
:::note
Neutral information.
:::

:::tip
Helpful suggestion.
:::

:::warning
Could cause problems if ignored.
:::

:::danger
Will cause errors or data loss.
:::
```

Max 2-3 admonitions per page.

### Tables for API Reference

```markdown
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `timeout` | `int` | `30000` | Timeout in ms |
```

### Collapsible Sections

```markdown
<details>
<summary>Advanced Configuration</summary>

Content most users don't need...

</details>
```

---

## Common Pitfalls

### MDX Parse Errors

**Problem:** Jinja `{{ }}` breaks MDX in tables.

**Solution:** Put Jinja in fenced code blocks, never inline in tables.

```markdown
<!-- BAD -->
| Variable | Example |
|----------|---------|
| `ctx.output_format` | {{ ctx.output_format }} |

<!-- GOOD -->
| Variable | Description |
|----------|-------------|
| `ctx.output_format` | Auto-generated format instructions |

Example:
```baml
prompt #"
    {{ ctx.output_format }}
"#
```
```

### Broken Links

- Only link to pages that exist
- Use relative paths: `./sibling` or `../parent/page`
- Run `npx docusaurus build` to catch broken links

### Over-Documentation

Apply the 5-second rule. If you can't find the answer in 5 seconds, restructure.

---

## Component Conversions (Fern → Docusaurus)

| Fern | Docusaurus |
|------|------------|
| `<CodeBlocks>` | `<Tabs>` + `<TabItem>` |
| `<Accordion>` | `<details>` + `<summary>` |
| `<Info>` | `:::info` |
| `<Warning>` | `:::warning` |
| `<Tip>` | `:::tip` |
| `<Note>` | `:::note` |
| `<ParamField>` | Markdown table |
| `<Card>` | Markdown link or table |

---

## Quality Checklist

Before submitting:

- [ ] Frontmatter complete (sidebar_position, sidebar_label, title, description)
- [ ] Page does ONE thing (matches Diátaxis type)
- [ ] Starts with code example or clear one-liner
- [ ] API params in tables, not prose
- [ ] Multi-language examples use `<Tabs>`
- [ ] No Jinja in table cells
- [ ] All links relative and valid
- [ ] Max 2-3 admonitions
- [ ] Build passes: `npx docusaurus build`
- [ ] No over-claiming (follows messaging guardrails)

---

## Agent Surface Contract (Required)

When docs change, keep the agent-facing surface reliable.

### Required Build/Serve Workflow

- Use `pnpm build` to generate post-build artifacts.
- Use `pnpm serve` (not `pnpm start`) to validate built artifacts.
- `pnpm start` runs source-mode dev server and may not expose `llms.txt` artifacts.

### Required Checks

- `pnpm build`
- `pnpm check:agent-surface`
- Verify `llms.txt` includes high-signal routes:
  - `/agent-start-here`
  - `/tour/hello-baml`
  - `/tutorials/getting-started`
  - `/reference/baml-syntax`

### If You Add A New Top-Level Section

- Update `llmsTxt.sections` in `docusaurus.config.js`
- Add at least one "start here" page for that section
- Add cross-links from at least two existing pages

### Monthly Maintenance

- Confirm `/llms.txt` and `/llms-full.txt` are reachable in production
- Confirm `robots.txt` and `sitemap.xml` are reachable
- Run agent quality checks and fix broken/stale routes

---

## Anti-Patterns

### Wall of Text

```markdown
<!-- BAD -->
# Configuration

The configuration system in BAML provides a flexible way to customize...
[500 words before any code]
```

### Redundant Explanations

```markdown
<!-- BAD -->
### timeout
The timeout parameter specifies the timeout duration. This is the amount
of time that the function will wait before timing out. The timeout is
specified in milliseconds...
```

### Mixing Concerns

```markdown
<!-- BAD: Tutorial mixed with reference -->
# Image Type

First, let's understand what images are...
[Background explanation]

Now let's build a complete application...
[Full tutorial]

Oh, and here's the API reference...
```

**Fix:** Split into three pages (concepts, tutorial, reference) and link between them.

---

## Examples of Good Documentation

### Good: Concise Reference

```markdown
# Image.from_url

Creates an Image object from a URL.

```python
from baml_py import Image
img = Image.from_url("https://example.com/photo.png")
```

## Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `url` | `str` | Yes | Image URL |
| `media_type` | `str` | No | MIME type (auto-detected) |

## Returns

`Image` - For use in BAML functions.

## See Also

- [Image.from_base64](./from-base64)
```

### Good: Focused How-to

```markdown
# How to Switch LLM Providers

Change your LLM provider without modifying your BAML functions.

## Steps

1. Update the client definition:

```baml
client<llm> MyClient {
  provider anthropic  // was: openai
  options {
    model "claude-sonnet-4-20250514"
  }
}
```

2. Set the API key:

```bash
export ANTHROPIC_API_KEY=your-key
```

That's it. Your functions now use Anthropic.

## See Also

- [Client Configuration](/reference/clients/overview)
- [Provider Reference](/reference/clients/providers/anthropic)
```

---

## Summary

1. **Know your Diátaxis type** - Tutorial, How-to, Reference, or Explanation
2. **Be concise** - Less is more
3. **Lead with code** - Show before explaining
4. **One page, one job** - Don't mix types
5. **Link don't repeat** - Reference other pages
6. **Follow the voice** - Confident without hype
7. **Test your build** - Catch errors early

---

## Appendix: Inspiration Sources

These docs exemplify what we're aiming for:

- [Rust Learn Router](https://www.rust-lang.org/learn) - Clear learning paths
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/) - Code-first learning
- [Go Tour](https://go.dev/tour/) - Interactive 5-minute wow
- [Gleam Tour](https://tour.gleam.run/) - In-browser learning
- [pkg.go.dev](https://pkg.go.dev/) - Scannable reference
- [Diátaxis](https://diataxis.fr/) - The organizing principle
