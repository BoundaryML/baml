import { mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { describe, expect, it } from "vitest";
import hljs from "highlight.js";
import baml from "../src/baml.js";

hljs.registerLanguage("baml", baml);

// The .baml fixtures are shared with the TextMate grammar package; this
// package registers the hljs definition and runs every fixture through it.
const fixturesDir = join(
  import.meta.dirname,
  "..",
  "..",
  "pkg-grammar",
  "tests",
  "fixtures",
);
const snapshotsDir = join(import.meta.dirname, "snapshots");

// Fixtures that exercise the showcase-style BAML surface (declaration blocks,
// prompts with Jinja, clients, retry policies, template strings). These must
// score relevance > 0 so hljs auto-detection has a chance on real BAML.
const SHOWCASE_FIXTURES = new Set([
  "generators.baml",
  "baml_tests__retry_policy_valid_retry.baml",
  "baml_tests__template_string_decls.baml",
  "baml_tests__class_decls.baml",
  "baml_tests__enum_decls.baml",
  "lsp_syntax_functions__jinja_control_prompt.baml",
]);

// The ns_* namespace demos and showcase__* fixtures are showcase-style by
// construction (real-world files: clients, prompts, generators, tests).
const isShowcase = (name: string) =>
  SHOWCASE_FIXTURES.has(name) ||
  name.startsWith("ns_") ||
  name.startsWith("showcase__");

// Representative fixtures whose highlighted HTML is snapshotted.
const SNAPSHOT_FIXTURES = new Set([
  "ns_sentiment__sentiment.baml", // client<llm>, prompt #"..."# with Jinja
  "generators.baml", // generator config blocks
  "baml_tests__retry_policy_valid_retry.baml", // client + retry_policy blocks
  "lsp_syntax_functions__jinja_control_prompt.baml", // {% for %} control flow in prompt
  "baml_tests__const_let_else_defer.baml", // statements: const/let/defer/watch/for/while
]);

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

describe("BAML highlight.js language", () => {
  mkdirSync(snapshotsDir, { recursive: true });
  const fixtures = fixturePaths().sort();

  it("finds the shared fixtures", () => {
    expect(fixtures.length).toBeGreaterThan(0);
    for (const showcase of [...SHOWCASE_FIXTURES, ...SNAPSHOT_FIXTURES]) {
      expect(fixtures.map(fixtureName)).toContain(showcase);
    }
  });

  it("registers under the baml alias", () => {
    expect(hljs.getLanguage("baml")?.name).toBe("BAML");
  });

  it("treats $-joined identifiers as single tokens, not keyword fragments", () => {
    // Lexer Word forms: `Foo$bar` segments and the `$`-prefixed `$stream`.
    const code = [
      "function ExtractResume$render_prompt(x: string) -> string {",
      "  let for$each = $stream;",
      "  let is$match = if$else;",
      "  for$each",
      "}",
    ].join("\n");

    const { value, illegal } = hljs.highlight(code, { language: "baml" });
    expect(illegal).toBe(false);

    // The declaration title spans the whole $-joined name.
    expect(value).toContain(
      '<span class="hljs-title function_">ExtractResume$render_prompt</span>',
    );

    // No keyword fragment may be carved out of a $-joined identifier: `for`,
    // `is`, `if`, `else` appear only inside `for$each` / `is$match` /
    // `if$else`, so the only keyword spans are the real ones.
    const keywordSpans = [...value.matchAll(/<span class="hljs-keyword">([^<]*)<\/span>/g)]
      .map((m) => m[1])
      .sort();
    expect(keywordSpans).toEqual(["function", "let", "let"]);
  });

  it("tracks nested braces inside backtick ${...} interpolation", () => {
    const code = 'let msg = `result: ${if ok { "yes" } else { "no" }} done`;';

    const { value, illegal } = hljs.highlight(code, { language: "baml" });
    expect(illegal).toBe(false);

    // The interpolation must stay open across the inner `{ "yes" }` block:
    // `else` sits between the nested blocks, so it only gets a keyword span if
    // the first inner `}` did not close the subst early.
    expect(value).toContain('<span class="hljs-keyword">if</span>');
    expect(value).toContain('<span class="hljs-keyword">else</span>');
    expect(value).toContain('<span class="hljs-string">&quot;yes&quot;</span>');
    expect(value).toContain('<span class="hljs-string">&quot;no&quot;</span>');

    // The subst closes on its balancing `}`, leaving ` done` and the closing
    // backtick inside the string scope.
    expect(value).toMatch(/}<\/span> done`<\/span>;$/);
  });

  for (const fixture of fixtures) {
    const name = fixtureName(fixture);

    it(`highlights ${name}`, async () => {
      const source = readFileSync(fixture, "utf8");

      // (a) must not throw
      const result = hljs.highlight(source, { language: "baml" });

      // (b) the definition uses no `illegal` patterns, so nothing may ever be
      // flagged illegal
      expect(result.illegal).toBe(false);

      // (c) showcase-style fixtures must produce positive relevance
      if (isShowcase(name)) {
        expect(result.relevance).toBeGreaterThan(0);
      }

      if (SNAPSHOT_FIXTURES.has(name)) {
        await expect(result.value).toMatchFileSnapshot(
          join(snapshotsDir, `${name}.html`),
        );
      }
    });
  }
});
