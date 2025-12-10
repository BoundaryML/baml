//! Candidate Applier - Apply candidate changes to create a modified runtime
//!
//! This module handles applying an ImprovedFunction's changes (prompt text,
//! class descriptions/aliases, enum descriptions) to create a new BamlRuntime
//! for evaluation.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Context, Result};

use super::candidate::{ClassDefinition, EnumDefinition, ImprovedFunction, OptimizableFunction};
use crate::{BamlRuntime, InternalRuntimeInterface};

/// Applies candidate changes to create a modified runtime
pub struct CandidateApplier {
    /// The original baml_src path
    baml_src_path: std::path::PathBuf,
    /// Environment variables for the runtime
    env_vars: HashMap<String, String>,
    /// Feature flags
    feature_flags: internal_baml_core::feature_flags::FeatureFlags,
}

impl CandidateApplier {
    /// Create a new candidate applier
    pub fn new(
        baml_src_path: &Path,
        env_vars: HashMap<String, String>,
        feature_flags: internal_baml_core::feature_flags::FeatureFlags,
    ) -> Self {
        Self {
            baml_src_path: baml_src_path.to_path_buf(),
            env_vars,
            feature_flags,
        }
    }

    /// Apply an improved function to create a modified runtime
    ///
    /// This creates a new runtime with:
    /// - The function's prompt replaced with the improved version
    /// - Class/enum descriptions and aliases updated
    pub fn apply(
        &self,
        base_runtime: &BamlRuntime,
        function_name: &str,
        improved: &ImprovedFunction,
    ) -> Result<BamlRuntime> {
        // Read all original source files
        let original_files = self.read_source_files()?;

        // Generate the modified files
        let modified_files =
            self.generate_modified_files(base_runtime, function_name, improved, &original_files)?;

        // Create a new runtime from the modified files
        let root_path = self.baml_src_path.to_string_lossy().to_string();
        BamlRuntime::from_file_content(
            &root_path,
            &modified_files,
            self.env_vars.clone(),
            self.feature_flags.clone(),
        )
        .context("Failed to create modified runtime")
    }

    /// Read all BAML source files from the baml_src directory
    fn read_source_files(&self) -> Result<HashMap<String, String>> {
        let mut files = HashMap::new();

        for entry in walkdir::WalkDir::new(&self.baml_src_path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "baml" {
                        let content = std::fs::read_to_string(path)
                            .with_context(|| format!("Failed to read {}", path.display()))?;
                        let rel_path = path
                            .strip_prefix(&self.baml_src_path)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .to_string();
                        files.insert(rel_path, content);
                    }
                }
            }
        }

        Ok(files)
    }

    /// Generate modified files with the improved function
    fn generate_modified_files(
        &self,
        base_runtime: &BamlRuntime,
        function_name: &str,
        improved: &ImprovedFunction,
        original_files: &HashMap<String, String>,
    ) -> Result<HashMap<String, String>> {
        let mut modified_files = original_files.clone();

        // Find and modify the file containing the function
        let ir = base_runtime.ir();

        // Find the function's config to get its prompt location
        let function = ir
            .walk_functions()
            .find(|f| f.name() == function_name)
            .with_context(|| format!("Function '{}' not found", function_name))?;

        // Get the prompt span to find which file contains the function
        if let Some(config) = function.elem().default_config() {
            let prompt_span = &config.prompt_span;
            let file_path_str = prompt_span.file.path();
            let file_path = std::path::Path::new(&file_path_str);

            // Find this file in our files map
            let rel_path = file_path
                .strip_prefix(&self.baml_src_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file_path_str);

            if let Some(original_content) = modified_files.get(&rel_path).cloned() {
                // Replace the prompt in the file
                let modified_content = self.replace_prompt_in_file(
                    &original_content,
                    function_name,
                    &config.prompt_template,
                    &improved.prompt_text,
                )?;
                modified_files.insert(rel_path, modified_content);
            }
        }

        // Update class definitions
        for class_def in &improved.classes {
            self.update_class_in_files(&mut modified_files, base_runtime, class_def)?;
        }

        // Update enum definitions
        for enum_def in &improved.enums {
            self.update_enum_in_files(&mut modified_files, base_runtime, enum_def)?;
        }

        Ok(modified_files)
    }

    /// Replace the prompt text in a file
    /// TODO: Use the span of the prompt instead of regex search.
    /// Otherwise this will fail for prompts in ##""## blocks
    /// (n-hash raw strings where n > 1).
    fn replace_prompt_in_file(
        &self,
        content: &str,
        function_name: &str,
        old_prompt: &str,
        new_prompt: &str,
    ) -> Result<String> {
        // Simple approach: Find the prompt block and replace it
        // The prompt is typically in a #"..."# block or `...` block

        // Normalize prompts by trimming whitespace for comparison
        let old_prompt_trimmed = old_prompt.trim();
        let new_prompt_trimmed = new_prompt.trim();

        // Try to find the exact old prompt and replace it
        // Only do simple replacement if old_prompt is non-empty and substantial
        if !old_prompt_trimmed.is_empty()
            && old_prompt_trimmed.len() > 10
            && content.contains(old_prompt)
        {
            return Ok(content.replace(old_prompt, new_prompt));
        }

        // Find the function and replace its prompt block
        // This handles empty prompts, whitespace differences, etc.
        if let Some(func_start) = content.find(&format!("function {}", function_name)) {
            // Find the prompt block after the function definition
            let after_func = &content[func_start..];

            // Look for prompt #"..."# pattern
            if let Some(prompt_start) = after_func.find("#\"") {
                let prompt_end_search = &after_func[prompt_start + 2..];
                if let Some(prompt_end) = prompt_end_search.find("\"#") {
                    let abs_start = func_start + prompt_start;
                    let abs_end = func_start + prompt_start + 2 + prompt_end + 2;

                    let before = &content[..abs_start];
                    let after = &content[abs_end..];

                    return Ok(format!(
                        "{}#\"\n{}\n\"#{}",
                        before, new_prompt_trimmed, after
                    ));
                }
            }
        }

        // If we couldn't find a good place to replace, return unchanged
        // This is a fallback - the evaluation will still work with original prompt
        log::warn!(
            "Could not find prompt to replace for function '{}', using original",
            function_name
        );
        Ok(content.to_string())
    }

    /// Update a class definition in the files
    fn update_class_in_files(
        &self,
        files: &mut HashMap<String, String>,
        base_runtime: &BamlRuntime,
        class_def: &ClassDefinition,
    ) -> Result<()> {
        let ir = base_runtime.ir();

        // Find the class in IR to get its location
        let class_walker = ir.walk_classes().find(|c| c.name() == class_def.class_name);

        let Some(class_walker) = class_walker else {
            // Class not found, skip
            return Ok(());
        };

        // Get the class's source location
        let span = class_walker.item.attributes.span.as_ref();
        let Some(span) = span else {
            return Ok(());
        };

        let file_path_str = span.file.path();
        let file_path = std::path::Path::new(&file_path_str);
        let rel_path = file_path
            .strip_prefix(&self.baml_src_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path_str);

        if let Some(content) = files.get(&rel_path).cloned() {
            let modified = self.update_class_attributes(&content, class_def)?;
            files.insert(rel_path, modified);
        }

        Ok(())
    }

    /// Update class attributes (description, alias) in file content
    fn update_class_attributes(
        &self,
        content: &str,
        class_def: &ClassDefinition,
    ) -> Result<String> {
        let mut result = content.to_string();
        let class_name = &class_def.class_name;

        // Find the class definition
        if let Some(class_start) = result.find(&format!("class {}", class_name)) {
            // Find the opening brace
            let after_class = &result[class_start..];
            if let Some(brace_pos) = after_class.find('{') {
                let insert_pos = class_start + brace_pos + 1;

                // Build the new attributes to insert
                let mut attrs = Vec::new();

                if let Some(desc) = &class_def.description {
                    // Escape the description for BAML string
                    let escaped = escape_baml_string(desc);
                    attrs.push(format!("  @@description(#\"{}\"#)", escaped));
                }

                // Update field attributes
                for field in &class_def.fields {
                    if field.description.is_some() || field.alias.is_some() {
                        result = self.update_field_attributes(&result, class_name, field)?;
                    }
                }

                if !attrs.is_empty() {
                    let attrs_str = format!("\n{}\n", attrs.join("\n"));

                    // Check if there's already a @@description and replace it
                    if let Some(desc_start) = result[class_start..].find("@@description") {
                        let abs_desc_start = class_start + desc_start;
                        // Find the end of this attribute (next newline or closing paren)
                        if let Some(desc_end) = find_attribute_end(&result[abs_desc_start..]) {
                            let before = &result[..abs_desc_start];
                            let after = &result[abs_desc_start + desc_end..];
                            if let Some(desc) = &class_def.description {
                                let escaped = escape_baml_string(desc);
                                result =
                                    format!("{}@@description(#\"{}\"#){}", before, escaped, after);
                            }
                        }
                    } else {
                        // Insert new attributes after the opening brace
                        result = format!(
                            "{}{}{}",
                            &result[..insert_pos],
                            attrs_str,
                            &result[insert_pos..]
                        );
                    }
                }
            }
        }

        Ok(result)
    }

    /// Update field attributes (description, alias) in file content
    fn update_field_attributes(
        &self,
        content: &str,
        class_name: &str,
        field: &super::candidate::SchemaFieldDefinition,
    ) -> Result<String> {
        let mut result = content.to_string();
        let field_name = &field.field_name;

        // Find the class definition first
        if let Some(class_start) = result.find(&format!("class {}", class_name)) {
            // Find the field within the class
            let after_class = &result[class_start..];

            // Search for the field name followed by a type (field_name type_name)
            // This is a simple approach that should work for most cases
            let field_pattern = format!("{} ", field_name);

            if let Some(field_pos) = after_class.find(&field_pattern) {
                let abs_field_pos = class_start + field_pos;
                let field_end = abs_field_pos + field_name.len();
                let after_field = &result[field_end..];

                // Find the end of the line (where we'll insert attributes)
                if let Some(newline_pos) = after_field.find('\n') {
                    let line_rest = &after_field[..newline_pos];

                    // Build the attributes to add/update
                    let mut attrs_to_add = Vec::new();

                    if let Some(desc) = &field.description {
                        let escaped = escape_baml_string(desc);
                        attrs_to_add.push(format!("@description(#\"{}\"#)", escaped));
                    }

                    if let Some(alias) = &field.alias {
                        let escaped = escape_baml_string(alias);
                        attrs_to_add.push(format!("@alias(#\"{}\"#)", escaped));
                    }

                    if attrs_to_add.is_empty() {
                        return Ok(result);
                    }

                    // Check if there's already a @description or @alias on this line
                    let has_desc = line_rest.contains("@description");
                    let has_alias = line_rest.contains("@alias");

                    // If we have descriptions/aliases to add or replace
                    if field.description.is_some() && has_desc {
                        // Replace existing @description
                        if let Some(desc_start) = line_rest.find("@description") {
                            let abs_desc_start = field_end + desc_start;
                            if let Some(desc_end) = find_attribute_end(&result[abs_desc_start..]) {
                                let escaped =
                                    escape_baml_string(field.description.as_ref().unwrap());
                                let before = &result[..abs_desc_start];
                                let after = &result[abs_desc_start + desc_end..];
                                result =
                                    format!("{}@description(#\"{}\"#){}", before, escaped, after);
                            }
                        }
                    } else if field.description.is_some() && !has_desc {
                        // Add new @description
                        let escaped = escape_baml_string(field.description.as_ref().unwrap());
                        let insert_pos = field_end + newline_pos;
                        result = format!(
                            "{} @description(#\"{}\"#){}",
                            &result[..insert_pos],
                            escaped,
                            &result[insert_pos..]
                        );
                    }

                    // Re-find positions after potential modification
                    if field.alias.is_some() {
                        // Re-find the field position after description changes
                        if let Some(class_start) = result.find(&format!("class {}", class_name)) {
                            let after_class = &result[class_start..];
                            if let Some(field_pos) = after_class.find(&field_pattern) {
                                let abs_field_pos = class_start + field_pos;
                                let field_end = abs_field_pos + field_name.len();
                                let after_field = &result[field_end..];

                                if let Some(newline_pos) = after_field.find('\n') {
                                    let line_rest = &after_field[..newline_pos];
                                    let has_alias_now = line_rest.contains("@alias");

                                    if has_alias_now {
                                        // Replace existing @alias
                                        if let Some(alias_start) = line_rest.find("@alias") {
                                            let abs_alias_start = field_end + alias_start;
                                            if let Some(alias_end) =
                                                find_attribute_end(&result[abs_alias_start..])
                                            {
                                                let escaped = escape_baml_string(
                                                    field.alias.as_ref().unwrap(),
                                                );
                                                let before = &result[..abs_alias_start];
                                                let after = &result[abs_alias_start + alias_end..];
                                                result = format!(
                                                    "{}@alias(#\"{}\"#){}",
                                                    before, escaped, after
                                                );
                                            }
                                        }
                                    } else {
                                        // Add new @alias
                                        let escaped =
                                            escape_baml_string(field.alias.as_ref().unwrap());
                                        let insert_pos = field_end + newline_pos;
                                        result = format!(
                                            "{} @alias(#\"{}\"#){}",
                                            &result[..insert_pos],
                                            escaped,
                                            &result[insert_pos..]
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Update an enum definition in the files
    fn update_enum_in_files(
        &self,
        files: &mut HashMap<String, String>,
        base_runtime: &BamlRuntime,
        enum_def: &EnumDefinition,
    ) -> Result<()> {
        let ir = base_runtime.ir();

        // Find the enum in IR to get its location
        let enum_walker = ir.walk_enums().find(|e| e.name() == enum_def.enum_name);

        let Some(enum_walker) = enum_walker else {
            return Ok(());
        };

        // Get the enum's source location
        let span = enum_walker.item.attributes.span.as_ref();
        let Some(span) = span else {
            return Ok(());
        };

        let file_path_str = span.file.path();
        let file_path = std::path::Path::new(&file_path_str);
        let rel_path = file_path
            .strip_prefix(&self.baml_src_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path_str);

        if let Some(content) = files.get(&rel_path).cloned() {
            let modified = self.update_enum_attributes(&content, enum_def)?;
            files.insert(rel_path, modified);
        }

        Ok(())
    }

    /// Update enum attributes (value descriptions) in file content
    fn update_enum_attributes(&self, content: &str, enum_def: &EnumDefinition) -> Result<String> {
        let mut result = content.to_string();
        let enum_name = &enum_def.enum_name;

        // Find the enum definition
        if let Some(enum_start) = result.find(&format!("enum {}", enum_name)) {
            // For each value with a description, add @description attribute
            for (value_name, description) in &enum_def.value_descriptions {
                // Find the value in the enum
                let search_start = enum_start;
                if let Some(value_pos) = result[search_start..].find(value_name) {
                    let abs_value_pos = search_start + value_pos;

                    // Check if there's already a @description for this value
                    let value_end = abs_value_pos + value_name.len();
                    let after_value = &result[value_end..];

                    // Look for existing @description on this line
                    if let Some(newline_pos) = after_value.find('\n') {
                        let line_rest = &after_value[..newline_pos];

                        if line_rest.contains("@description") {
                            // Replace existing description
                            if let Some(desc_start) = line_rest.find("@description") {
                                let abs_desc_start = value_end + desc_start;
                                if let Some(desc_end) =
                                    find_attribute_end(&result[abs_desc_start..])
                                {
                                    let escaped = escape_baml_string(description);
                                    let before = &result[..abs_desc_start];
                                    let after = &result[abs_desc_start + desc_end..];
                                    result = format!(
                                        "{}@description(#\"{}\"#){}",
                                        before, escaped, after
                                    );
                                }
                            }
                        } else {
                            // Add new description
                            let escaped = escape_baml_string(description);
                            let insert_pos = value_end + newline_pos;
                            result = format!(
                                "{} @description(#\"{}\"#){}",
                                &result[..insert_pos],
                                escaped,
                                &result[insert_pos..]
                            );
                        }
                    }
                }
            }
        }

        Ok(result)
    }
}

/// Escape a string for use in a BAML #"..."# literal
fn escape_baml_string(s: &str) -> String {
    // In raw string literals, we mainly need to avoid the closing sequence
    s.replace("\"#", "\\\"#")
}

/// Find the end of an attribute (closing paren followed by optional whitespace)
/// TODO: Replaces usages with the span of the attribute.
fn find_attribute_end(s: &str) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut chars = s.chars().peekable();
    let mut pos = 0;

    while let Some(ch) = chars.next() {
        match ch {
            '(' if !in_string => depth += 1,
            ')' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos + 1);
                }
            }
            '"' => in_string = !in_string,
            '#' if !in_string => {
                // Check for #"..."# raw string
                if chars.peek() == Some(&'"') {
                    chars.next();
                    pos += 1;
                    // Find the closing "#
                    while let Some(c) = chars.next() {
                        pos += 1;
                        if c == '"' && chars.peek() == Some(&'#') {
                            chars.next();
                            pos += 1;
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
        pos += ch.len_utf8();
    }

    None
}

/// Apply a candidate's changes without creating a full new runtime
///
/// This is a lighter-weight approach that modifies the files and returns them
/// for inspection or writing to disk.
pub fn generate_candidate_files(
    baml_src_path: &Path,
    base_runtime: &BamlRuntime,
    function_name: &str,
    improved: &ImprovedFunction,
) -> Result<HashMap<String, String>> {
    let applier = CandidateApplier::new(
        baml_src_path,
        HashMap::new(),
        internal_baml_core::feature_flags::FeatureFlags::default(),
    );

    let original_files = applier.read_source_files()?;
    applier.generate_modified_files(base_runtime, function_name, improved, &original_files)
}

/// Write candidate files to the optimization output directory
pub fn write_candidate_files(
    output_dir: &Path,
    candidate_id: usize,
    files: &HashMap<String, String>,
) -> Result<std::path::PathBuf> {
    let candidate_dir = output_dir.join(format!("candidate_{:04}", candidate_id));
    std::fs::create_dir_all(&candidate_dir)?;

    for (rel_path, content) in files {
        let file_path = candidate_dir.join(rel_path);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&file_path, content)?;
    }

    Ok(candidate_dir)
}

/// Apply a candidate's changes directly to the source files
///
/// Returns a list of (file_path, old_content, new_content) for files that were changed.
pub fn apply_to_source_files(
    baml_src_path: &Path,
    base_runtime: &BamlRuntime,
    function_name: &str,
    improved: &ImprovedFunction,
) -> Result<Vec<AppliedChange>> {
    let applier = CandidateApplier::new(
        baml_src_path,
        HashMap::new(),
        internal_baml_core::feature_flags::FeatureFlags::default(),
    );

    let original_files = applier.read_source_files()?;
    let modified_files =
        applier.generate_modified_files(base_runtime, function_name, improved, &original_files)?;

    let mut changes = Vec::new();

    for (rel_path, new_content) in &modified_files {
        let original_content = original_files.get(rel_path);

        // Only process files that actually changed
        if original_content.map(|c| c != new_content).unwrap_or(true) {
            let abs_path = baml_src_path.join(rel_path);

            changes.push(AppliedChange {
                file_path: abs_path.clone(),
                relative_path: rel_path.clone(),
                old_content: original_content.cloned().unwrap_or_default(),
                new_content: new_content.clone(),
            });
        }
    }

    Ok(changes)
}

/// Write the applied changes to disk
pub fn write_changes_to_disk(changes: &[AppliedChange]) -> Result<()> {
    for change in changes {
        std::fs::write(&change.file_path, &change.new_content)
            .with_context(|| format!("Failed to write {}", change.file_path.display()))?;
    }
    Ok(())
}

/// Represents a change to be applied to a source file
#[derive(Debug, Clone)]
pub struct AppliedChange {
    pub file_path: std::path::PathBuf,
    pub relative_path: String,
    pub old_content: String,
    pub new_content: String,
}

impl AppliedChange {
    /// Generate a unified diff of this change
    pub fn diff(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();
        writeln!(output, "--- a/{}", self.relative_path).unwrap();
        writeln!(output, "+++ b/{}", self.relative_path).unwrap();

        // Simple line-by-line diff
        let diff = similar::TextDiff::from_lines(&self.old_content, &self.new_content);

        for (idx, group) in diff.grouped_ops(3).iter().enumerate() {
            if idx > 0 {
                writeln!(output, "...").unwrap();
            }

            for op in group {
                for change in diff.iter_changes(op) {
                    let sign = match change.tag() {
                        similar::ChangeTag::Delete => "-",
                        similar::ChangeTag::Insert => "+",
                        similar::ChangeTag::Equal => " ",
                    };
                    write!(output, "{}{}", sign, change.value()).unwrap();
                }
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimize::candidate::SchemaFieldDefinition;

    #[test]
    fn test_escape_baml_string() {
        assert_eq!(escape_baml_string("hello"), "hello");
        assert_eq!(escape_baml_string("hello\"#world"), "hello\\\"#world");
    }

    #[test]
    fn test_find_attribute_end() {
        assert_eq!(find_attribute_end("@desc(\"hello\")"), Some(14));
        assert_eq!(find_attribute_end("@desc(#\"hello\"#)"), Some(16));
        assert_eq!(find_attribute_end("@desc(nested(foo))"), Some(18));
    }

    #[test]
    fn test_replace_prompt() {
        let content = "function MyFunc(input: string) -> Output {\n\
            client GPT4\n\
            prompt #\"\n\
                Old prompt here\n\
            \"#\n\
        }";
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let result = applier
            .replace_prompt_in_file(content, "MyFunc", "Old prompt here", "New prompt here")
            .unwrap();

        assert!(result.contains("New prompt here"));
        assert!(!result.contains("Old prompt here"));
    }

    #[test]
    fn test_replace_empty_prompt() {
        // Test that replacing an empty prompt doesn't cause infinite duplication
        // Use r##"..."## to avoid conflicts with #" in the content
        let content = r##"class Resume {
  name string
}

function ExtractResume(input: string) -> Resume {
  client GPT4
  prompt #"
  "#
}
"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let result = applier
            .replace_prompt_in_file(
                content,
                "ExtractResume",
                "", // Empty old prompt
                "Extract the name from the input.\n{{ ctx.output_format }}",
            )
            .unwrap();

        // Should contain the new prompt exactly once
        assert!(result.contains("Extract the name from the input."));
        assert_eq!(
            result.matches("Extract the name from the input.").count(),
            1,
            "New prompt should appear exactly once, got:\n{}",
            result
        );
        // Should be a reasonable size (not thousands of copies)
        assert!(
            result.len() < 500,
            "Result should be reasonable size, got {} bytes",
            result.len()
        );
    }

    #[test]
    fn test_replace_whitespace_only_prompt() {
        // Test that replacing a whitespace-only prompt works correctly
        let content = r##"function MyFunc(input: string) -> Output {
  client GPT4
  prompt #"

  "#
}"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let result = applier
            .replace_prompt_in_file(
                content,
                "MyFunc",
                "    \n  ", // Whitespace-only old prompt
                "New prompt content",
            )
            .unwrap();

        assert!(result.contains("New prompt content"));
        assert_eq!(
            result.matches("New prompt content").count(),
            1,
            "New prompt should appear exactly once"
        );
    }

    #[test]
    fn test_update_field_description_new() {
        // Test adding a new field description
        let content = r##"class Person {
  name string
  age int
}"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let field = SchemaFieldDefinition {
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            description: Some("The person's full name".to_string()),
            alias: None,
        };

        let result = applier
            .update_field_attributes(content, "Person", &field)
            .unwrap();

        assert!(result.contains("@description(#\"The person's full name\"#)"));
        // Original field should still be there
        assert!(result.contains("name string"));
    }

    #[test]
    fn test_update_field_description_replace() {
        // Test replacing an existing field description
        let content = r##"class Person {
  name string @description(#"Old description"#)
  age int
}"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let field = SchemaFieldDefinition {
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            description: Some("New description".to_string()),
            alias: None,
        };

        let result = applier
            .update_field_attributes(content, "Person", &field)
            .unwrap();

        assert!(result.contains("@description(#\"New description\"#)"));
        assert!(!result.contains("Old description"));
    }

    #[test]
    fn test_update_field_alias_new() {
        // Test adding a new field alias
        let content = r##"class Person {
  name string
  age int
}"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let field = SchemaFieldDefinition {
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            description: None,
            alias: Some("full_name".to_string()),
        };

        let result = applier
            .update_field_attributes(content, "Person", &field)
            .unwrap();

        assert!(result.contains("@alias(#\"full_name\"#)"));
    }

    #[test]
    fn test_update_field_description_and_alias() {
        // Test adding both description and alias
        let content = r##"class Person {
  name string
  age int
}"##;
        let applier = CandidateApplier::new(
            std::path::Path::new("/tmp"),
            HashMap::new(),
            internal_baml_core::feature_flags::FeatureFlags::default(),
        );

        let field = SchemaFieldDefinition {
            field_name: "name".to_string(),
            field_type: "string".to_string(),
            description: Some("The person's full name".to_string()),
            alias: Some("full_name".to_string()),
        };

        let result = applier
            .update_field_attributes(content, "Person", &field)
            .unwrap();

        assert!(result.contains("@description(#\"The person's full name\"#)"));
        assert!(result.contains("@alias(#\"full_name\"#)"));
    }
}
