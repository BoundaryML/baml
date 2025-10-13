use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{anyhow, Result};
use askama::Template;
use baml_compiler::{
    emit::{ChannelType, EmitChannels},
    hir::Hir,
    thir::typecheck::typecheck,
};
use dir_writer::GeneratorArgs;
use internal_baml_core::{
    feature_flags::FeatureFlags,
    internal_baml_diagnostics::{Diagnostics, SourceFile},
    validate,
};

use crate::{
    ir_to_ts::{stream_type_to_ts, type_to_ts},
    package::CurrentRenderPackage,
    r#type::SerializeType,
};

#[derive(Debug, Clone)]
pub struct VarEventTs {
    pub channel_name: String,
    pub method_suffix: String,
    pub value_type: String,
    pub stream_type: String,
}

#[derive(Debug, Clone)]
pub struct ChildCollectorTs {
    pub baml_name: String,
    pub ts_name: String,
    pub property_name: String,
    pub interface_name: String,
    pub factory_name: String,
}

#[derive(Debug, Clone)]
pub struct EventCollectorTs {
    pub baml_name: String,
    pub ts_name: String,
    pub interface_name: String,
    pub factory_name: String,
    pub var_events: Vec<VarEventTs>,
    pub child_collectors: Vec<ChildCollectorTs>,
    pub has_var_events: bool,
    pub has_child_collectors: bool,
}

struct CollectorBuilder {
    function_name: String,
    has_markdown: bool,
    var_channels: Vec<(String, baml_types::TypeIR)>,
    child_functions: BTreeSet<String>,
}

impl CollectorBuilder {
    fn new(function_name: String) -> Self {
        Self {
            function_name,
            has_markdown: false,
            var_channels: Vec::new(),
            child_functions: BTreeSet::new(),
        }
    }

    fn into_event_collector(
        self,
        pkg: &CurrentRenderPackage,
        function_name_map: &HashMap<String, String>,
    ) -> Result<EventCollectorTs> {
        let ts_name = function_name_map
            .get(&self.function_name)
            .ok_or_else(|| {
                anyhow!(
                    "Missing TypeScript name for function '{}'",
                    self.function_name
                )
            })?
            .clone();

        let mut used_var_suffixes: HashSet<String> = HashSet::new();
        let mut var_events = Vec::new();
        for (channel_name, ty) in self.var_channels.iter() {
            let base = sanitize_identifier(channel_name);
            let method_suffix = make_unique(base, &mut used_var_suffixes);
            let non_streaming = ty.to_non_streaming_type(pkg.lookup());
            let ts_type = type_to_ts(&non_streaming, pkg.lookup());
            let stream_type = stream_type_to_ts(&ty.to_streaming_type(pkg.lookup()), pkg.lookup());
            var_events.push(VarEventTs {
                channel_name: channel_name.clone(),
                method_suffix,
                value_type: ts_type.serialize_type(pkg),
                stream_type: stream_type.serialize_type(pkg),
            });
        }

        let mut used_child_names: HashSet<String> = HashSet::new();
        let mut child_collectors = Vec::new();
        for child_name in self.child_functions.iter() {
            if !function_name_map.contains_key(child_name) {
                continue;
            }
            let child_ts_name = function_name_map
                .get(child_name)
                .expect("Checked contains key above");
            let property_base = format!("function_{}", sanitize_identifier(child_ts_name));
            let property_name = make_unique(property_base, &mut used_child_names);
            child_collectors.push(ChildCollectorTs {
                baml_name: child_name.clone(),
                ts_name: child_ts_name.clone(),
                property_name,
                interface_name: event_interface_name(child_ts_name),
                factory_name: event_factory_name(child_ts_name),
            });
        }

        child_collectors.sort_by(|a, b| a.ts_name.cmp(&b.ts_name));
        var_events.sort_by(|a, b| a.channel_name.cmp(&b.channel_name));

        let has_var_events = !var_events.is_empty();
        let has_child_collectors = !child_collectors.is_empty();

        Ok(EventCollectorTs {
            baml_name: self.function_name,
            ts_name: ts_name.clone(),
            interface_name: event_interface_name(&ts_name),
            factory_name: event_factory_name(&ts_name),
            var_events,
            child_collectors,
            has_var_events,
            has_child_collectors,
        })
    }
}

pub fn build_event_collectors(
    args: &GeneratorArgs,
    pkg: &CurrentRenderPackage,
    function_name_map: &HashMap<String, String>,
) -> Result<Vec<EventCollectorTs>> {
    if args.inlined_file_map.is_empty() {
        return Ok(Vec::new());
    }

    let source_files: Vec<SourceFile> = args
        .inlined_file_map
        .iter()
        .map(|(relative_path, contents)| {
            let path = args.baml_src_dir.join(relative_path);
            SourceFile::from((&path, contents))
        })
        .collect();

    let validated = validate(&args.baml_src_dir, source_files, FeatureFlags::new());
    if validated.diagnostics.has_errors() {
        return Ok(Vec::new());
    }

    let hir = Hir::from_ast(validated.db.ast());
    let mut type_diagnostics = Diagnostics::new(args.baml_src_dir.clone());
    let thir = typecheck(&hir, &mut type_diagnostics);

    let mut emit_diagnostics = Diagnostics::new(args.baml_src_dir.clone());
    let emit_channels = EmitChannels::analyze_program(&thir, &mut emit_diagnostics);

    let mut builders: HashMap<String, CollectorBuilder> = HashMap::new();

    for (fn_name, channels) in emit_channels.functions_channels.iter() {
        if !function_name_map.contains_key(fn_name) {
            continue;
        }
        let entry = builders
            .entry(fn_name.clone())
            .or_insert_with(|| CollectorBuilder::new(fn_name.clone()));

        for (channel, ty) in channels.channels.iter() {
            match channel.r#type {
                ChannelType::Variable => {
                    if channel.namespace.is_none() {
                        entry.var_channels.push((channel.name.clone(), ty.clone()));
                    } else if let Some(namespace) = &channel.namespace {
                        if function_name_map.contains_key(namespace) {
                            entry.child_functions.insert(namespace.clone());
                        }
                    }
                }
                ChannelType::MarkdownHeader => {
                    if channel.namespace.is_none() {
                        entry.has_markdown = true;
                    } else if let Some(namespace) = &channel.namespace {
                        if function_name_map.contains_key(namespace) {
                            entry.child_functions.insert(namespace.clone());
                        }
                    }
                }
            }
        }
    }

    // Ensure child collectors exist so they can be referenced even if they have no direct events.
    let referenced_children: Vec<String> = builders
        .values()
        .flat_map(|builder| builder.child_functions.iter().cloned())
        .collect();

    for child in referenced_children {
        if function_name_map.contains_key(&child) {
            builders
                .entry(child.clone())
                .or_insert_with(|| CollectorBuilder::new(child));
        }
    }

    let mut names: Vec<String> = builders.keys().cloned().collect();
    names.sort();

    let mut collectors = Vec::with_capacity(names.len());
    for name in names {
        if let Some(builder) = builders.remove(&name) {
            collectors.push(builder.into_event_collector(pkg, function_name_map)?);
        }
    }

    collectors.sort_by(|a, b| a.ts_name.cmp(&b.ts_name));
    Ok(collectors)
}

#[derive(Template)]
#[template(path = "events.ts.j2", escape = "none", ext = "txt")]
struct EventsTemplate<'a> {
    collectors: &'a [EventCollectorTs],
}

pub fn render_events(collectors: &[EventCollectorTs]) -> Result<String> {
    Ok(EventsTemplate { collectors }.render()?)
}

fn sanitize_identifier(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for (idx, ch) in input.chars().enumerate() {
        let is_valid = matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$');
        if is_valid {
            if idx == 0 && ch.is_ascii_digit() {
                result.push('_');
            }
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        "_".to_string()
    } else {
        result
    }
}

fn make_unique(base: String, used: &mut HashSet<String>) -> String {
    let mut candidate = if base.is_empty() {
        "_".to_string()
    } else {
        base.clone()
    };
    let mut counter = 2;
    while used.contains(&candidate) {
        candidate = format!("{}{}", base, counter);
        counter += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn event_interface_name(ts_name: &str) -> String {
    format!("{}EventCollector", to_pascal_case(ts_name))
}

fn event_factory_name(ts_name: &str) -> String {
    sanitize_identifier(ts_name)
}

fn to_pascal_case(input: &str) -> String {
    let mut result = String::new();
    let mut uppercase_next = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                result.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                result.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    if result.is_empty() {
        "Collector".to_string()
    } else {
        result
    }
}
