// Guards on the artifacts the published package (and GitHub, via the
// boundaryml/textMate-baml mirror) consumes: grammar invariants that external
// registries pin, the dist/ ESM module, and language-configuration.json.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { createHighlighter } from "shiki";
import rawGrammar from "../baml.tmLanguage.json";

const pkgRoot = join(import.meta.dirname, "..");
const distPath = join(pkgRoot, "dist", "index.js");

describe("grammar invariants pinned by external consumers", () => {
  // GitHub Linguist's languages.yml maps BAML to tm_scope source.baml and
  // extension .baml. Changing either doesn't fail anywhere loudly — .baml
  // files on github.com just silently render as plain text.
  it("keeps scopeName source.baml", () => {
    expect(rawGrammar.scopeName).toBe("source.baml");
  });

  it("keeps name baml", () => {
    expect(rawGrammar.name).toBe("baml");
  });

  it("keeps fileTypes [baml]", () => {
    expect(rawGrammar.fileTypes).toContain("baml");
  });

  it("stays self-contained (no external grammar scopes embedded)", () => {
    const json = readFileSync(join(pkgRoot, "baml.tmLanguage.json"), "utf8");
    const includes = [...json.matchAll(/"include":\s*"([^"#$][^"]*)"/g)].map(
      (m) => m[1],
    );
    // Includes must be internal (#repo / $self / $base); an external scope
    // would require consumers to load a second grammar, which Shiki and
    // Linguist won't do for us.
    expect(includes).toEqual([]);
  });
});

describe("dist ESM artifact", () => {
  let distDefault: unknown;

  beforeAll(async () => {
    expect(
      existsSync(distPath),
      "dist/index.js missing — run `pnpm --filter @b/pkg-grammar build` first",
    ).toBe(true);
    distDefault = (await import(distPath)).default;
  });

  it("default export deep-equals the raw tmLanguage JSON", () => {
    // The mirror publishes both; if the inlined literal ever drifts from the
    // raw JSON, npm consumers and GitHub would highlight differently.
    expect(distDefault).toEqual(JSON.parse(JSON.stringify(rawGrammar)));
  });

  it("works as a Shiki LanguageRegistration end-to-end", async () => {
    const highlighter = await createHighlighter({
      themes: ["github-dark"],
      langs: [distDefault as never],
    });

    const sample = [
      "// greet the user",
      "function Greet(name: string) -> string {",
      "  client GPT4",
      '  prompt #"Hello {{ name }}"#',
      "}",
      "",
      "enum Mood { Happy Sad }",
    ].join("\n");

    const { tokens } = highlighter.codeToTokens(sample, {
      lang: "baml",
      theme: "github-dark",
      includeExplanation: "scopeName",
    });

    const scopes = tokens
      .flat()
      .flatMap((token) => token.explanation ?? [])
      .flatMap((part) => part.scopes ?? [])
      .map((scope) => scope.scopeName);

    expect(scopes).toContain("source.baml");
    // At least something beyond the root scope must match, otherwise the
    // grammar is loaded but effectively inert.
    expect(scopes.some((s) => s !== "source.baml")).toBe(true);
    highlighter.dispose();
  });
});

describe("language-configuration.json", () => {
  it("is strict JSON (npm consumers JSON.parse it; no JSONC comments)", () => {
    const raw = readFileSync(
      join(pkgRoot, "language-configuration.json"),
      "utf8",
    );
    const config = JSON.parse(raw);
    expect(config.comments.lineComment).toBe("//");
    expect(Array.isArray(config.brackets)).toBe(true);
  });
});
