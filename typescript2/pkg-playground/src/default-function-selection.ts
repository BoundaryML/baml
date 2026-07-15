import type { ControlFlowGraph } from './worker-protocol';

function lastNameSegment(name: string): string {
  const parts = name.split('.');
  return parts[parts.length - 1] ?? name;
}

export function selectMainFunctionName(
  functionNames: readonly string[],
): string | null {
  return (
    functionNames.find((name) => name === 'main') ??
    functionNames.find((name) => lastNameSegment(name) === 'main') ??
    null
  );
}

function resolveFunctionName(
  functionNames: readonly string[],
  rawName: string,
): string | null {
  return (
    functionNames.find(
      (name) =>
        name === rawName ||
        name.endsWith(`.${rawName}`) ||
        rawName.endsWith(`.${name}`),
    ) ?? null
  );
}

function graphCalleeNames(
  graph: ControlFlowGraph,
): Iterable<string> {
  const names: string[] = [];
  for (const node of Object.values(graph.nodes)) {
    if (node.calleeNames && node.calleeNames.length > 0) {
      names.push(...node.calleeNames);
    } else if (node.calleeName) {
      names.push(node.calleeName);
    }
  }
  return names;
}

export function selectLongestWorkflowFunctionName(
  functionNames: readonly string[],
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): string | null {
  const graphNames = functionNames.filter((name) => graphsByFunction.has(name));
  if (graphNames.length === 0) return null;

  const callers = new Map<string, Set<string>>();
  for (const fn of graphNames) {
    const graph = graphsByFunction.get(fn);
    if (!graph) continue;
    for (const rawCallee of graphCalleeNames(graph)) {
      const callee = resolveFunctionName(functionNames, rawCallee);
      if (!callee || callee === fn) continue;
      let set = callers.get(callee);
      if (!set) {
        set = new Set();
        callers.set(callee, set);
      }
      set.add(fn);
    }
  }

  const roots = graphNames.filter((name) => (callers.get(name)?.size ?? 0) === 0);
  const candidates = roots.length > 0 ? roots : graphNames;
  let best: string | null = null;
  let bestNodeCount = -1;
  for (const name of candidates) {
    const graph = graphsByFunction.get(name);
    const nodeCount = graph ? Object.keys(graph.nodes).length : 0;
    if (nodeCount > bestNodeCount) {
      best = name;
      bestNodeCount = nodeCount;
    }
  }
  return best;
}

export function selectDefaultFunctionName(
  functionNames: readonly string[],
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): string | null {
  return (
    selectMainFunctionName(functionNames) ??
    selectLongestWorkflowFunctionName(functionNames, graphsByFunction)
  );
}
