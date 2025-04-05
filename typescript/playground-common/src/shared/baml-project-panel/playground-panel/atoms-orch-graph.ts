import { atom } from 'jotai'
import { selectedFunctionObjectAtom } from './atoms'
import { runtimeAtom } from '../atoms'

export interface TypeCount {
  // options are F (Fallback), R (Retry), D (Direct), B (Round Robin)
  type: string

  // range from 0 to n
  index: number
  scope_name: string

  //only for retry
  retry_delay?: number
}

const getTypeLetter = (type: string): string => {
  switch (type) {
    case 'Fallback':
      return 'F'
    case 'Retry':
      return 'R'
    case 'Direct':
      return 'D'
    case 'RoundRobin':
      return 'B'
    default:
      return 'U'
  }
}

export interface ClientNode {
  name: string
  node_index: number
  type: string
  identifier: TypeCount[]
  retry_delay?: number

  //necessary for identifying unique round robins, as index matching is not enough
  round_robin_name?: string
}

export interface Edge {
  from_node: string
  to_node: string
  weight?: number
}

export interface NodeEntry {
  gid: ReturnType<typeof uuid>
  weight?: number
  node_index?: number
}
export interface GroupEntry {
  letter: string
  index: number
  orch_index?: number
  client_name?: string
  gid: ReturnType<typeof uuid>
  parentGid?: ReturnType<typeof uuid>
  Position?: Position
  Dimension?: Dimension
}

export interface Dimension {
  width: number
  height: number
}

export const orchIndexAtom = atom(0)
export const currentClientsAtom = atom((get) => {
  const func = get(selectedFunctionObjectAtom)
  const runtime = get(runtimeAtom).rt
  if (!func || !runtime) {
    return []
  }

  try {
    const wasmScopes = func.orchestration_graph(runtime)
    if (wasmScopes === null) {
      return []
    }

    const nodes = createClientNodes(wasmScopes)
    return nodes.map((node) => node.name)
  } catch (e) {
    console.error(e)
    return ['Error!']
  }
})

// something about the orchestration graph is broken, comment it out to make it work
export const orchestrationNodesAtom = atom((get): { nodes: GroupEntry[]; edges: Edge[] } => {
  const func = get(selectedFunctionObjectAtom)
  const runtime = get(runtimeAtom).rt
  if (!func || !runtime) {
    return { nodes: [], edges: [] }
  }

  const wasmScopes = func.orchestration_graph(runtime)
  if (wasmScopes === null) {
    return { nodes: [], edges: [] }
  }

  const nodes = createClientNodes(wasmScopes)
  const { unitNodes, groups } = buildUnitNodesAndGroups(nodes)

  const edges = createEdges(unitNodes)

  const positionedNodes = getPositions(groups)

  positionedNodes.forEach((posNode) => {
    const correspondingUnitNode = unitNodes.find((unitNode) => unitNode.gid === posNode.gid)
    if (correspondingUnitNode) {
      posNode.orch_index = correspondingUnitNode.node_index
    }
  })

  return { nodes: positionedNodes, edges }
})

interface Position {
  x: number
  y: number
}

function getPositions(nodes: { [key: string]: GroupEntry }): GroupEntry[] {
  const nodeEntries = Object.values(nodes)
  if (nodeEntries.length === 0) {
    return []
  }

  const adjacencyList: { [key: string]: string[] } = {}

  nodeEntries.forEach((node) => {
    if (node.parentGid) {
      if (!adjacencyList[node.parentGid]) {
        adjacencyList[node.parentGid] = []
      }
      adjacencyList[node.parentGid].push(node.gid)
    }
    if (!adjacencyList[node.gid]) {
      adjacencyList[node.gid] = []
    }
  })

  const rootNode = nodeEntries.find((node) => !node.parentGid)
  if (!rootNode) {
    console.error('No root node found')
    return []
  }

  const sizes = getSizes(adjacencyList, rootNode.gid)

  const positionsMap = getCoordinates(adjacencyList, rootNode.gid, sizes)
  const positionedNodes = nodeEntries.map((node) => ({
    ...node,
    Position: positionsMap[node.gid] || { x: 0, y: 0 },
    Dimension: sizes[node.gid] || { width: 0, height: 0 },
  }))

  return positionedNodes
}

function getCoordinates(
  adjacencyList: { [key: string]: string[] },
  rootNode: string,
  sizes: { [key: string]: { width: number; height: number } },
): { [key: string]: Position } {
  if (Object.keys(adjacencyList).length === 0 || Object.keys(sizes).length === 0) {
    return {}
  }

  const coordinates: { [key: string]: Position } = {}

  const PADDING = 60 // Define a constant padding value

  function recurse(node: string, horizontal: boolean, x: number, y: number): { x: number; y: number } {
    const children = adjacencyList[node]
    if (children.length === 0) {
      coordinates[node] = { x, y }
      return coordinates[node]
    }

    let childX = PADDING
    let childY = PADDING
    for (const child of children) {
      const childSize = recurse(child, !horizontal, childX, childY)

      if (!horizontal) {
        childY = childSize.y + PADDING + sizes[child].height
      } else {
        childX = childSize.x + PADDING + sizes[child].width
      }
    }

    coordinates[node] = { x, y }
    return coordinates[node]
  }

  recurse(rootNode, true, 0, 0)
  return coordinates
}

function getSizes(
  adjacencyList: { [key: string]: string[] },
  rootNode: string,
): { [key: string]: { width: number; height: number } } {
  if (Object.keys(adjacencyList).length === 0) {
    return {}
  }

  const sizes: { [key: string]: { width: number; height: number } } = {}

  const PADDING = 60 // Define a constant padding value

  function recurse(node: string, horizontal: boolean): { width: number; height: number } {
    const children = adjacencyList[node]
    if (children.length === 0) {
      sizes[node] = { width: 100, height: 50 }
      return sizes[node]
    }

    let width = horizontal ? PADDING : 0
    let height = horizontal ? 0 : PADDING
    for (const child of children) {
      const childSize = recurse(child, !horizontal)

      if (!horizontal) {
        width = Math.max(width, childSize.width)
        height += childSize.height + PADDING
      } else {
        width += childSize.width + PADDING
        height = Math.max(height, childSize.height)
      }
    }

    if (!horizontal) {
      width += 2 * PADDING // Add padding to the final width
    } else {
      height += 2 * PADDING // Add padding to the final height
    }

    sizes[node] = { width, height }
    return sizes[node]
  }

  recurse(rootNode, true)

  return sizes
}

function createClientNodes(wasmScopes: any[]): ClientNode[] {
  let indexOuter = 0
  const nodes: ClientNode[] = []

  for (const scope of wasmScopes) {
    const scopeInfo = scope.get_orchestration_scope_info()
    const scopePath = scopeInfo as any[]

    const stackGroup = createStackGroup(scopePath)

    // Always a direct node
    const lastScope = scopePath[scopePath.length - 1]

    const clientNode: ClientNode = {
      name: lastScope.name,
      node_index: indexOuter,
      type: lastScope.type,
      identifier: stackGroup,
    }

    nodes.push(clientNode)
    indexOuter++
  }

  return nodes
}

function createStackGroup(scopePath: any[]): TypeCount[] {
  const stackGroup: TypeCount[] = []

  for (let i = 0; i < scopePath.length; i++) {
    const scope = scopePath[i]
    const indexVal = scope.type === 'Retry' ? scope.count : scope.type === 'Direct' ? 0 : scope.index

    stackGroup.push({
      type: getTypeLetter(scope.type),
      index: indexVal,
      scope_name: scope.type === 'RoundRobin' ? scope.strategy_name : (scope.name ?? 'SOME_NAME'),
    })

    if (scope.type === 'Retry') {
      stackGroup[stackGroup.length - 1].retry_delay = scope.delay
    }
  }

  return stackGroup
}

function buildUnitNodesAndGroups(nodes: ClientNode[]): {
  unitNodes: NodeEntry[]
  groups: { [gid: string]: GroupEntry }
} {
  const unitNodes: NodeEntry[] = []
  const groups: { [gid: string]: GroupEntry } = {}
  const prevNodeIndexGroups: GroupEntry[] = []

  for (let index = 0; index < nodes.length; index++) {
    const node = nodes[index]
    const stackGroup = node.identifier
    let parentGid = ''
    let retry_cost = -1
    for (let stackIndex = 0; stackIndex < stackGroup.length; stackIndex++) {
      const scopeLayer = stackGroup[stackIndex]
      const prevScopeIdx = stackIndex > 0 ? stackGroup[stackIndex - 1].index : 0
      const prevNodeScope = prevNodeIndexGroups.at(stackIndex)
      const curGid = getScopeDetails(scopeLayer, prevScopeIdx, prevNodeScope)

      if (!(curGid in groups)) {
        groups[curGid] = {
          letter: scopeLayer.type,
          index: prevScopeIdx,
          client_name: scopeLayer.scope_name,
          gid: curGid,
          ...(parentGid && { parentGid }),
        }
        // Also clean indexGroups up to the current stackIndex
        prevNodeIndexGroups.length = stackIndex
      }

      prevNodeIndexGroups[stackIndex] = {
        letter: scopeLayer.type,
        index: prevScopeIdx,
        client_name: scopeLayer.scope_name,
        gid: curGid,
        ...(parentGid && { parentGid }),
      }

      parentGid = curGid

      if (scopeLayer.type === 'R' && scopeLayer.retry_delay !== 0) {
        retry_cost = scopeLayer.retry_delay ?? -1
      }
    }

    unitNodes.push({
      gid: parentGid,
      node_index: index,
      ...(retry_cost !== -1 && { weight: retry_cost }),
    })
  }

  return { unitNodes, groups }
}
let counter = 0
function uuid() {
  return String(counter++)
}
function getScopeDetails(scopeLayer: TypeCount, prevIdx: number, prevIndexGroupEntry: GroupEntry | undefined) {
  if (prevIndexGroupEntry === undefined) {
    return uuid()
  } else {
    const indexEntryGid = prevIndexGroupEntry.gid
    const indexEntryIdx = prevIndexGroupEntry.index
    const indexEntryScopeName = prevIndexGroupEntry.client_name

    switch (scopeLayer.type) {
      case 'B':
        if (scopeLayer.scope_name === indexEntryScopeName) {
          return indexEntryGid
        } else {
          return uuid()
        }
      default:
        if (prevIdx === indexEntryIdx) {
          return indexEntryGid
        } else {
          return uuid()
        }
    }
  }
}

function createEdges(unitNodes: NodeEntry[]): Edge[] {
  return unitNodes.slice(0, -1).map((fromNode, index) => ({
    from_node: fromNode.gid,
    to_node: unitNodes[index + 1].gid,
    ...(fromNode.weight !== null && { weight: fromNode.weight }),
  }))
}
