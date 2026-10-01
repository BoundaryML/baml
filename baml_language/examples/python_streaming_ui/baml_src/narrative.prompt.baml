// String workload: one long piece of prose about the same pull request.

function WriteNarrative(pr_text: string, scenario: int) -> string {
    client: scenario_client(scenario)
    prompt: `
        ${role("user")}
        Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
        what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.

        ${pr_text}
    `
}
