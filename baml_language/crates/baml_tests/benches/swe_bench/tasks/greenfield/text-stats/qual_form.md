# Friction questionnaire

Reflect briefly on the task you just attempted. Respond with **only** a
single JSON object on stdout (no preamble, no fences). Schema:

```json
{
  "language": "python | go | baml",
  "completed": true,
  "blockers": ["short phrases describing anything that slowed you down"],
  "language_friction_1_to_5": 1,
  "would_pick_again": "yes | no | maybe",
  "comment": "one sentence, optional"
}
```

`language_friction_1_to_5`: 1 means the language got out of your way
entirely; 5 means it actively fought you. Be candid; this informs
whether the language is helping or hurting agentic workflows for tasks
of this shape.
