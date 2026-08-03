# Skills

## Skills are not a primitive

A skill is a named bundle of expertise: a description the model always
sees, a body of instructions loaded when needed, and optionally tools
that come with it. Other systems make this a runtime feature. Here it is
a composition of three primitives you already have — a tool, a custom
event, and middleware. This page is the recipe.

## The data

```baml
class Skill {
    name: string,
    description: string,   // always visible to the model
    body: string,          // loaded on demand
    tools: baml.session.Tool[],
}

//# a SKILL.md with frontmatter is just a file to parse
function skill_from_file(path: string) -> Skill {
    let raw = baml.fs.read(path);
    let fm = parse_frontmatter(raw);
    Skill {
        name: fm.get("name") ?? path,
        description: fm.get("description") ?? "",
        body: body_after_frontmatter(raw),
        tools: [],
    }
}
```

## Always-on skills

If the skill should apply to every turn, it is just a string in the
template. No machinery:

```baml
function Reviewer(diff: string) -> Review {
    client: "anthropic/claude-sonnet-5"
    prompt: `
        ${baml.fs.read("skills/review-checklist.md")}
        Review this diff: ${diff}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}
```

Cost: the body occupies context on every call. For a handful of short
skills this is fine, and it should be the first thing you try.

## Loaded on demand

For many skills, or long ones, use progressive disclosure: the model
always sees the catalog (names and descriptions), and pulls in a body
only when relevant. This takes one tool, one event, one middleware.

**1. The event.** Loading a skill appends an event whose rendering is the
skill body. From that point in the conversation, the model has the
instructions — via the same `${ctx.transcript}` as everything else:

```baml
class SkillLoaded {
    name: string,
    body: string,
    implements baml.session.Promptable {
        function to_prompt(self) -> string? { `## Skill: ${self.name}\n${self.body}` }
    }
}
```

**2. The tool.** A closure over the skill list; the catalog lives in its
description, so the model can browse without loading:

```baml
function skill_loader(skills: Skill[]) -> baml.session.Tool {
    let catalog = skills.map((s) -> { `- ${s.name}: ${s.description}` }).join("\n");
    baml.session.tool(
        (name: string) -> {
            if let sk: Skill = skills.find((s) -> { s.name == name }) {
                baml.session.emit(SkillLoaded { name: sk.name, body: sk.body });
                `Skill "${name}" loaded. Follow its instructions.`
            } else {
                `No such skill: ${name}. Available:\n${catalog}`
            }
        },
        name = "load_skill",
        description = `Load a skill for specialized instructions. Available:\n${catalog}`,
    )
}
```

**3. The middleware.** If a skill bundles tools, mount them when it
loads:

```baml
class WithSkills {
    inner: baml.session.Policy,
    skills: Skill[],

    implements baml.session.Policy {
        type Ev = baml.session.Event | SkillLoaded
        function update(self, st: SessionState, j: Journal<Self.Ev>, e: Self.Ev) -> Command[] {
            match (e) {
                let s: SkillLoaded => {
                    if let sk: Skill = self.skills.find((k) -> { k.name == s.name }) {
                        if (sk.tools.length() > 0) {
                            return [MountTools { names: sk.tools.map((t) -> { t.name }) }];
                        }
                    }
                    []
                },
                _ => self.inner.update(st, j, e),
            }
        }
    }
}
```

Wiring it up:

```baml
function CodeAgent(goal: string) -> Report {
    client: "anthropic/claude-sonnet-5"
    prompt: `
        You are a coding agent. Work the task to completion.
        Goal: ${goal}
        ${ctx.transcript}
        ${ctx.output_format}
    `
}

let skills = [
    skill_from_file("skills/review-checklist.md"),
    skill_from_file("skills/release-process.md"),
];

let s = CodeAgent@session(
    goal = t,
    $tools = [read_file, run_bash, skill_loader(skills)],
    $policy = WithSkills { skills: skills, inner: baml.session.ToolLoop { max_steps: 100 } },
);
```

No `CallModel` is needed after loading: the load happened inside a tool
call, so the default loop recalls the model when the tool batch
completes, and the next render includes the body.

## What falls out for free

Because loading is an event, the standard properties apply without
additional work:

- **Audit.** The journal records which skill was loaded, with which exact
  body, at which point. "Did the agent read the checklist before
  approving?" is a journal query, and an eval rubric.
- **Reproducible resume.** The body is captured at load time. Editing the
  skill file later does not silently change a session that already loaded
  it; new sessions pick up the new text.
- **Snapshots and replay.** The body travels in the snapshot; replay
  renders the identical transcript.
- **Compaction.** A loaded body is transcript content like any other;
  `with_compaction` can summarize it out when it stops paying rent.

## Design notes

- **Unloading.** There is no un-render. To retire a skill mid-session,
  append a `Promptable` notice ("skill X no longer applies") and, if it
  mounted tools, `UnmountTools`. Compaction eventually removes the body.
- **Version pinning.** Capturing the body at load time is a deliberate
  choice: the journal describes what the model actually saw. If you need
  sessions to track a live skill file, re-load the skill rather than
  mutating history.
- **Skills with resources.** A skill that ships scripts or reference
  files is a skill whose `tools` read them. Files themselves are a
  sandbox concern, out of scope for this recipe.
