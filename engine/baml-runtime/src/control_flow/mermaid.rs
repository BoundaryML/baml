use std::{collections::HashMap, fmt::Write};

use indexmap::IndexMap;

use super::{ControlFlowVisualization, Node, NodeId, NodeType};

/// Convert a `ControlFlowVisualization` into a Mermaid flowchart definition.
pub fn to_mermaid(viz: &ControlFlowVisualization) -> String {
    MermaidRenderer::new(viz).render()
}

struct MermaidRenderer<'a> {
    viz: &'a ControlFlowVisualization,
    aliases: HashMap<NodeId, String>,
    roots: Vec<NodeId>,
    children: IndexMap<NodeId, Vec<NodeId>>,
    edges_by_parent: IndexMap<NodeId, Vec<(NodeId, NodeId)>>,
    root_edges: Vec<(NodeId, NodeId)>,
}

impl<'a> MermaidRenderer<'a> {
    fn new(viz: &'a ControlFlowVisualization) -> Self {
        let mut aliases = HashMap::new();
        for (idx, node) in viz.nodes.values().enumerate() {
            aliases.insert(node.id, format!("n{idx}"));
        }

        let mut roots = Vec::new();
        let mut children: IndexMap<NodeId, Vec<NodeId>> = IndexMap::new();
        for node in viz.nodes.values() {
            if let Some(parent) = &node.parent_node_id {
                children.entry(*parent).or_default().push(node.id);
            } else {
                roots.push(node.id);
            }
        }

        let mut edges_by_parent: IndexMap<NodeId, Vec<(NodeId, NodeId)>> = IndexMap::new();
        let mut root_edges = Vec::new();
        for (src_id, list) in &viz.edges_by_src {
            let parent = viz.nodes.get(src_id).and_then(|node| node.parent_node_id);

            match parent {
                Some(parent_id) => {
                    let entry = edges_by_parent.entry(parent_id).or_default();
                    for edge in list {
                        entry.push((*src_id, edge.dst));
                    }
                }
                None => {
                    for edge in list {
                        root_edges.push((*src_id, edge.dst));
                    }
                }
            }
        }

        Self {
            viz,
            aliases,
            roots,
            children,
            edges_by_parent,
            root_edges,
        }
    }

    fn render(&self) -> String {
        let mut output = String::from("flowchart TD\n");
        if self.roots.is_empty() {
            return output;
        }

        for root in &self.roots {
            self.render_node(root, 0, &mut output);
        }

        if !self.root_edges.is_empty() {
            for (src, dst) in &self.root_edges {
                let _ = writeln!(output, "{} --> {}", self.alias(src), self.alias(dst));
            }
        }

        output
    }

    fn render_node(&self, node_id: &NodeId, depth: usize, output: &mut String) {
        let node = match self.viz.nodes.get(node_id) {
            Some(node) => node,
            None => return,
        };
        let indent = "  ".repeat(depth);
        let label = escape_label(&format_label(node));
        let alias = self.alias(node_id);

        if let Some(children) = self.children.get(node_id) {
            let _ = writeln!(output, "{indent}subgraph {alias}[\"{label}\"]");
            let _ = writeln!(output, "{indent}  direction TB");
            for child in children {
                self.render_node(child, depth + 1, output);
            }

            if let Some(edges) = self.edges_by_parent.get(node_id) {
                let child_indent = "  ".repeat(depth + 1);
                for (src, dst) in edges {
                    let _ = writeln!(
                        output,
                        "{child_indent}{} --> {}",
                        self.alias(src),
                        self.alias(dst)
                    );
                }
            }

            let _ = writeln!(output, "{indent}end");
        } else {
            let _ = writeln!(output, "{indent}{alias}[\"{label}\"]");
        }
    }

    fn alias(&self, node_id: &NodeId) -> &str {
        self.aliases
            .get(node_id)
            .map(|s| s.as_str())
            .unwrap_or("unknown")
    }
}

fn format_label(node: &Node) -> String {
    let base = match node.node_type {
        NodeType::FunctionRoot | NodeType::HeaderContextEnter => {
            if node.label.trim().is_empty() {
                default_label_for(&node.node_type)
            } else {
                node.label.clone()
            }
        }
        NodeType::BranchGroup => prefixed_label("BranchGroup", &node.label),
        NodeType::BranchArm => prefixed_label("BranchArm", &node.label),
        NodeType::Loop => prefixed_label("Loop", &node.label),
        NodeType::OtherScope => prefixed_label("OtherScope", &node.label),
    };
    format!("{}: {base}", node.id.encode())
}

fn prefixed_label(prefix: &str, label: &str) -> String {
    if label.trim().is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}: {label}")
    }
}

fn default_label_for(node_type: &NodeType) -> String {
    match node_type {
        NodeType::FunctionRoot => "Function".to_string(),
        NodeType::HeaderContextEnter => "Header".to_string(),
        NodeType::BranchGroup => "BranchGroup".to_string(),
        NodeType::BranchArm => "BranchArm".to_string(),
        NodeType::Loop => "Loop".to_string(),
        NodeType::OtherScope => "OtherScope".to_string(),
    }
}

fn escape_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => {
                // quotes break Mermaid rendering; drop them entirely
            }
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
