/**
 * Curated `baml_language` stdlib signatures, keyed by the symbol/method name a
 * solver would type into `baml describe`. The website worker's LSP hover has no
 * docs for builtins, so this table answers those lookups directly; anything not
 * here falls back to a live hover (which resolves user-defined symbols).
 *
 * Extracted from baml_language/crates/baml_builtins2/baml_std/baml/*.baml.
 */
export const STDLIB_DOCS: Record<string, string> = {
  // strings
  length: 'string.length() -> int   //  also on arrays and maps',
  chars: 'string.chars() -> string[]   //  characters of the string',
  char_at: 'string.char_at(index: int) -> string',
  substring: 'string.substring(start: int, end: int) -> string   //  [start, end)',
  split: 'string.split(delimiter: string) -> string[]',
  lines: 'string.lines() -> string[]',
  to_lower_case: 'string.to_lower_case() -> string',
  to_upper_case: 'string.to_upper_case() -> string',
  trim: 'string.trim() -> string',
  includes: 'string.includes(search: string) -> bool   //  also array.includes(item)',
  starts_with: 'string.starts_with(prefix: string) -> bool',
  ends_with: 'string.ends_with(suffix: string) -> bool',
  replace: 'string.replace(search: string, replacement: string) -> string',
  replace_all: 'string.replace_all(search: string, replacement: string) -> string',
  index_of: 'string.index_of(search: string) -> int?   //  also array.index_of(item)',
  repeat: 'string.repeat(count: int) -> string',
  is_numeric: 'string.is_numeric() -> bool',
  is_alphabetic: 'string.is_alphabetic() -> bool',
  is_alphanumeric: 'string.is_alphanumeric() -> bool',
  is_uppercase: 'string.is_uppercase() -> bool',
  is_lowercase: 'string.is_lowercase() -> bool',
  is_whitespace: 'string.is_whitespace() -> bool',

  // arrays
  at: 'array.at(index: int) -> T?   //  bounds-checked; nums[i] throws out of range',
  push: 'array.push(item: T) -> int   //  appends, returns new length',
  pop: 'array.pop() -> T?',
  shift: 'array.shift() -> T?',
  unshift: 'array.unshift(item: T) -> int',
  remove_at: 'array.remove_at(index: int) -> T?',
  insert: 'array.insert(item: T, idx: int)',
  concat: 'array.concat(other: T[]) -> T[]',
  reverse: 'array.reverse() -> T[]',
  slice: 'array.slice(start: int, end: int) -> T[]   //  also string.slice',
  join: 'array.join(separator: string) -> string',
  sort: 'array.sort() -> T[]   //  ascending; comparable T only (not T[][])',
  filled: 'Array.filled(length: int, value: T) -> T[]',
  last_index_of: 'array.last_index_of(item: T) -> int?',

  // maps
  has: 'map.has(key: K) -> bool',
  keys: 'map.keys() -> K[]',
  values: 'map.values() -> V[]',
  set: 'map.set(key: K, value: V) -> V?',
  get: 'map.get(key: K) -> V?   //  null when the key is absent',
  delete: 'map.delete(key: K) -> V?',
  get_or_insert: 'map.get_or_insert(key: K, default: V) -> V',
  clear: 'map.clear()   //  also array.clear()',

  // ints
  abs: 'int.abs() -> int',
  min: 'int.min(other: int) -> int',
  max: 'int.max(other: int) -> int',
  clamp: 'int.clamp(min: int, max: int) -> int',
  pow: 'int.pow(exp: int) -> int',
  isqrt: 'int.isqrt() -> int',
  parse: 'int.parse(text: string) -> int   //  throws on bad input',
  count_ones: 'int.count_ones() -> int   //  popcount / set bits',
  count_zeros: 'int.count_zeros() -> int',
  max_value: 'int.max_value() -> int',
  min_value: 'int.min_value() -> int',
  to_string: 'value.to_string() -> string',

  // baml.*
  deep_equals: 'baml.deep_equals(a, b) -> bool   //  structural equality',
  stringify: 'baml.json.stringify(j: json) -> string',
  panic: 'baml.sys.panic(message: string)   //  aborts the run',
};

/** Look up a curated stdlib doc by the last identifier in `expr`, if present. */
export function stdlibDoc(expr: string): string | null {
  const ids = [...expr.matchAll(/[A-Za-z_]\w*/g)].map((m) => m[0]);
  for (let i = ids.length - 1; i >= 0; i--) {
    const doc = STDLIB_DOCS[ids[i]];
    if (doc) return doc;
  }
  return null;
}
