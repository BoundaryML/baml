// This file attempts to find all possible JSON objects in a string and parse them.

use std::iter::Peekable;

use anyhow::Result;

/* Try and see if there is a json object somewhere in the string
 * Could be a "[...] some text" or "{...} some text" or even a:
 * ```json
 * ...
 * ```
 * block.
 */
fn find_in_json_markdown(str: &str, options: &JSONishOptions) -> Result<serde_json::Value> {
    let mut values = vec![];

    let mut remaining = str;
    let mut curr_start = 0;
    // First, check for explicit markdown JSON blocks
    while let Some(idx) = remaining.find("```json") {
        let start_idx = idx + 7 + curr_start;
        if let Some(end_idx) = str[start_idx..].find("```") {
            let end_idx = end_idx + start_idx;
            let json_str = str[start_idx..end_idx].trim();
            if json_str.len() > 0 {
                match parse_jsonish_value(json_str, options.recursive()) {
                    Ok(value) => {
                        values.push(value);
                    }
                    Err(_) => {}
                }
            }
            if end_idx + 3 >= remaining.len() {
                break;
            }
            curr_start = end_idx + 3;
            remaining = &remaining[end_idx + 3..];
        } else {
            let json_str = str[start_idx..].trim();
            if json_str.len() > 0 {
                match parse_jsonish_value(json_str, options.recursive()) {
                    Ok(value) => {
                        values.push(value);
                    }
                    Err(_) => {}
                }
            }
            break;
        }
    }

    match values.len() {
        0 => return Err(anyhow::anyhow!("No JSON object found")),
        1 => return Ok(values[0].clone()),
        _ => return Ok(serde_json::Value::Array(values)),
    }
}

fn find_all_json_objects(input: &str, options: &JSONishOptions) -> Result<serde_json::Value> {
    let mut stack = Vec::new();
    let mut json_str_start = None;
    let mut json_objects = Vec::new();

    for (index, character) in input.char_indices() {
        match character {
            '{' | '[' => {
                if stack.is_empty() {
                    json_str_start = Some(index);
                }
                stack.push(character);
            }
            '}' | ']' => {
                if let Some(last) = stack.last() {
                    let expected_open = if character == '}' { '{' } else { '[' };
                    if *last == expected_open {
                        stack.pop();
                    } else {
                        return Err(anyhow::anyhow!("Mismatched brackets"));
                    }
                }

                if stack.is_empty() {
                    // Assuming json_str_start is never None when stack is empty
                    let end_index = index + 1;
                    let json_str = &input[json_str_start.unwrap()..end_index];
                    match parse_jsonish_value(json_str, options.recursive()) {
                        Ok(json) => json_objects.push(json),
                        Err(e) => {
                            // Ignore errors
                            log::error!("Failed to parse JSON object: {:?}", e);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    match json_objects.len() {
        0 => Err(anyhow::anyhow!("No JSON objects found")),
        1 => Ok(json_objects[0].clone()),
        _ => Ok(json_objects.into()),
    }
}

#[derive(Debug)]
enum JsonCollection {
    // Key, Value
    Object(Vec<String>, Vec<serde_json::Value>),
    Array(Vec<serde_json::Value>),
    QuotedString(String),
    SingleQuotedString(String),
    // Handles numbers, booleans, null, and unquoted strings
    UnquotedString(String),
    // Starting with // or #
    TrailingComment(String),
    // Content between /* and */
    BlockComment(String),
}

impl JsonCollection {
    fn name(&self) -> &'static str {
        match self {
            JsonCollection::Object(_, _) => "Object",
            JsonCollection::Array(_) => "Array",
            JsonCollection::QuotedString(_) => "String",
            JsonCollection::SingleQuotedString(_) => "String",
            JsonCollection::UnquotedString(_) => "UnquotedString",
            JsonCollection::TrailingComment(_) => "Comment",
            JsonCollection::BlockComment(_) => "Comment",
        }
    }
}

impl From<JsonCollection> for Option<serde_json::Value> {
    fn from(collection: JsonCollection) -> Option<serde_json::Value> {
        Some(match collection {
            JsonCollection::TrailingComment(_) | JsonCollection::BlockComment(_) => return None,
            JsonCollection::Object(keys, values) => {
                let mut object = serde_json::Map::new();
                for (key, value) in keys.into_iter().zip(values.into_iter()) {
                    object.insert(key, value);
                }
                serde_json::Value::Object(object)
            }
            JsonCollection::Array(values) => serde_json::Value::Array(values),
            JsonCollection::QuotedString(s) => serde_json::Value::String(s),
            JsonCollection::SingleQuotedString(s) => serde_json::Value::String(s),
            JsonCollection::UnquotedString(s) => {
                let s = s.trim();
                if s == "true" {
                    serde_json::Value::Bool(true)
                } else if s == "false" {
                    serde_json::Value::Bool(false)
                } else if s == "null" {
                    serde_json::Value::Null
                } else if let Ok(n) = s.parse::<i64>() {
                    serde_json::Value::Number(n.into())
                } else if let Ok(n) = s.parse::<u64>() {
                    serde_json::Value::Number(n.into())
                } else if let Ok(n) = s.parse::<f64>() {
                    serde_json::Value::Number(serde_json::Number::from_f64(n).unwrap())
                } else {
                    serde_json::Value::String(s.into())
                }
            }
        })
    }
}

struct JsonParseState {
    collection_stack: Vec<JsonCollection>,

    // Technically we may find multiple values in a single string
    completed_values: Vec<(&'static str, serde_json::Value)>,
}

impl JsonParseState {
    fn new() -> Self {
        JsonParseState {
            collection_stack: vec![],
            completed_values: vec![],
        }
    }

    fn complete_collection(&mut self) {
        let collection = match self.collection_stack.pop() {
            Some(collection) => collection,
            None => return,
        };

        let name = collection.name();

        log::debug!("Completed: {:?} -> {:?}", name, collection);

        let value: serde_json::Value = match collection.into() {
            Some(value) => value,
            None => return,
        };

        if let Some(last) = self.collection_stack.last_mut() {
            match last {
                JsonCollection::Object(keys, values) => {
                    if keys.len() == values.len() {
                        match value {
                            serde_json::Value::String(s) => keys.push(s),
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
            self.completed_values.push((name, value));
        }
    }

    fn consume(&mut self, token: char) -> Result<usize> {
        let last = self.collection_stack.last_mut().unwrap();
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
        if let Some(last) = self.collection_stack.last() {
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
                .map(|c| match c {
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
                                        return Some(idx);
                                    }
                                    _ => {
                                        let _ = self.consume(c);
                                    }
                                }
                            } else {
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
                    .map(|c| match c {
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
        if let Some(last) = self.collection_stack.last() {
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
                    .push(JsonCollection::Object(vec![], vec![]));
            }
            '[' => {
                self.collection_stack.push(JsonCollection::Array(vec![]));
            }
            '"' => {
                self.collection_stack
                    .push(JsonCollection::QuotedString(String::new()));
            }
            '\'' => {
                self.collection_stack
                    .push(JsonCollection::SingleQuotedString(String::new()));
            }
            '/' => {
                // Could be a comment
                match next.peek() {
                    Some((_, '/')) => {
                        self.collection_stack
                            .push(JsonCollection::TrailingComment(String::new()));
                        return Ok(1);
                    }
                    Some((_, '*')) => {
                        self.collection_stack
                            .push(JsonCollection::BlockComment(String::new()));
                        return Ok(1);
                    }
                    _ => {}
                }
            }
            x if x.is_whitespace() => {}
            x => {
                self.collection_stack
                    .push(JsonCollection::UnquotedString(x.into()));
                if let Some(count) = self.should_close_unescaped_string(next) {
                    self.complete_collection();
                    return Ok(count);
                }
            }
        };

        return Ok(0);
    }
}

pub fn try_fix_jsonish<'a>(str: &str) -> Result<serde_json::Value> {
    // Try to fix some common JSON issues
    // - Unquoted single word strings
    // - Single quoted strings
    // - Double quoted strings with badly escaped characters
    // - Numbers
    // - Numbers starting with a .
    // - Booleans
    // - Null
    // - Arrays
    // - Objects
    // - Comments
    // - Trailing commas
    // - Leading commas
    // - Unterminated comments
    // - Unterminated arrays
    // - Unterminated objects
    // - Unterminated strings

    let mut state = JsonParseState::new();

    let mut chars = str.char_indices().peekable();
    while let Some((count, c)) = chars.next() {
        let peekable = str[count + c.len_utf8()..].char_indices().peekable();
        match state.process_token(c, peekable) {
            Ok(increments) => {
                for _ in 0..increments {
                    chars.next();
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    // If we still have a collection open, close it
    while !state.collection_stack.is_empty() {
        state.complete_collection();
    }

    // Determine what to return.

    match state.completed_values.len() {
        0 => Err(anyhow::anyhow!("No JSON objects found")),
        1 => {
            let (_name, value) = state.completed_values.pop().unwrap();
            Ok(value)
        }
        _ => {
            if state.completed_values.iter().all(|f| f.0 == "string") {
                Ok(serde_json::Value::Array(
                    state.completed_values.iter().map(|f| f.1.clone()).collect(),
                ))
            } else {
                // Filter for only objects and arrays
                let values: Vec<serde_json::Value> = state
                    .completed_values
                    .iter()
                    .filter_map(|f| {
                        if f.0 == "Object" || f.0 == "Array" {
                            Some(f.1.clone())
                        } else {
                            None
                        }
                    })
                    .collect();
                match values.len() {
                    0 => Err(anyhow::anyhow!("No JSON objects found")),
                    1 => Ok(values[0].clone()),
                    _ => Ok(serde_json::Value::Array(values)),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct JSONishOptions {
    all_finding_all_json_objects: bool,
    allow_markdown_json: bool,
    allow_fixes: bool,
    allow_as_string: bool,
    depth: usize,
}

impl JSONishOptions {
    pub fn default() -> Self {
        JSONishOptions {
            all_finding_all_json_objects: true,
            allow_markdown_json: true,
            allow_fixes: true,
            allow_as_string: true,
            depth: 0,
        }
    }

    fn recursive(&self) -> Self {
        JSONishOptions {
            all_finding_all_json_objects: false,
            allow_markdown_json: false,
            allow_fixes: true,
            allow_as_string: false,
            depth: self.depth + 1,
        }
    }
}

// Responsible for taking a string --> valid JSON
// TODO: @hellovai add max recursive loop
pub fn parse_jsonish_value<'a>(str: &'a str, options: JSONishOptions) -> Result<serde_json::Value> {
    log::debug!("Parsing:\n{:?}\n-------\n{:?}\n-------", options, str);

    if options.depth > 10 {
        return Err(anyhow::anyhow!("Max recursion depth reached"));
    }

    // Try naive parsing first to see if it's valid JSON
    match serde_json::from_str(str) {
        Ok(value) => return Ok(value),
        Err(e) => {
            log::trace!("Failed to parse JSON: {:?}\n{str}", e);
        }
    }

    if options.allow_markdown_json {
        // Then try searching for json-like objects recursively
        if let Ok(value) = find_in_json_markdown(str, &options) {
            if options.depth > 0 {
                return Ok(value);
            }
            return Ok(serde_json::Value::Array(vec![
                value,
                serde_json::Value::String(str.into()),
            ]));
        }
    }

    if options.all_finding_all_json_objects {
        // Then try searching for json-like objects recursively
        if let Ok(value) = find_all_json_objects(str, &options) {
            if options.depth > 0 {
                return Ok(value);
            }
            return Ok(serde_json::Value::Array(vec![
                value,
                serde_json::Value::String(str.into()),
            ]));
        }
    }

    // Finally, try to fix common JSON issues
    if options.allow_fixes {
        match try_fix_jsonish(str) {
            Ok(value) => {
                return Ok(serde_json::Value::Array(vec![
                    value,
                    serde_json::Value::String(str.into()),
                ]));
            }
            Err(e) => {
                log::trace!("Failed to fix JSON: {:?}", e);
            }
        }
    }

    // If all else fails, return the original string
    if options.allow_as_string {
        // If all else fails, return the original string
        Ok(serde_json::Value::String(str.into()))
    } else {
        Err(anyhow::anyhow!("Failed to parse JSON"))
    }
}
