pub use crate::EmittableTrivia;
use crate::{FormatOptions, ast::Token};
use baml_compiler_syntax::Item;
use rowan::{TextRange, ast::AstNode};

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

    /// Create a new printer with the same configuration as the given printer but with an empty output buffer and no warnings.
    pub fn new_empty_like(printer: &'a Printer) -> Self {
        Printer::new_empty(printer.input, &printer.config, printer.trivia)
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

pub trait Printable {
    /// Prints to the printer.
    ///
    /// trivia is emitted by the parent of an element, not the element itself.
    fn print(&self, shape: Shape, printer: &mut Printer) -> PrintInfo;
}

#[derive(Debug, Clone)]
pub enum PrinterWarning {}

#[derive(Debug, Clone)]
pub struct Shape {
    /// The maximum width of the printed code, not including base indentation.
    pub width: usize,
    /// The number of spaces that should be added before every line printed,
    /// except for the first line.
    pub indent: usize,
    /// This number is the column offset of the first line printed.
    /// It should be subtracted from the available width when printing the first line.
    pub first_line_offset: usize,
}
