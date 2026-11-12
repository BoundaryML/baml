//! The formatter's document IR and the printer that lays it out.
//!
//! A formatter rule does not decide where lines break. It builds a *document*
//! that says where a break *may* go and how the pieces nest, and the printer
//! decides, measuring each group against the line width. That split — Wadler's
//! prettier-printing, as used by Prettier and Biome — is what makes layout
//! decisions compose: a rule never needs to know how much of the line its
//! parent has already used.
//!
//! # Documents
//!
//! A [`Doc`] is an immutable tree node, allocated in a bump arena through
//! [`Docs`] and shared by reference: copying a `Doc` shares it (which is how
//! [`Docs::best_fitting`] variants share their common parts), the builders
//! take the copyable `Docs` handle by value, and a node knows at creation
//! whether it contains a forced break, since its children exist before it
//! does. Leaves with no payload are constants on `Doc`.
//!
//! * `text` — a run of text, borrowed: a token of the file, read in place, or
//!   a literal; `copy_text` copies a string the rule computed into the arena.
//!   The display width is fixed at creation. Text with a newline in it (a
//!   multi-line string, a block comment) is printed as written, its later
//!   lines at their own columns, and counts as a forced break.
//! * `verbatim` — source text with a newline in it whose later lines move
//!   with the node: each keeps its indentation relative to the line the
//!   text started on in the source, measured from the indent the node is
//!   printed at. For a node a rule has no layout for yet, printed deeper or
//!   shallower than it was written.
//! * `literal` — a multi-line literal the rule laid out itself (a backtick
//!   string), copied in: its later lines are indented as written relative
//!   to the line it starts on, whatever indents enclose it, and it is an
//!   atom to the layout around it — no forced break, measured by its first
//!   line — so a call hugs it as it hugs a long string.
//! * `Doc::SPACE`, `Doc::SOFT_LINE`, `Doc::SOFT_LINE_OR_SPACE`,
//!   `Doc::HARD_LINE`, `Doc::EMPTY_LINE` — a soft line is nothing when its
//!   group is flat and a newline when the group breaks; `SOFT_LINE_OR_SPACE`
//!   is a space when flat; a hard line is always a newline; an empty line
//!   leaves one blank line.
//! * `group` — the unit of layout: printed flat if everything in it fits on
//!   the rest of the current line, otherwise every soft line in it breaks.
//! * `indent` — one indent level deeper for the lines that start inside it.
//! * `if_break` / `if_group_breaks` — content that exists only in one of the
//!   two layouts (a trailing comma, say), judged by the enclosing group or by
//!   a named group: a [`GroupId`] is the group node itself, so it can only
//!   come from `group_with_id` and can never name anything else.
//! * `line_suffix` — text deferred to the end of the current line (a
//!   trailing `// comment`, which must follow the `,` its node is printed
//!   with), counted in the fits-check or not per [`SuffixWidth`];
//!   `trailing_line` — text deferred to a line of its own after the current
//!   line, at the indent it was emitted at (a comment on its own line after
//!   the last item of a block, which must still follow the `;` printed with
//!   that item); and `Doc::LINE_SUFFIX_BOUNDARY`, which ends the line if a
//!   suffix is pending.
//! * `Doc::EXPAND_PARENT` — forces the enclosing group to break.
//! * `best_fitting` — ordered layouts, most flat first: the printer prints
//!   the first one whose first line fits, with what follows it on that line,
//!   or the last one if none does. This is how "all arguments on one line /
//!   hug the last argument / one per line" is expressed without any rule
//!   computing widths itself.
//!
//! # Printing
//!
//! A group containing a forced break (a hard or empty line, an
//! `EXPAND_PARENT`, a multi-line `text` or `line_suffix`, a `trailing_line`,
//! an `expanded_group`, an `if_break` with one in either arm) always prints
//! broken, so `if (c) { … }` with a block body breaks its enclosing group
//! without the rule saying so. The flag does not cross into `best_fitting`
//! variants: a forced break inside a variant is that variant's own business,
//! and the most flat variant never has one.
//!
//! The printer recurses over the document, passing each node the layout and
//! indent it is printed in and, borrowed from the enclosing concats, what
//! follows it. Its state is only what it has produced: the output items and
//! the layout each named group was given. A group's fits-check is a pure
//! function of that state — it measures the group flat together with what
//! follows it up to the first line break, and so does the check of a
//! `best_fitting` variant, as in Prettier and Biome.
//!
//! The output is a flat sequence of borrowed text, deferred line ends and
//! newlines, written once at the end. It keeps these promises: a line the
//! printer ends has no trailing spaces (a line suffix brings its own leading
//! space; the lines inside a multi-line text are kept exactly); a blank line
//! carries no indentation; there is never more than one blank line in a row,
//! since a hard line on a line with nothing on it only sets the indent and an
//! empty line is idempotent; a line end before the first content prints
//! nothing; and a newline inside a text never flushes the pending suffixes,
//! which render at the end of the line the printer ends. Widths are display
//! columns (`unicode-width`), not bytes; the line width and indent come from
//! the formatter's [`FormatOptions`](crate::FormatOptions).

mod doc;
mod printer;

#[cfg(test)]
mod tests;

pub(crate) use doc::display_width;
pub use doc::{Doc, Docs, GroupId, SuffixWidth};
