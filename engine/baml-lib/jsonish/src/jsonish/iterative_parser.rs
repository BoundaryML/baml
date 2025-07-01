// This file attempts to find all possible JSON objects in a string and parse them.

use std::iter::Peekable;

use anyhow::Result;
use baml_types::CompletionState;

use crate::jsonish::Value;

/* Try and see if there is a json object somewhere in the string
 * Could be a "[...] some text" or "{...} some text" or even a:
 * ```json
 * ...
 * ```
 * block.
 */
fn find_in_json_markdown(str: &str, options: &JSONishOptions) -> Result<Value> {
    let mut values = vec![];

    let mut remaining = str;
    let mut curr_start = 0;
    // First, check for explicit markdown JSON blocks
    while let Some(idx) = remaining.find("```json") {
        let start_idx = idx + 7 + curr_start;
        if let Some(end_idx) = str[start_idx..].find("```") {
            let end_idx = end_idx + start_idx;
            let json_str = str[start_idx..end_idx].trim();
            if !json_str.is_empty() {
                if let Ok(value) = parse_jsonish_value(json_str, options.recursive()) {
                    values.push(value);
                }
            }
            if end_idx + 3 >= remaining.len() {
                break;
            }
            curr_start = end_idx + 3;
            remaining = &remaining[end_idx + 3..];
        } else {
            let json_str = str[start_idx..].trim();
            if !json_str.is_empty() {
                if let Ok(value) = parse_jsonish_value(json_str, options.recursive()) {
                    values.push(value);
                }
            }
            break;
        }
    }

    match values.len() {
        0 => Err(anyhow::anyhow!("No JSON object found")),
        1 => Ok(values[0].clone()),
        _ => Ok(Value::Array(values, CompletionState::Complete)),
    }
}

fn find_all_json_objects(input: &str, options: &JSONishOptions) -> Result<Value> {
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
                            log::error!("Failed to parse JSON object: {e:?}");
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
        _ => Ok(Value::Array(json_objects, CompletionState::Incomplete)),
    }
}

#[derive(Debug)]
enum JsonCollection {
    // Key, Value
    Object(Vec<String>, Vec<Value>, CompletionState),
    Array(Vec<Value>, CompletionState),
    QuotedString(String, CompletionState),
    SingleQuotedString(String, CompletionState),
    // Handles numbers, booleans, null, and unquoted strings
    UnquotedString(String, CompletionState),
    // Starting with // or #
    TrailingComment(String, CompletionState),
    // Content between /* and */
    BlockComment(String, CompletionState),
}

impl JsonCollection {
    fn name(&self) -> &'static str {
        match self {
            JsonCollection::Object(_, _, _) => "Object",
            JsonCollection::Array(_, _) => "Array",
            JsonCollection::QuotedString(_, _) => "String",
            JsonCollection::SingleQuotedString(_, _) => "String",
            JsonCollection::UnquotedString(_, _) => "UnquotedString",
            JsonCollection::TrailingComment(_, _) => "Comment",
            JsonCollection::BlockComment(_, _) => "Comment",
        }
    }
}

impl From<JsonCollection> for Option<Value> {
    fn from(collection: JsonCollection) -> Option<Value> {
        Some(match collection {
            JsonCollection::TrailingComment(_, _) | JsonCollection::BlockComment(_, _) => {
                return None
            }
            JsonCollection::Object(keys, values, object_completion) => {
                let mut object = Vec::new();
                for (key, value) in keys.into_iter().zip(values.into_iter()) {
                    object.push((key, value));
                }
                Value::Object(object, object_completion)
            }
            JsonCollection::Array(values, completion_state) => {
                Value::Array(values, completion_state)
            }
            JsonCollection::QuotedString(s, completion_state) => Value::String(s, completion_state),
            JsonCollection::SingleQuotedString(s, completion_state) => {
                Value::String(s, completion_state)
            }
            JsonCollection::UnquotedString(s, completion_state) => {
                let s = s.trim();
                if s == "true" {
                    Value::Boolean(true)
                } else if s == "false" {
                    Value::Boolean(false)
                } else if s == "null" {
                    Value::Null
                } else if let Ok(n) = s.parse::<i64>() {
                    Value::Number(n.into(), completion_state)
                } else if let Ok(n) = s.parse::<u64>() {
                    Value::Number(n.into(), completion_state)
                } else if let Ok(n) = s.parse::<f64>() {
                    Value::Number(serde_json::Number::from_f64(n).unwrap(), completion_state)
                } else {
                    Value::String(s.into(), completion_state)
                }
            }
        })
    }
}

struct JsonParseState {
    collection_stack: Vec<JsonCollection>,

    // Technically we may find multiple values in a single string
    completed_values: Vec<(&'static str, Value)>,
}

impl JsonParseState {
    fn new() -> Self {
        JsonParseState {
            collection_stack: vec![],
            completed_values: vec![],
        }
    }

    fn complete_collection(&mut self, completion_state: CompletionState) {
        let collection = match self.collection_stack.pop() {
            Some(collection) => collection,
            None => return,
        };

        let name = collection.name();

        log::debug!("Completed: {name:?} -> {collection:?}");

        let mut value: crate::jsonish::Value = match collection.into() {
            Some(value) => value,
            None => return,
        };
        if completion_state == CompletionState::Complete {
            value.complete_deeply();
        }

        if let Some(last) = self.collection_stack.last_mut() {
            match last {
                JsonCollection::Object(keys, values, completion_state) => {
                    if keys.len() == values.len() {
                        match value {
                            Value::String(s, completion_state) => keys.push(s),
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
            self.completed_values.push((name, value));
        }
    }

    fn consume(&mut self, token: char) -> Result<usize> {
        let last = self.collection_stack.last_mut().unwrap();
        match last {
            JsonCollection::QuotedString(s, _)
            | JsonCollection::BlockComment(s, _)
            | JsonCollection::SingleQuotedString(s, _)
            | JsonCollection::UnquotedString(s, _)
            | JsonCollection::TrailingComment(s, _) => {
                // println!("Consuming: {s} + {:?}", token);
                s.push(token);
            }
            _ => {
                panic!("Unexpected token: {token:?} in: {last:?}");
            }
        }
        Ok(0)
    }

    fn is_string_complete(&self) -> bool {
        // TODO: Do we need to consider the CompletionState here?
        let Some(JsonCollection::UnquotedString(v, _)) = self.collection_stack.last() else {
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
        let pos = if self.collection_stack.len() >= 2 {
            self.collection_stack
                .get(self.collection_stack.len() - 2)
                .map(|c| match c {
                    JsonCollection::Object(keys, values, _) => {
                        if keys.len() == values.len() {
                            2
                        } else {
                            3
                        }
                    }
                    JsonCollection::Array(_, _) => 4,
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
            1 => CloseStringResult::Continue,
            2 => {
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
                                        return CloseStringResult::Close(
                                            idx,
                                            CompletionState::Complete,
                                        );
                                    }
                                    _ => {
                                        let _ = self.consume(c);
                                    }
                                }
                            } else {
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
            4 => {
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
                CloseStringResult::Close(counter, CompletionState::Incomplete)
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
                    if let CloseStringResult::Close(count, completion_state) =
                        self.should_close_unescaped_string(next)
                    {
                        self.complete_collection(completion_state);
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
                self.collection_stack.push(JsonCollection::Object(
                    vec![],
                    vec![],
                    CompletionState::Incomplete,
                ));
            }
            '[' => {
                self.collection_stack
                    .push(JsonCollection::Array(vec![], CompletionState::Incomplete));
            }
            '"' => {
                self.collection_stack.push(JsonCollection::QuotedString(
                    String::new(),
                    CompletionState::Incomplete,
                ));
            }
            '\'' => {
                self.collection_stack
                    .push(JsonCollection::SingleQuotedString(
                        String::new(),
                        CompletionState::Incomplete,
                    ));
            }
            '/' => {
                // Could be a comment
                match next.peek() {
                    Some((_, '/')) => {
                        self.collection_stack.push(JsonCollection::TrailingComment(
                            String::new(),
                            CompletionState::Incomplete,
                        ));
                        return Ok(1);
                    }
                    Some((_, '*')) => {
                        self.collection_stack.push(JsonCollection::BlockComment(
                            String::new(),
                            CompletionState::Incomplete,
                        ));
                        return Ok(1);
                    }
                    _ => {}
                }
            }
            x if x.is_whitespace() => {}
            x => {
                self.collection_stack.push(JsonCollection::UnquotedString(
                    x.into(),
                    CompletionState::Incomplete,
                ));
                if let CloseStringResult::Close(count, completion_state) =
                    self.should_close_unescaped_string(next)
                {
                    self.complete_collection(completion_state);
                    return Ok(count);
                }
            }
        };

        Ok(0)
    }
}

pub fn try_fix_jsonish(str: &str) -> Result<Value> {
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
        state.complete_collection(CompletionState::Incomplete);
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
                Ok(Value::Array(
                    state.completed_values.iter().map(|f| f.1.clone()).collect(),
                    CompletionState::Incomplete,
                ))
            } else {
                // Filter for only objects and arrays
                let values: Vec<Value> = state
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
                    _ => Ok(Value::Array(values, CompletionState::Incomplete)), // TODO: Correct completion state?
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
pub fn parse_jsonish_value(str: &str, options: JSONishOptions) -> Result<Value> {
    log::debug!("Parsing:\n{options:?}\n-------\n{str:?}\n-------");

    if options.depth > 10 {
        return Err(anyhow::anyhow!("Max recursion depth reached"));
    }

    // Try naive parsing first to see if it's valid JSON
    match serde_json::from_str(str) {
        Ok(value) => return Ok(value),
        Err(e) => {
            log::trace!("Failed to parse JSON: {e:?}\n{str}");
        }
    }

    if options.allow_markdown_json {
        // Then try searching for json-like objects recursively
        if let Ok(value) = find_in_json_markdown(str, &options) {
            if options.depth > 0 {
                return Ok(value);
            }
            return Ok(Value::Array(
                vec![
                    value,
                    Value::String(str.into(), CompletionState::Incomplete), // TODO: Correct?
                ],
                CompletionState::Complete,
            )); // TODO: Correct?
        }
    }

    if options.all_finding_all_json_objects {
        // Then try searching for json-like objects recursively
        if let Ok(value) = find_all_json_objects(str, &options) {
            if options.depth > 0 {
                return Ok(value);
            }
            return Ok(Value::Array(
                vec![
                    value,
                    Value::String(str.into(), CompletionState::Complete), // TODO: Correct?
                ],
                CompletionState::Complete,
            )); // TODO: Correct?
        }
    }

    // Finally, try to fix common JSON issues
    if options.allow_fixes {
        match try_fix_jsonish(str) {
            Ok(value) => {
                return Ok(Value::Array(
                    vec![
                        value,
                        Value::String(str.into(), CompletionState::Complete), // TODO: Correct completion state?
                    ],
                    CompletionState::Complete,
                )); // TODO: Correct completion state?
            }
            Err(e) => {
                log::trace!("Failed to fix JSON: {e:?}");
            }
        }
    }

    // If all else fails, return the original string
    if options.allow_as_string {
        // If all else fails, return the original string
        Ok(Value::String(str.into(), CompletionState::Incomplete))
    } else {
        Err(anyhow::anyhow!("Failed to parse JSON"))
    }
}

enum CloseStringResult {
    Close(usize, CompletionState),
    Continue,
}
