"use client";

import { useEffect, useState } from "react";
import { codeToHtml } from "shiki";
import { SHIKI_THEMES } from "@/lib/shiki-themes";

interface ShikiCodeBlockProps {
  code: string;
  language?: string;
  showLineNumbers?: boolean;
}

// Map common language aliases
const languageMap: Record<string, string> = {
  js: "javascript",
  ts: "typescript",
  py: "python",
  rb: "ruby",
  sh: "bash",
  shell: "bash",
  yml: "yaml",
  baml: "typescript",
  text: "plaintext",
  txt: "plaintext",
  "": "plaintext",
};

function normalizeLanguage(lang?: string): string {
  if (!lang) return "typescript";
  const normalized = lang.toLowerCase();
  return languageMap[normalized] || normalized;
}

// Display names for languages
const languageDisplayNames: Record<string, string> = {
  javascript: "JavaScript",
  typescript: "TypeScript",
  python: "Python",
  ruby: "Ruby",
  bash: "Bash",
  shell: "Shell",
  json: "JSON",
  yaml: "YAML",
  html: "HTML",
  css: "CSS",
  sql: "SQL",
  go: "Go",
  rust: "Rust",
  java: "Java",
  cpp: "C++",
  c: "C",
  plaintext: "Text",
};

function getDisplayName(lang: string): string {
  return languageDisplayNames[lang] || lang.charAt(0).toUpperCase() + lang.slice(1);
}

export function ShikiCodeBlock({
  code,
  language,
  showLineNumbers = true,
}: ShikiCodeBlockProps) {
  const [html, setHtml] = useState<string>("");
  const [isLoading, setIsLoading] = useState(true);
  
  const normalizedLang = normalizeLanguage(language);
  const originalLang = language?.toLowerCase() || "";
  // Don't show language bar for BAML (it's the default language for this app)
  const showLanguageBar = originalLang !== "baml" && originalLang !== "";

  useEffect(() => {
    let cancelled = false;

    async function highlight() {
      try {
        const lang = normalizeLanguage(language);
        const result = await codeToHtml(code, {
          lang,
          themes: SHIKI_THEMES,
          transformers: [
            {
              line(node, line) {
                node.properties["data-line"] = line;
                if (showLineNumbers) {
                  node.children.unshift({
                    type: "element",
                    tagName: "span",
                    properties: { class: "line-number" },
                    children: [{ type: "text", value: String(line) }],
                  });
                }
              },
            },
          ],
        });

        if (!cancelled) {
          setHtml(result);
          setIsLoading(false);
        }
      } catch (err) {
        console.error("[Shiki] Error:", err);
        if (!cancelled) {
          // Fallback to plain text
          setHtml(`<pre><code>${code.replace(/</g, "&lt;").replace(/>/g, "&gt;")}</code></pre>`);
          setIsLoading(false);
        }
      }
    }

    highlight();
    return () => { cancelled = true; };
  }, [code, language, showLineNumbers]);

  // Loading/fallback state: uses semantic code tokens for theme consistency
  if (isLoading) {
    const lines = code.split("\n");
    return (
      <div className="bep-shiki-code not-prose my-5 rounded-xl border border-code-border overflow-hidden">
        {showLanguageBar && (
          <div className="bg-muted border-b border-code-border px-4 py-2 text-xs font-medium text-muted-foreground">
            {getDisplayName(normalizedLang)}
          </div>
        )}
        <div className="bg-code-bg overflow-x-auto">
          <pre className="p-4 m-0">
            <code className="font-mono text-[13px] leading-5 text-foreground">
              {lines.map((line, i) => (
                <div key={i} className="flex">
                  {showLineNumbers && (
                    <span className="select-none text-muted-foreground text-right w-8 pr-3 mr-3 shrink-0 border-r border-code-border">
                      {i + 1}
                    </span>
                  )}
                  <span className="whitespace-pre">{line || " "}</span>
                </div>
              ))}
            </code>
          </pre>
        </div>
      </div>
    );
  }

  return (
    <div className="bep-shiki-code not-prose my-5 rounded-xl border border-code-border overflow-hidden">
      {showLanguageBar && (
        <div className="bg-muted border-b border-code-border px-4 py-2 text-xs font-medium text-muted-foreground">
          {getDisplayName(normalizedLang)}
        </div>
      )}
      <div
        className={`
          [&_pre]:m-0
          [&_pre]:p-4
          [&_pre]:overflow-x-auto
          [&_pre]:text-[13px]
          [&_pre]:leading-5
          [&_code]:block
          [&_code]:font-mono
          [&_.line]:inline-flex
          [&_.line-number]:select-none
          [&_.line-number]:text-right
          [&_.line-number]:w-8
          [&_.line-number]:pr-3
          [&_.line-number]:mr-3
          [&_.line-number]:text-muted-foreground
          [&_.line-number]:shrink-0
          [&_.line-number]:border-r
          [&_.line-number]:border-code-border
        `}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </div>
  );
}
