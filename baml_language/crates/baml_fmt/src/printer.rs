use rowan::TextRange;

pub use crate::EmittableTrivia;
use crate::{FormatOptions, TriviaInfo, ast::Token};

pub struct Printer<'a> {
    pub input: &'a str,
    pub config: &'a FormatOptions,
    pub output: String,
    pub trivia: &'a TriviaInfo,
    pub warnings: Vec<PrinterWarning>,
}
impl<'a> Printer<'a> {
    #[inline]
    #[must_use]
    pub fn new_empty(input: &'a str, config: &'a FormatOptions, trivia: &'a TriviaInfo) -> Self {
        Printer {
            input,
            config,
            output: String::new(),
            trivia,
            warnings: Vec::new(),
        }
    }

    #[inline]
    #[must_use]
    pub fn new(
        input: &'a str,
        config: &'a FormatOptions,
        output: String,
        trivia: &'a TriviaInfo,
    ) -> Self {
        Printer {
            input,
            config,
            output,
            trivia,
            warnings: Vec::new(),
        }
    }

    /// Prints some number of spaces. Useful for indentation.
    #[inline]
    pub fn print_spaces(&mut self, num: usize) {
        self.output.extend(std::iter::repeat_n(' ', num));
    }

    #[inline]
    pub fn print_newline(&mut self) {
        self.output.push('\n');
    }

    /// Prints the token byte-for-byte from the input string.
    ///
    /// For tokens like [`crate::ast::RawString`] that implement [`Printable`], you generally should [`Self::print`].
    pub fn print_raw_token(&mut self, token: &impl Token) {
        let text = &self.input[token.span()];
        self.output.push_str(text);
    }

    /// Prints the text byte-for-byte.
    #[inline]
    pub fn print_str(&mut self, text: &str) {
        self.output.push_str(text);
    }

    /// Should try to print the given element in the given shape.
    ///
    /// Tries to print the element single-line first, then multi-line if it doesn't fit.
    #[allow(unused_must_use)]
    pub fn print(&mut self, printable: &impl Printable, shape: Shape) -> PrintInfo {
        printable.print(shape, self)
    }

    /// Prints the given range of the input string, byte-for-byte.
    #[allow(unused_must_use)]
    pub fn print_input_range(&mut self, range: TextRange) {
        let text = &self.input[range];
        self.output.push_str(text);
    }

    /// Prints a text range with leading whitespace stripped from the first line.
    ///
    /// Continuation lines are printed verbatim at their original absolute
    /// positions, which keeps the output idempotent: the caller already
    /// provides indentation for the first line, and subsequent lines are
    /// stable across formatting passes.
    pub fn print_input_range_trimmed_start(&mut self, range: TextRange) {
        let text = &self.input[range];
        self.output.push_str(text.trim_start());
    }

    /// Prints an emittable trivia, without a newline.
    ///
    /// Empty lines print nothing, while the comments print their comment.
    pub fn print_trivia(&mut self, trivia: &EmittableTrivia) {
        match trivia {
            EmittableTrivia::EmptyLine { .. } | EmittableTrivia::EmptyLineBeforeEOF => {}
            EmittableTrivia::CommentBeforeEOF { comment }
            | EmittableTrivia::LeadingBlockComment { comment, .. }
            | EmittableTrivia::LeadingLineComment { comment, .. }
            | EmittableTrivia::TrailingBlockComment { comment, .. }
            | EmittableTrivia::TrailingLineComment { comment, .. } => {
                self.print_input_range(*comment);
            }
        }
    }

    /// Prints all the passed emitted trivia, each indented by `indent` spaces and followed by a newline.
    ///
    /// Example with indent 4:
    /// ```baml
    ///    // <-- added indent but not newline
    ///    // second comment
    ///
    ///    // ^^ no indent on empty line
    ///    /* multiline
    /// comment */
    ///    // includes trailing newline (if anything was printed) vvv
    /// ```
    pub fn print_trivia_with_newline(&mut self, trivia: &[EmittableTrivia], indent: usize) {
        for trivia in trivia {
            if trivia.is_comment() {
                self.print_spaces(indent);
            }
            self.print_trivia(trivia);
            self.print_newline();
        }
    }

    /// Prints all leading emittable trivia for the given range.
    /// intended for comments each on their own lines.
    ///
    /// Newlines are printed between each trivia (except empty lines).
    /// Each non-empty line is indented by `indent` spaces.
    ///
    /// Example with indent 4:
    /// ```baml
    ///    // <-- added indent but not newline
    ///    // second comment
    ///
    ///    // ^^ no indent on empty line
    ///    // includes trailing newline (if anything was printed) vvv
    ///
    /// ```
    ///
    /// Returns the number of trivia items printed.
    #[allow(unused_must_use)]
    pub fn print_trivia_all_leading_with_newline_for(
        &mut self,
        range: TextRange,
        indent: usize,
    ) -> usize {
        let (leading, _) = self.trivia.get_for_range_split(range);
        self.print_trivia_with_newline(leading, indent);
        leading.len()
    }

    /// Prints all the trivia as trailing comments
    /// Each trivia gets one space before it.
    ///
    /// This is useful for printing trailing trivia at the end of a line.
    ///
    /// ```baml
    /// let x; /* first trivia */ /* second trivia */ // third trivia
    /// ```
    ///
    /// Returns `true` if there was a line comment.
    #[allow(unused_must_use)]
    pub fn print_trivia_trailing(&mut self, trivia: &[EmittableTrivia]) -> bool {
        let mut has_line_comment = false;
        for trivia in trivia {
            if matches!(trivia, EmittableTrivia::TrailingLineComment { .. }) {
                has_line_comment = true;
            }
            self.print_spaces(1);
            self.print_trivia(trivia);
        }
        has_line_comment
    }

    /// Prints all trailing trivia attached to the given range.
    /// Each trivia gets one space before it.
    ///
    /// This is useful for printing trailing trivia at the end of a line.
    ///
    /// ```baml
    /// let x; /* first trivia */ /* second trivia */ // third trivia
    /// ```
    ///
    /// Convenience method for [`Self::print_trivia_trailing`]
    ///
    /// Returns `(num_trivia, has_line_comment)`.
    #[allow(unused_must_use)]
    pub fn print_trivia_all_trailing_for(&mut self, range: TextRange) -> (usize, bool) {
        let (_, trailing) = self.trivia.get_for_range_split(range);
        let has_line_comment = self.print_trivia_trailing(trailing);
        (trailing.len(), has_line_comment)
    }

    /// For standalone items which are fully on their own line (and may be multiline), print them with all their trivia.
    /// For example, a function definition or a statement in a block.
    ///
    /// This is basically the combination of
    /// - [`Self::print_trivia_all_leading_with_newline_for`]
    /// - [`Self::print`]
    /// - [`Self::print_trivia_all_trailing_for`]
    ///
    /// Example:
    /// ```baml
    ///     // <-- adds indentation at start
    ///     // <- note the indent
    ///     let x = {
    ///         "printable may be multiline, or not"
    ///     }; // trailing trivia, no newline at end -->
    /// ```
    pub fn print_standalone_with_trivia(&mut self, printable: &impl Printable, indent: usize) {
        let (leading_trivia, trailing_trivia) = self.trivia.get_for_element(printable);
        self.print_trivia_with_newline(leading_trivia, indent);
        self.print_spaces(indent);
        let shape = Shape {
            width: self.config.line_width.saturating_sub(indent),
            indent,
            first_line_offset: 0,
        };
        self.print(printable, shape);
        self.print_trivia_trailing(trailing_trivia);
    }

    /// Checks that all the trivia can fit on a single line (no line comments or block comments containing newlines).
    /// If so, prints all the trivia with no spaces between and returns the length of the trivia.
    ///
    /// Returns `None` if the trivia cannot fit on a single line.
    ///
    /// Also see [`Self::print_trivia_squished`]. It should be used if we cannot multi-line to allow line comments,
    /// (and so will skip them while still printing single-line trivia).
    ///
    /// It uses [`EmittableTrivia::single_line_len`], and similarly does not count or print empty lines.
    #[allow(unused_must_use)]
    pub fn try_print_trivia_single_line_squished(
        &mut self,
        trivia: &[EmittableTrivia],
    ) -> Option<usize> {
        let trivia_len = trivia
            .iter()
            .map(|t| t.single_line_len(self.input))
            .sum::<Option<usize>>()?;
        for t in trivia {
            if t.is_comment() {
                self.print_trivia(t);
            }
        }
        Some(trivia_len)
    }

    /// Prints only the single-line block trivia in the given slice,
    /// each with no space between them.
    ///
    /// Will always print single-line.
    ///
    /// Similar to [`Self::try_print_trivia_single_line_squished`], but will just skip the non-single-line trivia.
    /// It should be used instead if we could multi-line to allow line comments (instead of skipping them).
    ///
    /// ## Returns
    /// The total length printed.
    #[allow(unused_must_use)]
    pub fn print_trivia_squished(&mut self, trivia: &[EmittableTrivia]) -> usize {
        let mut trivia_len = 0;
        for t in trivia {
            if t.is_comment()
                && let Some(len) = t.single_line_len(self.input)
            {
                self.print_trivia(t);
                trivia_len += len;
            }
        }
        trivia_len
    }

    /// Append the output and warnings from another printer to this one.
    ///
    /// Generally used to append the output from a nested printer.
    pub fn append_from_printer(&mut self, other: Printer) {
        self.output.push_str(&other.output);
        self.warnings.extend(other.warnings);
    }

    #[must_use]
    pub fn sub_printer<'s>(&'s self) -> Printer<'a>
    where
        'a: 's,
    {
        Printer::new_empty(self.input, self.config, self.trivia)
    }

    /// Runs the function on a sub-printer (a copy of the current printer but with an empty output).
    ///
    /// The output of the sub-printer is returned, without changing the current printer.
    pub fn with_sub_printer(
        &self,
        f: impl FnOnce(&mut Printer<'a>) -> PrintInfo,
    ) -> (String, PrintInfo, Vec<PrinterWarning>) {
        let mut empty_copy = Printer::new_empty(self.input, self.config, self.trivia);
        let info = f(&mut empty_copy);
        (empty_copy.output, info, empty_copy.warnings)
    }

    /// Runs the function on a sub-printer  (a copy of the current printer but with an empty output).
    /// If the function returns `Some(info)`, the sub-printer
    /// is appended to the current printer and the info is returned. Otherwise, the sub-printer is
    /// not appended and `None` is returned.
    ///
    /// Like [`Self::try_in_sub_printer`], but directly appends the sub-printer to the current printer if successful.
    pub fn try_sub_printer(
        &mut self,
        f: impl FnOnce(&mut Printer<'a>) -> Option<PrintInfo>,
    ) -> Option<PrintInfo> {
        if let Some((sub_printer, info)) = self.try_in_sub_printer(f) {
            self.append_from_printer(sub_printer);
            Some(info)
        } else {
            None
        }
    }

    /// Runs the function on a sub-printer (a copy of the current printer but with an empty output).
    /// If the function returns `Some(info)`, the sub-printer is returned along with the info.
    /// Otherwise, the sub-printer is not returned.
    pub fn try_in_sub_printer(
        &self,
        f: impl FnOnce(&mut Printer<'a>) -> Option<PrintInfo>,
    ) -> Option<(Printer<'a>, PrintInfo)> {
        let mut sub_printer = Printer::new_empty(self.input, self.config, self.trivia);
        f(&mut sub_printer).map(|info| (sub_printer, info))
    }

    /// The current line length of the current line.
    /// Includes indentation.
    #[must_use]
    pub fn current_line_len(&self) -> usize {
        // TODO: we can probably sometimes cache this

        match self.output.rfind('\n') {
            Some(i) => self.output.len() - (i + 1),
            None => self.output.len(),
        }
    }

    /// The remaining width of the current line.
    ///
    /// Equivalent to `self.config.line_width - self.current_line_len()`.
    #[must_use]
    pub fn current_line_remaining_width(&self) -> usize {
        self.config
            .line_width
            .saturating_sub(self.current_line_len())
    }

    /// The current length of the output.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.output.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.output.is_empty()
    }
}

/// Information about the data that was just printed out.
pub struct PrintInfo {
    /// If the printed thing took up multiple lines.
    /// Can also be set if it is only one line, but there is a trailing line comment,
    /// as nothing can come after it on the same line.
    pub multi_lined: bool,
}

impl PrintInfo {
    #[must_use]
    pub fn default_single_line() -> Self {
        Self { multi_lined: false }
    }

    #[must_use]
    pub fn default_multi_lined() -> Self {
        Self { multi_lined: true }
    }
}

/// Main trait for printing elements.
///
/// ## Trivia
/// A node should print its internal trivia, but not the outer trivia
/// (leading trivia on `Self::leftmost_token` and trailing trivia on `Self::rightmost_token`).
/// The outer trivia is handled by whichever parent node has it as internal trivia.
///
/// The only exception is [`crate::ast::SourceFile`]: it can print EOF-attached trivia.
pub trait Printable {
    /// Prints to the printer.
    ///
    /// trivia is emitted by the parent of an element, not the element itself.
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo;
    /// The span of the leftmost (earliest) non-trivia token in the element.
    fn leftmost_token(&self) -> TextRange;
    /// The span of the rightmost (latest) non-trivia token in the element.
    fn rightmost_token(&self) -> TextRange;
}

pub trait PrintMultiLine {
    /// Prints the element, does not try to single-line it.
    ///
    /// However, if it does turn out to be single-lined anyway, should return as such in the info.
    fn print_multi_line(&self, shape: Shape, printer: &mut Printer) -> PrintInfo;
}

#[derive(Debug, Clone)]
pub enum PrinterWarning {}

/// The shape available to print an element.
///
/// ## Single-line
/// For printing single-line, `width` is the maximum width.
/// When the available width is unknown (e.g. more items potentially later on the line),
/// a large width may be set (e.g. `Shape::unlimited_single_line()`).
/// Then once the other elements are printed, the total width can be calculated.
/// It is preferable to use more efficient methods for calculating the width
/// if available.
///
/// ## Multi-line
/// For printing multi-line, for example:
/// ```baml
/// function MaxFunction(a: int, b: int) -> int {
///     if (a > b) {
///         return a;
///     } else {
///         return b;
///     }
/// }
/// ```
///
/// For the body of the if statement, `indent = 4` and `first_line_offset = 11`.
/// This is because the baseline indentation at that line is `4` spaces (one indentation level)
/// and the length of the other characters in the line "`if (a > b) `" is `11`.
#[derive(Debug, Clone)]
pub struct Shape {
    /// SINGLE-LINE ONLY
    ///
    /// The maximum width of the printed code if single-lined, not including base indentation.
    pub width: usize,
    /// MULTI-LINE ONLY
    ///
    /// The number of spaces that should be added before every line printed,
    /// except for the first line.
    pub indent: usize,
    /// MULTI-LINE ONLY
    ///
    /// This number is the column offset of the first line printed.
    /// It should be subtracted from the available width when printing the first line.
    pub first_line_offset: usize,
}

impl Shape {
    /// A shape that has no width limit and no indentation.
    ///
    /// Useful for trying to print single-lined with no chance that we will use the output if it is multi-lined
    #[must_use]
    pub const fn unlimited_single_line() -> Self {
        Shape {
            width: usize::MAX,
            indent: 0,
            first_line_offset: 0,
        }
    }

    /// A shape that is suitable for a standalone line (or beginning of a multiline element) of text.
    ///
    /// - `width = line_width - indent`
    /// - `indent = indent`
    /// - `first_line_offset = 0`
    #[must_use]
    pub const fn standalone(line_width: usize, indent: usize) -> Self {
        Shape {
            width: line_width.saturating_sub(indent),
            indent,
            first_line_offset: 0,
        }
    }
}
