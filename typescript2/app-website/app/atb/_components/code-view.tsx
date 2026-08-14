"use client";

// Shiki-highlighted code, with the BAML TextMate grammar registered.
// The highlighter is built lazily on first use and shared; until it
// resolves the plain <pre> renders, so nothing flashes or errors.

import { useEffect, useState } from "react";
import bamlGrammar from "@/app/atb/_lib/baml-grammar.json";

const EXT_LANG: Record<string, string> = {
  baml: "baml",
  bash: "bash",
  js: "javascript",
  json: "json",
  jsx: "tsx",
  md: "markdown",
  py: "python",
  rs: "rust",
  sh: "bash",
  toml: "toml",
  ts: "typescript",
  tsx: "tsx",
  txt: "text",
  yaml: "yaml",
  yml: "yaml",
};

export function langFor(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return EXT_LANG[ext] ?? "text";
}

let highlighterPromise: Promise<{
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
  getLoadedLanguages: () => string[];
}> | null = null;

function getHighlighter() {
  if (!highlighterPromise) {
    highlighterPromise = Promise.all([
      import("shiki"),
      // Pure-JS regex engine: shiki's default oniguruma WASM fails to load
      // in the browser bundle and silently leaves code unhighlighted.
      import("shiki/engine/javascript"),
    ]).then(([{ createHighlighter }, { createJavaScriptRegexEngine }]) =>
      createHighlighter({
        engine: createJavaScriptRegexEngine({ forgiving: true }),
        langs: [
          "bash",
          "javascript",
          "json",
          "markdown",
          "python",
          "rust",
          "toml",
          "tsx",
          "typescript",
          "yaml",
          { ...(bamlGrammar as object), aliases: [], name: "baml" },
        ] as never,
        themes: ["github-light"],
      }),
    );
  }
  return highlighterPromise;
}

export function CodeView({
  path,
  content,
  className = "",
}: {
  path: string;
  content: string;
  className?: string;
}) {
  const [html, setHtml] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    getHighlighter()
      .then((hl) => {
        const want = langFor(path);
        const lang = hl.getLoadedLanguages().includes(want) ? want : "text";
        const out = hl.codeToHtml(content ?? "", { lang, theme: "github-light" });
        if (alive) setHtml(out);
      })
      .catch(() => alive && setHtml(null));
    return () => {
      alive = false;
    };
  }, [path, content]);

  if (html === null) {
    return (
      <pre
        className={`atb-scroll bg-white text-atb-ink text-xs p-3.5 overflow-auto leading-relaxed ${className}`}
      >
        {content}
      </pre>
    );
  }
  return (
    <div
      className={`atb-code-hl atb-scroll overflow-auto ${className}`}
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}
