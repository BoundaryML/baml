pub use crate::EmittableTrivia;
use crate::{FormatOptions, ast::Token};
use rowan::TextRange;

pub struct Printer<'a> {
    pub input: &'a str,
    pub config: &'a FormatOptions,
    pub output: String,
    pub trivia: &'a [EmittableTrivia],
    pub warnings: Vec<PrinterWarning>,
}
impl<'a> Printer<'a> {
    #[inline]
    pub fn new_empty(
        input: &'a str,
        config: &'a FormatOptions,
        trivia: &'a [EmittableTrivia],
    ) -> Self {
        Printer {
            input,
            config,
            output: String::new(),
            trivia,
            warnings: Vec::new(),
        }
    }

    #[inline]
    pub fn new(
        input: &'a str,
        config: &'a FormatOptions,
        output: String,
        trivia: &'a [EmittableTrivia],
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

    /// Append the output and warnings from another printer to this one.
    ///
    /// Generally used to append the output from a nested printer.
    pub fn append_from_printer(&mut self, other: Printer) {
        self.output.push_str(&other.output);
        self.warnings.extend(other.warnings);
    }

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
    pub fn try_sub_printer(
        &mut self,
        f: impl FnOnce(&mut Printer<'a>) -> Option<PrintInfo>,
    ) -> Option<PrintInfo> {
        let mut sub_printer = Printer::new_empty(self.input, self.config, self.trivia);
        if let Some(info) = f(&mut sub_printer) {
            self.append_from_printer(sub_printer);
            Some(info)
        } else {
            None
        }
    }

    /// The current line length of the current line.
    /// Includes indentation.
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
    pub fn current_line_remaining_width(&self) -> usize {
        self.config
            .line_width
            .saturating_sub(self.current_line_len())
    }

    /// The current length of the output.
    pub const fn len(&self) -> usize {
        self.output.len()
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
    pub fn default_single_line() -> Self {
        Self { multi_lined: false }
    }

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
    pub const fn unlimited_single_line() -> Self {
        Shape {
            width: usize::MAX,
            indent: 0,
            first_line_offset: 0,
        }
    }
}
