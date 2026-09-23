export type CodeToken = { text: string; kind?: "comment" | "string" | "keyword" | "number" | "type" };
const keywords = new Set("function class enum interface type client generator test testset retry_policy let const if else for in while match return break continue throw throws catch try assert import from as true false null int float string bool map void stream with where prompt model options provider dynamic".split(" "));

/** Lexical coloring only. Text stays text and is never interpreted as HTML. */
export function bamlTokens(source: string): CodeToken[] {
  if (source.length > 100_000) return [{ text: source }];
  const pattern = /\/\/[^\n]*|\/\*[\s\S]*?(?:\*\/|$)|#"[\s\S]*?"#|"(?:\\[\s\S]|[^"\\])*"|`(?:\\[\s\S]|[^`\\])*`|\b\d+(?:\.\d+)?\b|\b[A-Za-z_]\w*\b/g;
  const tokens: CodeToken[] = [];
  let offset = 0;
  for (const match of source.matchAll(pattern)) {
    const index = match.index!;
    if (index > offset) tokens.push({ text: source.slice(offset, index) });
    const text = match[0];
    const kind = text.startsWith("//") || text.startsWith("/*") ? "comment"
      : text.startsWith('"') || text.startsWith('#"') || text.startsWith("`") ? "string"
      : keywords.has(text) ? "keyword"
      : /^\d/.test(text) ? "number"
      : /^[A-Z]/.test(text) ? "type" : undefined;
    tokens.push({ text, kind });
    offset = index + text.length;
  }
  if (offset < source.length) tokens.push({ text: source.slice(offset) });
  return tokens;
}

const pythonKeywords = new Set("and as assert async await break class continue def del elif else except False finally for from global if import in is lambda None nonlocal not or pass raise return True try while with yield".split(" "));

/** Keep unknown languages as escaped plain text. */
export function codeTokens(source: string, language: string): CodeToken[] {
  const lang = language.toLowerCase().split(".").pop();
  if (lang === "baml") return bamlTokens(source);
  if (lang !== "python" && lang !== "py") return [{ text: source }];
  if (source.length > 100_000) return [{ text: source }];
  const pattern = /#[^\n]*|"""[\s\S]*?(?:"""|$)|'''[\s\S]*?(?:'''|$)|"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|\b\d+(?:\.\d+)?\b|\b[A-Za-z_]\w*\b/g;
  const tokens: CodeToken[] = [];
  let offset = 0;
  for (const match of source.matchAll(pattern)) {
    const index = match.index!;
    if (index > offset) tokens.push({ text: source.slice(offset, index) });
    const text = match[0];
    const kind = text.startsWith("#") ? "comment"
      : text.startsWith('"') || text.startsWith("'") ? "string"
      : pythonKeywords.has(text) ? "keyword"
      : /^\d/.test(text) ? "number"
      : /^[A-Z]/.test(text) ? "type" : undefined;
    tokens.push({ text, kind });
    offset = index + text.length;
  }
  if (offset < source.length) tokens.push({ text: source.slice(offset) });
  return tokens;
}
