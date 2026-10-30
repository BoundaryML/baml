pub mod ast;
mod newline_style;
pub mod printer;
mod trivia_classifier;

use ast::FromCST as _;
use baml_db::{
    ProjectDatabase, SourceRootSpec,
    baml_compiler_diagnostics::ParseError,
    baml_compiler_lexer, baml_compiler_parser,
    baml_compiler_syntax::{SyntaxElement, SyntaxNode},
};
use newline_style::apply_newline_style;
use printer::{Printer, Shape};
pub use trivia_classifier::{EmittableTrivia, TriviaInfo};

#[cfg(test)]
mod formatter_scenario_tests;

/// Runs the formatter on the given source code. The output keeps the line
/// ending of `source`'s first line, `\n` or `\r\n`. A file that opts out with
/// `baml-format: ignore` comes back byte for byte.
///
/// Also see [`format_salsa`] if you already have a [`salsa::Database`] with the source files in it.
///
/// # Errors
/// Errors can occur if the source code is invalid: the parser or AST errors will be returned.
pub fn format(source: &str, options: &FormatOptions) -> Result<String, FormatterError> {
    let (db, source_file) = single_file_db("file.baml", source);
    Ok(format_salsa(&db, source_file, *options)?.apply_to(source))
}

/// A throwaway database holding exactly one workspace file.
///
/// Formatting is purely syntactic (lexer + parser over one file), so the
/// database carries no stdlib — only the workspace root the file must belong
/// to. The root's virtual path never touches the filesystem.
pub(crate) fn single_file_db(name: &str, source: &str) -> (ProjectDatabase, baml_db::SourceFile) {
    let mut db = ProjectDatabase::new();
    let root = db
        .add_source_root(SourceRootSpec::new(
            "<fmt>",
            baml_db::SourceRootKind::Workspace,
        ))
        .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
    let file =
        db.add_or_update_file_in(root, &std::path::PathBuf::from("<fmt>").join(name), source);
    (db, file)
}

/// What formatting a file produced. The database's copy of the file has
/// normalized line endings, so turning this into new file text needs the
/// file's text as it is on disk or in the editor: see [`Formatted::apply_to`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    /// The formatted file, with `\n` line breaks and no `\r`; `None` if the
    /// file opts out with a `baml-format: ignore` comment.
    output: Option<String>,
}

impl Formatted {
    /// The file's new text, given `raw`, its text as it is on disk or in the
    /// editor: `raw` itself for a `baml-format: ignore` file, otherwise the
    /// formatted file with `raw`'s line ending.
    pub fn apply_to(self, raw: &str) -> String {
        match self.output {
            None => raw.to_owned(),
            Some(output) => apply_newline_style(output, raw),
        }
    }
}

/// Formats `file` as the database holds it.
#[salsa::tracked(returns(clone))]
#[allow(clippy::drop_non_drop)] // salsa macro expands to drop() of the args tuple
pub fn format_salsa(
    db: &dyn salsa::Database,
    file: baml_db::SourceFile,
    options: FormatOptions,
) -> Result<Formatted, FormatterError> {
    let tokens = baml_compiler_lexer::lex_file(db, file);
    let (parsed, errors) = baml_compiler_parser::parse_file(&tokens);
    let cst = SyntaxNode::new_root(parsed);

    // Honor the documented escape hatch at the shared library boundary so
    // CLI and LSP behavior cannot diverge. Inspect parser-classified comment
    // tokens rather than the raw source: directive-like text inside a string
    // must not disable formatting. This check intentionally precedes the parse
    // error gate so the directive can protect an incomplete or intentionally
    // non-canonical file.
    if has_ignore_directive(&cst) {
        return Ok(Formatted { output: None });
    }

    if !errors.is_empty() {
        return Err(FormatterError::ParseErrors(errors));
    }

    let trivia = TriviaInfo::classify_trivia(&cst);
    let strong_ast = ast::SourceFile::from_cst(SyntaxElement::Node(cst))?;

    let mut printer = Printer::new_empty(file.text(db), &options, &trivia);
    printer.print(
        &strong_ast,
        Shape {
            width: options.line_width,
            indent: 0,
            first_line_offset: 0,
        },
    );
    Ok(Formatted {
        output: Some(printer.output),
    })
}

fn has_ignore_directive(cst: &SyntaxNode) -> bool {
    cst.descendants_with_tokens().any(|element| {
        let SyntaxElement::Token(token) = element else {
            return false;
        };
        matches!(
            token.kind(),
            baml_db::baml_compiler_syntax::SyntaxKind::LINE_COMMENT
                | baml_db::baml_compiler_syntax::SyntaxKind::BLOCK_COMMENT
        ) && comment_contains_ignore_directive(token.text())
    })
}

fn comment_contains_ignore_directive(comment: &str) -> bool {
    const PREFIX: &str = "baml-format";
    let lowercase = comment.to_ascii_lowercase();
    lowercase.match_indices(PREFIX).any(|(start, _)| {
        lowercase[start + PREFIX.len()..]
            .trim_start()
            .strip_prefix(':')
            .is_some_and(|rest| rest.trim_start().starts_with("ignore"))
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormatOptions {
    /// Maximum line width before wrapping kicks in. Default: `100`
    pub line_width: usize,
    /// Indent width. Default: `4`
    pub indent_width: usize,
}
impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            line_width: 100,
            indent_width: CANONICAL_INDENT_WIDTH,
        }
    }
}

/// Canonical indentation used by the CLI, LSP, and default library formatter.
pub const CANONICAL_INDENT_WIDTH: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatterError {
    #[error("{0:?}")]
    ParseErrors(Vec<ParseError>),
    #[error("{0}")]
    StrongAstError(#[from] ast::StrongAstError),
}

#[cfg(test)]
mod format_options_tests {
    use super::*;

    #[test]
    fn bigint_literal_unions_preserve_the_suffix() {
        let source = "function value() -> 9007199254740993n | \"unsure\" { 9007199254740993n }\n";
        let options = FormatOptions::default();
        let formatted = format(source, &options).expect("bigint literal union should format");
        assert!(formatted.contains("-> 9007199254740993n | \"unsure\""));
        assert_eq!(format(&formatted, &options).unwrap(), formatted);
    }

    #[test]
    fn default_options_use_the_canonical_four_space_indent() {
        let options = FormatOptions::default();
        assert_eq!(CANONICAL_INDENT_WIDTH, 4);
        assert_eq!(options.indent_width, CANONICAL_INDENT_WIDTH);
        let formatted = format("function value() -> int {\n  result\n}\n", &options)
            .expect("default formatter options should format a function body");
        assert_eq!(formatted, "function value() -> int {\n    result\n}\n");
    }
}

#[cfg(test)]
mod ignore_directive_tests {
    use super::*;

    #[test]
    fn line_comment_directive_preserves_invalid_source_verbatim() {
        let source = "// BAML-FORMAT : ignore\nfunction unfinished(\n";
        let formatted = format(source, &FormatOptions::default())
            .expect("the ignore directive should bypass parse errors");
        assert_eq!(formatted, source);
    }

    #[test]
    fn directive_text_inside_a_string_does_not_disable_formatting() {
        let source = "function main() -> string {\n  let marker = \"// baml-format: ignore\";\n  marker\n}\n";
        let formatted = format(source, &FormatOptions::default())
            .expect("directive-like string content is ordinary source text");
        assert_ne!(formatted, source);
        assert!(formatted.contains("    let marker = \"// baml-format: ignore\";"));
    }
}

#[cfg(test)]
mod rejection_tests {
    use super::*;

    /// A lambda body must be a block; an expression body is a parse error,
    /// and the formatter refuses rather than guessing a layout for it.
    #[test]
    fn expression_bodied_lambda_is_rejected() {
        let options = FormatOptions::default();
        for arrow in ["->", "=>"] {
            let source = format!(
                "function apply() -> int {{\n    let add_one = (x: int) {arrow} x + 1\n    add_one(41)\n}}\n"
            );
            assert!(
                format(&source, &options).is_err(),
                "formatter must reject a lambda body without braces for {arrow}"
            );
        }
    }

    /// `//#` header comments are statement-level syntax; anywhere else they
    /// are a parse error the formatter surfaces instead of moving them.
    #[test]
    fn header_comments_in_declarations_are_rejected() {
        for source in [
            "interface Animal {\n    //# methods\n    function name(self) -> string\n}\n",
            "//# generated text\nfunction generate() -> string {\n    client: \"openai/gpt-4o\"\n    prompt: `hello`\n}\n",
        ] {
            let error = format(source, &FormatOptions::default())
                .expect_err("formatter should reject headers outside expression functions");
            assert!(
                format!("{error:?}")
                    .contains("header comments (`//#`) are only allowed in expression functions")
            );
        }
    }

    /// The removed `type T = …` / `unreflect` spellings are parse errors
    /// (E0098), and the formatter leaves a file with parse errors alone.
    #[test]
    fn removed_type_binding_syntax_does_not_format() {
        for source in [
            "function f(t: reflect.Type) -> int {\n    type T = unreflect(t)\n    1\n}\n",
            "function f() -> int {\n    type T = int[]\n    1\n}\n",
            "function f(t: reflect.Type) -> int {\n    g<unreflect(t)>()\n}\n",
        ] {
            assert!(
                matches!(
                    format(source, &FormatOptions::default()),
                    Err(FormatterError::ParseErrors(_))
                ),
                "expected a parse error for:\n{source}"
            );
        }
    }
}
