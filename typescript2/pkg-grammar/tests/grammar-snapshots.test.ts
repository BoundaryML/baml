import { mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { describe, expect, it } from "vitest";
import { createHighlighter, type ThemedToken } from "shiki";
import bamlGrammar from "../baml.tmLanguage.json";

const THEME = "github-dark";

type ScopeExplanation = {
  content: string;
  scopes?: { scopeName: string }[];
};

const highlighter = await createHighlighter({
  themes: [THEME],
  langs: [bamlGrammar as never],
});

const fixturesDir = join(import.meta.dirname, "fixtures");
const snapshotsDir = join(import.meta.dirname, "snapshots");

function fixturePaths(dir = fixturesDir): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);

    if (entry.isDirectory()) {
      return fixturePaths(path);
    }

    return entry.isFile() && entry.name.endsWith(".baml") ? [path] : [];
  });
}

function fixtureName(path: string) {
  return relative(fixturesDir, path).split(sep).join("/");
}

function explanationParts(token: ThemedToken): ScopeExplanation[] {
  return (
    (token.explanation as ScopeExplanation[] | undefined) ?? [
      { content: token.content, scopes: [] },
    ]
  );
}

function formatScopeSnapshot(source: string) {
  const { tokens } = highlighter.codeToTokens(source, {
    lang: "baml",
    theme: THEME,
    includeExplanation: "scopeName",
  });

  const rows: string[] = [];

  tokens.forEach((line, lineIndex) => {
    let column = 0;

    for (const token of line) {
      for (const part of explanationParts(token)) {
        const start = column;
        const end = start + part.content.length;
        column = end;

        if (/^\s*$/.test(part.content)) {
          continue;
        }

        const range = `${lineIndex + 1}:${start + 1}-${end + 1}`;
        const text = JSON.stringify(part.content).padEnd(18);
        const scopes = (part.scopes ?? []).map((scope) => scope.scopeName);

        rows.push(`${range.padEnd(12)} ${text} ${scopes.join(" ")}`);
      }
    }
  });

  return `${rows.join("\n")}\n`;
}

describe("BAML TextMate grammar", () => {
  mkdirSync(snapshotsDir, { recursive: true });
  const fixtures = fixturePaths().sort();

  for (const fixture of fixtures) {
    const name = fixtureName(fixture);

    it(`tokenizes ${name}`, async () => {
      const source = readFileSync(fixture, "utf8");

      await expect(formatScopeSnapshot(source)).toMatchFileSnapshot(
        join(snapshotsDir, `${name}.scope.txt`),
      );
    });
  }
});
