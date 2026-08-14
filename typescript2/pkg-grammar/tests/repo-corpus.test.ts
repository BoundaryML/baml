// Tokenize every .baml file in the repository, not just the curated fixtures.
// The snapshot suite catches scope regressions; this catches the failure modes
// that matter for GitHub and other bulk consumers: tokenizer crashes and
// catastrophic-backtracking regexes that make highlighting hang or bail on
// real-world files.

import { execSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { createHighlighter } from "shiki";
import bamlGrammar from "../baml.tmLanguage.json";

const THEME = "github-dark";
// Skip the multi-MB LFS stress files; everything a human would read stays in.
const MAX_BYTES = 256 * 1024;
// Generous per-file wall-clock budget (slow CI runners included). A
// pathological regex blows way past this; normal files tokenize in
// milliseconds.
const PER_FILE_TIMEOUT_MS = 15_000;

const repoRoot = join(import.meta.dirname, "..", "..", "..");

const files = execSync("git ls-files -z -- '*.baml'", {
  cwd: repoRoot,
  maxBuffer: 64 * 1024 * 1024,
})
  .toString("utf8")
  .split("\0")
  .filter(Boolean)
  .filter((path) => {
    try {
      return statSync(join(repoRoot, path)).size <= MAX_BYTES;
    } catch {
      return false;
    }
  });

const highlighter = await createHighlighter({
  themes: [THEME],
  langs: [bamlGrammar as never],
});

describe(`repo corpus (${files.length} .baml files)`, () => {
  it("found a non-trivial corpus", () => {
    expect(files.length).toBeGreaterThan(100);
  });

  for (const file of files) {
    it(
      `tokenizes ${file}`,
      () => {
        const source = readFileSync(join(repoRoot, file), "utf8");
        const { tokens } = highlighter.codeToTokens(source, {
          lang: "baml",
          theme: THEME,
        });
        expect(tokens.length).toBeGreaterThan(0);
      },
      PER_FILE_TIMEOUT_MS,
    );
  }
});
