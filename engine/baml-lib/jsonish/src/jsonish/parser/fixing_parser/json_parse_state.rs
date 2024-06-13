use std::iter::Peekable;

use crate::jsonish::{value::Fixes, Value};
use anyhow::Result;

use super::json_collection::JsonCollection;

pub struct JsonParseState {
    pub collection_stack: Vec<(JsonCollection, Vec<Fixes>)>,

    // Technically we may find multiple values in a single string
    pub completed_values: Vec<(&'static str, Value, Vec<Fixes>)>,
}

impl JsonParseState {
    pub fn new() -> Self {
        JsonParseState {
            collection_stack: vec![],
            completed_values: vec![],
        }
    }

    pub fn complete_collection(&mut self) {
        let (collection, fixes) = match self.collection_stack.pop() {
            Some(collection) => collection,
            None => return,
        };

        let name = collection.name();

        let value: Value = match collection.into() {
            Some(value) => value,
            None => return,
        };

        if let Some((last, _fixes)) = self.collection_stack.last_mut() {
            match last {
                JsonCollection::Object(keys, values) => {
                    if keys.len() == values.len() {
                        match value {
                            Value::String(s) => keys.push(s),
                            Value::AnyOf(_, s) => keys.push(s),
                            _ => keys.push(value.to_string()),
                        }
                    } else {
                        values.push(value);
                    }
                }
                JsonCollection::Array(values) => {
                    values.push(value);
                }
                _ => {
                    // TODO: this should never happen as we should only be pushing objects and arrays
                    panic!(
                        "Unexpected value: {:?} in collection stack: {:?}",
                        value, last
                    );
                }
            }
        } else {
            self.completed_values.push((name, value, fixes));
        }
    }

    fn consume(&mut self, token: char) -> Result<usize> {
        let (last, _) = self.collection_stack.last_mut().unwrap();
        match last {
            JsonCollection::QuotedString(s)
            | JsonCollection::BlockComment(s)
            | JsonCollection::SingleQuotedString(s)
            | JsonCollection::UnquotedString(s)
            | JsonCollection::TrailingComment(s) => {
                // println!("Consuming: {s} + {:?}", token);
                s.push(token);
            }
            _ => {
                panic!("Unexpected token: {:?} in: {:?}", token, last);
            }
        }
        Ok(0)
    }

    fn is_string_complete(&self) -> bool {
        if let Some((last, _)) = self.collection_stack.last() {
            match last {
                JsonCollection::UnquotedString(v) => {
                    // Check if the token is a valid json character
                    match v.as_str() {
                        "true" | "false" | "null" => {
                            return true;
                        }
                        _ => {
                            // Check if the token parses as a number
                            if let Ok(_) = v.parse::<f64>() {
                                return true;
                            }
                            false
                        }
                    }
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn should_close_unescaped_string(
        &mut self,
        mut next: Peekable<impl Iterator<Item = (usize, char)>>,
    ) -> Option<usize> {
        let pos = if self.collection_stack.len() >= 2 {
            self.collection_stack
                .get(self.collection_stack.len() - 2)
                .map(|(c, _)| match c {
                    JsonCollection::Object(keys, values) => {
                        if keys.len() == values.len() {
                            2
                        } else {
                            3
                        }
                    }
                    JsonCollection::Array(_) => 4,
                    _ => 1,
                })
                .unwrap()
        } else {
            0
        };
        match pos {
            0 => {
                // in nothing, so perhaps the first '{' or '[' is the start of a new object or array
                let mut counter = 0;
                while let Some((idx, c)) = next.next() {
                    counter = idx;
                    match c {
                        // If at some point we find a valid json character, we'll close the string
                        '{' | '[' => return Some(idx),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                Some(counter)
            }
            1 => None,
            2 => {
                // in object key
                let mut counter = 0;
                while let Some((idx, c)) = next.next() {
                    counter = idx;
                    match c {
                        ':' => return Some(idx),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                Some(counter)
            }
            3 => {
                // in object value
                let mut counter = 0;
                while let Some((idx, c)) = next.next() {
                    counter = idx;
                    match c {
                        ',' => {
                            if let Some((_, next_c)) = next.peek() {
                                match next_c {
                                    '\n' => {
                                        log::debug!("Closing due to: newline after comma");
                                        return Some(idx);
                                    }
                                    ' ' => {
                                        log::debug!("Testing for comment after space + comma");
                                        // If after the space we have "//" or "/*" or the beginning of a key, we'll close the string
                                        let mut buffer = ",".to_string();
                                        while let Some((_, next_next_c)) = next.next() {
                                            buffer.push(next_next_c);
                                            match next_next_c {
                                                ' ' | '\n' => {}
                                                '/' => match next.peek() {
                                                    Some((_, '/')) => {
                                                        // This is likely a comment
                                                        return Some(idx);
                                                    }
                                                    Some((_, '*')) => {
                                                        // This is likely a comment
                                                        return Some(idx);
                                                    }
                                                    _ => {
                                                        // let _ = self.consume(c);
                                                    }
                                                },
                                                '"' => {
                                                    // This is likely a new key
                                                    log::debug!("Closing due to: new key after space + comma");
                                                    return Some(idx);
                                                }
                                                x => {
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
                                return Some(idx);
                            }
                        }
                        '}' => return Some(idx),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                Some(counter)
            }
            4 => {
                // in array
                let mut counter = 0;
                while let Some((idx, c)) = next.next() {
                    counter = idx;
                    match c {
                        ',' => return Some(idx),
                        ']' => return Some(idx),
                        x => {
                            let _ = self.consume(x);
                        }
                    }
                }
                Some(counter)
            }
            _ => unreachable!("Invalid position"),
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
                        JsonCollection::Object(keys, values) => {
                            if keys.len() == values.len() {
                                (true, false, false)
                            } else {
                                (false, true, true)
                            }
                        }
                        JsonCollection::Array(_) => (false, false, true),
                        _ => (false, false, false),
                    })
                    .map(|(a, b, c)| (true, a, b, c))
                    .unwrap()
            } else {
                (false, false, false, false)
            };

        if let Some((idx, next_char)) = next.peek() {
            let _idx = *idx;
            match next_char {
                ':' | '}' if in_object_key => {
                    // We're ready to close the key
                    log::debug!("Closing due to: key");
                    true
                }
                ',' | '}' if in_object_value => {
                    // We're ready to close the value
                    log::debug!("Closing due to: value",);
                    true
                }
                ',' | ']' if in_array => {
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
        if let Some((last, _)) = self.collection_stack.last() {
            match last {
                JsonCollection::Object(_, _) => {
                    match token {
                        '}' => {
                            // We're ready to close the object
                            self.complete_collection();
                            Ok(0)
                        }
                        // We can safely ignore these tokens
                        ',' | ':' => Ok(0),
                        // look for a new key or value
                        _ => self.find_any_starting_value(token, next),
                    }
                }
                JsonCollection::Array(_) => {
                    // We could be expecting:
                    // - A value
                    // - a comma
                    // - a closing bracket
                    match token {
                        ']' => {
                            // We're ready to close the array
                            self.complete_collection();
                            Ok(0)
                        }
                        // Skip these tokens
                        ',' => Ok(0),
                        _ => self.find_any_starting_value(token, next),
                    }
                }
                JsonCollection::QuotedString(_) => {
                    // We could be expecting:
                    // - A closing quote
                    // - A character
                    match token {
                        '"' => {
                            // It's possible that the LLM messed up the escaping
                            // We'll try to fix it.
                            if self.should_close_string(next, '"') {
                                self.complete_collection();
                                Ok(0)
                            } else {
                                self.consume(token)
                            }
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::SingleQuotedString(_) => {
                    // We could be expecting:
                    // - A closing quote
                    // - A character
                    // - A space
                    match token {
                        '\'' => {
                            // It's possible that the LLM messed up the escaping
                            // We'll try to fix it.
                            if self.should_close_string(next, '\'') {
                                self.complete_collection();
                                Ok(0)
                            } else {
                                self.consume(token)
                            }
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::UnquotedString(_) => {
                    // We could be expecting:
                    // - A terminating json character (comma, colon, bracket, space, newline)
                    // - A character
                    let res = self.consume(token);
                    if let Some(count) = self.should_close_unescaped_string(next) {
                        self.complete_collection();
                        Ok(count)
                    } else {
                        res
                    }
                }
                JsonCollection::TrailingComment(_) => {
                    // We could be expecting:
                    // - A newline
                    // - A character
                    match token {
                        '\n' => {
                            // We're ready to close the comment
                            self.complete_collection();
                            Ok(0)
                        }
                        _ => self.consume(token),
                    }
                }
                JsonCollection::BlockComment(_) => {
                    // We could be expecting:
                    // - A closing comment
                    // - A character
                    match token {
                        '*' => {
                            // We could be closing the comment
                            match next.peek() {
                                Some((_, '/')) => {
                                    // We're ready to close the comment
                                    self.complete_collection();
                                    Ok(1)
                                }
                                _ => Ok(0),
                            }
                        }
                        _ => self.consume(token),
                    }
                }
            }
        } else {
            // We could be expecting:
            // - A value
            // - Any leading whitespace
            let preview = next.peekable();
            self.find_any_starting_value(token, preview)
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
                self.collection_stack
                    .push((JsonCollection::Object(vec![], vec![]), Default::default()));
            }
            '[' => {
                self.collection_stack
                    .push((JsonCollection::Array(vec![]), Default::default()));
            }
            '"' => {
                self.collection_stack.push((
                    JsonCollection::QuotedString(String::new()),
                    Default::default(),
                ));
            }
            '\'' => {
                self.collection_stack.push((
                    JsonCollection::SingleQuotedString(String::new()),
                    Default::default(),
                ));
            }
            '/' => {
                // Could be a comment
                match next.peek() {
                    Some((_, '/')) => {
                        self.collection_stack.push((
                            JsonCollection::TrailingComment(String::new()),
                            Default::default(),
                        ));
                        return Ok(1);
                    }
                    Some((_, '*')) => {
                        self.collection_stack.push((
                            JsonCollection::BlockComment(String::new()),
                            Default::default(),
                        ));
                        return Ok(1);
                    }
                    _ => {}
                }
            }
            x if x.is_whitespace() => {}
            x => {
                self.collection_stack
                    .push((JsonCollection::UnquotedString(x.into()), Default::default()));
                if let Some(count) = self.should_close_unescaped_string(next) {
                    self.complete_collection();
                    return Ok(count);
                }
            }
        };

        return Ok(0);
    }
}
