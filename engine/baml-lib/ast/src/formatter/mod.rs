#[cfg(test)]
mod tests;

use std::{
    borrow::BorrowMut,
    cell::{RefCell, RefMut},
    rc::Rc,
    sync::Arc,
};

use crate::parser::{BAMLParser, Rule};
use anyhow::{anyhow, Context, Result};
use pest::{
    iterators::{Pair, Pairs},
    Parser,
};
use pretty::RcDoc;
use regex::Regex;

pub struct FormatOptions {
    pub indent_width: isize,
    pub fail_on_unhandled_rule: bool,
}

pub fn format_schema(source: &str, format_options: FormatOptions) -> Result<String> {
    let ignore_directive_regex = Regex::new(r"(?i)baml-format\s*:\s*ignore")?;
    if ignore_directive_regex.is_match(source) {
        return Ok(source.to_string());
    }

    let mut schema = BAMLParser::parse(Rule::schema, source)?;
    let schema_pair = schema.next().ok_or(anyhow!("Expected a schema"))?;
    if schema_pair.as_rule() != Rule::schema {
        return Err(anyhow!("Expected a schema"));
    }

    let formatter = Formatter {
        indent_width: format_options.indent_width,
        fail_on_unhandled_rule: format_options.fail_on_unhandled_rule,
    };

    let doc = formatter.schema_to_doc(schema_pair.into_inner())?;
    let mut w = Vec::new();
    doc.render(10, &mut w)
        .map_err(|_| anyhow!("Failed to render doc"))?;
    Ok(String::from_utf8(w).map_err(|_| anyhow!("Failed to convert to string"))?)
}

macro_rules! next_pair {
    ($pairs:ident, $rule:expr) => {{
        loop {
            match $pairs.peek() {
                Some(pair) => {
                    if pair.as_rule() == Rule::NEWLINE {
                        $pairs.next();
                        continue;
                    }
                    if pair.as_rule() != $rule {
                        break Err(anyhow!(
                            "Expected a {:?}, got a {:?} ({}:{})",
                            $rule,
                            pair.as_rule(),
                            file!(),
                            line!()
                        ));
                    } else {
                        $pairs.next();
                        break Ok(pair);
                    }
                }
                None => break Err(anyhow!("Expected a {}", stringify!($rule))),
            }
        }
    }};

    ($pairs:ident, $rule:expr, optional) => {{
        match $pairs.peek() {
            Some(pair) => {
                if pair.as_rule() == $rule {
                    $pairs.next()
                } else {
                    None
                }
            }
            None => None,
        }
    }};
}

trait ToDoc {
    type DocType;

    fn to_doc(&self) -> Self::DocType;
}

impl<'a> ToDoc for Pair<'a, Rule> {
    type DocType = RcDoc<'a, ()>;

    /// Embed the exact contents of the corresponding source in the output.
    ///
    /// This is our formatting "bail-out" effectively, where if we don't know
    /// how to format something, we just emit the original source.
    ///
    /// NB: according to the `RcDoc::text` docs, this is an API violation,
    /// because we call `to_doc()` on many Pest pairs that contain newlines
    /// within them. I suspect that this is less of a "the 'pretty' crate will
    /// break catastrophically in unexpected ways if text symbols contain
    /// newlines" problem, and more of a "having newlines in text symbols may
    /// produce surprising formatting" issue. It would be pretty bizarre for
    /// the 'pretty' crate to inspect tokens for newlines (but not unreasonable!)
    /// given how Wadler pretty prints work, but we need to rely on this
    /// property to be able to incrementally implement our formatter.
    fn to_doc(&self) -> Self::DocType {
        if self.as_rule() == Rule::empty_lines {
            // If we're formatting empty lines, superfluous whitespace should get stripped.
            let newline_count = self.as_str().matches('\n').count();
            return RcDoc::concat(std::iter::repeat(RcDoc::hardline()).take(newline_count));
        }
        RcDoc::text(self.as_str())
    }
}

struct Formatter {
    indent_width: isize,
    fail_on_unhandled_rule: bool,
}

impl Formatter {
    /// The number of spaces to add before an inline trailing comment.
    /// Here, "trailing comment" does not refer to the trailing_comment Pest rule, but rather just
    /// a comment in this style:
    ///    
    ///   class Foo {
    ///       field string   // comment
    ///                   ^^^------------ This is what will get replaced with SPACES_BEFORE_TRAILING_COMMENT
    ///   }
    const SPACES_BEFORE_TRAILING_COMMENT: &'static str = "  ";

    fn schema_to_doc<'a>(&self, mut pairs: Pairs<'a, Rule>) -> Result<RcDoc<'a, ()>> {
        let mut doc = RcDoc::nil();

        for pair in &mut pairs {
            match pair.as_rule() {
                Rule::type_expression_block => {
                    match self.type_expression_block_to_doc(pair.clone().into_inner()) {
                        Ok(pair_doc) => {
                            doc = doc.append(pair_doc.group());
                        }
                        Err(e) => {
                            log::debug!("Error formatting type_expression_block: {:#?}", e);
                            doc = doc.append(pair.to_doc());
                        }
                    }
                }
                Rule::EOI => {
                    // skip
                }
                Rule::value_expression_block | Rule::empty_lines => {
                    doc = doc.append(pair.to_doc());
                }
                _ => {
                    doc = doc.append(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }
        Ok(doc)
    }

    fn type_expression_block_to_doc<'a>(
        &self,
        mut pairs: Pairs<'a, Rule>,
    ) -> Result<RcDoc<'a, ()>> {
        let class_or_enum = next_pair!(pairs, Rule::identifier)?;
        let ident = next_pair!(pairs, Rule::identifier)?;
        next_pair!(pairs, Rule::named_argument_list, optional);
        next_pair!(pairs, Rule::BLOCK_OPEN)?;
        let contents = next_pair!(pairs, Rule::type_expression_contents)?;
        next_pair!(pairs, Rule::BLOCK_CLOSE)?;

        Ok(RcDoc::nil()
            .append(pair_to_doc_text(class_or_enum))
            .append(RcDoc::space())
            .append(pair_to_doc_text(ident))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(
                self.type_expression_contents_to_doc(contents.into_inner())?
                    .nest(self.indent_width)
                    .group(),
            )
            .append(RcDoc::text("}")))
    }

    fn type_expression_contents_to_doc<'a>(
        &self,
        mut pairs: Pairs<'a, Rule>,
    ) -> Result<RcDoc<'a, ()>> {
        let mut content_docs = vec![];

        for pair in &mut pairs {
            let error_context = format!("type_expression: {:#?}", pair);
            match pair.as_rule() {
                Rule::type_expression => {
                    content_docs.push(
                        self.type_expression_to_doc(pair.into_inner())
                            .context(error_context)?,
                    );
                }
                Rule::block_attribute => {
                    content_docs.push(pair_to_doc_text(pair));
                }
                Rule::comment_block => {
                    content_docs.push(pair_to_doc_text(pair));
                }
                Rule::empty_lines => {
                    // skip
                }
                _ => {
                    content_docs.push(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }

        let doc = if content_docs.len() > 0 {
            content_docs
                .into_iter()
                .fold(RcDoc::hardline(), |acc, doc| {
                    acc.append(doc).append(RcDoc::hardline())
                })
        } else {
            RcDoc::nil()
        };

        Ok(doc)
    }

    fn type_expression_to_doc<'a>(&self, mut pairs: Pairs<'a, Rule>) -> Result<RcDoc<'a, ()>> {
        let ident = next_pair!(pairs, Rule::identifier)?;
        let field_type_chain = next_pair!(pairs, Rule::field_type_chain)?;

        let mut doc = RcDoc::nil()
            .append(pair_to_doc_text(ident))
            .append(RcDoc::space());

        // Since our compiler currently doesn't allow newlines in type expressions, we can't
        // put comments in the middle of a type expression, so we can rely on this hack to
        // cascade comments all the way out of a type expression.
        let (field_type_chain_doc, field_type_chain_comments) =
            self.field_type_chain_to_doc(field_type_chain.into_inner())?;
        doc = doc.append(field_type_chain_doc);
        if let Some(field_type_chain_comments) = field_type_chain_comments {
            doc = doc.append(field_type_chain_comments);
        }

        for pair in pairs {
            match pair.as_rule() {
                Rule::NEWLINE => {
                    // skip
                }
                Rule::field_attribute => {
                    doc = doc.append(pair_to_doc_text(pair).nest(self.indent_width).group());
                }
                Rule::trailing_comment => {
                    doc = doc.append(pair_to_doc_text(pair).nest(self.indent_width).group());
                }
                _ => {
                    doc = doc.append(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }

        Ok(doc)
    }

    fn field_type_chain_to_doc<'a>(
        &self,
        pairs: Pairs<'a, Rule>,
    ) -> Result<(RcDoc<'a, ()>, Option<RcDoc<'a, ()>>)> {
        let mut docs = vec![];
        let mut comments = vec![];

        for pair in pairs {
            match pair.as_rule() {
                Rule::field_type_with_attr => {
                    let (field_type_doc, field_type_comments) =
                        self.field_type_with_attr_to_doc(pair.into_inner())?;
                    docs.push(field_type_doc);
                    if let Some(field_type_comments) = field_type_comments {
                        comments.push(field_type_comments);
                    }
                }
                Rule::field_operator => {
                    docs.push(RcDoc::text("|"));
                }
                _ => {
                    docs.push(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }

        Ok((
            RcDoc::intersperse(docs, RcDoc::space())
                .nest(self.indent_width)
                .group(),
            if comments.is_empty() {
                None
            } else {
                Some(RcDoc::concat(comments).group())
            },
        ))
    }

    fn field_type_with_attr_to_doc<'a>(
        &self,
        mut pairs: Pairs<'a, Rule>,
    ) -> Result<(RcDoc<'a, ()>, Option<RcDoc<'a, ()>>)> {
        let mut docs = vec![];
        // This is a hack: we cascade comments all the way out of a type
        // expression, relying on the (current) limitation that our users can't
        // have newlines in a type expression today.
        //
        // The correct way to handle this is to either (1) make our lexer understand that
        // trailing comments are not actually a part of a type expression or (2) teach the
        // formatter how to push comments to the correct context.
        //
        // Arguably we're currently using (2), and just implementing it in a naive way,
        // because we just push all comments to the context of the type expression, rather
        // than, say, an operand of the type expression.
        let mut comments = vec![];

        for pair in &mut pairs {
            match pair.as_rule() {
                Rule::field_type => {
                    docs.push(self.field_type_to_doc(pair.into_inner())?);
                }
                Rule::field_attribute => {
                    docs.push(pair_to_doc_text(pair));
                }
                Rule::trailing_comment => {
                    if comments.is_empty() {
                        comments.push(RcDoc::text(Self::SPACES_BEFORE_TRAILING_COMMENT));
                    }
                    comments.push(pair_to_doc_text(pair));
                }
                Rule::NEWLINE => {
                    comments.push(RcDoc::hardline());
                }
                _ => {
                    docs.push(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }

        Ok((
            RcDoc::intersperse(docs, RcDoc::space())
                .nest(self.indent_width)
                .group(),
            if comments.is_empty() {
                None
            } else {
                Some(RcDoc::concat(comments).group())
            },
        ))
    }

    fn field_type_to_doc<'a>(&self, pairs: Pairs<'a, Rule>) -> Result<RcDoc<'a, ()>> {
        let mut docs = vec![];
        self.field_type_to_doc_impl(pairs, &mut docs)?;
        Ok(docs
            .into_iter()
            .fold(RcDoc::nil(), |acc, doc| acc.append(doc)))
    }

    fn field_type_to_doc_impl<'a>(
        &self,
        pairs: Pairs<'a, Rule>,
        docs: &mut Vec<RcDoc<'a, ()>>,
    ) -> Result<()> {
        for pair in pairs {
            match pair.as_rule() {
                Rule::field_type | Rule::union => {
                    self.field_type_to_doc_impl(pair.into_inner(), docs)?;
                }
                Rule::field_operator => {
                    docs.push(RcDoc::space());
                    docs.push(RcDoc::text("|"));
                    docs.push(RcDoc::space());
                }
                Rule::base_type_with_attr | Rule::non_union => {
                    docs.push(pair_to_doc_text(pair));
                }
                _ => {
                    docs.push(self.unhandled_rule_to_doc(pair)?);
                }
            }
        }

        Ok(())
    }

    fn unhandled_rule_to_doc<'a>(&self, pair: Pair<'a, Rule>) -> Result<RcDoc<'a, ()>> {
        if self.fail_on_unhandled_rule {
            Err(anyhow!("Unhandled rule: {:?}", pair.as_rule()))
        } else {
            // Don't trim the str repr of unhandled rules, so
            // we can see the original source.
            Ok(RcDoc::text(pair.as_str()))
        }
    }
}

fn pair_to_doc_text<'a>(pair: Pair<'a, Rule>) -> RcDoc<'a, ()> {
    RcDoc::text(pair.as_str().trim())
}
