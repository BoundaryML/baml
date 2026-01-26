use std::iter::Peekable;

use anyhow::Result;
use baml_types::CompletionState;

use super::json_collection::JsonCollection;
use crate::jsonish::{value::Fixes, Value};

/// Tracks quote and backslash state incrementally for quoted strings
/// to avoid O(n²) rescanning when determining if a quote closes a string.
#[derive(Debug, Default, Clone)]
struct StringQuoteTracking {
    /// Number of consecutive backslashes at the end of the current string content.
    /// Used to determine if a quote is escaped.
    trailing_backslashes: usize,
    /// Count of unescaped quotes (quotes preceded by an even number of backslashes).
    /// Used in should_close_string to decide whether to close.
    unescaped_quote_count: usize,
}

#[derive(Debug)]
pub struct JsonParseState {
    /// The stack of Json collection values being assembled.
    /// The stack-ness is used in order to parse nested values,
    /// e.g. an object with fields of list, or lists of lists.
    pub collection_stack: Vec<(JsonCollection, Vec<Fixes>)>,

    /// Values for which parsing is completed, and popped off of the
    /// collection stack.
    /// Technically we may find multiple values in a single string
    pub completed_values: Vec<(&'static str, Value, Vec<Fixes>)>,

    /// Incremental tracking state for the current quoted string being parsed.
    /// Reset when a new string is started, used to avoid O(n²) quote counting.
    string_quote_tracking: StringQuoteTracking,
}

#[derive(Clone, Debug)]
enum Pos {
    InNothing,     // 0
    Unknown,       // 1
    InObjectKey,   // 2
    InObjectValue, // 3
    InArray,       // 4
}

impl JsonParseState {
    pub fn new() -> Self {
        JsonParseState {
            collection_stack: vec![],
            completed_values: vec![],
            string_quote_tracking: StringQuoteTracking::default(),
        }
    }

    /// Reset the quote tracking state when starting a new quoted string
    fn reset_quote_tracking(&mut self) {
        self.string_quote_tracking = StringQuoteTracking::default();
    }

    /// Update quote tracking when consuming a character into a quoted string.
    /// Must be called BEFORE the character is added to the string.
    fn update_quote_tracking(&mut self, token: char) {
        if token == '\\' {
            self.string_quote_tracking.trailing_backslashes += 1;
        } else {
            if token == '"' {
                // A quote is "unescaped" if preceded by an even number of backslashes
                if self
                    .string_quote_tracking
                    .trailing_backslashes
                    .is_multiple_of(2)
                {
                    self.string_quote_tracking.unescaped_quote_count += 1;
                }
            }
            self.string_quote_tracking.trailing_backslashes = 0;
        }
    }

    /// Examine the top of the collection stack, popping it off and
    /// adding it to `completed_values` if it is ready.
    ///
    /// The `completion_state` parameter is applied to the value being
    /// completed. If it is `CompletionState::Complete`, we also apply
    /// that state to the children of the value being completed.
    pub fn complete_collection(&mut self, completion_state: CompletionState) {
        let (collection, fixes) = match self.collection_stack.pop() {
            Some(collection) => collection,
            None => return,
        };

        let name = collection.name();

        let mut value: Value = match collection.into() {
            Some(value) => value,
            None => return,
        };
        if completion_state == CompletionState::Complete {
            value.complete_deeply();
        }

        if let Some((last, _fixes)) = self.collection_stack.last_mut() {
            match last {
                JsonCollection::Object(keys, values, _) => {
                    if keys.len() == values.len() {
                        match value {
                            Value::String(s, _) => keys.push(s),
                            Value::AnyOf(_, s) => keys.push(s),
                            _ => keys.push(value.to_string()),
                        }
                    } else {
                        values.push(value);
                    }
                }
                JsonCollection::Array(values, _) => {
                    values.push(value);
                }
                _ => {
                    // TODO: this should never happen as we should only be pushing objects and arrays
                    panic!("Unexpected value: {value:?} in collection stack: {last:?}");
                }
            }
        } else {
            self.completed_values.push((name, value, fixes));
        }
    }

    fn consume(&mut self, token: char) -> Result<usize> {
        // First check if we're in a QuotedString and need to update tracking
        // (done before getting mutable borrow to avoid borrow checker conflict)
        let is_quoted_string = matches!(
            self.collection_stack.last(),
            Some((JsonCollection::QuotedString(..), _))
        );
        if is_quoted_string {
            // Track quote/backslash state incrementally for O(1) quote counting
            self.update_quote_tracking(token);
        }

        // Now get mutable access to push the token
        let Some((last, _)) = self.collection_stack.last_mut() else {
            return Err(anyhow::anyhow!(
                "No collection to consume token: {:?}",
                token
            ));
        };
        match last {
            JsonCollection::QuotedString(s, _)
            | JsonCollection::TripleQuotedString(s, _)
            | JsonCollection::BlockComment(s, _)
            | JsonCollection::SingleQuotedString(s, _)
            | JsonCollection::BacktickString(s, _)
            | JsonCollection::TripleBacktickString {
                content: (s, _), ..
            }
            | JsonCollection::UnquotedString(s, _)
            | JsonCollection::TrailingComment(s, _) => {
                // println!("Consuming: {s} + {:?}", token);
                s.push(token);
            }
            JsonCollection::Object(_, _, _) | JsonCollection::Array(_, _) => {
                panic!("Unexpected token: {token:?} in: {last:?}");
            }
        }
        Ok(0)
    }

    fn is_string_complete(&self) -> bool {
        let Some((JsonCollection::UnquotedString(v, _), _)) = self.collection_stack.last() else {
            return false;
        };

        // Check if the token is a valid json character
        match v.as_str() {
            "true" | "false" | "null" => true,
            _ => {
                // Check if the token parses as a number
                if v.parse::<f64>().is_ok() {
                    return true;
                }
                false
            }
        }
    }

    fn should_close_unescaped_string(
        &mut self,
        mut next: Peekable<impl Iterator<Item = (usize, char)>>,
    ) -> CloseStringResult {
        let pos: Pos = if self.collection_stack.len() >= 2 {
            self.collection_stack
                .get(self.collection_stack.len() - 2)
                .map(|(c, _)| match c {
                    JsonCollection::Object(keys, values, _) => {
                        if keys.len() == values.len() {
                            Pos::InObjectKey
                        } else {
                            Pos::InObjectValue
                        }
                    }
                    JsonCollection::Array(_, _) => Pos::InArray,
                    _ => Pos::Unknown,
                })
                .unwrap()
        } else {
            Pos::InNothing
        };
        match pos {
            Pos::InNothing => {
                // in nothing, so perhaps the first '{' or '[' is the start of a new object or array
                let mut counter = 0;
                for (idx, c) in next.by_ref() {
                    counter = idx;
                    match c {
                        // If at some point we find a valid json character, we'll close the string
                        '{' | '[' => {
                            return CloseStringResult::Close(idx, CompletionState::Complete)
                        }
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                CloseStringResult::Close(counter, CompletionState::Incomplete)
            }
            Pos::Unknown => CloseStringResult::Continue,
            Pos::InObjectKey => {
                // in object key
                let mut counter = 0;
                for (idx, c) in next.by_ref() {
                    counter = idx;
                    match c {
                        ':' => return CloseStringResult::Close(idx, CompletionState::Complete),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                CloseStringResult::Close(counter, CompletionState::Incomplete)
            }
            Pos::InObjectValue => {
                // in object value
                let mut counter = 0;
                while let Some((idx, c)) = next.next() {
                    counter = idx;
                    match c {
                        ',' => {
                            // Check if we have just numeric values in the string so far.
                            let Some((JsonCollection::UnquotedString(current_value, _), _)) =
                                self.collection_stack.last()
                            else {
                                return CloseStringResult::Close(idx, CompletionState::Complete);
                            };

                            // current value could be a numeric looking things.
                            let is_numeric = current_value.trim().parse::<f64>().is_ok();
                            let is_bool = current_value.trim().eq_ignore_ascii_case("true")
                                || current_value.trim().eq_ignore_ascii_case("false");
                            let is_null = current_value.trim().eq_ignore_ascii_case("null");
                            let is_identifier =
                                !(current_value.contains(" ") || current_value.contains("("));
                            let is_possible_value =
                                is_numeric || is_bool || is_null || is_identifier;

                            if let Some((_, next_c)) = next.peek() {
                                match next_c {
                                    '\n' => {
                                        log::debug!("Closing due to: newline after comma");
                                        return CloseStringResult::Close(
                                            idx,
                                            CompletionState::Complete,
                                        );
                                    }
                                    ' ' => {
                                        log::debug!("Testing for comment after space + comma");
                                        if is_possible_value {
                                            return CloseStringResult::Close(
                                                idx,
                                                CompletionState::Complete,
                                            );
                                        }
                                        // If after the space we have "//" or "/*" or the beginning of a key, we'll close the string
                                        let mut buffer = ",".to_string();
                                        let mut anything_but_whitespace = false;
                                        while let Some((_, next_next_c)) = next.next() {
                                            anything_but_whitespace = anything_but_whitespace
                                                || !next_next_c.is_whitespace();
                                            buffer.push(next_next_c);
                                            match next_next_c {
                                                ' ' => {}
                                                '\n' => {
                                                    if anything_but_whitespace {
                                                    } else {
                                                        // Likely end of the key as the LLM generated a ", " token by mistake instead of a ","
                                                        // so drop the comma
                                                        log::debug!("Closing due to: newline after comma + space");
                                                        return CloseStringResult::Close(
                                                            idx,
                                                            CompletionState::Complete,
                                                        );
                                                    }
                                                }
                                                '/' => match next.peek() {
                                                    Some((_, '/')) => {
                                                        // This is likely a comment
                                                        return CloseStringResult::Close(
                                                            idx,
                                                            CompletionState::Complete,
                                                        );
                                                    }
                                                    Some((_, '*')) => {
                                                        // This is likely a comment
                                                        return CloseStringResult::Close(
                                                            idx,
                                                            CompletionState::Complete,
                                                        );
                                                    }
                                                    _ => {
                                                        // let _ = self.consume(c);
                                                    }
                                                },
                                                '"' => {
                                                    // This is likely a new key
                                                    log::debug!("Closing due to: new key after space + comma");
                                                    return CloseStringResult::Close(
                                                        idx,
                                                        CompletionState::Complete,
                                                    );
                                                }
                                                _x => {
                                                    break;
                                                }
                                            }
                                        }
                                        for c in buffer.chars() {
                                            let _ = self.consume(c);
                                        }
                                    }
                                    _ => {
                                        let _ = self.consume(c);
                                    }
                                }
                            } else {
                                // Don't include the comma
                                return CloseStringResult::Close(idx, CompletionState::Complete);
                            }
                        }
                        '}' => return CloseStringResult::Close(idx, CompletionState::Complete),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                CloseStringResult::Close(counter, CompletionState::Incomplete)
            }
            Pos::InArray => {
                // in array
                let mut counter = 0;
                for (idx, c) in next {
                    counter = idx;
                    match c {
                        ',' => return CloseStringResult::Close(idx, CompletionState::Complete),
                        ']' => return CloseStringResult::Close(idx, CompletionState::Complete),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                counter += 1; // Indicate that we called next() one time after the final `Some`.
                CloseStringResult::Close(counter, CompletionState::Incomplete)
            }
        }
    }

    fn should_close_string(
        &mut self,
        mut next: Peekable<impl Iterator<Item = (usize, char)>>,
        closing_char: char,
    ) -> bool {
        let (has_some_object, in_object_key, in_object_value, in_array) =
            if self.collection_stack.len() >= 2 {
                self.collection_stack
                    .get(self.collection_stack.len() - 2)
                    .map(|(c, _)| match c {
                        JsonCollection::Object(keys, values, _) => {
                            if keys.len() == values.len() {
                                (true, false, false)
                            } else {
                                (false, true, true)
                            }
                        }
                        JsonCollection::Array(_, _) => (false, false, true),
                        _ => (false, false, false),
                    })
                    .map(|(a, b, c)| (true, a, b, c))
                    .unwrap()
            } else {
                (false, false, false, false)
            };
        // Use pre-computed quote count from incremental tracking (O(1) instead of O(n²))
        let closing_char_count = if closing_char == '"' {
            let (last, _) = self.collection_stack.last().unwrap();
            match last {
                JsonCollection::QuotedString(..) => {
                    self.string_quote_tracking.unescaped_quote_count
                }
                _ => 0,
            }
        } else {
            0
        };

        if let Some((idx, next_char)) = next.peek() {
            let _idx = *idx;
            match next_char {
                ':' | '}' if in_object_key => {
                    // We're ready to close the key
                    log::debug!("Closing due to: key");
                    true
                }
                ',' if in_object_value || in_array => {
                    if closing_char_count % 2 == 0 {
                        // We're ready to close the value
                        log::debug!("Closing due to: value",);
                        true
                    } else {
                        // We're not ready to close the value
                        false
                    }
                }
                '}' if in_object_value => {
                    // We're ready to close the value
                    log::debug!("Closing due to: value",);
                    true
                }
                ']' if in_array => {
                    // We're ready to close the value
                    log::debug!("Closing due to: array");
                    true
                }
                ' ' | '\t' | '\n' => {
                    // look ahead and see if we can find a closing bracket or comma
                    while let Some((_, c)) = next.next() {
                        match c {
                            ' ' | '\t' | '\n' => {}
                            '}' if in_object_key || in_object_value => return true,
                            ':' if in_object_key => return true,
                            ',' if in_object_value => return true,
                            ',' | ']' if in_array => return true,
                            '/' => {
                                // Could be a comment
                                match next.peek() {
                                    Some((_, '/')) => {
                                        // We're ready to close the comment
                                        return true;
                                    }
                                    Some((_, '*')) => {
                                        // We're ready to close the comment
                                        return true;
                                    }
                                    _ => return false,
                                }
                            }
                            _ => return false,
                        }
                    }
                    // If we faile, terminate the string
                    true
                }
                x if closing_char == *x => {
                    // We'll close the string the next time around.
                    false
                }
                '{' | '"' | '\'' | '[' => {
                    if !has_some_object {
                        // We're in a string
                        true
                    } else {
                        false
                    }
                }
                _ => {
                    // Almost every other character should not close the string
                    false
                }
            }
        } else {
            true
        }
    }

    pub fn process_token(
        &mut self,
        token: char,
        mut next: Peekable<impl Iterator<Item = (usize, char)>>,
    ) -> Result<usize> {
        // println!("Processing: {:?}..{:?}", token, next.peek());
        match self.collection_stack.last() {
            Some((last, _)) => match last {
                JsonCollection::Object(_, _, _) => {
                    match token {
                        '}' => {
                            // We're ready to close the object
                            self.complete_collection(CompletionState::Complete);
                            Ok(0)
                        }
                        // We can safely ignore these tokens
                        ',' | ':' => Ok(0),
                        // look for a new key or value
                        _ => self.find_any_starting_value(token, next),
                    }
                }
                JsonCollection::Array(_, _) => {
                    // We could be expecting:
                    // - A value
                    // - a comma
                    // - a closing bracket
                    match token {
                        ']' => {
                            // We're ready to close the array
                            self.complete_collection(CompletionState::Complete);
                            Ok(0)
                        }
                        // Skip these tokens
                        ',' => Ok(0),
                        _ => self.find_any_starting_value(token, next),
                    }
                }
                JsonCollection::TripleQuotedString(_, _) => {
                    // We should be expecting:
                    if token == '"' {
                        // TODO: this logic is busted. peekable.peek() does not
                        // advance the iterator (this is easily verified with
                        // a unit test), but to fix this we need to do a bit of
                        // refactoring, so for now we'll live with it.
                        let is_triple_quoted = match next.peek() {
                            Some((_, '"')) => matches!(next.peek(), Some((_, '"')) | None),
                            None => true,
                            _ => false,
                        };

                        if is_triple_quoted {
                            self.complete_collection(CompletionState::Complete);
                            Ok(3)
                        } else {
                            self.consume(token)
                        }
                    } else {
                        self.consume(token)
                    }
                }
                JsonCollection::QuotedString(_, _) => {
                    // We could be expecting:
                    // - A closing quote
                    // - A character
                    match token {
                        '"' => {
                            // It's possible that the LLM messed up the escaping
                            // We'll try to fix it.
                            if self.should_close_string(next, '"') {
                                self.complete_collection(CompletionState::Complete);
                                Ok(0)
                            } else {
                                self.consume(token)
                            }
                        }
                        '\\' => {
                            // Capture escaped characters
                            match next.peek() {
                                Some((_, 'n')) => {
                                    self.consume('\n')?;
                                    Ok(1)
                                }
                                Some((_, 't')) => {
                                    self.consume('\t')?;
                                    Ok(1)
                                }
                                Some((_, 'r')) => {
                                    self.consume('\r')?;
                                    Ok(1)
                                }
                                Some((_, 'b')) => {
                                    self.consume('\x08')?;
                                    Ok(1)
                                }
                                Some((_, 'f')) => {
                                    self.consume('\x0C')?;
                                    Ok(1)
                                }
                                Some((_, '\\')) => {
                                    self.consume('\\')?;
                                    Ok(1)
                                }
                                Some((_, '"')) => {
                                    self.consume('"')?;
                                    Ok(1)
                                }
                                Some((_, 'u')) => {
                                    // We'll consume the 'u' and the next 4 characters
                                    let mut buffer = String::new();
                                    buffer.push(token);
                                    for _ in 0..4 {
                                        if let Some((_, c)) = next.next() {
                                            buffer.push(c);
                                        } else {
                                            break;
                                        }
                                    }
                                    for c in buffer.chars() {
                                        let _ = self.consume(c);
                                    }
                                    Ok(5)
                                }
                                _ => self.consume(token),
                            }
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::TripleBacktickString { .. } => {
                    // We could be expecting:
                    // - A closing backtick
                    // - A character
                    if token == '`' {
                        let is_triple_quoted = next
                            .next_if(|&(_, c)| c == '`')
                            .and_then(|_| next.next_if(|&(_, c)| c == '`'))
                            .is_some();

                        if is_triple_quoted {
                            self.complete_collection(CompletionState::Complete);
                            Ok(2)
                        } else {
                            self.consume(token)
                        }
                    } else {
                        self.consume(token)
                    }
                }
                JsonCollection::BacktickString(_, _) => {
                    // We could be expecting:
                    // - A closing backtick
                    // - A character
                    match token {
                        '`' => {
                            if self.should_close_string(next, '`') {
                                self.complete_collection(CompletionState::Complete);
                                Ok(0)
                            } else {
                                self.consume(token)
                            }
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::SingleQuotedString(_, _) => {
                    // We could be expecting:
                    // - A closing quote
                    // - A character
                    // - A space
                    match token {
                        '\'' => {
                            // It's possible that the LLM messed up the escaping
                            // We'll try to fix it.
                            if self.should_close_string(next, '\'') {
                                self.complete_collection(CompletionState::Complete);
                                Ok(0)
                            } else {
                                self.consume(token)
                            }
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::UnquotedString(_, _) => {
                    // We could be expecting:
                    // - A terminating json character (comma, colon, bracket, space, newline)
                    // - A character
                    let res = self.consume(token);
                    if let CloseStringResult::Close(count, completion) =
                        self.should_close_unescaped_string(next)
                    {
                        self.complete_collection(completion);
                        Ok(count)
                    } else {
                        res
                    }
                }
                JsonCollection::TrailingComment(_, _) => {
                    // We could be expecting:
                    // - A newline
                    // - A character
                    match token {
                        '\n' => {
                            // We're ready to close the comment
                            self.complete_collection(CompletionState::Complete);
                            Ok(0)
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::BlockComment(_, _) => {
                    // We could be expecting:
                    // - A closing comment
                    // - A character
                    match token {
                        '*' => {
                            // We could be closing the comment
                            match next.peek() {
                                Some((_, '/')) => {
                                    // We're ready to close the comment
                                    self.complete_collection(CompletionState::Complete);
                                    Ok(1)
                                }
                                _ => Ok(0),
                            }
                        }
                        _ => self.consume(token),
                    }
                }
            },
            None => {
                // We could be expecting:
                // - A value
                // - Any leading whitespace
                let preview = next.peekable();
                self.find_any_starting_value(token, preview)
            }
        }
    }

    // Returns the number of increments to skip after processing the token
    fn find_any_starting_value(
        &mut self,
        token: char,
        mut next: Peekable<impl Iterator<Item = (usize, char)>>,
    ) -> Result<usize> {
        match token {
            '{' => {
                self.collection_stack.push((
                    JsonCollection::Object(vec![], vec![], CompletionState::Incomplete),
                    Default::default(),
                ));
            }
            '[' => {
                self.collection_stack.push((
                    JsonCollection::Array(vec![], CompletionState::Incomplete),
                    Default::default(),
                ));
            }
            '"' => {
                // Peek if next 2 characters are also quotes
                let is_triple_quoted = {
                    next.next_if(|&(_, c)| c == '"')
                        .and_then(|_| next.next_if(|&(_, c)| c == '"'))
                        .is_some()
                };

                if is_triple_quoted {
                    self.collection_stack.push((
                        JsonCollection::TripleQuotedString(
                            String::new(),
                            CompletionState::Incomplete,
                        ),
                        Default::default(),
                    ));
                    return Ok(2);
                } else {
                    // Reset quote tracking for the new string
                    self.reset_quote_tracking();
                    self.collection_stack.push((
                        JsonCollection::QuotedString(String::new(), CompletionState::Incomplete),
                        Default::default(),
                    ))
                }
            }
            '\'' => {
                self.collection_stack.push((
                    JsonCollection::SingleQuotedString(String::new(), CompletionState::Incomplete),
                    Default::default(),
                ));
            }
            '`' => {
                // Peek if next 2 characters are also quotes
                let is_triple_quoted = {
                    next.next_if(|&(_, c)| c == '`')
                        .and_then(|_| next.next_if(|&(_, c)| c == '`'))
                        .is_some()
                };

                if is_triple_quoted {
                    self.collection_stack.push((
                        JsonCollection::TripleBacktickString {
                            lang: None,
                            path: None,
                            content: (String::new(), CompletionState::Incomplete),
                        },
                        Default::default(),
                    ));
                    return Ok(2);
                } else {
                    self.collection_stack.push((
                        JsonCollection::BacktickString(String::new(), CompletionState::Incomplete),
                        Default::default(),
                    ))
                }
            }
            '/' => {
                // Could be a comment
                match next.peek() {
                    Some((_, '/')) => {
                        self.collection_stack.push((
                            JsonCollection::TrailingComment(
                                String::new(),
                                CompletionState::Incomplete,
                            ),
                            Default::default(),
                        ));
                        return Ok(1);
                    }
                    Some((_, '*')) => {
                        self.collection_stack.push((
                            JsonCollection::BlockComment(
                                String::new(),
                                CompletionState::Incomplete,
                            ),
                            Default::default(),
                        ));
                        return Ok(1);
                    }
                    _ => {
                        // if we're in an object, this could be the beginning of a string
                        // say a path?
                        if matches!(
                            self.collection_stack.last(),
                            Some((JsonCollection::Object(_, _, _), _))
                        ) {
                            self.collection_stack.push((
                                JsonCollection::UnquotedString(
                                    token.into(),
                                    CompletionState::Incomplete,
                                ),
                                Default::default(),
                            ));
                            return Ok(0);
                        }
                    }
                }
            }
            x if x.is_whitespace() => {}
            x => {
                self.collection_stack.push((
                    JsonCollection::UnquotedString(x.into(), CompletionState::Incomplete),
                    Default::default(),
                ));
                if let CloseStringResult::Close(count, completion) =
                    self.should_close_unescaped_string(next)
                {
                    self.complete_collection(completion);
                    return Ok(count);
                }
            }
        };

        Ok(0)
    }
}

#[derive(Debug, PartialEq)]
enum CloseStringResult {
    Close(usize, CompletionState),
    Continue,
}
