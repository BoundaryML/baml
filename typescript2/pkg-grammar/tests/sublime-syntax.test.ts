// Guards on baml.sublime-syntax, the Sublime Text / syntect (bat, delta, ...)
// artifact generated from baml.tmLanguage.json by scripts/emit-sublime.ts:
// a drift check (the committed file must match what the committed grammar
// generates) and structural sanity checks on the emitted YAML, since nothing
// in this repo loads sublime-syntax files natively.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { emitSublimeSyntax } from "../scripts/emit-sublime.ts";

const pkgRoot = join(import.meta.dirname, "..");
const committed = readFileSync(join(pkgRoot, "baml.sublime-syntax"), "utf8");
const grammar = JSON.parse(
  readFileSync(join(pkgRoot, "baml.tmLanguage.json"), "utf8"),
);

describe("baml.sublime-syntax stays in sync with the grammar", () => {
  it("matches a fresh in-memory conversion of baml.tmLanguage.json", () => {
    expect(
      committed,
      "baml.sublime-syntax is stale — run `npx tsx scripts/emit-sublime.ts`",
    ).toBe(emitSublimeSyntax(grammar));
  });
});

describe("baml.sublime-syntax structure", () => {
  // The emitter's output shape is fixed (see renderYaml in emit-sublime.ts),
  // so a line-based reading is exact — no YAML parser dependency needed.
  const lines = committed.split("\n");

  // context name -> its body lines
  const contexts = new Map<string, string[]>();
  {
    let current: string[] | undefined;
    let inContexts = false;
    for (const line of lines) {
      if (line === "contexts:") {
        inContexts = true;
        continue;
      }
      if (!inContexts) {
        continue;
      }
      const header = /^  ([^\s:]+):$/.exec(line);
      if (header) {
        current = [];
        contexts.set(header[1], current);
      } else if (line.trim() !== "") {
        expect(current, `context body line before any header: ${line}`).toBeDefined();
        current?.push(line);
      }
    }
  }

  it("has the sublime-syntax header and top-level keys", () => {
    expect(lines[0]).toBe("%YAML 1.2");
    expect(lines[1]).toBe("---");
    expect(lines).toContain("name: BAML");
    expect(lines).toContain("scope: source.baml");
    expect(lines).toContain("version: 2");
    const extIndex = lines.indexOf("file_extensions:");
    expect(extIndex).toBeGreaterThan(-1);
    expect(lines[extIndex + 1]).toBe("  - baml");
    expect(lines).toContain("contexts:");
  });

  it("defines a main context with entries", () => {
    expect(contexts.has("main")).toBe(true);
    expect(contexts.get("main")!.length).toBeGreaterThan(0);
  });

  it("defines a context for every repository entry", () => {
    for (const key of Object.keys(grammar.repository)) {
      expect(contexts.has(key), `missing context for repository.${key}`).toBe(true);
    }
  });

  it("only includes contexts that are defined", () => {
    for (const [name, body] of contexts) {
      for (const line of body) {
        const include = /^    - include: (.+)$/.exec(line);
        if (include) {
          expect(
            contexts.has(include[1]),
            `context ${name} includes undefined context ${include[1]}`,
          ).toBe(true);
        }
      }
    }
  });

  it("only pushes/sets contexts that are defined", () => {
    let pushCount = 0;
    for (const [name, body] of contexts) {
      for (const line of body) {
        const target = /^      (?:push|set): (.+)$/.exec(line);
        if (target) {
          pushCount += 1;
          expect(
            contexts.has(target[1]),
            `context ${name} pushes undefined context ${target[1]}`,
          ).toBe(true);
        }
      }
    }
    // The grammar is full of begin/end rules; if no pushes were found the
    // regexes above went stale against the emitter's output format.
    expect(pushCount).toBeGreaterThan(50);
  });

  it("keeps every match single-line and every pop match first in its context", () => {
    for (const [name, body] of contexts) {
      for (const [i, line] of body.entries()) {
        expect(line, `literal newline artifact in ${name}`).not.toContain("\\x0a");
        const pop = line === "      pop: true";
        if (pop) {
          // The pop match must be the first match entry of its context
          // (TextMate tries end patterns before child patterns).
          const before = body.slice(0, i).filter((l) => l.startsWith("    - match:"));
          expect(before.length, `pop match is not first in ${name}`).toBe(1);
        }
      }
    }
  });
});
