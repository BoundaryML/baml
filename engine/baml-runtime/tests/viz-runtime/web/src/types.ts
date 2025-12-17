export type LexicalState = "not_running" | "running" | "completed";

export interface EventRecord {
  kind: string;
  header?: {
    level: number;
    title: string;
  };
  viz_event?: VizExecEvent | null;
}

export interface StateUpdate {
  node_id: string;
  log_filter_key: string;
  new_state: LexicalState;
}

export type VizExecDelta = "enter" | "exit";

export interface VizExecEvent {
  event: VizExecDelta;
  node_id: number;
  node_type: string;
  path_segment: {
    kind: string;
    slug?: string;
    ordinal: number;
  };
  label: string;
  header_level?: number | null;
}

export interface SnapshotRow {
  watch_event: EventRecord;
  stack_after?: string[] | null;
  emitted_events?: StateUpdate[] | null;
}

export interface SnapshotEntry {
  fixture: string;
  inputFile?: string;
  rows: SnapshotRow[];
}

export interface CombinedRow {
  index: number;
  watchEvent: EventRecord;
  stackAfter: string[];
  emittedEvents: StateUpdate[];
}
