mod tests;

use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use clap::Args;

#[derive(Args, Clone, Debug)]
pub struct DotenvArgs {
    #[arg(
        long,
        help = "Load environment variables from a .env file",
        default_value_t = true
    )]
    pub dotenv: bool,

    #[arg(long, help = "Custom path to a .env file")]
    pub dotenv_path: Option<PathBuf>,
}

impl DotenvArgs {
    pub fn load(&self) -> Result<()> {
        match (self.dotenv, self.dotenv_path.as_ref()) {
            (true, _) => {
                baml_log::warn!("Loading environment variables from .env file");
                dotenv(self.dotenv_path.clone())?;
                Ok(())
            }
            (false, None) => Ok(()),
            (false, Some(dotenv_path)) => {
                baml_log::warn!(
                    "--dotenv was set to false, skipping environment variable loading from {}",
                    dotenv_path.display()
                );
                Ok(())
            }
        }
    }
}

/// Loads environment variables from a .env file
///
/// Features:
/// - Handles multiline strings
/// - Supports variable interpolation (${VAR} or $VAR)
/// - Processes escape sequences
/// - Ignores comments
/// - Supports quoted values (both single and double quotes)
/// - Automatically loads from common .env file locations
/// - Optional variable expansion using current environment
pub fn load_env_file(path: PathBuf) -> Result<HashMap<String, String>> {
    let file = File::open(&path)?;
    let mut reader = BufReader::new(file);
    let mut content = String::new();
    reader.read_to_string(&mut content)?;

    load_env_from_string(&content)
}

pub fn load_env_from_string(content: &str) -> Result<HashMap<String, String>> {
    let mut env_vars = HashMap::new();
    let mut line_index = 0;

    while line_index < content.lines().count() {
        let raw_line = content.lines().nth(line_index).unwrap();
        line_index += 1;

        let line = raw_line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle key-value assignment
        if let Some(equals_pos) = line.find('=') {
            let key = line[..equals_pos].trim().to_string();
            let mut value = line[equals_pos + 1..].trim().to_string();

            // Handle quoted values
            if (value.starts_with('\'') && value.ends_with('\''))
                || (value.starts_with('"') && value.ends_with('"'))
            {
                // Single line quoted value
                value = value[1..value.len() - 1].to_string();
            } else if value.starts_with('\'') || value.starts_with('"') {
                // Add the trailing spaces to the value
                let trailing_spaces = raw_line.trim_start()[equals_pos + 1..]
                    .chars()
                    .rev()
                    .take_while(|c| c.is_whitespace());
                for c in trailing_spaces.collect::<Vec<_>>().into_iter().rev() {
                    value.push(c);
                }

                // Start of multiline string
                let quote = value.chars().next().unwrap();

                // Extract the first line of the multiline value
                value = value[1..].to_string();

                // Continue reading lines until we find the closing quote
                let mut multiline_complete = false;

                while line_index < content.lines().count() && !multiline_complete {
                    let next_line = content.lines().nth(line_index).unwrap();
                    line_index += 1;

                    if next_line.trim().ends_with(quote) {
                        // Found the closing quote
                        let end_index = next_line.rfind(quote).unwrap();
                        value.push('\n');
                        value.push_str(&next_line[..end_index]);
                        multiline_complete = true;
                    } else {
                        // Add the line to our multiline value
                        value.push('\n');
                        value.push_str(next_line);
                    }
                }
            }

            // Handle escaped characters
            value = parse_escaped_chars(&value);

            env_vars.insert(key, value);
        }
    }

    // Handle variable interpolation in a second pass
    expand_variables(&mut env_vars)?;

    Ok(env_vars)
}

fn parse_escaped_chars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('\'') => result.push('\''),
                Some('"') => result.push('"'),
                Some(other) => {
                    result.push('\\');
                    result.push(other);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Expands variable references in values (like $VAR or ${VAR})
fn expand_variables(env_vars: &mut HashMap<String, String>) -> Result<()> {
    let keys: Vec<String> = env_vars.keys().cloned().collect();
    let mut changes_made = true;
    while changes_made {
        changes_made = false;
        for key in keys.clone() {
            let value = env_vars.get(&key).unwrap().clone();
            let expanded = expand_value(&value, env_vars)?;
            if expanded != value {
                env_vars.insert(key, expanded);
                changes_made = true;
            }
        }
    }

    Ok(())
}

/// Expands a single value, replacing any variable references
fn expand_value(value: &str, env_vars: &HashMap<String, String>) -> Result<String> {
    let mut result = String::new();
    let mut chars = value.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek().is_some() {
            let next_char = *chars.peek().unwrap();

            // Handle ${VAR} format
            if next_char == '{' {
                chars.next(); // consume '{'
                let mut var_name = String::new();

                // Read until closing '}'
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    var_name.push(c);
                }

                // Try to find the variable in our env_vars or system env
                if let Some(var_value) = env_vars.get(&var_name) {
                    result.push_str(var_value);
                } else if let Ok(var_value) = env::var(&var_name) {
                    result.push_str(&var_value);
                } else {
                    // Variable not found, leave as is
                    result.push('$');
                    result.push('{');
                    result.push_str(&var_name);
                    result.push('}');
                }
            }
            // Handle $VAR format
            else if next_char.is_alphabetic() || next_char == '_' {
                let mut var_name = String::new();
                var_name.push(next_char);
                chars.next(); // consume first char

                // Read until non-alphanumeric character
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        var_name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                // Try to find the variable in our env_vars or system env
                if let Some(var_value) = env_vars.get(&var_name) {
                    result.push_str(var_value);
                } else if let Ok(var_value) = env::var(&var_name) {
                    result.push_str(&var_value);
                } else {
                    // Variable not found, leave as is
                    result.push('$');
                    result.push_str(&var_name);
                }
            } else {
                result.push('$');
            }
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Loads environment variables from commonly used .env file locations
fn load_env_from_common_locations() -> Result<Option<(HashMap<String, String>, PathBuf)>> {
    let common_locations = [".env", ".env.local", ".env.development", ".env.production"];
    let mut current_dir = std::env::current_dir()?;

    loop {
        for location in &common_locations {
            let path = current_dir.join(location);
            if path.exists() {
                let file_env_vars = load_env_file(path.clone())?;
                return Ok(Some((file_env_vars, path)));
            }
        }

        // Move to the parent directory
        if !current_dir.pop() {
            break; // Reached the root directory
        }
    }

    Ok(None)
}

/// Loads and applies environment variables to the current process
pub fn dotenv(path: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let (env_vars, path) = match path {
        Some(path) => load_env_file(path.clone()).map(|env_vars| (env_vars, path))?,
        None => match load_env_from_common_locations()? {
            Some((env_vars, path)) => (env_vars, path),
            None => return Ok(None),
        },
    };

    for (key, value) in env_vars {
        env::set_var(key, value);
    }

    Ok(Some(path))
}
