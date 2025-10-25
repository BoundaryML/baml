pub mod diagram_generator;
mod graph;
mod header_collector;
pub(super) mod mermaid_debug;

use std::collections::{HashMap, HashSet};

// Toggle: render function call nodes (e.g., SummarizeVideo, CreatePR) alongside headers.
const SHOW_CALL_NODES: bool = false;

use baml_types::BamlMap;
use graph::{Cluster, ClusterId, Direction, Graph, Node, NodeId, NodeKind};
use internal_baml_diagnostics::SerializedSpan;
use serde_json;
