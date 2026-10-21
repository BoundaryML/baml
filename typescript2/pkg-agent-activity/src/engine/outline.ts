// Declarations in a source file: what a reader would put in an outline.
//
// This is a line scanner, not a parser. It tracks nesting through a small
// lexer that skips strings, comments, BAML/Rust raw strings, and template
// literals, then matches declaration shapes per language at the depths where
// declarations live. It is tolerant by design: the agent is mid-edit most of
// the time, so the text is often unbalanced or does not compile.

export type SymbolKind =
  | 'class'
  | 'struct'
  | 'enum'
  | 'interface'
  | 'trait'
  | 'type'
  | 'function'
  | 'method'
  | 'impl'
  | 'const'
  | 'client'
  | 'test'
  | 'template'
  | 'module'
  | 'heading'
  | 'table';

export interface Member {
  name: string;
  /** Normalized one-line text, e.g. `name string @description("…")`. */
  text: string;
  line: number;
}

export interface OutlineSymbol {
  /** Identity within the file: stable across edits that keep the name. */
  key: string;
  name: string;
  kind: SymbolKind;
  /** Enclosing impl, class, trait, or heading. */
  container?: string;
  /** Normalized declaration text without the body. */
  signature: string;
  /** 0-based inclusive line range. */
  start: number;
  end: number;
  members: Member[];
  /** Nesting level: 0 at file scope, 1 inside an impl or class. */
  level: number;
}

export type Language =
  | 'baml'
  | 'rust'
  | 'typescript'
  | 'python'
  | 'go'
  | 'markdown'
  | 'toml'
  | 'json'
  | 'yaml'
  | 'css'
  | 'shell'
  | 'text';

export function languageOf(path: string): Language {
  const lower = path.toLowerCase();
  const ext = lower.slice(lower.lastIndexOf('.') + 1);
  switch (ext) {
    case 'baml':
      return 'baml';
    case 'rs':
      return 'rust';
    case 'ts':
    case 'tsx':
    case 'js':
    case 'jsx':
    case 'mjs':
    case 'cjs':
    case 'mts':
      return 'typescript';
    case 'py':
    case 'pyi':
      return 'python';
    case 'go':
      return 'go';
    case 'md':
    case 'mdx':
    case 'markdown':
      return 'markdown';
    case 'toml':
      return 'toml';
    case 'json':
    case 'jsonc':
      return 'json';
    case 'yaml':
    case 'yml':
      return 'yaml';
    case 'css':
    case 'scss':
      return 'css';
    case 'sh':
    case 'bash':
    case 'zsh':
      return 'shell';
    default:
      return 'text';
  }
}

// ── Lexing ──────────────────────────────────────────────────────────────────

interface LexState {
  /** Closing delimiter of a multi-line construct we are inside, if any. */
  until: string | null;
}

interface LexedLine {
  /** The line with strings and comments blanked out (same length). */
  code: string;
  /** Brace depth before the first character of the line. */
  depthBefore: number;
  depthAfter: number;
}

function lexLines(lines: string[], lang: Language): LexedLine[] {
  const state: LexState = { until: null };
  const out: LexedLine[] = [];
  let depth = 0;
  const lineComment =
    lang === 'python' || lang === 'toml' || lang === 'yaml' || lang === 'shell'
      ? '#'
      : '//';
  for (const line of lines) {
    const depthBefore = depth;
    let code = '';
    let i = 0;
    while (i < line.length) {
      if (state.until) {
        const close = line.indexOf(state.until, i);
        if (close < 0) {
          code += ' '.repeat(line.length - i);
          i = line.length;
          break;
        }
        code += ' '.repeat(close + state.until.length - i);
        i = close + state.until.length;
        state.until = null;
        continue;
      }
      const rest = line.slice(i);
      if (rest.startsWith(lineComment)) {
        code += ' '.repeat(line.length - i);
        break;
      }
      if (lang !== 'python' && rest.startsWith('/*')) {
        state.until = '*/';
        code += '  ';
        i += 2;
        continue;
      }
      // BAML block strings (#"…"#, ##"…"##) and Rust raw strings (r#"…"#).
      const raw = /^r?(#+)"/.exec(rest);
      if (raw && (lang === 'baml' || lang === 'rust')) {
        state.until = `"${raw[1]}`;
        code += ' '.repeat(raw[0].length);
        i += raw[0].length;
        continue;
      }
      if (
        lang === 'python' &&
        (rest.startsWith('"""') || rest.startsWith("'''"))
      ) {
        state.until = rest.slice(0, 3);
        code += '   ';
        i += 3;
        continue;
      }
      const ch = line[i];
      // Template strings: TypeScript's, and BAML's backtick prompts.
      if (ch === '`' && (lang === 'typescript' || lang === 'baml')) {
        state.until = '`';
        code += ' ';
        i += 1;
        continue;
      }
      if (ch === '"' || (ch === "'" && lang !== 'rust')) {
        // Single-line string; skip escapes. Rust lifetimes ('a) are left alone.
        let j = i + 1;
        while (j < line.length && line[j] !== ch) j += line[j] === '\\' ? 2 : 1;
        code += ' '.repeat(Math.min(j + 1, line.length) - i);
        i = j + 1;
        continue;
      }
      if (ch === '{') depth++;
      else if (ch === '}') depth = Math.max(0, depth - 1);
      code += ch;
      i++;
    }
    out.push({ code, depthAfter: depth, depthBefore });
  }
  return out;
}

/** Last line of the block whose opening brace is on or after `start`. */
function blockEnd(lexed: LexedLine[], start: number): number {
  const base = lexed[start].depthBefore;
  let opened = false;
  for (let i = start; i < lexed.length; i++) {
    const l = lexed[i];
    if (!opened) {
      if (l.code.includes('{')) opened = true;
      else if (
        /[;=]\s*$/.test(l.code.trim()) ||
        (i > start && l.code.trim() === '')
      )
        return i;
      else if (i - start > 12) return start;
    }
    if (opened && l.depthAfter <= base) return i;
  }
  return lexed.length - 1;
}

const collapse = (s: string) => s.replace(/\s+/g, ' ').trim();

/** Declaration text from `start` up to (not including) the body's opening brace. */
function signatureFrom(
  lines: string[],
  lexed: LexedLine[],
  start: number,
): string {
  let text = '';
  for (let i = start; i < Math.min(lines.length, start + 8); i++) {
    const code = lexed[i].code;
    const brace = code.indexOf('{');
    if (brace >= 0) {
      text += ` ${lines[i].slice(0, brace)}`;
      return collapse(text).replace(/\s*(where)?\s*$/, '');
    }
    text += ` ${lines[i]}`;
    if (/[;=]\s*$/.test(code.trim())) break;
  }
  return collapse(text).replace(/[;{]\s*$/, '');
}

// ── Per-language declaration shapes ─────────────────────────────────────────

interface Shape {
  re: RegExp;
  kind: SymbolKind;
  /** Where the body's members are: fields for records, methods for containers. */
  body?: 'fields' | 'variants' | 'methods';
  /** The name is a string literal, which lexing blanks; match the raw line. */
  quotedName?: boolean;
}

const SHAPES: Partial<Record<Language, Shape[]>> = {
  baml: [
    { body: 'fields', kind: 'class', re: /^\s*class\s+([A-Za-z_]\w*)/ },
    { body: 'variants', kind: 'enum', re: /^\s*enum\s+([A-Za-z_]\w*)/ },
    {
      body: 'methods',
      kind: 'interface',
      re: /^\s*interface\s+([A-Za-z_]\w*)/,
    },
    { kind: 'function', re: /^\s*function\s+([A-Za-z_]\w*)/ },
    { kind: 'client', re: /^\s*(?:client<\w+>|client)\s+([A-Za-z_][\w-]*)/ },
    // Test names are identifiers or quoted strings: `test "parses a date" { … }`.
    {
      kind: 'test',
      quotedName: true,
      re: /^\s*test\s+"?([^"{\s][^"{]*?)"?\s*\{?\s*$/,
    },
    { kind: 'template', re: /^\s*template_string\s+([A-Za-z_]\w*)/ },
    { kind: 'type', re: /^\s*type\s+([A-Za-z_]\w*)/ },
    {
      body: 'methods',
      kind: 'impl',
      re: /^\s*impl\b(?:<[^>]*>)?\s+([\w.<>, ]+?)(?:\s+for\s+([\w.<>, ]+))?\s*\{?\s*$/,
    },
    { kind: 'const', re: /^\s*(?:let|const)\s+([A-Za-z_]\w*)/ },
    { kind: 'const', re: /^\s*generator\s+([A-Za-z_]\w*)/ },
    { kind: 'const', re: /^\s*retry_policy\s+([A-Za-z_]\w*)/ },
  ],
  go: [
    {
      body: 'fields',
      kind: 'struct',
      re: /^\s*type\s+([A-Za-z_]\w*)\s+struct\b/,
    },
    {
      body: 'fields',
      kind: 'interface',
      re: /^\s*type\s+([A-Za-z_]\w*)\s+interface\b/,
    },
    { kind: 'type', re: /^\s*type\s+([A-Za-z_]\w*)/ },
    {
      kind: 'method',
      re: /^\s*func\s+\([^)]*?\*?\s*([A-Za-z_]\w*)\)\s+([A-Za-z_]\w*)/,
    },
    { kind: 'function', re: /^\s*func\s+([A-Za-z_]\w*)/ },
  ],
  rust: [
    {
      body: 'fields',
      kind: 'struct',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_]\w*)/,
    },
    {
      body: 'variants',
      kind: 'enum',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_]\w*)/,
    },
    {
      body: 'methods',
      kind: 'trait',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+([A-Za-z_]\w*)/,
    },
    { kind: 'type', re: /^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_]\w*)/ },
    {
      kind: 'function',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z_]\w*)/,
    },
    {
      body: 'methods',
      kind: 'impl',
      re: /^\s*(?:unsafe\s+)?impl\b(?:<[^>]*>)?\s+([^{]+?)\s*(?:where\b.*)?\{?\s*$/,
    },
    {
      kind: 'const',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Z_][A-Z0-9_]*)\s*:/,
    },
    {
      body: 'methods',
      kind: 'module',
      re: /^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_]\w*)\s*\{/,
    },
    { kind: 'function', re: /^\s*macro_rules!\s+([A-Za-z_]\w*)/ },
  ],
  typescript: [
    {
      body: 'methods',
      kind: 'class',
      re: /^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)/,
    },
    {
      body: 'fields',
      kind: 'interface',
      re: /^\s*(?:export\s+)?(?:declare\s+)?interface\s+([A-Za-z_$][\w$]*)/,
    },
    {
      kind: 'type',
      re: /^\s*(?:export\s+)?(?:declare\s+)?type\s+([A-Za-z_$][\w$]*)/,
    },
    {
      body: 'variants',
      kind: 'enum',
      re: /^\s*(?:export\s+)?(?:declare\s+)?(?:const\s+)?enum\s+([A-Za-z_$][\w$]*)/,
    },
    {
      kind: 'function',
      re: /^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)/,
    },
    {
      kind: 'function',
      re: /^\s*(?:export\s+)?const\s+([A-Za-z_$][\w$]*)\s*(?::[^=]+)?=\s*(?:async\s+)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*(?::[^=]+)?=>/,
    },
    {
      kind: 'function',
      re: /^\s*(?:export\s+)?const\s+([A-Za-z_$][\w$]*)\s*=\s*(?:React\.)?(?:memo|forwardRef|lazy)\b/,
    },
    { kind: 'const', re: /^\s*export\s+(?:const|let)\s+([A-Za-z_$][\w$]*)/ },
  ],
};

const METHOD_SHAPES: Partial<Record<Language, RegExp[]>> = {
  baml: [/^\s*function\s+([A-Za-z_]\w*)/],
  rust: [
    /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_]\w*)/,
    /^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_]\w*)/,
  ],
  typescript: [
    /^\s*(?:public\s+|private\s+|protected\s+|static\s+|readonly\s+|async\s+|override\s+|get\s+|set\s+)*(?:#?[A-Za-z_$][\w$]*)\s*(?:<[^>]*>)?\s*\(/,
  ],
};

// ── Extraction ──────────────────────────────────────────────────────────────

export function extractOutline(
  path: string,
  text: string | null,
): OutlineSymbol[] {
  if (text === null) return [];
  const lang = languageOf(path);
  const lines = text.split('\n');
  switch (lang) {
    case 'markdown':
      return markdownOutline(lines);
    case 'python':
      return pythonOutline(lines);
    case 'toml':
      return tomlOutline(lines);
    default:
      break;
  }
  const shapes = SHAPES[lang];
  if (!shapes) return [];
  const lexed = lexLines(lines, lang);
  const symbols: OutlineSymbol[] = [];
  const seen = new Map<string, number>();
  const keyFor = (base: string) => {
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return n === 0 ? base : `${base}#${n + 1}`;
  };

  for (let i = 0; i < lines.length; i++) {
    const l = lexed[i];
    if (l.depthBefore !== 0) continue;
    const code = l.code;
    if (!code.trim()) continue;
    for (const shape of shapes) {
      const m = shape.re.exec(
        (shape.quotedName ? lines[i] : code).replace(/\s+$/, ''),
      );
      if (!m) continue;
      // `code` has strings blanked, so recover names from the original line.
      const original = shape.re.exec(lines[i]) ?? m;
      let name = original[1]?.trim() ?? m[1]?.trim() ?? '';
      const kind = shape.kind;
      let container: string | undefined;
      if (kind === 'impl') {
        const trait = original[2]?.trim();
        name = trait ? `${trait} for ${original[1].trim()}` : name;
        if (lang === 'rust') {
          const forMatch = /^(.*?)\s+for\s+(.*)$/.exec(name);
          name = forMatch
            ? `${forMatch[1].trim()} for ${forMatch[2].trim()}`
            : name;
        }
      }
      if (kind === 'method' && lang === 'go') {
        container = original[1];
        name = original[2];
      }
      if (!name) continue;
      const end = blockEnd(lexed, i);
      const signature = signatureFrom(lines, lexed, i);
      const sym: OutlineSymbol = {
        container,
        end,
        key: keyFor(`${container ? `${container}.` : ''}${kind}:${name}`),
        kind,
        level: 0,
        members: [],
        name,
        signature,
        start: i,
      };
      symbols.push(sym);
      if (shape.body === 'fields' || shape.body === 'variants') {
        sym.members = recordMembers(lines, lexed, i, end, lang, shape.body);
      } else if (shape.body === 'methods') {
        symbols.push(
          ...containerMethods(lines, lexed, i, end, lang, name, keyFor),
        );
      }
      break;
    }
  }
  return symbols;
}

function recordMembers(
  lines: string[],
  lexed: LexedLine[],
  start: number,
  end: number,
  lang: Language,
  body: 'fields' | 'variants',
): Member[] {
  const members: Member[] = [];
  const inner = lexed[start].depthBefore + 1;
  for (let i = start + 1; i < end; i++) {
    if (lexed[i].depthBefore !== inner) continue;
    const code = lexed[i].code.trim();
    if (
      !code ||
      code.startsWith('}') ||
      code.startsWith('@@') ||
      code.startsWith('#[')
    )
      continue;
    let name: string | undefined;
    if (lang === 'baml') {
      name = /^([A-Za-z_]\w*)/.exec(code)?.[1];
    } else if (lang === 'rust') {
      name =
        body === 'fields'
          ? /^(?:pub(?:\([^)]*\))?\s+)?([A-Za-z_]\w*)\s*:/.exec(code)?.[1]
          : /^([A-Z]\w*)/.exec(code)?.[1];
    } else if (lang === 'typescript') {
      name =
        body === 'fields'
          ? /^(?:readonly\s+)?([A-Za-z_$][\w$]*)\??\s*[:(]/.exec(code)?.[1]
          : /^([A-Za-z_$][\w$]*)/.exec(code)?.[1];
    } else if (lang === 'go') {
      name = /^([A-Za-z_]\w*)/.exec(code)?.[1];
    }
    if (!name) continue;
    members.push({ line: i, name, text: collapse(lines[i]).replace(/,$/, '') });
  }
  return members;
}

function containerMethods(
  lines: string[],
  lexed: LexedLine[],
  start: number,
  end: number,
  lang: Language,
  container: string,
  keyFor: (base: string) => string,
): OutlineSymbol[] {
  const shapes = METHOD_SHAPES[lang];
  if (!shapes) return [];
  const out: OutlineSymbol[] = [];
  const inner = lexed[start].depthBefore + 1;
  const owner = container.replace(/^.* for /, '').replace(/<.*$/, '');
  for (let i = start + 1; i < end; i++) {
    if (lexed[i].depthBefore !== inner) continue;
    const code = lexed[i].code;
    for (const re of shapes) {
      const m = re.exec(lines[i]) ?? re.exec(code);
      if (!m) continue;
      let name = m[1];
      if (!name && lang === 'typescript') {
        name = /([#A-Za-z_$][\w$]*)\s*(?:<[^>]*>)?\s*\(/.exec(code)?.[1] ?? '';
        if (
          [
            'if',
            'for',
            'while',
            'switch',
            'return',
            'catch',
            'super',
            'constructor',
          ].includes(name) &&
          name !== 'constructor'
        )
          continue;
      }
      if (!name) continue;
      const methodEnd = blockEnd(lexed, i);
      out.push({
        container: owner,
        end: methodEnd,
        key: keyFor(`${owner}.method:${name}`),
        kind: 'method',
        level: 1,
        members: [],
        name,
        signature: signatureFrom(lines, lexed, i),
        start: i,
      });
      i = Math.max(i, methodEnd);
      break;
    }
  }
  return out;
}

function markdownOutline(lines: string[]): OutlineSymbol[] {
  const out: OutlineSymbol[] = [];
  let fence = false;
  const heads: { level: number; sym: OutlineSymbol }[] = [];
  lines.forEach((line, i) => {
    if (/^\s*(```|~~~)/.test(line)) fence = !fence;
    if (fence) return;
    const m = /^(#{1,4})\s+(.+?)\s*#*\s*$/.exec(line);
    if (!m) return;
    const level = m[1].length;
    const sym: OutlineSymbol = {
      end: lines.length - 1,
      key: `heading:${m[2]}`,
      kind: 'heading',
      level: level - 1,
      members: [],
      name: m[2],
      signature: `${m[1]} ${m[2]}`,
      start: i,
    };
    for (let h = heads.length - 1; h >= 0; h--) {
      if (heads[h].level >= level && heads[h].sym.end === lines.length - 1)
        heads[h].sym.end = i - 1;
    }
    const parent = [...heads].reverse().find((h) => h.level < level);
    if (parent) sym.container = parent.sym.name;
    heads.push({ level, sym });
    out.push(sym);
  });
  return out;
}

function pythonOutline(lines: string[]): OutlineSymbol[] {
  const out: OutlineSymbol[] = [];
  const indentOf = (s: string) => s.length - s.trimStart().length;
  const endOf = (start: number) => {
    const indent = indentOf(lines[start]);
    let last = start;
    for (let i = start + 1; i < lines.length; i++) {
      if (!lines[i].trim()) continue;
      if (indentOf(lines[i]) <= indent) break;
      last = i;
    }
    return last;
  };
  let currentClass: {
    name: string;
    indent: number;
    sym: OutlineSymbol;
  } | null = null;
  lines.forEach((line, i) => {
    if (!line.trim()) return;
    const indent = indentOf(line);
    if (currentClass && indent <= currentClass.indent) currentClass = null;
    const cls = /^class\s+([A-Za-z_]\w*)/.exec(line);
    if (cls) {
      const sym: OutlineSymbol = {
        end: endOf(i),
        key: `class:${cls[1]}`,
        kind: 'class',
        level: 0,
        members: [],
        name: cls[1],
        signature: collapse(line).replace(/:\s*$/, ''),
        start: i,
      };
      out.push(sym);
      currentClass = { indent, name: cls[1], sym };
      return;
    }
    const def = /^(\s*)(?:async\s+)?def\s+([A-Za-z_]\w*)/.exec(line);
    if (def) {
      const inClass = currentClass && indent > currentClass.indent;
      if (!inClass && indent > 0) return;
      out.push({
        container: inClass ? currentClass!.name : undefined,
        end: endOf(i),
        key: inClass
          ? `${currentClass!.name}.method:${def[2]}`
          : `function:${def[2]}`,
        kind: inClass ? 'method' : 'function',
        level: inClass ? 1 : 0,
        members: [],
        name: def[2],
        signature: collapse(line).replace(/:\s*$/, ''),
        start: i,
      });
      return;
    }
    if (currentClass && indent > currentClass.indent) {
      const field = /^\s*([A-Za-z_]\w*)\s*:\s*[^=]+/.exec(line);
      if (field && indent === currentClass.indent + 4) {
        currentClass.sym.members.push({
          line: i,
          name: field[1],
          text: collapse(line),
        });
      }
    }
  });
  return out;
}

function tomlOutline(lines: string[]): OutlineSymbol[] {
  const out: OutlineSymbol[] = [];
  lines.forEach((line, i) => {
    const m = /^\s*\[\[?([^\]]+)\]\]?/.exec(line);
    if (!m) return;
    const previous = out.at(-1);
    if (previous) previous.end = i - 1;
    out.push({
      end: lines.length - 1,
      key: `table:${m[1]}`,
      kind: 'table',
      level: 0,
      members: [],
      name: m[1],
      signature: line.trim(),
      start: i,
    });
  });
  for (const sym of out) {
    for (let i = sym.start + 1; i <= sym.end; i++) {
      const kv = /^\s*([\w.-]+)\s*=/.exec(lines[i]);
      if (kv)
        sym.members.push({ line: i, name: kv[1], text: collapse(lines[i]) });
    }
  }
  return out;
}

// ── Comparing outlines ──────────────────────────────────────────────────────

export type SymbolChangeKind =
  | 'added'
  | 'removed'
  | 'signature'
  | 'members'
  | 'body';

export interface MemberChange {
  name: string;
  kind: 'added' | 'removed' | 'changed';
  before?: string;
  after?: string;
}

export interface SymbolChange {
  key: string;
  name: string;
  kind: SymbolKind;
  container?: string;
  change: SymbolChangeKind;
  before?: OutlineSymbol;
  after?: OutlineSymbol;
  members: MemberChange[];
}

/**
 * Declarations that differ between two outlines of one file. `touchedNew` is
 * the set of new-text line numbers the edit changed; a declaration whose
 * signature and members are unchanged but whose range contains one of them
 * reports a `body` change.
 */
export function compareOutlines(
  before: OutlineSymbol[],
  after: OutlineSymbol[],
  touchedNew: Set<number>,
  touchedOld: Set<number>,
): SymbolChange[] {
  const byKeyBefore = new Map(before.map((s) => [s.key, s]));
  const byKeyAfter = new Map(after.map((s) => [s.key, s]));
  const changes: SymbolChange[] = [];
  for (const s of after) {
    const prev = byKeyBefore.get(s.key);
    if (!prev) {
      changes.push({
        after: s,
        change: 'added',
        container: s.container,
        key: s.key,
        kind: s.kind,
        members: [],
        name: s.name,
      });
      continue;
    }
    const members = compareMembers(prev.members, s.members);
    const common = {
      after: s,
      before: prev,
      container: s.container,
      key: s.key,
      kind: s.kind,
      name: s.name,
    };
    if (prev.signature !== s.signature) {
      changes.push({ ...common, change: 'signature', members });
    } else if (members.length) {
      changes.push({ ...common, change: 'members', members });
    } else if (rangeTouched(s, touchedNew) || rangeTouched(prev, touchedOld)) {
      changes.push({ ...common, change: 'body', members: [] });
    }
  }
  for (const s of before) {
    if (!byKeyAfter.has(s.key)) {
      changes.push({
        before: s,
        change: 'removed',
        container: s.container,
        key: s.key,
        kind: s.kind,
        members: [],
        name: s.name,
      });
    }
  }
  return changes;
}

function rangeTouched(s: OutlineSymbol, lines: Set<number>): boolean {
  for (const line of lines) if (line >= s.start && line <= s.end) return true;
  return false;
}

function compareMembers(before: Member[], after: Member[]): MemberChange[] {
  const b = new Map(before.map((m) => [m.name, m]));
  const a = new Map(after.map((m) => [m.name, m]));
  const out: MemberChange[] = [];
  for (const m of after) {
    const prev = b.get(m.name);
    if (!prev) out.push({ after: m.text, kind: 'added', name: m.name });
    else if (prev.text !== m.text)
      out.push({
        after: m.text,
        before: prev.text,
        kind: 'changed',
        name: m.name,
      });
  }
  for (const m of before)
    if (!a.has(m.name))
      out.push({ before: m.text, kind: 'removed', name: m.name });
  return out;
}
