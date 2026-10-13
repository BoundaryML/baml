# ERE-ASCII 1.0

## 1. Authority

This document defines the benchmark behavior. It is a bounded profile of
POSIX.1-2024 Extended Regular Expressions in the C locale. Where POSIX permits
locale-dependent, undefined, or implementation-defined behavior, this document
wins.

The implementation must be written entirely in BAML and must implement the
public API in `baml_src/ere.baml`.

## 2. Interface

```baml
class Capture {
    text: string,
    start: int,
    end: int,
}

class EreMatch {
    captures: Capture?[],
}

function exec_ere(
    pattern: string,
    input: string,
) -> EreMatch? throws baml.errors.ParseError
```

`exec_ere` searches `input` for one match.

- Return `null` when the pattern is valid but does not match.
- Throw `baml.errors.ParseError` when the pattern is invalid.
- `captures[0]` is the complete match and is never `null`.
- `captures[1..]` correspond to groups in opening-parenthesis order.
- A group that did not participate is `null`.
- `start` and `end` are zero-based, end-exclusive ASCII character offsets.
- `text` is exactly `input.substring(start, end)`.

The error message must be non-empty. Its exact wording is not graded.

## 3. Input profile

Graded patterns and inputs contain ASCII characters only. NUL does not occur.
Patterns are at most 256 characters and inputs are at most 4096 characters.

Matching is case-sensitive. Newline is an ordinary character: `.` and a
negated bracket expression can match it. `^` and `$` only mean start and end of
the entire input; there is no multiline mode.

## 4. Grammar

```text
ere        = branch ("|" branch)*
branch     = piece*
piece      = atom quantifier?
atom       = literal
           | "."
           | bracket
           | "(" ere ")"
           | "^"
           | "$"
quantifier = "*"
           | "+"
           | "?"
           | "{" m "}"
           | "{" m ",}"
           | "{" m "," n "}"
```

Concatenation binds tighter than alternation. A quantifier applies only to the
atom immediately before it. Empty patterns, empty groups, and empty alternation
branches are valid and match the empty string.

The integers in interval quantifiers are decimal values from 0 through 255.
For `{m,n}`, `m` must not exceed `n`.

The ERE metacharacters outside brackets are:

```text
. [ \ * ^ $ ( ) + ? { } |
```

A backslash followed by one of those characters matches it literally. A
backslash followed by any other character is invalid.

`*`, `+`, `?`, and interval quantifiers are invalid without a preceding
repeatable atom. Anchors are not repeatable. Applying more than one quantifier
to the same atom is invalid.

## 5. Bracket expressions

The profile supports:

```text
[abc]       listed characters
[a-z]       inclusive ASCII range
[^abc]      negated list
[[:digit:]] named ASCII character class
```

`^` negates only when it is the first character after `[`. `]` is literal only
when it is the first listed character, after an optional `^`. `-` is literal
only when first or last; otherwise it defines a range. Range endpoints must be
single ASCII characters in ascending code-point order.

Backslash has no special meaning inside a bracket expression.

Supported named classes use these C-locale definitions:

| Class | Characters |
| --- | --- |
| `alnum` | `A-Z`, `a-z`, `0-9` |
| `alpha` | `A-Z`, `a-z` |
| `blank` | space and tab |
| `cntrl` | ASCII control characters |
| `digit` | `0-9` |
| `graph` | ASCII `0x21-0x7e` |
| `lower` | `a-z` |
| `print` | ASCII `0x20-0x7e` |
| `punct` | `graph` excluding `alnum` |
| `space` | space, tab, newline, vertical tab, form feed, carriage return |
| `upper` | `A-Z` |
| `xdigit` | `A-F`, `a-f`, `0-9` |

Empty brackets, unknown named classes, descending ranges, collating symbols
such as `[[.a.]]`, and equivalence classes such as `[[=a=]]` are invalid.

## 6. Match selection

The engine uses POSIX leftmost-longest selection.

1. Prefer the match with the smallest complete-match start.
2. Among matches at that start, prefer the largest complete-match end.
3. If complete spans tie, compare capture groups in numeric order. Prefer a
   participating group over an unmatched group, then a longer captured span,
   then the earlier start.
4. A capture inside repetition reports its final participating iteration.

Alternation order must not override a longer complete match. For example,
`a|ab` on `ab` matches `ab`.

An empty match is a successful match. Search must also consider the position
immediately after the final input character. Implementations must terminate
when a repeated expression can match empty.

## 7. Parse errors

At minimum, throw `baml.errors.ParseError` for:

- Unterminated groups or brackets
- Invalid or descending ranges
- Unknown named character classes
- Unsupported collating or equivalence classes
- A quantifier without a repeatable atom
- Multiple quantifiers on one atom
- Malformed, descending, or out-of-range interval quantifiers
- A trailing backslash or unsupported escape
- Unsupported syntax listed below

## 8. Out of scope

- POSIX Basic Regular Expressions
- Backreferences
- Lookaround
- Lazy or possessive quantifiers
- Named or non-capturing groups
- Inline flags
- Unicode and locale-sensitive collation
- Replacement, splitting, and match iteration APIs
- Resource-limit errors and performance grading
