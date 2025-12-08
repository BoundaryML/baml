mod viz_state_reducer;

use serde::{Deserialize, Serialize};
pub use viz_state_reducer::{encode_segments, Frame, LexicalState, StateUpdate, VizStateReducer};

/// Indicates whether execution is entering or exiting a context node.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VizExecDelta {
    Enter,
    Exit,
}

/// Structural node types that show up in the runtime control-flow visualization.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeNodeType {
    FunctionRoot,
    HeaderContextEnter,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

/// A single encoded lexical path segment (mirrors control_flow.rs).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSegment {
    FunctionRoot { ordinal: u16 },
    Header { slug: String, ordinal: u16 },
    BranchGroup { slug: String, ordinal: u16 },
    BranchArm { slug: String, ordinal: u16 },
    Loop { slug: String, ordinal: u16 },
    OtherScope { slug: String, ordinal: u16 },
}

impl PathSegment {
    /// Encode the segment into its string representation used by `encode_segments`.
    pub fn encode(&self) -> String {
        match self {
            PathSegment::FunctionRoot { ordinal } => format!("root:{ordinal}"),
            PathSegment::Header { slug, ordinal } => format!("hdr:{slug}:{ordinal}"),
            PathSegment::BranchGroup { slug, ordinal } => format!("bg:{slug}:{ordinal}"),
            PathSegment::BranchArm { slug, ordinal } => format!("arm:{slug}:{ordinal}"),
            PathSegment::Loop { slug, ordinal } => format!("loop:{slug}:{ordinal}"),
            PathSegment::OtherScope { slug, ordinal } => format!("scope:{slug}:{ordinal}"),
        }
    }
}

/// Parse the final segment from an encoded lexical id.
pub fn parse_path_segment(encoded: &str) -> Option<PathSegment> {
    let mut parts = encoded.split(':');
    let tag = parts.next()?;
    match tag {
        "root" => {
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::FunctionRoot { ordinal })
        }
        "hdr" => {
            let slug = parts.next()?.to_string();
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::Header { slug, ordinal })
        }
        "bg" => {
            let slug = parts.next()?.to_string();
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::BranchGroup { slug, ordinal })
        }
        "arm" => {
            let slug = parts.next()?.to_string();
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::BranchArm { slug, ordinal })
        }
        "loop" => {
            let slug = parts.next()?.to_string();
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::Loop { slug, ordinal })
        }
        "scope" => {
            let slug = parts.next()?.to_string();
            let ordinal: u16 = parts.next()?.parse().ok()?;
            Some(PathSegment::OtherScope { slug, ordinal })
        }
        _ => None,
    }
}

/// A single control-flow context event emitted by the runtime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VizExecEvent {
    /// Enter/exit transition.
    pub event: VizExecDelta,
    /// Stable, pre-order node identifier that matches `control_flow.rs::ControlFlowVizBuilder`.
    pub node_id: u32,
    /// The logical node type being visited.
    pub node_type: RuntimeNodeType,
    /// Path segment identifying this node (log_filter_key is reconstructed by the reducer).
    pub path_segment: PathSegment,
    /// Human-readable label for the node (header text or synthetic scope label).
    pub label: String,
    /// Header level (only meaningful for header nodes).
    pub header_level: Option<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_viz_exec_event() {
        let original = VizExecEvent {
            event: VizExecDelta::Enter,
            node_id: 42,
            node_type: RuntimeNodeType::HeaderContextEnter,
            path_segment: PathSegment::Header {
                slug: "verify-payment".to_string(),
                ordinal: 0,
            },
            label: "//# Verify payment".to_string(),
            header_level: Some(1),
        };

        let serialized = serde_json::to_string(&original).expect("serialize viz exec event");
        let restored: VizExecEvent =
            serde_json::from_str(&serialized).expect("deserialize viz exec event");

        assert_eq!(restored, original);
    }
}
