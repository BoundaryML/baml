use serde::{Deserialize, Serialize};

use crate::{PathSegment, RuntimeNodeType, VizExecEvent};

/// Tracking state for a lexical node while replaying runtime events.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LexicalState {
    NotRunning,
    Running,
    Completed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateUpdate {
    /// Raw node id.
    pub node_id: u32,
    pub log_filter_key: String,
    pub new_state: LexicalState,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Frame {
    pub node_id: u32,
    pub lexical_segment: PathSegment,
    pub log_filter_key: String,
    pub function_name: String,
    pub node_type: RuntimeNodeType,
    pub label: String,
    pub header_level: Option<u8>,
}

/// Reducer with a single unified stack; function contexts are frames on this stack.
#[derive(Default, Debug)]
pub struct VizStateReducer {
    frames: Vec<Frame>,
}

impl VizStateReducer {
    /// Apply a single viz exec event and return the resulting state updates.
    pub fn apply(&mut self, function_name: &str, viz_event: &VizExecEvent) -> Vec<StateUpdate> {
        self.dispatch(function_name, viz_event)
    }

    /// Current stack of frames (root at index 0).
    pub fn dump(&self) -> Vec<Frame> {
        self.frames.clone()
    }

    /// Current log_filter_key stack (root at index 0).
    pub fn log_filter_stack(&self) -> Vec<String> {
        self.frames
            .iter()
            .map(|fr| fr.log_filter_key.clone())
            .collect()
    }
    fn dispatch(&mut self, function_name: &str, viz_event: &VizExecEvent) -> Vec<StateUpdate> {
        match viz_event.event {
            crate::VizExecDelta::Enter => self.handle_enter(function_name, viz_event),
            crate::VizExecDelta::Exit => self.handle_exit(function_name, viz_event),
        }
    }

    fn handle_enter(&mut self, function: &str, viz_event: &VizExecEvent) -> Vec<StateUpdate> {
        let mut updates = Vec::new();

        // Determine invocation context.
        let invocation_fn = if viz_event.node_type == RuntimeNodeType::FunctionRoot {
            function.to_string()
        } else if let Some(top) = self.frames.last() {
            top.function_name.clone()
        } else {
            function.to_string()
        };

        // Pop headers at or deeper than the incoming header level; mirrors HirTraversalContext::pop_headers_to_level.
        if viz_event.node_type == RuntimeNodeType::HeaderContextEnter {
            let new_level = viz_event.header_level.unwrap_or(1).max(1);
            while let Some(top) = self.frames.last() {
                if top.node_type == RuntimeNodeType::HeaderContextEnter
                    && top.function_name == invocation_fn
                {
                    let top_level = top.header_level.unwrap_or(1);
                    if top_level >= new_level {
                        let frame = self.frames.pop().expect("frame to exist");
                        updates.push(StateUpdate {
                            node_id: frame.node_id,
                            log_filter_key: frame.log_filter_key.clone(),
                            new_state: LexicalState::Completed,
                        });
                        continue;
                    }
                }
                break;
            }
        }

        let log_filter_key = {
            let mut segments: Vec<PathSegment> = self
                .frames
                .iter()
                .filter(|f| f.function_name == invocation_fn)
                .map(|f| f.lexical_segment.clone())
                .collect();
            segments.push(viz_event.path_segment.clone());
            encode_segments(&invocation_fn, &segments)
        };

        let frame = Frame {
            node_id: viz_event.node_id,
            lexical_segment: viz_event.path_segment.clone(),
            log_filter_key: log_filter_key.clone(),
            function_name: invocation_fn.clone(),
            node_type: viz_event.node_type.clone(),
            label: viz_event.label.clone(),
            header_level: viz_event.header_level,
        };
        self.frames.push(frame);

        updates.push(StateUpdate {
            node_id: viz_event.node_id,
            log_filter_key,
            new_state: LexicalState::Running,
        });

        updates
    }

    fn handle_exit(&mut self, function: &str, viz_event: &VizExecEvent) -> Vec<StateUpdate> {
        let mut updates = Vec::new();
        let mut popped: Vec<Frame> = Vec::new();
        let mut found = false;

        while let Some(frame) = self.frames.pop() {
            let matches =
                frame.lexical_segment == viz_event.path_segment && frame.function_name == function;
            popped.push(frame);
            if matches {
                found = true;
                break;
            }
        }

        if !found {
            for frame in popped.into_iter().rev() {
                self.frames.push(frame);
            }
            return updates;
        }

        for frame in popped {
            updates.push(StateUpdate {
                node_id: frame.node_id,
                log_filter_key: frame.log_filter_key.clone(),
                new_state: LexicalState::Completed,
            });
        }

        updates
    }
}

/// Encode a full log_filter_key from function name + segments (matches control_flow.rs).
pub fn encode_segments(function: &str, segments: &[PathSegment]) -> String {
    let mut encoded = String::from(function);
    for segment in segments {
        encoded.push('|');
        encoded.push_str(&segment.encode());
    }
    encoded
}
