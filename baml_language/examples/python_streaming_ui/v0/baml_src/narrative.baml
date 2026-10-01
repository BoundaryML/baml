// v0 copy of baml_src/narrative.prompt.baml; keep the two in sync.

function WriteNarrative(pr_text: string) -> string {
  client BedrockHaiku45
  prompt #"
    {{ _.role("user") }}
    Write a detailed, roughly 1500-word engineering narrative of this pull request for a changelog:
    what it adds, why, how it is tested, the gaps it found, and what follows up. Plain prose with markdown headings.

    {{ pr_text }}
  "#
}
