import type { ControlFlowGraph, FunctionInfo } from './worker-protocol';

type FunctionNode = {
  component?: Component;
  edges: Set<FunctionNode>;
  functionInfo: FunctionInfo;
  lowLink: number;
  onStack: boolean;
  originalIndex: number;
  tarjanIndex: number;
};

type Component = {
  firstOriginalIndex: number;
  indegree: number;
  members: FunctionNode[];
  outgoing: Set<Component>;
};

/**
 * Order callers before the functions they call.
 *
 * Recursive functions form strongly connected components. Those components
 * keep their original relative order, as do otherwise unconstrained functions.
 */
export function topologicallySortFunctions(
  functions: readonly FunctionInfo[],
  graphsByFunction: ReadonlyMap<string, ControlFlowGraph>,
): FunctionInfo[] {
  if (functions.length < 2 || graphsByFunction.size === 0) {
    return [...functions];
  }

  const nodes: FunctionNode[] = functions.map(
    (functionInfo, originalIndex) => ({
      edges: new Set(),
      functionInfo,
      lowLink: -1,
      onStack: false,
      originalIndex,
      tarjanIndex: -1,
    }),
  );
  const nodesByExactName = new Map(
    nodes.map((node) => [node.functionInfo.name, node]),
  );
  const resolveFunction = (rawName: string): FunctionNode | undefined =>
    nodesByExactName.get(rawName) ??
    nodes.find(
      (node) =>
        node.functionInfo.name.endsWith(`.${rawName}`) ||
        rawName.endsWith(`.${node.functionInfo.name}`),
    );

  for (const caller of nodes) {
    const graph = graphsByFunction.get(caller.functionInfo.name);
    if (!graph) continue;

    for (const node of Object.values(graph.nodes)) {
      const calleeNames =
        node.calleeNames && node.calleeNames.length > 0
          ? node.calleeNames
          : node.calleeName
            ? [node.calleeName]
            : [];
      for (const rawCallee of calleeNames) {
        const callee = resolveFunction(rawCallee);
        if (callee && callee !== caller) caller.edges.add(callee);
      }
    }
  }

  const components = stronglyConnectedComponents(nodes);
  for (const caller of nodes) {
    const callerComponent = componentFor(caller);
    for (const callee of caller.edges) {
      const calleeComponent = componentFor(callee);
      if (
        callerComponent !== calleeComponent &&
        !callerComponent.outgoing.has(calleeComponent)
      ) {
        callerComponent.outgoing.add(calleeComponent);
        calleeComponent.indegree += 1;
      }
    }
  }

  const ready = components
    .filter((component) => component.indegree === 0)
    .sort(compareComponents);
  const sorted: FunctionInfo[] = [];

  while (ready.length > 0) {
    const component = ready.shift();
    if (!component) break;
    component.members.sort(
      (left, right) => left.originalIndex - right.originalIndex,
    );
    sorted.push(...component.members.map((member) => member.functionInfo));

    for (const callee of component.outgoing) {
      callee.indegree -= 1;
      if (callee.indegree === 0) {
        ready.push(callee);
        ready.sort(compareComponents);
      }
    }
  }

  return sorted;
}

function stronglyConnectedComponents(nodes: FunctionNode[]): Component[] {
  const stack: FunctionNode[] = [];
  const components: Component[] = [];
  let nextIndex = 0;

  const visit = (node: FunctionNode) => {
    node.tarjanIndex = nextIndex;
    node.lowLink = nextIndex;
    nextIndex += 1;
    stack.push(node);
    node.onStack = true;

    for (const callee of node.edges) {
      if (callee.tarjanIndex === -1) {
        visit(callee);
        node.lowLink = Math.min(node.lowLink, callee.lowLink);
      } else if (callee.onStack) {
        node.lowLink = Math.min(node.lowLink, callee.tarjanIndex);
      }
    }

    if (node.lowLink !== node.tarjanIndex) return;

    const members: FunctionNode[] = [];
    while (stack.length > 0) {
      const member = stack.pop();
      if (!member) break;
      member.onStack = false;
      members.push(member);
      if (member === node) break;
    }
    const component: Component = {
      firstOriginalIndex: Math.min(
        ...members.map((member) => member.originalIndex),
      ),
      indegree: 0,
      members,
      outgoing: new Set(),
    };
    for (const member of members) member.component = component;
    components.push(component);
  };

  for (const node of nodes) {
    if (node.tarjanIndex === -1) visit(node);
  }
  return components;
}

function componentFor(node: FunctionNode): Component {
  if (!node.component) {
    throw new Error(
      `Missing call-graph component for ${node.functionInfo.name}`,
    );
  }
  return node.component;
}

function compareComponents(left: Component, right: Component): number {
  return left.firstOriginalIndex - right.firstOriginalIndex;
}
