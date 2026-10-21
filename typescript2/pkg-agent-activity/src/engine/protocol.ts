// Wire protocol between `baml agent watch` and the Agent Activity UI.
//
// The server sends JSON text frames on `/api/agent-activity/ws`. On connect it
// sends one `hello` carrying the session, the baseline of every file changed
// so far, and the ordered change and run history; after that it streams one
// `change` per settled file write, a `run` when something is executed and
// again when it finishes, and a `git` frame whenever the branch or HEAD moves.
//
// A change carries only the file's new content (or line splices). The client
// derives the previous content from the baseline and earlier changes, which
// keeps the history compact and makes a reconnect replay exactly what a live
// client saw.
//
// Recorded sessions (the replay scenarios) are logs of these same messages
// with a time offset each, so a replay and a live session drive the UI
// through the same path.
//
// Keep in sync with `baml_agent_watch::protocol` on the Rust side.

export const AGENT_ACTIVITY_PROTOCOL = 1;

export interface CommitInfo {
  sha: string;
  subject: string;
  /** Commit time, Unix milliseconds. */
  time: number;
}

export interface GitState {
  branch: string | null;
  head: string;
  headSubject: string;
  /** HEAD when the session started. */
  startHead: string;
  /** Commits reachable from HEAD but not from `startHead`, oldest first. */
  commits: CommitInfo[];
}

export interface SessionInfo {
  /** Absolute path of the watched directory. */
  root: string;
  /** Display name, usually the directory's basename. */
  name: string;
  /** Unix milliseconds. */
  startedAt: number;
  git: GitState | null;
}

export type ChangeKind = 'created' | 'modified' | 'deleted';

/**
 * An image to load instead of content: a URL on the server (whose path ends
 * in the content's hash), or a `data:` URL in recorded sessions.
 */
export interface MediaRef {
  url: string;
  mime: string;
}

/** Replace `remove` lines starting at 0-based line `at` with `insert`. */
export interface Splice {
  at: number;
  remove: number;
  insert: string[];
}

export interface FileChange {
  /** Monotonic per session, starting at 1. */
  seq: number;
  /** Unix milliseconds when the write settled. */
  time: number;
  /** Relative to the session root, `/`-separated. */
  path: string;
  kind: ChangeKind;
  /**
   * New content; `null` for deletions and for files that are not shown as
   * text. Omitted when `splices` describes the change instead.
   */
  after?: string | null;
  /**
   * The change as line splices applied in order to the previous content. A
   * compact alternative to `after` for small edits to large files; recorded
   * sessions use it.
   */
  splices?: Splice[];
  /** The file is binary; `after` is `null`. */
  binary?: boolean;
  /** The file exceeds the server's size cap; `after` is `null`. */
  tooLarge?: boolean;
  size: number | null;
  /** The new content when it is an image. */
  media?: MediaRef;
}

export type RunKind = 'baml-run' | 'baml-test' | 'command';
export type RunStatus = 'running' | 'passed' | 'failed';

/** A function or declaration a run executed, located in the source. */
export interface RunFrame {
  name: string;
  path: string;
  /** 0-based line of the declaration. */
  line?: number;
  durationMs?: number;
  /** Present when this frame made an LLM call. */
  llm?: {
    client: string;
    model?: string;
    inputTokens?: number;
    outputTokens?: number;
  };
}

/**
 * Something the agent executed. `baml run` and `baml test` report which
 * functions ran and what they returned; any other command is known only by
 * its command line and output.
 */
export interface RunInfo {
  id: string;
  kind: RunKind;
  command: string;
  status: RunStatus;
  startedAt: number;
  endedAt?: number;
  /** The entry point: the function or test that was run. */
  target?: RunFrame;
  /** Every function that executed, in call order, entry point first. */
  frames?: RunFrame[];
  /** Files whose code ran (or that the command is about), most relevant first. */
  touches: string[];
  input?: unknown;
  output?: unknown;
  error?: string;
  /** Tail of the combined stdout and stderr. */
  log?: string;
}

/**
 * Something the agent said, from its transcript when one is available (Claude
 * Code and Codex keep them on disk). Shown as captions next to the work.
 */
export interface AgentNote {
  id: string;
  time: number;
  text: string;
  /** Which agent's transcript this came from, e.g. `claude-code`. */
  agent?: string;
}

export interface NoteMessage {
  type: 'note';
  note: AgentNote;
}

/**
 * A BAML workflow: an entry function (one no other function calls) and its
 * control-flow graph with user-function callees inlined, as the compiler
 * builds it for the playground's graph view.
 */
export interface Workflow {
  name: string;
  /** Root-relative file declaring the entry function. */
  path: string;
  /**
   * The prepared `ControlFlowGraph` (see `@b/pkg-playground` worker-protocol).
   * Node source spans carry root-relative `filePath`s.
   */
  graph: unknown;
}

/** Every workflow of one BAML project, sent whenever its sources change. */
export interface FlowsMessage {
  type: 'flows';
  /** Root-relative project directory (the one holding `baml.toml` or `baml_src/`). */
  project: string;
  workflows: Workflow[];
}

export interface HelloMessage {
  type: 'hello';
  protocol: number;
  session: SessionInfo;
  /** Content at session start for every path in `changes`; `null` if absent then. */
  baselines: Record<string, string | null>;
  /** Paths in `baselines` that were images when the session started. */
  baselineMedia?: Record<string, MediaRef>;
  changes: FileChange[];
  /** Runs so far, in start order, each in its latest state. */
  runs?: RunInfo[];
  /** Agent messages so far, oldest first. */
  notes?: AgentNote[];
  /** Latest workflows of each BAML project the session touched. */
  flows?: FlowsMessage[];
}

/** Sent when a run starts and again when it finishes; `run.id` matches. */
export interface RunMessage {
  type: 'run';
  run: RunInfo;
}

export interface ChangeMessage {
  type: 'change';
  change: FileChange;
  /** Present when this is the first change to `change.path`. */
  baseline?: string | null;
  /** With `baseline`, when the path was an image when the session started. */
  baselineMedia?: MediaRef;
}

export interface GitMessage {
  type: 'git';
  git: GitState;
}

export type ServerMessage =
  | HelloMessage
  | ChangeMessage
  | GitMessage
  | RunMessage
  | NoteMessage
  | FlowsMessage;
