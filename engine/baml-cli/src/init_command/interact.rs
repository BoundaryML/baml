use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Input};

use crate::errors::CliError;

/// Generic function to either return a default value or prompt the user interactively.
///
/// # Arguments
///
/// * `prompt_message` - The message to display if prompting the user.
/// * `default_value` - The default value to use if not prompting.
/// * `no_prompt` - Whether to actually prompt the user.
pub(crate) fn get_value_or_default<T>(
    prompt_message: &str,
    default_value: T,
    no_prompt: bool,
) -> Result<T, CliError>
where
    T: std::str::FromStr + Clone + std::fmt::Display, // Ensure T can be parsed from str, cloned, and converted to a string
    <T as std::str::FromStr>::Err: ToString, // Ensure the error type for parsing T can be converted to a string
{
    if no_prompt {
        Ok(default_value)
    } else {
        match Input::<T>::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt_message)
            .default(default_value)
            .interact()
        {
            Ok(value) => Ok(value),
            Err(_) => Err(CliError::StringError(
                "Failed to get value from user".to_string(),
            )),
        }
    }
}

pub(crate) fn get_value<T>(
    prompt_message: &str,
    default_value: Option<T>,
    no_prompt: bool,
) -> Result<T, CliError>
where
    T: std::str::FromStr + Clone + std::fmt::Display, // Ensure T can be parsed from str, cloned, and converted to a string
    <T as std::str::FromStr>::Err: ToString, // Ensure the error type for parsing T can be converted to a string
{
    match default_value {
        Some(default_value) => get_value_or_default(prompt_message, default_value, no_prompt),
        None => {
            if no_prompt {
                Err(CliError::StringError(format!(
                    "No default value: {}",
                    prompt_message
                )))
            } else {
                Input::<T>::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt_message)
                    .interact()
                    .map_err(|_| CliError::StringError("Failed to get value from user".to_string()))
            }
        }
    }
}

pub(crate) fn get_selection_or_default(
    prompt_message: &str,
    items: &[&str],
    default: usize,
    no_prompt: bool,
) -> Result<usize, CliError> {
    if no_prompt {
        Ok(default)
    } else {
        match dialoguer::Select::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt_message)
            .items(items)
            .default(default)
            .interact()
        {
            Ok(value) => Ok(value),
            Err(_) => Err(CliError::StringError(
                "Failed to get value from user".to_string(),
            )),
        }
    }
}

pub(crate) fn get_multi_selection_or_default(
    prompt_message: &str,
    items: &[&str],
    default: &[bool],
    no_prompt: bool,
) -> Result<Vec<usize>, CliError> {
    if no_prompt {
        Ok(default
            .iter()
            .enumerate()
            .filter_map(|(i, &selected)| if selected { Some(i) } else { None })
            .collect())
    } else {
        let modified_prompt = format!(
            "{}\n{}",
            prompt_message,
            "Space to select, Enter to continue".dimmed()
        );
        match dialoguer::MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt(&modified_prompt)
            .items(items)
            .defaults(default)
            .interact()
        {
            Ok(value) => Ok(value),
            Err(_) => Err(CliError::StringError(
                "Failed to get value from user".to_string(),
            )),
        }
    }
}
