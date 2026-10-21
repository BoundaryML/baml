// Flow: the program's workflows, drawn the way the BAML playground draws a
// function. Each workflow is an entry function (one nothing else calls) with
// the functions it calls inlined, as the compiler builds it; the watch server
// sends a project's workflows whenever its `.baml` files change.
//
// The code being edited sits above the graphs. An edit lights up the nodes
// whose source lines it changed (in every workflow that inlines them); a run
// marks the steps it executed; pointing at a line of code lights up its node.

import { GraphView as CfgGraphView } from '@b/pkg-playground/graph/GraphView';
import type { NodeExecutionState } from '@b/pkg-playground/graph/types';
import type {
  CfgNode,
  ControlFlowGraph,
} from '@b/pkg-playground/worker-protocol';
import { memo, useMemo, useState } from 'react';
import { EditCaptionLine } from '../components/caption';
import { type DiffLine, EditDiff } from '../components/edit-diff';
import { splitPath } from '../components/symbols';
import type { Edit, Run } from '../engine/model';
import type { Workflow } from '../engine/protocol';
import { useModel, usePlayer } from '../runtime';

/** Workflows shown at once; the rest are one click away. */
const MAX_SHOWN = 4;

export function FlowView() {
  const model = useModel();
  const snap = usePlayer();
  const [pointed, setPointed] = useState<{ path: string; line: number } | null>(
    null,
  );
  const [pinnedName, setPinnedName] = useState<string | null>(null);

  const edit = snap.active?.edit ?? model.edits[snap.played - 1];
  const workflows = useMemo(
    () => model.workflowsAt(snap.played),
    [model.version, snap.played],
  );

  // Workflows that run code the session touched, most recently touched first.
  const relevant = useMemo(() => {
    const touched = new Map<string, number>();
    for (const f of model.touchedFiles(snap.played))
      touched.set(f.path, model.lastEditOf(f.path, snap.played) ?? -1);
    const scored = workflows
      .map((w) => {
        let last = -1;
        for (const n of nodesOf(w)) {
          const p = n.sourceSpan?.filePath;
          if (p && touched.has(p)) last = Math.max(last, touched.get(p)!);
        }
        return { last, w };
      })
      .filter((s) => s.last >= 0)
      .sort((a, b) => b.last - a.last);
    return scored.map((s) => s.w);
  }, [workflows, model.version, snap.played]);

  const pinned = pinnedName
    ? relevant.find((w) => w.name === pinnedName)
    : undefined;
  const shown = pinned
    ? [pinned, ...relevant.filter((w) => w !== pinned)].slice(0, MAX_SHOWN)
    : relevant.slice(0, MAX_SHOWN);
  const hidden = relevant.filter((w) => !shown.includes(w));

  const lastRun =
    snap.runsShown > 0 ? model.runs[snap.runsShown - 1] : undefined;
  const shownRun =
    lastRun && lastRun.afterEdit >= snap.played && !snap.active
      ? lastRun
      : undefined;

  const onLine = (line: DiffLine | null) => {
    setPointed(
      line && edit && line.side === 'new'
        ? { line: line.line, path: edit.path }
        : null,
    );
  };

  return (
    <div className="aa-graph-host aa-flow-host" data-mode="split">
      <div className="aa-graph-split-code">
        {edit ? (
          <section
            aria-label={edit.path}
            className="aa-pane aa-graph-code"
            data-active={snap.active?.edit === edit}
          >
            <header className="aa-pane-head">
              <span className="aa-pane-path" title={edit.path}>
                <span className="dir">{`‎${splitPath(edit.path).dir}‎`}</span>
                <span className="name">{splitPath(edit.path).name}</span>
              </span>
              <span className="aa-pane-meta">
                {snap.active?.edit === edit ? (
                  <span className="aa-writing">writing</span>
                ) : null}
                <span>Point at a line to find it in the flow</span>
              </span>
            </header>
            <EditCaptionLine edit={edit} />
            <div className="aa-pane-body">
              <EditDiff
                active={snap.active}
                context={8}
                edit={edit}
                onLine={(l) => onLine(l)}
              />
            </div>
          </section>
        ) : (
          <div className="aa-empty">The code being edited shows here.</div>
        )}
      </div>
      <div className="aa-flow-grid" data-count={shown.length}>
        {shown.length === 0 && (
          <div className="aa-waiting">
            <h2>No workflows yet</h2>
            <p>
              When the watched directory holds a BAML project,{' '}
              <code>baml agent watch</code> compiles it after each change and
              draws every entry function here, with the functions it calls
              inlined.
            </p>
          </div>
        )}
        {shown.map((w) => (
          <WorkflowCard
            edit={edit ?? null}
            editing={Boolean(snap.active)}
            key={`${w.path}:${w.name}`}
            pointed={pointed}
            run={shownRun}
            workflow={w}
          />
        ))}
        {hidden.length > 0 && (
          <div className="aa-flow-more">
            {hidden.map((w) => (
              <button
                className="aa-text-button"
                key={`${w.path}:${w.name}`}
                onClick={() => setPinnedName(w.name)}
                type="button"
              >
                {w.name}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

const WorkflowCard = memo(function WorkflowCard({
  workflow,
  edit,
  editing,
  pointed,
  run,
}: {
  workflow: Workflow;
  edit: Edit | null;
  editing: boolean;
  pointed: { path: string; line: number } | null;
  run: Run | undefined;
}) {
  const graph = workflow.graph as ControlFlowGraph;
  const edited = useMemo(
    () => (edit ? nodesTouchedBy(edit, graph) : new Set<string>()),
    [edit, graph],
  );
  const pointedNodes = useMemo(
    () =>
      pointed
        ? innermost(
            graph,
            nodesAtLines(graph, pointed.path, new Set([pointed.line])),
          )
        : null,
    [pointed, graph],
  );
  const states = useMemo(
    () => (run ? runStates(run, workflow) : undefined),
    [run, workflow],
  );
  const highlighted = pointedNodes && pointedNodes.size ? pointedNodes : edited;
  const { dir, name } = splitPath(workflow.path);
  return (
    <section
      aria-label={workflow.name}
      className="aa-flow-card"
      data-active={editing && edited.size > 0}
    >
      <header className="aa-flow-card-head">
        <b>{workflow.name}</b>
        <span className="aa-flow-file">
          {dir}
          {name}
        </span>
        {editing && edited.size > 0 && (
          <span className="aa-writing">editing</span>
        )}
        {run && states?.size ? (
          <span className="aa-flow-ran" data-status={run.status}>
            {run.status === 'failed' ? 'failed' : 'ran'}
          </span>
        ) : null}
      </header>
      <div className="aa-flow-canvas">
        <CfgGraphView
          functionName={`agent-activity:${workflow.name}`}
          graph={graph}
          highlightedNodeIds={highlighted}
          nodeExecutionStates={states}
          onNodeClick={() => {}}
          selectedNodeId={null}
        />
      </div>
    </section>
  );
});

function nodesOf(w: Workflow): CfgNode[] {
  return Object.values((w.graph as ControlFlowGraph).nodes ?? {});
}

/** Nodes whose source span in `path` covers one of `lines` (0-based). */
function nodesAtLines(
  graph: ControlFlowGraph,
  path: string,
  lines: Set<number>,
): Set<string> {
  const out = new Set<string>();
  for (const [id, n] of Object.entries(graph.nodes)) {
    const span = n.sourceSpan;
    if (!span || span.filePath !== path) continue;
    const end = span.endLine ?? span.line;
    for (const l of lines) {
      if (l >= span.line && l <= end) {
        out.add(id);
        break;
      }
    }
  }
  return out;
}

/** Drop nodes that contain another selected node, so a line lights its step, not the whole function. */
function innermost(graph: ControlFlowGraph, ids: Set<string>): Set<string> {
  const out = new Set(ids);
  for (const id of ids) {
    let parent = graph.nodes[id]?.parentNodeId;
    while (parent !== null && parent !== undefined) {
      out.delete(String(parent));
      parent = graph.nodes[String(parent)]?.parentNodeId;
    }
  }
  return out;
}

/**
 * The nodes an edit touched: the innermost nodes whose source it changed,
 * and the call nodes of functions whose declaration it changed.
 */
function nodesTouchedBy(edit: Edit, graph: ControlFlowGraph): Set<string> {
  const lines = new Set<number>();
  for (const op of edit.ops) if (op.type === 'insert') lines.add(op.newIndex);
  if (lines.size === 0) lines.add(edit.focusLine);
  const touched = innermost(graph, nodesAtLines(graph, edit.path, lines));
  const changedFns = new Set(
    edit.symbols.filter((s) => s.kind === 'function').map((s) => s.name),
  );
  for (const [id, n] of Object.entries(graph.nodes)) {
    if (n.calleeName && changedFns.has(n.calleeName)) touched.add(id);
  }
  return touched;
}

/** Execution states for the nodes a run executed. */
function runStates(
  run: Run,
  workflow: Workflow,
): Map<string, NodeExecutionState> {
  const graph = workflow.graph as ControlFlowGraph;
  const names = new Set((run.frames ?? []).map((f) => f.name));
  if (run.target) names.add(run.target.name);
  const state: NodeExecutionState =
    run.status === 'failed'
      ? 'error'
      : run.status === 'running'
        ? 'running'
        : 'success';
  const out = new Map<string, NodeExecutionState>();
  for (const [id, n] of Object.entries(graph.nodes)) {
    const isRoot = n.nodeType === 'functionRoot' && names.has(workflow.name);
    if (
      isRoot ||
      (n.calleeName && names.has(n.calleeName)) ||
      names.has(n.label)
    )
      out.set(id, state);
  }
  return out;
}
