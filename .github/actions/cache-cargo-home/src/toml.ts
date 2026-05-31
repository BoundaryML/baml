/**
 * A small, dependency-free TOML parser.
 *
 * It is intentionally self-contained (we do not want to pull a TOML library into
 * the bundled action) but it is a real parser, not a regex hack: it tokenises and
 * walks the document and supports the subset of TOML that Cargo.lock uses plus a
 * fair bit more, so it stays robust as the lockfile format evolves:
 *
 *   - comments (`# ...`)
 *   - bare / quoted / dotted keys
 *   - standard tables `[a.b.c]`
 *   - arrays of tables `[[package]]`
 *   - basic strings, multiline basic strings (with escapes + line-ending backslash)
 *   - literal strings and multiline literal strings
 *   - integers (decimal with `_`, plus 0x / 0o / 0b), floats, booleans
 *   - offset/local date-times and dates (kept verbatim as strings)
 *   - arrays (single- and multi-line, trailing commas) and inline tables
 *
 * The result is a plain JSON-ish object graph.
 */

export type TomlValue =
  | string
  | number
  | boolean
  | TomlValue[]
  | { [key: string]: TomlValue };

export type TomlTable = { [key: string]: TomlValue };

class Parser {
  private readonly s: string;
  private i = 0;
  private line = 1;
  private col = 1;

  constructor(input: string) {
    // Normalise newlines and strip a UTF-8 BOM if present.
    this.s = input.replace(/^﻿/, "").replace(/\r\n/g, "\n");
  }

  parse(): TomlTable {
    const root: TomlTable = {};
    // Tracks tables explicitly opened with [table] so we can reject redefinition,
    // and tables created implicitly via dotted keys / array-of-tables.
    let current: TomlTable = root;

    for (;;) {
      this.skipWhitespaceAndComments();
      if (this.eof()) break;

      const ch = this.peek();
      if (ch === "[") {
        current = this.parseTableHeader(root);
        continue;
      }
      // key = value line
      this.parseKeyValue(current);
      this.skipInlineWhitespace();
      this.skipComment();
      if (!this.eof() && this.peek() !== "\n") {
        throw this.err(`unexpected trailing content`);
      }
    }

    return root;
  }

  // ---- table headers -------------------------------------------------------

  private parseTableHeader(root: TomlTable): TomlTable {
    this.expect("[");
    const isArray = this.peek() === "[";
    if (isArray) this.next();

    this.skipInlineWhitespace();
    const path = this.parseKeyPath();
    this.skipInlineWhitespace();
    this.expect("]");
    if (isArray) this.expect("]");

    this.skipInlineWhitespace();
    this.skipComment();
    if (!this.eof() && this.peek() !== "\n") {
      throw this.err("unexpected content after table header");
    }

    // Walk/create intermediate tables.
    let node: TomlTable = root;
    for (let k = 0; k < path.length - 1; k++) {
      const key = path[k]!;
      node = this.descend(node, key);
    }

    const leaf = path[path.length - 1]!;
    if (isArray) {
      let arr = node[leaf];
      if (arr === undefined) {
        arr = [];
        node[leaf] = arr;
      }
      if (!Array.isArray(arr)) {
        throw this.err(`'${leaf}' is not an array of tables`);
      }
      const entry: TomlTable = {};
      (arr as TomlValue[]).push(entry);
      return entry;
    }

    if (node[leaf] === undefined) {
      const t: TomlTable = {};
      node[leaf] = t;
      return t;
    }
    const existing = node[leaf];
    if (typeof existing !== "object" || Array.isArray(existing)) {
      throw this.err(`cannot redefine '${leaf}' as a table`);
    }
    return existing as TomlTable;
  }

  /** Descend into a table by key, following the last element of an array-of-tables. */
  private descend(node: TomlTable, key: string): TomlTable {
    const next = node[key];
    if (next === undefined) {
      const t: TomlTable = {};
      node[key] = t;
      return t;
    }
    if (Array.isArray(next)) {
      const last = next[next.length - 1];
      if (last === undefined || typeof last !== "object" || Array.isArray(last)) {
        throw this.err(`cannot descend into '${key}'`);
      }
      return last as TomlTable;
    }
    if (typeof next !== "object") {
      throw this.err(`cannot descend into '${key}'`);
    }
    return next as TomlTable;
  }

  // ---- key/value -----------------------------------------------------------

  private parseKeyValue(table: TomlTable): void {
    const path = this.parseKeyPath();
    this.skipInlineWhitespace();
    this.expect("=");
    this.skipInlineWhitespace();
    const value = this.parseValue();

    let node = table;
    for (let k = 0; k < path.length - 1; k++) {
      const key = path[k]!;
      const next = node[key];
      if (next === undefined) {
        const t: TomlTable = {};
        node[key] = t;
        node = t;
      } else if (typeof next === "object" && !Array.isArray(next)) {
        node = next as TomlTable;
      } else {
        throw this.err(`cannot assign into '${key}'`);
      }
    }
    const leaf = path[path.length - 1]!;
    if (node[leaf] !== undefined) {
      throw this.err(`duplicate key '${leaf}'`);
    }
    node[leaf] = value;
  }

  /** Parse a dotted key path (e.g. `a.b."c d"`). */
  private parseKeyPath(): string[] {
    const parts: string[] = [];
    for (;;) {
      this.skipInlineWhitespace();
      parts.push(this.parseKeyComponent());
      this.skipInlineWhitespace();
      if (this.peek() === ".") {
        this.next();
        continue;
      }
      break;
    }
    return parts;
  }

  private parseKeyComponent(): string {
    const ch = this.peek();
    if (ch === '"') return this.parseBasicString();
    if (ch === "'") return this.parseLiteralString();
    // bare key: A-Za-z0-9_-
    let out = "";
    while (!this.eof()) {
      const c = this.peek();
      if (/[A-Za-z0-9_-]/.test(c)) {
        out += c;
        this.next();
      } else break;
    }
    if (out.length === 0) throw this.err("expected a key");
    return out;
  }

  // ---- values --------------------------------------------------------------

  private parseValue(): TomlValue {
    const ch = this.peek();
    if (ch === '"') {
      if (this.startsWith('"""')) return this.parseMultilineBasicString();
      return this.parseBasicString();
    }
    if (ch === "'") {
      if (this.startsWith("'''")) return this.parseMultilineLiteralString();
      return this.parseLiteralString();
    }
    if (ch === "[") return this.parseArray();
    if (ch === "{") return this.parseInlineTable();
    if (ch === "t" || ch === "f") return this.parseBoolean();
    // numbers, dates, inf/nan
    return this.parseAtom();
  }

  private parseBoolean(): boolean {
    if (this.startsWith("true")) {
      this.advance(4);
      return true;
    }
    if (this.startsWith("false")) {
      this.advance(5);
      return false;
    }
    throw this.err("invalid literal");
  }

  /**
   * Parse a bare atom: integer, float, inf/nan, or a date-time.
   * Date-times are returned verbatim as strings (Cargo.lock never uses them, but
   * we keep them intact rather than mangling them into numbers).
   */
  private parseAtom(): TomlValue {
    const start = this.i;
    while (!this.eof()) {
      const c = this.peek();
      if (/[0-9A-Za-z_\-+.:]/.test(c)) {
        this.next();
      } else break;
    }
    const raw = this.s.slice(start, this.i);
    if (raw.length === 0) throw this.err("expected a value");

    // Date-time / date / time → keep as string.
    if (/^\d{4}-\d{2}-\d{2}([T ]\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})?)?$/.test(raw)) {
      return raw;
    }
    if (/^\d{2}:\d{2}:\d{2}(\.\d+)?$/.test(raw)) return raw;

    if (raw === "inf" || raw === "+inf") return Infinity;
    if (raw === "-inf") return -Infinity;
    if (raw === "nan" || raw === "+nan" || raw === "-nan") return NaN;

    const cleaned = raw.replace(/_/g, "");
    if (/^[+-]?0x[0-9A-Fa-f]+$/.test(cleaned)) return parseInt(cleaned, 16);
    if (/^[+-]?0o[0-7]+$/.test(cleaned)) {
      const neg = cleaned.startsWith("-");
      return (neg ? -1 : 1) * parseInt(cleaned.replace(/^[+-]?0o/, ""), 8);
    }
    if (/^[+-]?0b[01]+$/.test(cleaned)) {
      const neg = cleaned.startsWith("-");
      return (neg ? -1 : 1) * parseInt(cleaned.replace(/^[+-]?0b/, ""), 2);
    }
    if (/^[+-]?\d+$/.test(cleaned)) return parseInt(cleaned, 10);
    if (/^[+-]?(\d+(\.\d+)?([eE][+-]?\d+)?|\.\d+([eE][+-]?\d+)?)$/.test(cleaned)) {
      return parseFloat(cleaned);
    }
    throw this.err(`invalid value '${raw}'`);
  }

  private parseArray(): TomlValue[] {
    this.expect("[");
    const out: TomlValue[] = [];
    for (;;) {
      this.skipWhitespaceAndComments();
      if (this.eof()) throw this.err("unterminated array");
      if (this.peek() === "]") {
        this.next();
        break;
      }
      out.push(this.parseValue());
      this.skipWhitespaceAndComments();
      if (this.peek() === ",") {
        this.next();
        continue;
      }
      this.skipWhitespaceAndComments();
      if (this.peek() === "]") {
        this.next();
        break;
      }
      throw this.err("expected ',' or ']' in array");
    }
    return out;
  }

  private parseInlineTable(): TomlTable {
    this.expect("{");
    const out: TomlTable = {};
    this.skipInlineWhitespace();
    if (this.peek() === "}") {
      this.next();
      return out;
    }
    for (;;) {
      this.skipInlineWhitespace();
      const path = this.parseKeyPath();
      this.skipInlineWhitespace();
      this.expect("=");
      this.skipInlineWhitespace();
      const value = this.parseValue();
      let node = out;
      for (let k = 0; k < path.length - 1; k++) {
        const key = path[k]!;
        const next = node[key];
        if (next === undefined) {
          const t: TomlTable = {};
          node[key] = t;
          node = t;
        } else if (typeof next === "object" && !Array.isArray(next)) {
          node = next as TomlTable;
        } else {
          throw this.err(`cannot assign into '${key}'`);
        }
      }
      node[path[path.length - 1]!] = value;
      this.skipInlineWhitespace();
      const c = this.peek();
      if (c === ",") {
        this.next();
        continue;
      }
      if (c === "}") {
        this.next();
        break;
      }
      throw this.err("expected ',' or '}' in inline table");
    }
    return out;
  }

  // ---- strings -------------------------------------------------------------

  private parseBasicString(): string {
    this.expect('"');
    let out = "";
    while (!this.eof()) {
      const c = this.next();
      if (c === '"') return out;
      if (c === "\n") throw this.err("unterminated string");
      if (c === "\\") {
        out += this.readEscape();
      } else {
        out += c;
      }
    }
    throw this.err("unterminated string");
  }

  private parseMultilineBasicString(): string {
    this.advance(3);
    // A newline immediately after the opening delimiter is trimmed.
    if (this.peek() === "\n") this.next();
    let out = "";
    while (!this.eof()) {
      if (this.startsWith('"""')) {
        this.advance(3);
        // Up to two extra quotes may be part of the content.
        let extra = "";
        while (this.peek() === '"' && extra.length < 2) {
          extra += this.next();
        }
        return out + extra;
      }
      const c = this.next();
      if (c === "\\") {
        // Line-ending backslash trims the newline and following whitespace.
        if (this.peek() === "\n" || /[ \t]/.test(this.peek())) {
          let j = this.i;
          while (j < this.s.length && /[ \t]/.test(this.s[j]!)) j++;
          if (this.s[j] === "\n") {
            this.advanceTo(j + 1);
            while (!this.eof() && /[ \t\n]/.test(this.peek())) this.next();
            continue;
          }
        }
        out += this.readEscape();
      } else {
        out += c;
      }
    }
    throw this.err("unterminated multiline string");
  }

  private parseLiteralString(): string {
    this.expect("'");
    let out = "";
    while (!this.eof()) {
      const c = this.next();
      if (c === "'") return out;
      if (c === "\n") throw this.err("unterminated literal string");
      out += c;
    }
    throw this.err("unterminated literal string");
  }

  private parseMultilineLiteralString(): string {
    this.advance(3);
    if (this.peek() === "\n") this.next();
    let out = "";
    while (!this.eof()) {
      if (this.startsWith("'''")) {
        this.advance(3);
        let extra = "";
        while (this.peek() === "'" && extra.length < 2) {
          extra += this.next();
        }
        return out + extra;
      }
      out += this.next();
    }
    throw this.err("unterminated multiline literal string");
  }

  private readEscape(): string {
    const c = this.next();
    switch (c) {
      case "b":
        return "\b";
      case "t":
        return "\t";
      case "n":
        return "\n";
      case "f":
        return "\f";
      case "r":
        return "\r";
      case '"':
        return '"';
      case "\\":
        return "\\";
      case "u":
        return this.readUnicode(4);
      case "U":
        return this.readUnicode(8);
      default:
        throw this.err(`invalid escape '\\${c}'`);
    }
  }

  private readUnicode(len: number): string {
    let hex = "";
    for (let k = 0; k < len; k++) {
      const c = this.next();
      if (!/[0-9A-Fa-f]/.test(c)) throw this.err("invalid unicode escape");
      hex += c;
    }
    return String.fromCodePoint(parseInt(hex, 16));
  }

  // ---- low-level cursor ----------------------------------------------------

  private eof(): boolean {
    return this.i >= this.s.length;
  }

  private peek(offset = 0): string {
    return this.s[this.i + offset] ?? "";
  }

  private startsWith(str: string): boolean {
    return this.s.startsWith(str, this.i);
  }

  private next(): string {
    const c = this.s[this.i] ?? "";
    this.i++;
    if (c === "\n") {
      this.line++;
      this.col = 1;
    } else {
      this.col++;
    }
    return c;
  }

  private advance(n: number): void {
    for (let k = 0; k < n; k++) this.next();
  }

  private advanceTo(target: number): void {
    while (this.i < target) this.next();
  }

  private expect(ch: string): void {
    if (this.peek() !== ch) throw this.err(`expected '${ch}'`);
    this.next();
  }

  private skipInlineWhitespace(): void {
    while (!this.eof() && /[ \t]/.test(this.peek())) this.next();
  }

  private skipComment(): void {
    if (this.peek() === "#") {
      while (!this.eof() && this.peek() !== "\n") this.next();
    }
  }

  private skipWhitespaceAndComments(): void {
    for (;;) {
      if (this.eof()) return;
      const c = this.peek();
      if (c === " " || c === "\t" || c === "\n") {
        this.next();
      } else if (c === "#") {
        this.skipComment();
      } else {
        return;
      }
    }
  }

  private err(msg: string): Error {
    return new Error(`TOML parse error at line ${this.line}, col ${this.col}: ${msg}`);
  }
}

export function parseToml(input: string): TomlTable {
  return new Parser(input).parse();
}
