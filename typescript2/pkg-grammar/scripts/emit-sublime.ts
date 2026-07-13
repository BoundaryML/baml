#!/usr/bin/env tsx
// Convert the canonical TextMate grammar (baml.tmLanguage.json) into a
// Sublime Text .sublime-syntax file (baml.sublime-syntax). That artifact is
// what syntect-based tools (bat, delta, Zola, ...) and Sublime Text itself
// consume, and it ships through the read-only mirror repo alongside the
// tmLanguage JSON. Run via `npx tsx scripts/emit-sublime.ts`; the drift test
// in tests/sublime-syntax.test.ts regenerates it in-memory and fails if the
// committed file is stale.
//
// The converter is deliberately narrow: it handles exactly the constructs
// tmlanguage-generator emits for src/baml.ts (match/name/captures,
// begin/end with begin-/endCaptures and child patterns, #repo includes,
// patterns-only containers) and throws on anything else (`while`,
// `applyEndPatternLast`, `contentName`, $self/$base includes, backreferences
// in end patterns, multi-line regexes). Regexes are Oniguruma on both sides
// and pass through unchanged.
//
// Mapping notes:
// - Each repository entry becomes a context with the same name. A begin/end
//   rule becomes a match that pushes `<key>--body`, whose first entry is the
//   end regex with `pop: true` (TextMate tries the end pattern before child
//   patterns, so the pop match must come first). Rule `name` -> `meta_scope`.
// - TextMate capture 0 has no numbered equivalent in sublime-syntax; it maps
//   to the match's whole-match `scope:` field instead.
// - TextMate allows `captures` entries that re-tokenize the captured text
//   with nested patterns; sublime-syntax has no equivalent. Those captures
//   (identifier paths like `a.b.c`, quoted strings, type expressions) are
//   approximated with a single representative scope: the `name` of the last
//   included rule that is not punctuation/meta (the most general sub-matcher,
//   e.g. `storage.type.annotation.baml` for attribute paths). Only the
//   coloring of interior separators/special roots is lost.
// - `\G` in begin patterns (TextMate: anchor to the end of the parent begin
//   match) passes through: Oniguruma's \G matches at the search start, which
//   coincides with that position on the first attempt inside the pushed
//   context. This is the standard tmLanguage->sublime-syntax approximation.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

interface TmCapture {
  name?: string;
  patterns?: TmRule[];
}

interface TmRule {
  include?: string;
  match?: string;
  begin?: string;
  end?: string;
  name?: string;
  captures?: Record<string, TmCapture>;
  beginCaptures?: Record<string, TmCapture>;
  endCaptures?: Record<string, TmCapture>;
  patterns?: TmRule[];
}

interface TmGrammar {
  $schema?: string;
  name: string;
  scopeName: string;
  fileTypes: string[];
  patterns: TmRule[];
  repository: Record<string, TmRule>;
}

// One `- match: ...` entry in a sublime-syntax context.
interface MatchEntry {
  kind: "match";
  regex: string;
  scope?: string;
  captures?: Map<number, string>;
  push?: string;
  pop?: boolean;
}

type ContextEntry =
  | MatchEntry
  | { kind: "include"; target: string }
  | { kind: "meta_scope"; scope: string };

const RULE_KEYS = new Set([
  "include",
  "match",
  "begin",
  "end",
  "name",
  "captures",
  "beginCaptures",
  "endCaptures",
  "patterns",
]);

function fail(where: string, message: string): never {
  throw new Error(`emit-sublime: ${where}: ${message}`);
}

function checkRuleKeys(rule: TmRule, where: string): void {
  for (const key of Object.keys(rule)) {
    if (!RULE_KEYS.has(key)) {
      fail(where, `unhandled TextMate construct \`${key}\` — teach scripts/emit-sublime.ts about it before shipping`);
    }
  }
}

function checkRegex(regex: string, where: string): void {
  if (regex.includes("\n")) {
    fail(where, "regex contains a literal newline; sublime-syntax matches are single-line");
  }
  if (regex.includes("(?s)")) {
    fail(where, "regex uses (?s); sublime-syntax matches are single-line");
  }
}

function checkEndRegex(regex: string, where: string): void {
  checkRegex(regex, where);
  if (/\\[0-9]/.test(regex)) {
    fail(where, "end pattern uses a backreference into the begin match; sublime-syntax cannot express this");
  }
}

export function emitSublimeSyntax(grammar: TmGrammar): string {
  if (grammar.scopeName !== "source.baml") {
    fail("grammar", `unexpected scopeName ${grammar.scopeName}`);
  }
  const repository = grammar.repository;
  for (const key of Object.keys(repository)) {
    // `--` is reserved for the generated `<key>--body` contexts of begin/end
    // rules; `main` and `prototype` are special context names in Sublime.
    if (key.includes("--") || key === "main" || key === "prototype") {
      fail(`repository.${key}`, "repository key collides with generated/reserved context names");
    }
  }

  // Contexts are emitted in insertion order: main, then each repository entry
  // (in JSON order) followed immediately by any --body contexts it spawns.
  const contexts = new Map<string, ContextEntry[]>();

  const resolveInclude = (include: string, where: string): string => {
    if (include === "$self" || include === "$base") {
      fail(where, `\`${include}\` include cannot be converted mechanically — handle it explicitly`);
    }
    if (!include.startsWith("#")) {
      fail(where, `external include \`${include}\`; the grammar must stay self-contained`);
    }
    const key = include.slice(1);
    if (!(key in repository)) {
      fail(where, `include references missing repository entry \`${key}\``);
    }
    return key;
  };

  // TextMate captures may carry nested `patterns` that re-tokenize the
  // captured text; sublime-syntax cannot do that. Approximate with the scope
  // of the most general (last non-punctuation, non-meta) included sub-rule.
  const resolveCapturePatterns = (patterns: TmRule[], where: string): string | undefined => {
    const candidates: string[] = [];
    for (const [i, sub] of patterns.entries()) {
      const subWhere = `${where}.patterns[${i}]`;
      checkRuleKeys(sub, subWhere);
      if (sub.include === undefined) {
        fail(subWhere, "capture patterns must be plain #includes to be approximated with a single scope");
      }
      const target = repository[resolveInclude(sub.include, subWhere)];
      if (target.name !== undefined) {
        candidates.push(target.name);
      }
    }
    const preferred = candidates.filter(
      (scope) => !scope.startsWith("punctuation.") && !scope.startsWith("meta."),
    );
    const fallback = candidates.filter((scope) => !scope.startsWith("punctuation."));
    return preferred.at(-1) ?? fallback.at(-1);
  };

  const resolveCapture = (capture: TmCapture, where: string): string | undefined => {
    if (capture.name !== undefined && capture.patterns !== undefined) {
      fail(where, "capture with both `name` and `patterns` is unhandled");
    }
    if (capture.patterns !== undefined) {
      return resolveCapturePatterns(capture.patterns, where);
    }
    if (capture.name === undefined) {
      fail(where, "capture with neither `name` nor `patterns`");
    }
    return capture.name;
  };

  // Splits a TextMate captures object into the whole-match scope (capture 0,
  // which sublime-syntax expresses via the match's `scope:` field) and the
  // numbered group captures.
  const convertCaptures = (
    captures: Record<string, TmCapture> | undefined,
    where: string,
  ): { wholeMatch?: string; groups?: Map<number, string> } => {
    if (captures === undefined) {
      return {};
    }
    let wholeMatch: string | undefined;
    const groups = new Map<number, string>();
    const numbers = Object.keys(captures)
      .map((raw) => {
        if (!/^\d+$/.test(raw)) {
          fail(where, `non-numeric capture key \`${raw}\``);
        }
        return Number(raw);
      })
      .sort((a, b) => a - b);
    for (const num of numbers) {
      const scope = resolveCapture(captures[String(num)], `${where}[${num}]`);
      if (scope === undefined) {
        continue; // capture-patterns with no representative scope: leave unscoped
      }
      if (num === 0) {
        wholeMatch = scope;
      } else {
        groups.set(num, scope);
      }
    }
    return { wholeMatch, groups: groups.size > 0 ? groups : undefined };
  };

  // Body contexts are named `<repoKey>--body` (with a numeric suffix if one
  // repository entry ever spawns several) so push targets stay greppable.
  const bodyCounters = new Map<string, number>();
  const nextBodyName = (ownerKey: string): string => {
    const count = (bodyCounters.get(ownerKey) ?? 0) + 1;
    bodyCounters.set(ownerKey, count);
    return count === 1 ? `${ownerKey}--body` : `${ownerKey}--body-${count}`;
  };

  const convertRule = (rule: TmRule, ownerKey: string, where: string): ContextEntry[] => {
    checkRuleKeys(rule, where);

    if (rule.include !== undefined) {
      return [{ kind: "include", target: resolveInclude(rule.include, where) }];
    }

    if (rule.match !== undefined) {
      if (rule.begin !== undefined || rule.end !== undefined || rule.patterns !== undefined) {
        fail(where, "`match` rule combined with begin/end/patterns is unhandled");
      }
      checkRegex(rule.match, where);
      const { wholeMatch, groups } = convertCaptures(rule.captures, `${where}.captures`);
      // Rule `name` and capture 0 both scope the whole match (name is the
      // outer scope); sublime-syntax stacks space-separated scopes the same way.
      const scope = [rule.name, wholeMatch].filter((s) => s !== undefined).join(" ");
      return [
        {
          kind: "match",
          regex: rule.match,
          scope: scope !== "" ? scope : undefined,
          captures: groups,
        },
      ];
    }

    if (rule.begin !== undefined) {
      if (rule.end === undefined) {
        fail(where, "`begin` without `end` (a `while` rule?) is unhandled");
      }
      if (rule.captures !== undefined) {
        fail(where, "`captures` shorthand on a begin/end rule is unhandled; use begin-/endCaptures");
      }
      checkRegex(rule.begin, where);
      checkEndRegex(rule.end, where);

      const beginCaps = convertCaptures(rule.beginCaptures, `${where}.beginCaptures`);
      const endCaps = convertCaptures(rule.endCaptures, `${where}.endCaptures`);

      const bodyName = nextBodyName(ownerKey);
      const body: ContextEntry[] = [];
      if (rule.name !== undefined) {
        body.push({ kind: "meta_scope", scope: rule.name });
      }
      // The pop match comes first: TextMate tries the end pattern before the
      // child patterns at every position (this grammar never sets
      // applyEndPatternLast, which would reverse that).
      body.push({
        kind: "match",
        regex: rule.end,
        scope: endCaps.wholeMatch,
        captures: endCaps.groups,
        pop: true,
      });
      for (const [i, child] of (rule.patterns ?? []).entries()) {
        body.push(...convertRule(child, ownerKey, `${where}.patterns[${i}]`));
      }
      contexts.set(bodyName, body);

      return [
        {
          kind: "match",
          regex: rule.begin,
          scope: beginCaps.wholeMatch,
          captures: beginCaps.groups,
          push: bodyName,
        },
      ];
    }

    if (rule.patterns !== undefined) {
      // Patterns-only grouping is transparent in TextMate; flatten in place.
      if (rule.name !== undefined || rule.captures !== undefined) {
        fail(where, "patterns-only rule with extra keys is unhandled");
      }
      return rule.patterns.flatMap((child, i) =>
        convertRule(child, ownerKey, `${where}.patterns[${i}]`),
      );
    }

    fail(where, `rule with keys [${Object.keys(rule).join(", ")}] is unhandled`);
  };

  contexts.set(
    "main",
    grammar.patterns.flatMap((rule, i) => convertRule(rule, "main", `patterns[${i}]`)),
  );
  for (const [key, rule] of Object.entries(repository)) {
    contexts.set(key, convertRule(rule, key, `repository.${key}`));
  }

  return renderYaml(grammar, contexts);
}

// -- YAML rendering -----------------------------------------------------------
// Hand-rolled on purpose: the shape is fixed and small, and Sublime's YAML
// loader is strict about regex escaping, so everything that could contain a
// backslash is emitted as a single-quoted scalar (only ' needs doubling there).

function singleQuote(value: string): string {
  return `'${value.replaceAll("'", "''")}'`;
}

// Scope names are plain dotted-lowercase words (possibly space-separated
// stacks); emit them unquoted when safe so the file stays readable.
function scopeScalar(scope: string, where: string): string {
  if (!/^[A-Za-z0-9_.-]+( [A-Za-z0-9_.-]+)*$/.test(scope)) {
    fail(where, `scope \`${scope}\` needs quoting rules the emitter does not implement`);
  }
  return scope;
}

function renderYaml(grammar: TmGrammar, contexts: Map<string, ContextEntry[]>): string {
  const lines: string[] = [];
  lines.push("%YAML 1.2");
  lines.push("---");
  lines.push("# Generated from baml.tmLanguage.json by scripts/emit-sublime.ts. Do not edit.");
  lines.push("name: BAML");
  lines.push("file_extensions:");
  for (const ext of grammar.fileTypes) {
    lines.push(`  - ${ext}`);
  }
  lines.push(`scope: ${grammar.scopeName}`);
  lines.push("version: 2");
  lines.push("");
  lines.push("contexts:");

  for (const [name, entries] of contexts) {
    lines.push(`  ${name}:`);
    for (const [i, entry] of entries.entries()) {
      switch (entry.kind) {
        case "meta_scope":
          if (i !== 0) {
            fail(name, "meta_scope must be the first entry of a context");
          }
          lines.push(`    - meta_scope: ${scopeScalar(entry.scope, name)}`);
          break;
        case "include":
          lines.push(`    - include: ${entry.target}`);
          break;
        case "match":
          lines.push(`    - match: ${singleQuote(entry.regex)}`);
          if (entry.scope !== undefined) {
            lines.push(`      scope: ${scopeScalar(entry.scope, name)}`);
          }
          if (entry.captures !== undefined) {
            lines.push("      captures:");
            for (const [num, scope] of entry.captures) {
              lines.push(`        ${num}: ${scopeScalar(scope, name)}`);
            }
          }
          if (entry.push !== undefined) {
            lines.push(`      push: ${entry.push}`);
          }
          if (entry.pop === true) {
            lines.push("      pop: true");
          }
          break;
      }
    }
  }

  lines.push("");
  return lines.join("\n");
}

// -- CLI ----------------------------------------------------------------------

const isMain =
  process.argv[1] !== undefined &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href;

if (isMain) {
  const here = dirname(fileURLToPath(import.meta.url));
  const pkgRoot = resolve(here, "..");
  const grammar: TmGrammar = JSON.parse(
    readFileSync(resolve(pkgRoot, "baml.tmLanguage.json"), "utf8"),
  );
  const out = resolve(pkgRoot, "baml.sublime-syntax");
  writeFileSync(out, emitSublimeSyntax(grammar));
  console.log("generated baml.sublime-syntax");
}
