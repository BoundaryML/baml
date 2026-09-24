/** A small BAML tokenizer for the source view. It works on one line at a time. */

export type TokenKind = "comment" | "string" | "number" | "keyword" | "type" | "call" | "plain";

export interface Token {
  kind: TokenKind;
  text: string;
}

const KEYWORDS = new Set([
  "function", "class", "enum", "let", "while", "for", "in", "if", "else", "return", "spawn", "await",
  "break", "continue", "match", "throw", "catch", "client", "test", "true", "false", "null",
]);

const PRIMITIVES = new Set(["string", "int", "float", "bool", "bigint", "image", "audio", "map"]);

const TOKEN = /(\/\/.*$)|("(?:[^"\\]|\\.)*"?)|(\b\d+(?:\.\d+)?n?\b)|([A-Za-z_][A-Za-z0-9_]*)|(\s+|.)/g;

export function tokenizeLine(line: string): Token[] {
  const tokens: Token[] = [];
  const push = (kind: TokenKind, text: string): void => {
    const last = tokens[tokens.length - 1];
    if (last && last.kind === kind && kind === "plain") last.text += text;
    else tokens.push({ kind, text });
  };
  TOKEN.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = TOKEN.exec(line)) !== null) {
    const [text, comment, str, num, word] = match;
    if (text === "") break;
    if (comment !== undefined) push("comment", text);
    else if (str !== undefined) push("string", text);
    else if (num !== undefined) push("number", text);
    else if (word !== undefined) {
      if (KEYWORDS.has(word)) push("keyword", text);
      else if (PRIMITIVES.has(word) || /^[A-Z]/.test(word)) push("type", text);
      else if (line[TOKEN.lastIndex] === "(") push("call", text);
      else push("plain", text);
    } else push("plain", text);
  }
  return tokens;
}
