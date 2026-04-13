//! Extracts client option field names and provider mappings from `llm_types.baml`.
//!
//! This avoids manually maintaining field lists in `lower_cst.rs` -- adding a
//! new field to `PrimitiveClientOptions` or a new provider options class in the
//! BAML file is enough; the compiler picks it up automatically.

use std::io::Write;

const LLM_TYPES_BAML: &str = "../baml_builtins2/baml_std/baml/ns_llm/llm_types.baml";

/// Namespace prefix for provider options types.
const NS_PREFIX: &str = "baml.llm.";

fn main() {
    println!("cargo:rerun-if-changed={LLM_TYPES_BAML}");
    println!("cargo:rerun-if-changed=build.rs");

    let source = std::fs::read_to_string(LLM_TYPES_BAML)
        .unwrap_or_else(|e| panic!("failed to read {LLM_TYPES_BAML}: {e}"));

    let client_option_fields = extract_class_fields(&source, "PrimitiveClientOptions");
    let provider_configs = extract_provider_configs(&source);
    let no_options_providers = extract_no_options_providers(&source);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = format!("{out_dir}/client_fields_generated.rs");
    let mut out = std::fs::File::create(&out_path)
        .unwrap_or_else(|e| panic!("failed to create {out_path}: {e}"));

    // Emit CLIENT_OPTION_FIELDS
    writeln!(
        out,
        "/// Field names on `PrimitiveClientOptions`, extracted from `llm_types.baml`."
    )
    .unwrap();
    writeln!(out, "pub(super) const CLIENT_OPTION_FIELDS: &[&str] = &[").unwrap();
    for field in &client_option_fields {
        writeln!(out, "    \"{field}\",").unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();

    // Emit ProviderFieldConfig struct
    writeln!(
        out,
        "/// Provider-to-options-class mapping, extracted from `llm_types.baml`."
    )
    .unwrap();
    writeln!(out, "#[derive(Debug)]").unwrap();
    writeln!(out, "pub(super) struct ProviderFieldConfig {{").unwrap();
    writeln!(out, "    pub(super) providers: &'static [&'static str],").unwrap();
    writeln!(out, "    pub(super) options_type: Option<&'static str>,").unwrap();
    writeln!(out, "    pub(super) fields: &'static [&'static str],").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();

    // Emit PROVIDER_CONFIGS
    writeln!(
        out,
        "pub(super) const PROVIDER_CONFIGS: &[ProviderFieldConfig] = &["
    )
    .unwrap();

    for config in &provider_configs {
        let providers_list: Vec<String> = config
            .providers
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect();
        let fields_list: Vec<String> = config.fields.iter().map(|f| format!("\"{f}\"")).collect();
        writeln!(out, "    ProviderFieldConfig {{").unwrap();
        writeln!(out, "        providers: &[{}],", providers_list.join(", ")).unwrap();
        writeln!(
            out,
            "        options_type: Some(\"{NS_PREFIX}{}\"),",
            config.class_name
        )
        .unwrap();
        writeln!(out, "        fields: &[{}],", fields_list.join(", ")).unwrap();
        writeln!(out, "    }},").unwrap();
    }

    // Emit no-options providers
    if !no_options_providers.is_empty() {
        let providers_list: Vec<String> = no_options_providers
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect();
        writeln!(out, "    ProviderFieldConfig {{").unwrap();
        writeln!(out, "        providers: &[{}],", providers_list.join(", ")).unwrap();
        writeln!(out, "        options_type: None,").unwrap();
        writeln!(out, "        fields: &[],").unwrap();
        writeln!(out, "    }},").unwrap();
    }

    writeln!(out, "];").unwrap();
}

struct ProviderConfig {
    providers: Vec<String>,
    class_name: String,
    fields: Vec<String>,
}

/// Extract field names from a BAML class definition.
fn extract_class_fields(source: &str, class_name: &str) -> Vec<String> {
    let marker = format!("class {class_name} {{");
    let Some(start) = source.find(&marker) else {
        panic!("class {class_name} not found in {LLM_TYPES_BAML}");
    };
    let body = &source[start + marker.len()..];
    let mut fields = Vec::new();
    let mut brace_depth = 1;

    for line in body.lines() {
        // Strip inline comments before counting braces.
        let code = line.split_once("//").map_or(line, |(code, _)| code).trim();
        brace_depth += code.chars().filter(|&c| c == '{').count();
        brace_depth -= code.chars().filter(|&c| c == '}').count();
        if brace_depth == 0 {
            break;
        }
        // Skip empty lines and nested blocks (functions, etc.)
        if code.is_empty() || code.starts_with("function ") {
            continue;
        }
        // Skip lines that are inside nested blocks (brace_depth > 1)
        if brace_depth > 1 {
            continue;
        }
        // Field line: first word is the field name
        if let Some(name) = code.split_whitespace().next() {
            // Skip keywords
            if name == "class" || name == "}" || name == "{" {
                continue;
            }
            fields.push(name.to_string());
        }
    }
    fields
}

/// Extract `// @providers:` annotations and the class that follows each one.
fn extract_provider_configs(source: &str) -> Vec<ProviderConfig> {
    let mut configs = Vec::new();
    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// @providers:") {
            let providers: Vec<String> = rest
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            // Find the next `class` line
            for next_line in &lines[(i + 1)..] {
                let next = next_line.trim();
                if next.is_empty() || next.starts_with("//") {
                    continue;
                }
                if let Some(rest) = next.strip_prefix("class ") {
                    let class_name = rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim_end_matches('{')
                        .trim()
                        .to_string();

                    let fields = extract_class_fields(source, &class_name);
                    configs.push(ProviderConfig {
                        providers,
                        class_name,
                        fields,
                    });
                } else {
                    panic!("@providers annotation not followed by a class, found: {next}");
                }
                break;
            }
        }
    }
    configs
}

/// Extract `// @providers-no-options:` provider list.
fn extract_no_options_providers(source: &str) -> Vec<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("// @providers-no-options:") {
            return rest
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}
