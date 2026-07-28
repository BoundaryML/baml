import type { ControlFlowGraph, FunctionInfo } from './worker-protocol';

function calleeNames(graph: ControlFlowGraph): string[] {
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

function resolveFunctionName(
  functionNames: readonly string[],
  callerName: string,
  rawName: string,
): string | null {
  if (functionNames.includes(rawName)) return rawName;

  const namespaceEnd = callerName.lastIndexOf('.');
  if (namespaceEnd >= 0) {
    const sameNamespaceName = `${callerName.slice(0, namespaceEnd)}.${rawName}`;
    if (functionNames.includes(sameNamespaceName)) return sameNamespaceName;
  }

  const matches = functionNames.filter(
    (name) => name.endsWith(`.${rawName}`) || rawName.endsWith(`.${name}`),
  );
  return matches.length === 1 ? matches[0]! : null;
}

export function functionCallBreadths(
  functionNames: readonly string[],
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): ReadonlyMap<string, number> {
  const calleesByFunction = new Map<string, Set<string>>();
  for (const functionName of functionNames) {
    const graph = graphsByFunction.get(functionName);
    const callees = new Set<string>();
    if (graph) {
      for (const rawName of calleeNames(graph)) {
        const callee = resolveFunctionName(
          functionNames,
          functionName,
          rawName,
        );
        if (callee && callee !== functionName) callees.add(callee);
      }
    }
    calleesByFunction.set(functionName, callees);
  }

  const breadths = new Map<string, number>();
  for (const functionName of functionNames) {
    const reachable = new Set<string>();
    const pending = [...(calleesByFunction.get(functionName) ?? [])];
    while (pending.length > 0) {
      const callee = pending.pop()!;
      if (reachable.has(callee)) continue;
      reachable.add(callee);
      pending.push(...(calleesByFunction.get(callee) ?? []));
    }
    reachable.delete(functionName);
    breadths.set(functionName, reachable.size);
  }
  return breadths;
}

export function orderFunctionsByCallBreadth(
  functions: readonly FunctionInfo[],
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): FunctionInfo[] {
  const breadths = functionCallBreadths(
    functions.map((fn) => fn.name),
    graphsByFunction,
  );
  return [...functions].sort((left, right) => {
    const breadthDifference =
      (breadths.get(right.name) ?? 0) - (breadths.get(left.name) ?? 0);
    if (breadthDifference !== 0) return breadthDifference;
    return left.name < right.name ? -1 : left.name > right.name ? 1 : 0;
  });
}

export function orderFunctionsForExplorer(
  functions: readonly FunctionInfo[],
  graphResponses: ReadonlyMap<string, ControlFlowGraph | null>,
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): FunctionInfo[] {
  const hasAllGraphResponses = functions.every((fn) =>
    graphResponses.has(fn.name),
  );
  return hasAllGraphResponses
    ? orderFunctionsByCallBreadth(functions, graphsByFunction)
    : [...functions];
}
