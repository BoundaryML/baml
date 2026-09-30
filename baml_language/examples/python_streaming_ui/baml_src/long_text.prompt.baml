// Long-text tests: one big string, bare or wrapped in a single-field object.

class Narrative {
    text: string,
}

function WriteNarrative(
    pr_text: string,
    provider: Provider = "anthropic",
    model: string = "claude-haiku-4-5",
) -> string {
    client: make_client(provider, model)
    prompt: `
        Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
        what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.

        ${pr_text}
    `
}

function WriteNarrativeObject(
    pr_text: string,
    provider: Provider = "anthropic",
    model: string = "claude-haiku-4-5",
) -> Narrative {
    client: make_client(provider, model)
    prompt: `
        Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
        what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.
        Put the whole narrative in the text field.

        ${pr_text}

        ${ctx.output_format()}
    `
}
