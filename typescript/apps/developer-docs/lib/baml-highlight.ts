import "server-only"

import { readFile } from "node:fs/promises"
import path from "node:path"

import type { LanguageRegistration } from "shiki"
import { createHighlighter } from "shiki"

const grammarPath = path.resolve(process.cwd(), "../../../typescript2/pkg-grammar/baml.tmLanguage.json")

let highlighterPromise: ReturnType<typeof createHighlighter> | undefined

export async function highlightBaml(code: string) {
  if (!highlighterPromise) {
    highlighterPromise = (async () => {
      const grammar = JSON.parse(await readFile(grammarPath, "utf8")) as LanguageRegistration
      return createHighlighter({ langs: [grammar], themes: ["github-light", "github-dark"] })
    })()
  }

  const highlighter = await highlighterPromise
  return highlighter.codeToHtml(code, {
    lang: "baml",
    themes: { light: "github-light", dark: "github-dark" },
    defaultColor: false,
  })
}
